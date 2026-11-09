use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rscope"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

fn success(dir: &Path, args: &[&str]) -> String {
    let output = run(dir, args);
    assert!(
        output.status.success(),
        "{:?}: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn project() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("sample.py"),
        include_str!("fixtures/navigation.py"),
    )
    .unwrap();
    success(dir.path(), &["-b", "sample.py"]);
    dir
}

fn locations(output: &str) -> Vec<(String, usize)> {
    output
        .lines()
        .map(|line| {
            let parts: Vec<_> = line.splitn(4, ' ').collect();
            assert_eq!(parts.len(), 4);
            (parts[1].to_owned(), parts[2].parse().unwrap())
        })
        .collect()
}

#[test]
fn definitions_and_exact_identifier_occurrences() {
    let dir = project();
    assert_eq!(
        locations(&success(dir.path(), &["-dL1", "leaf"])),
        vec![("<global>".into(), 1)]
    );
    assert_eq!(
        locations(&success(dir.path(), &["-L1", "Worker"])),
        vec![("<global>".into(), 18)]
    );
    assert_eq!(
        locations(&success(dir.path(), &["-L1", "fetch"])),
        vec![("<global>".into(), 23)]
    );
    assert_eq!(
        locations(&success(dir.path(), &["-L0", "leaf"])),
        vec![
            ("<global>".into(), 1),
            ("<global>".into(), 5),
            ("outer".into(), 8),
            ("inner".into(), 12),
            ("<lambda>".into(), 14),
            ("save".into(), 20),
            ("<global>".into(), 27),
        ]
    );
    assert!(success(dir.path(), &["-0", "phantom"]).is_empty());
    assert!(success(dir.path(), &["-0", "lea"]).is_empty());
    assert!(success(dir.path(), &["-0", "' OR 1=1 --"]).is_empty());
    assert_eq!(
        locations(&success(dir.path(), &["-0", "message"])),
        vec![("outer".into(), 7)]
    );
}

#[test]
fn calls_have_correct_direction_and_lexical_body_context() {
    let dir = project();
    assert_eq!(
        locations(&success(dir.path(), &["-d", "-L", "-2", "outer"])),
        vec![("leaf".into(), 8), ("save".into(), 9), ("inner".into(), 15),]
    );
    assert_eq!(
        locations(&success(dir.path(), &["-3", "leaf"])),
        vec![
            ("<global>".into(), 5),
            ("outer".into(), 8),
            ("inner".into(), 12),
            ("<lambda>".into(), 14),
            ("save".into(), 20),
            ("<global>".into(), 27),
        ]
    );
    assert_eq!(
        locations(&success(dir.path(), &["-2", "fetch"])),
        vec![("load".into(), 24)]
    );
    assert_eq!(
        locations(&success(dir.path(), &["-3", "save"])),
        vec![("outer".into(), 9)]
    );
}

#[test]
fn file_lists_duplicates_custom_database_and_cwd_independence() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("one.py"), "def one():\n    pass\n").unwrap();
    fs::write(dir.path().join("rscope.files"), "'one.py'\none.py\n").unwrap();
    success(dir.path(), &["-b", "-f", "custom.db"]);
    let absolute = dir.path().join("custom.db");
    let output = success(
        Path::new("/tmp"),
        &["-dL", "-f", absolute.to_str().unwrap(), "-1", "one"],
    );
    assert_eq!(locations(&output), vec![("<global>".into(), 1)]);
    assert!(output.starts_with(dir.path().to_str().unwrap()));
    fs::rename(
        dir.path().join("rscope.files"),
        dir.path().join("inputs.txt"),
    )
    .unwrap();
    success(dir.path(), &["-b", "-i", "inputs.txt"]);
}

#[test]
fn legacy_lists_require_explicit_selection_and_explicit_files_override_the_default() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("current.py"), "def current():\n    pass\n").unwrap();
    fs::write(dir.path().join("legacy.py"), "def legacy():\n    pass\n").unwrap();
    fs::write(dir.path().join("cscope.files"), "legacy.py\n").unwrap();
    let output = run(dir.path(), &["-b"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("rscope.files"));
    assert!(!dir.path().join("cscope.out").exists());

    fs::write(dir.path().join("rscope.files"), "current.py\n").unwrap();
    success(dir.path(), &["-b"]);
    assert_eq!(locations(&success(dir.path(), &["-1", "current"])).len(), 1);
    assert!(success(dir.path(), &["-1", "legacy"]).is_empty());

    success(dir.path(), &["-b", "-i", "cscope.files"]);
    assert_eq!(locations(&success(dir.path(), &["-1", "legacy"])).len(), 1);
    assert!(success(dir.path(), &["-1", "current"]).is_empty());

    success(dir.path(), &["-b", "current.py"]);
    assert_eq!(locations(&success(dir.path(), &["-1", "current"])).len(), 1);
    assert!(success(dir.path(), &["-1", "legacy"]).is_empty());
}

#[test]
fn rebuild_removes_old_files_and_old_symbols() {
    let dir = project();
    fs::remove_file(dir.path().join("sample.py")).unwrap();
    fs::write(
        dir.path().join("replacement.py"),
        "def fresh():\n    pass\n",
    )
    .unwrap();
    success(dir.path(), &["-b", "replacement.py"]);
    assert!(success(dir.path(), &["-1", "leaf"]).is_empty());
    assert_eq!(locations(&success(dir.path(), &["-1", "fresh"])).len(), 1);
}

#[test]
fn failed_rebuild_preserves_previous_index() {
    let dir = project();
    for (name, content) in [
        ("bad.py", "def broken(:\n"),
        ("wrong.cpp", "int main() {}"),
        ("bad_utf8.py", ""),
    ] {
        fs::write(dir.path().join(name), content).unwrap();
        if name == "bad_utf8.py" {
            fs::write(dir.path().join(name), [0xff]).unwrap();
        }
        let output = run(dir.path(), &["-b", "sample.py", name]);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert_eq!(locations(&success(dir.path(), &["-1", "leaf"])).len(), 1);
    }
    assert!(!run(dir.path(), &["-b", "missing.py"]).status.success());
    assert_eq!(locations(&success(dir.path(), &["-1", "leaf"])).len(), 1);
}

#[test]
fn missing_index_invalid_arguments_and_unsafe_paths_are_errors() {
    let dir = tempfile::tempdir().unwrap();
    assert!(!run(dir.path(), &["-dL1", "x"]).status.success());
    assert!(!dir.path().join("cscope.out").exists());
    for args in [
        vec!["-5", "x"],
        vec!["-l"],
        vec!["-1", "x", "-3", "x"],
        vec!["-b", "-d"],
        vec!["-b"],
    ] {
        assert!(!run(dir.path(), &args).status.success(), "{args:?}");
    }
    fs::write(dir.path().join("has space.py"), "x=1\n").unwrap();
    assert!(!run(dir.path(), &["-b", "has space.py"]).status.success());
    assert!(!dir.path().join("cscope.out").exists());
}

#[test]
fn unrelated_databases_are_not_modified_and_stale_indexes_require_rebuild() {
    let dir = project();
    let unrelated = rusqlite::Connection::open(dir.path().join("other.db")).unwrap();
    unrelated
        .execute_batch("CREATE TABLE metadata (value TEXT); INSERT INTO metadata VALUES ('keep');")
        .unwrap();
    assert!(
        !run(dir.path(), &["-b", "-f", "other.db", "sample.py"])
            .status
            .success()
    );
    let value: String = unrelated
        .query_row("SELECT value FROM metadata", [], |r| r.get(0))
        .unwrap();
    assert_eq!(value, "keep");
    let own = rusqlite::Connection::open(dir.path().join("cscope.out")).unwrap();
    own.execute("UPDATE metadata SET value='old'", []).unwrap();
    assert!(!run(dir.path(), &["-1", "leaf"]).status.success());
    success(dir.path(), &["-b", "sample.py"]);
    assert_eq!(locations(&success(dir.path(), &["-1", "leaf"])).len(), 1);
}

#[test]
fn decorators_defaults_fstrings_and_complex_call_targets() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("special.py"),
        "@decorate(factory())\ndef target(value=setup()):\n    result = f\"{helper(value)}\"\n    handlers[key]()\n    return result\n",
    ).unwrap();
    success(dir.path(), &["-b", "special.py"]);
    assert_eq!(
        locations(&success(dir.path(), &["-2", "target"])),
        vec![("helper".into(), 3)]
    );
    assert_eq!(
        locations(&success(dir.path(), &["-3", "setup"])),
        vec![("<global>".into(), 2)]
    );
    assert_eq!(
        locations(&success(dir.path(), &["-3", "decorate"])),
        vec![("<global>".into(), 1)]
    );
    assert_eq!(
        locations(&success(dir.path(), &["-3", "factory"])),
        vec![("<global>".into(), 1)]
    );
    assert_eq!(
        locations(&success(dir.path(), &["-0", "helper"])),
        vec![("target".into(), 3)]
    );
    assert!(success(dir.path(), &["-3", "handlers"]).is_empty());
}

#[test]
fn same_named_functions_are_combined_without_claiming_resolution() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.py"), "def same():\n    left()\n").unwrap();
    fs::write(dir.path().join("b.py"), "def same():\n    right()\n").unwrap();
    success(dir.path(), &["-b", "a.py", "b.py"]);
    assert_eq!(locations(&success(dir.path(), &["-1", "same"])).len(), 2);
    assert_eq!(
        locations(&success(dir.path(), &["-2", "same"])),
        vec![("left".into(), 2), ("right".into(), 2)]
    );
}

fn search_project() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("search.py"),
        include_str!("fixtures/searches.py"),
    )
    .unwrap();
    fs::write(dir.path().join("empty.py"), "").unwrap();
    success(dir.path(), &["-b", "search.py", "empty.py"]);
    dir
}

#[test]
fn text_and_regex_search_live_files_including_comments_and_strings() {
    let dir = search_project();
    assert_eq!(
        locations(&success(dir.path(), &["-dL4", "needle"])),
        [7, 8, 20, 26, 27].map(|line| ("<unknown>".into(), line))
    );
    // Literal search does not interpret regex metacharacters, SQL wildcards,
    // or leading hyphens. Multiple hits on one line produce one result.
    for text in ["[x].*", "100%_", "-needle", "café"] {
        assert_eq!(
            locations(&success(dir.path(), &["-4", text])),
            vec![("<unknown>".into(), 7)]
        );
    }
    assert!(success(dir.path(), &["-0", "needle"]).is_empty());
    assert_eq!(
        locations(&success(dir.path(), &["-6", "^    # comment-only needle$"])),
        vec![("<unknown>".into(), 20)]
    );
    assert_eq!(
        locations(&success(dir.path(), &["-6", "(?i)needle"])),
        [7, 8, 20, 23, 26, 27].map(|line| ("<unknown>".into(), line))
    );
    assert!(success(dir.path(), &["-4", "absent"]).is_empty());
    // Text search follows live source contents without a rebuild.
    fs::write(dir.path().join("search.py"), "# fresh needle\n").unwrap();
    assert_eq!(
        locations(&success(dir.path(), &["-4", "needle"])),
        vec![("<unknown>".into(), 1)]
    );
    fs::remove_file(dir.path().join("search.py")).unwrap();
    let missing = run(dir.path(), &["-4", "needle"]);
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("opening source"));
    success(dir.path(), &["-b", "empty.py"]);
    assert!(success(dir.path(), &["-6", "needle"]).is_empty());
}

#[test]
fn file_search_finds_empty_files_and_matches_absolute_paths() {
    let dir = search_project();
    assert_eq!(
        locations(&success(dir.path(), &["-7", r"empty\.py$"])),
        vec![("<unknown>".into(), 1)]
    );
    assert_eq!(locations(&success(dir.path(), &["-7", r"\.py$"])).len(), 2);
    let output = success(dir.path(), &["-7", dir.path().to_str().unwrap()]);
    assert_eq!(output.lines().count(), 2);
    assert!(success(dir.path(), &["-7", r"missing\.py$"]).is_empty());
}

#[test]
fn import_search_uses_written_modules_without_guessing_member_or_alias_resolution() {
    let dir = search_project();
    for (pattern, line, context) in [
        ("^os$", 1, "<global>"),
        (r"^package\.module$", 2, "<global>"),
        ("^sys$", 2, "<global>"),
        (r"^package\.tools$", 3, "<global>"),
        (r"^\.relative$", 4, "<global>"),
        (r"^\.\.$", 5, "<global>"),
        (r"^local\.module$", 21, "work"),
        (r"^local\.tools$", 22, "work"),
    ] {
        assert_eq!(
            locations(&success(dir.path(), &["-8", pattern])),
            vec![(context.into(), line)],
            "{pattern}"
        );
    }
    for name in ["alias", "execute", "Item", "thing", "local_alias"] {
        assert!(success(dir.path(), &["-8", name]).is_empty(), "{name}");
    }
    assert_eq!(locations(&success(dir.path(), &["-8", "package"])).len(), 2);
    // Import records must not add dotted names to identifier-only searches.
    assert!(success(dir.path(), &["-0", "package.module"]).is_empty());
}

#[test]
fn assignments_track_written_targets_and_exclude_reads_and_annotations() {
    let dir = search_project();
    for (name, expected) in [
        ("counter", vec![9, 15, 17]),
        ("first", vec![11]),
        ("second", vec![11]),
        ("left", vec![12]),
        ("right", vec![12]),
        ("rest", vec![12]),
        ("field", vec![13]),
        ("mapping", vec![14]),
        ("found", vec![16]),
    ] {
        let expected: Vec<_> = expected
            .into_iter()
            .map(|n| ("<global>".into(), n))
            .collect();
        assert_eq!(
            locations(&success(dir.path(), &["-9", name])),
            expected,
            "{name}"
        );
    }
    for name in ["annotation_only", "obj", "key", "lookup", "alias", "value"] {
        assert!(success(dir.path(), &["-9", name]).is_empty(), "{name}");
    }
    assert_eq!(
        locations(&success(dir.path(), &["-9", "local_value"])),
        vec![("work".into(), 23)]
    );
    assert_eq!(locations(&success(dir.path(), &["-0", "counter"])).len(), 4);
}

#[test]
fn invalid_patterns_and_conflicting_query_flags_fail_without_partial_output() {
    let dir = search_project();
    for mode in ["-6", "-7", "-8"] {
        let output = run(dir.path(), &[mode, "["]);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("invalid search regex"));
        // Rust regex syntax deliberately excludes backreferences and look-around.
        assert!(!run(dir.path(), &[mode, "(?=needle)"]).status.success());
    }
    for args in [
        vec!["-4", "needle", "-6", "needle"],
        vec!["-b", "-7", "search.py"],
        vec!["-9", "counter", "-1", "counter"],
    ] {
        assert!(!run(dir.path(), &args).status.success());
    }
    // The same invalid-regex string is a valid literal text pattern.
    assert_eq!(
        locations(&success(dir.path(), &["-4", "["])),
        vec![("<unknown>".into(), 7), ("<unknown>".into(), 14)]
    );
}

#[test]
fn failed_rebuild_keeps_manifest_and_symbol_records() {
    let dir = search_project();
    fs::write(dir.path().join("bad.py"), "def broken(:").unwrap();
    assert!(!run(dir.path(), &["-b", "bad.py"]).status.success());
    assert_eq!(locations(&success(dir.path(), &["-4", "needle"])).len(), 5);
    assert_eq!(locations(&success(dir.path(), &["-7", r"\.py$"])).len(), 2);
    assert_eq!(locations(&success(dir.path(), &["-8", "package"])).len(), 2);
    assert_eq!(locations(&success(dir.path(), &["-9", "counter"])).len(), 3);
}

#[test]
fn old_indexes_require_rebuild_and_can_be_replaced_atomically() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("search.py"),
        include_str!("fixtures/searches.py"),
    )
    .unwrap();
    fs::write(dir.path().join("empty.py"), "").unwrap();
    let connection = rusqlite::Connection::open(dir.path().join("cscope.out")).unwrap();
    connection
        .pragma_update(None, "application_id", 0x52534350)
        .unwrap();
    connection
        .execute_batch(
            "CREATE TABLE metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL);
        INSERT INTO metadata VALUES ('index_version','schema2-python0.25.0-ts0.25.10-extract2');
        CREATE TABLE files(path TEXT PRIMARY KEY);
        CREATE TABLE source_lines(path TEXT,line INTEGER,text TEXT);
        INSERT INTO source_lines VALUES ('old.py',1,'old snapshot');",
        )
        .unwrap();
    for mode in ["-0", "-4", "-6", "-7", "-8", "-9"] {
        let output = run(dir.path(), &[mode, "needle"]);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("rebuild with -b"));
    }
    success(dir.path(), &["-b", "search.py", "empty.py"]);
    assert_eq!(locations(&success(dir.path(), &["-4", "needle"])).len(), 5);
    // Existing readers keep their generation; the new file has no source table.
    let old_text: String = connection
        .query_row("SELECT text FROM source_lines", [], |r| r.get(0))
        .unwrap();
    assert_eq!(old_text, "old snapshot");
    let new = rusqlite::Connection::open(dir.path().join("cscope.out")).unwrap();
    assert!(new.prepare("SELECT * FROM source_lines").is_err());
}

#[test]
fn crlf_blank_lines_and_missing_final_newlines_keep_source_line_numbers() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("lines.py"),
        "# needle\r\n\r\nvalue = 'needle'",
    )
    .unwrap();
    success(dir.path(), &["-b", "lines.py"]);
    assert_eq!(
        locations(&success(dir.path(), &["-6", "needle.*$"])),
        vec![("<unknown>".into(), 1), ("<unknown>".into(), 3)]
    );
    assert_eq!(
        locations(&success(dir.path(), &["-6", "^$"])),
        vec![("<unknown>".into(), 2)]
    );
    assert_eq!(
        locations(&success(dir.path(), &["-9", "value"])),
        vec![("<global>".into(), 3)]
    );
}

#[test]
fn multiline_imports_normalize_module_names_and_return_each_source_line_once() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("imports.py"), "import package . spaced as alias, package.other\nimport long.\\\nmodule\nfrom package . tools import (\n    run,\n    stop,\n)\nfrom wild import *\n").unwrap();
    success(dir.path(), &["-b", "imports.py"]);
    assert_eq!(
        locations(&success(dir.path(), &["-8", "^package"])),
        vec![("<global>".into(), 1), ("<global>".into(), 4)]
    );
    assert_eq!(
        locations(&success(dir.path(), &["-8", r"^long\.module$"])),
        vec![("<global>".into(), 2)]
    );
    assert_eq!(
        locations(&success(dir.path(), &["-8", "^wild$"])),
        vec![("<global>".into(), 8)]
    );
}

#[test]
fn lean_index_stores_file_ids_and_locations_without_source_text() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("tiny.py"),
        "value = 'SOURCE_PAYLOAD_DO_NOT_STORE'\n",
    )
    .unwrap();
    success(dir.path(), &["-b", "tiny.py"]);
    let size_before = fs::metadata(dir.path().join("cscope.out")).unwrap().len();
    let mut text = "value = 'SOURCE_PAYLOAD_DO_NOT_STORE'\n".to_owned();
    text.push_str("\"\"\"\n");
    text.push_str(&"SOURCE_PAYLOAD_DO_NOT_STORE text without symbols\n".repeat(50_000));
    text.push_str("\"\"\"\n");
    fs::write(dir.path().join("tiny.py"), text).unwrap();
    success(dir.path(), &["-b", "tiny.py"]);
    let size_after = fs::metadata(dir.path().join("cscope.out")).unwrap().len();
    assert!(
        size_after <= size_before + 4096,
        "source text should not create line rows: {size_before} -> {size_after}"
    );
    let connection = rusqlite::Connection::open(dir.path().join("cscope.out")).unwrap();
    let mut statement = connection
        .prepare("PRAGMA table_info(occurrences)")
        .unwrap();
    let columns: Vec<String> = statement
        .query_map([], |r| r.get(1))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert!(columns.contains(&"file_id".into()));
    assert!(
        !columns
            .iter()
            .any(|c| matches!(c.as_str(), "path" | "snippet" | "text" | "context"))
    );
    let file_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))
        .unwrap();
    assert_eq!(file_count, 1);
    assert!(connection.prepare("SELECT * FROM source_lines").is_err());
    let bytes = fs::read(dir.path().join("cscope.out")).unwrap();
    assert!(
        !bytes
            .windows(b"SOURCE_PAYLOAD_DO_NOT_STORE".len())
            .any(|w| w == b"SOURCE_PAYLOAD_DO_NOT_STORE")
    );
    assert!(success(dir.path(), &["-1", "absent"]).is_empty());
    assert!(success(dir.path(), &["-9", "value"]).contains("SOURCE_PAYLOAD_DO_NOT_STORE"));
}

#[test]
fn changed_files_are_live_for_text_but_stale_for_symbol_queries() {
    let dir = search_project();
    fs::write(
        dir.path().join("search.py"),
        "# live needle\nimport renamed\ncounter = 20000\n",
    )
    .unwrap();
    assert_eq!(
        locations(&success(dir.path(), &["-4", "needle"])),
        vec![("<unknown>".into(), 1)]
    );
    for args in [
        vec!["-0", "counter"],
        vec!["-1", "work"],
        vec!["-3", "lookup"],
        vec!["-8", "package"],
        vec!["-9", "counter"],
    ] {
        let output = run(dir.path(), &args);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains("source changed since indexing") && error.contains("rebuild with -b"),
            "{error}"
        );
    }
    success(dir.path(), &["-b", "search.py", "empty.py"]);
    assert_eq!(
        locations(&success(dir.path(), &["-9", "counter"])),
        vec![("<global>".into(), 3)]
    );
    assert!(success(dir.path(), &["-8", "package"]).is_empty());
    assert_eq!(locations(&success(dir.path(), &["-8", "renamed"])).len(), 1);
}

#[test]
fn freshness_checks_detect_same_size_edits_and_size_changes_with_preserved_mtime() {
    use std::time::Duration;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("change.py");
    fs::write(&path, "def before():\n    pass\n").unwrap();
    success(dir.path(), &["-b", "change.py"]);
    let original_time = fs::metadata(&path).unwrap().modified().unwrap();
    fs::write(&path, "def after_():\n    pass\n").unwrap();
    fs::File::open(&path)
        .unwrap()
        .set_modified(original_time + Duration::from_secs(1))
        .unwrap();
    assert!(!run(dir.path(), &["-1", "before"]).status.success());
    fs::write(&path, "def after_():\n    pass\n# extra line\n").unwrap();
    fs::File::open(&path)
        .unwrap()
        .set_modified(original_time)
        .unwrap();
    assert!(!run(dir.path(), &["-1", "before"]).status.success());
}

#[test]
fn only_manifest_files_are_searched_and_missing_files_are_reported() {
    let dir = project();
    fs::write(dir.path().join("new.py"), "# new unique term\n").unwrap();
    assert!(success(dir.path(), &["-4", "new unique term"]).is_empty());
    success(dir.path(), &["-b", "sample.py", "new.py"]);
    assert_eq!(
        locations(&success(dir.path(), &["-4", "new unique term"])).len(),
        1
    );
    fs::remove_file(dir.path().join("sample.py")).unwrap();
    for args in [vec!["-1", "leaf"], vec!["-6", "leaf"], vec!["-7", "sample"]] {
        let output = run(dir.path(), &args);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("sample.py"));
    }
}

#[test]
fn late_build_failure_preserves_database_bytes_and_cleans_temporary_files() {
    let dir = project();
    let original = fs::read(dir.path().join("cscope.out")).unwrap();
    fs::write(dir.path().join("new.py"), "def added():\n    pass\n").unwrap();
    fs::write(dir.path().join("broken.py"), "def broken(:").unwrap();
    assert!(
        !run(dir.path(), &["-b", "new.py", "broken.py"])
            .status
            .success()
    );
    assert_eq!(fs::read(dir.path().join("cscope.out")).unwrap(), original);
    assert_eq!(locations(&success(dir.path(), &["-1", "leaf"])).len(), 1);
    assert!(success(dir.path(), &["-1", "added"]).is_empty());
    let files: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|p| p.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(!files.iter().any(|p| p.starts_with(".tmp")), "{files:?}");
    assert!(
        !run(dir.path(), &["-b", "-f", "first.db", "new.py", "broken.py"])
            .status
            .success()
    );
    assert!(!dir.path().join("first.db").exists());
}

#[test]
fn many_files_are_indexed_and_result_lines_are_read_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let mut args = vec!["-b".to_owned()];
    for i in 0..100 {
        let path = format!("file{i:03}.py");
        fs::write(
            dir.path().join(&path),
            "def run():\n    leaf(); other()\n    leaf()\n",
        )
        .unwrap();
        args.push(path);
    }
    let refs: Vec<_> = args.iter().map(String::as_str).collect();
    success(dir.path(), &refs);
    assert_eq!(locations(&success(dir.path(), &["-1", "run"])).len(), 100);
    let output = locations(&success(dir.path(), &["-2", "run"]));
    assert_eq!(output.len(), 300);
    for chunk in output.chunks(3) {
        assert_eq!(
            chunk,
            [("leaf".into(), 2), ("other".into(), 2), ("leaf".into(), 3)]
        );
    }
}
