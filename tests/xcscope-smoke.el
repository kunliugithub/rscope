;;; xcscope-smoke.el --- Exercise the installed xcscope adapter -*- lexical-binding: t; -*-

;; Run after cargo build; see README.md. xcscope is a separate prerequisite.
;; Set RSCOPE_TEST_JOBS=4 to exercise navigation on parallel-built indexes.
(require 'ert)
(require 'xcscope)

(defun rscope-smoke--navigation (filename content queries)
  (let* ((binary (or (getenv "RSCOPE_BINARY")
                     (expand-file-name "target/debug/rscope")))
         (directory (make-temp-file "rscope-emacs-" t))
         (default-directory (file-name-as-directory directory))
         (source (expand-file-name filename directory))
         (cscope-program binary)
         (cscope-database-file "cscope.out")
         (cscope-initial-directory default-directory)
         (cscope-database-regexps nil)
         (cscope-option-do-not-update-database t)
         (cscope-option-include-directories nil)
         (cscope-option-disable-compression nil)
         (cscope-option-kernel-mode nil)
         (cscope-option-use-inverted-index nil)
         (cscope-option-other nil)
         (cscope-display-cscope-buffer t)
         (cscope-edit-single-match nil)
         (cscope-display-times nil))
    (unwind-protect
        (progn
          (with-temp-file source (insert content))
          (should (= 0 (call-process binary nil nil nil "-b" "--jobs"
                                     (or (getenv "RSCOPE_TEST_JOBS") "1") source)))
          (switch-to-buffer (find-file-noselect source))
          (dolist (query queries)
            (when (get-buffer cscope-output-buffer-name)
              (kill-buffer cscope-output-buffer-name))
            (switch-to-buffer (find-file-noselect source))
            (funcall (car query) (cadr query))
            (let ((deadline (+ (float-time) 5)))
              (while (and cscope-process (< (float-time) deadline))
                (accept-process-output nil 0.05)))
            (should-not cscope-process)
            (let ((results (get-buffer cscope-output-buffer-name))
                  (lines nil)
                  first-entry)
              (with-current-buffer results
                (let ((position (point-min)))
                  (while (< position (point-max))
                    (let ((line (get-text-property position 'cscope-line-number)))
                      (when line
                        (unless first-entry (setq first-entry position))
                        (should (equal source (get-text-property position 'cscope-file)))
                        (push line lines)))
                    (setq position (next-single-property-change
                                    position 'cscope-line-number nil (point-max))))))
              (should (equal (nreverse lines) (nth 2 query)))
              ;; Follow a result using xcscope's own navigation implementation.
              (cscope-show-entry-internal
               (cscope-get-navigation-properties first-entry results) t)
              (should (equal (buffer-file-name) source))
              (should (= (line-number-at-pos) (car (nth 2 query)))))))
      (when cscope-process (delete-process cscope-process) (setq cscope-process nil))
      (when (get-buffer cscope-output-buffer-name) (kill-buffer cscope-output-buffer-name))
      (when (get-file-buffer source) (kill-buffer (get-file-buffer source)))
      (delete-directory directory t))))

(ert-deftest rscope-xcscope-python-navigation ()
  (rscope-smoke--navigation
   "sample.py"
   (concat "def helper():\n    pass\n\ndef main():\n    helper()\n"
           "# text-only needle [x]\nmessage = \"needle\"\n"
           "import package.module\nfrom package.tools import helper\n"
           "value = 1\nvalue += 1\n")
   '((cscope-find-global-definition "helper" (1))
     (cscope-find-this-symbol "helper" (1 5 9))
     (cscope-find-called-functions "main" (5))
     (cscope-find-functions-calling-this-function "helper" (5))
     (cscope-find-this-text-string "needle" (6 7))
     (cscope-find-egrep-pattern "^# text-only.*needle" (6))
     (cscope-find-this-file "sample\\.py$" (1))
     (cscope-find-files-including-file "^package\\." (8 9))
     (cscope-find-assignments-to-this-symbol "value" (10 11)))))

(ert-deftest rscope-xcscope-c-navigation ()
  (rscope-smoke--navigation
   "sample.c"
   (concat "#include <stddef.h>\n#define LIMIT 4\nint helper(void) {\n    return LIMIT;\n}\n"
           "int main(void) {\n    int value = helper();\n    value += 1;\n    return value;\n}\n"
           "// text-only needle\nconst char *message = \"needle\";\n")
   '((cscope-find-global-definition "helper" (3))
     (cscope-find-this-symbol "helper" (3 7))
     (cscope-find-called-functions "main" (7))
     (cscope-find-functions-calling-this-function "helper" (7))
     (cscope-find-this-text-string "needle" (11 12))
     (cscope-find-egrep-pattern "^// text-only.*needle" (11))
     (cscope-find-this-file "sample\\.c$" (1))
     (cscope-find-files-including-file "^stddef\\.h$" (1))
     (cscope-find-assignments-to-this-symbol "value" (7 8)))))

(ert-deftest rscope-xcscope-rust-navigation ()
  (rscope-smoke--navigation
   "sample.rs"
   (concat "use std::fmt;\nfn helper() {}\nfn main() {\n"
           "    let mut value = 1;\n    helper();\n    value += 1;\n}\n"
           "// text-only needle\nconst MESSAGE: &str = \"needle\";\n")
   '((cscope-find-global-definition "helper" (2))
     (cscope-find-this-symbol "helper" (2 5))
     (cscope-find-called-functions "main" (5))
     (cscope-find-functions-calling-this-function "helper" (5))
     (cscope-find-this-text-string "needle" (8 9))
     (cscope-find-egrep-pattern "^// text-only.*needle" (8))
     (cscope-find-this-file "sample\\.rs$" (1))
     (cscope-find-files-including-file "^std::fmt$" (1))
     (cscope-find-assignments-to-this-symbol "value" (4 6)))))

(ert-deftest rscope-xcscope-java-navigation ()
  (rscope-smoke--navigation
   "Sample.java"
   (concat "import java.util.List;\nclass Sample {\n    void helper() {}\n"
           "    void main() {\n        int value = 1;\n        helper();\n        value++;\n    }\n"
           "    // text-only needle\n    String message = \"needle\";\n}\n")
   '((cscope-find-global-definition "helper" (3))
     (cscope-find-this-symbol "helper" (3 6))
     (cscope-find-called-functions "main" (6))
     (cscope-find-functions-calling-this-function "helper" (6))
     (cscope-find-this-text-string "needle" (9 10))
     (cscope-find-egrep-pattern "^    // text-only.*needle" (9))
     (cscope-find-this-file "Sample\\.java$" (1))
     (cscope-find-files-including-file "^java\\.util\\.List$" (1))
     (cscope-find-assignments-to-this-symbol "value" (5 7)))))

(ert-run-tests-batch-and-exit)
