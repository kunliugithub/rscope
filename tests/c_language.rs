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
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn locations(output: &str) -> Vec<(String, usize)> {
    output
        .lines()
        .map(|line| {
            let fields: Vec<_> = line.splitn(4, ' ').collect();
            assert_eq!(fields.len(), 4);
            (fields[1].to_owned(), fields[2].parse().unwrap())
        })
        .collect()
}

fn project() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("navigation.c"),
        include_str!("fixtures/navigation.c"),
    )
    .unwrap();
    fs::write(
        dir.path().join("c_nodes.h"),
        include_str!("fixtures/c_nodes.h"),
    )
    .unwrap();
    success(dir.path(), &["-b", "navigation.c", "c_nodes.h"]);
    dir
}

#[test]
fn c_definitions_distinguish_prototypes_externs_function_pointers_and_type_bodies() {
    let dir = project();
    for (name, line, context) in [
        ("leaf", 6, "<global>"),
        ("pointer_result", 7, "<global>"),
        ("factory", 8, "<global>"),
        ("callback", 9, "<global>"),
        ("counter", 5, "<global>"),
        ("no_initializer", 10, "<global>"),
        ("local", 13, "outer"),
        ("Data", 5, "<global>"),
        ("Status", 6, "<global>"),
        ("READY", 6, "<global>"),
        ("DONE", 6, "<global>"),
        ("visit", 4, "<global>"),
        ("LIMIT", 4, "<global>"),
        ("SCALE", 3, "<global>"),
        ("RSCOPE_NODES_H", 2, "<global>"),
    ] {
        assert_eq!(
            locations(&success(dir.path(), &["-1", name])),
            vec![(context.into(), line)],
            "{name}"
        );
    }
    assert_eq!(
        locations(&success(dir.path(), &["-1", "Node"])),
        vec![("<global>".into(), 3), ("<global>".into(), 4)]
    );
    for name in ["declared", "external"] {
        assert!(success(dir.path(), &["-1", name]).is_empty());
    }
    assert_eq!(
        locations(&success(dir.path(), &["-0", "declared"])).len(),
        1
    );
    assert_eq!(
        locations(&success(dir.path(), &["-1", "selected"])).len(),
        2
    );
}

#[test]
fn c_calls_are_syntactic_with_correct_direction_and_function_context() {
    let dir = project();
    assert_eq!(
        locations(&success(dir.path(), &["-dL2", "outer"])),
        vec![
            ("leaf".into(), 13),
            ("leaf".into(), 18),
            ("visit".into(), 19),
            ("callback".into(), 20),
            ("SCALE".into(), 25),
        ]
    );
    assert_eq!(
        locations(&success(dir.path(), &["-3", "leaf"])),
        vec![
            ("outer".into(), 13),
            ("outer".into(), 18),
            ("selected".into(), 28),
            ("selected".into(), 30),
        ]
    );
    assert!(success(dir.path(), &["-0", "phantom"]).is_empty());
    assert!(success(dir.path(), &["-3", "phantom"]).is_empty());
    assert_eq!(locations(&success(dir.path(), &["-4", "phantom"])).len(), 1);
    assert!(success(dir.path(), &["-2", "factory"]).is_empty());
}

#[test]
fn c_writes_distinguish_initializers_updates_pointees_fields_containers_and_reads() {
    let dir = project();
    for (name, expected) in [
        (
            "counter",
            vec![("<global>".into(), 5), ("outer".into(), 15)],
        ),
        ("local", vec![("outer".into(), 13), ("outer".into(), 16)]),
        ("value", vec![("outer".into(), 14)]),
        ("values", vec![("outer".into(), 17), ("outer".into(), 18)]),
        ("ptr", vec![("outer".into(), 21), ("outer".into(), 22)]),
        ("callback", vec![("<global>".into(), 9)]),
    ] {
        assert_eq!(
            locations(&success(dir.path(), &["-9", name])),
            expected,
            "{name}"
        );
    }
    for name in ["node", "leaf", "no_initializer", "DONE"] {
        assert!(success(dir.path(), &["-9", name]).is_empty(), "{name}");
    }
}

#[test]
fn c_includes_and_live_searches_share_the_existing_query_interface() {
    let dir = project();
    assert_eq!(
        locations(&success(dir.path(), &["-8", r"^c_nodes\.h$"])),
        vec![("<global>".into(), 1)]
    );
    assert_eq!(
        locations(&success(dir.path(), &["-8", r"^stddef\.h$"])),
        vec![("<global>".into(), 2)]
    );
    assert_eq!(
        locations(&success(dir.path(), &["-7", r"\.[ch]$"])).len(),
        2
    );
    assert_eq!(
        locations(&success(dir.path(), &["-6", "^#define (SCALE|LIMIT)"])).len(),
        2
    );
    fs::write(dir.path().join("navigation.c"), "// live needle\n").unwrap();
    assert_eq!(
        locations(&success(dir.path(), &["-4", "needle"])),
        vec![("<unknown>".into(), 1)]
    );
    let stale = run(dir.path(), &["-1", "leaf"]);
    assert!(!stale.status.success());
    assert!(String::from_utf8_lossy(&stale.stderr).contains("source changed since indexing"));
}

#[test]
fn mixed_language_file_lists_preserve_python_results_and_failed_build_safety() {
    let dir = project();
    fs::write(
        dir.path().join("sample.py"),
        "def leaf(value):\n    return value\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("rscope.files"),
        "navigation.c\nc_nodes.h\nsample.py\n",
    )
    .unwrap();
    success(dir.path(), &["-b"]);
    assert_eq!(locations(&success(dir.path(), &["-1", "leaf"])).len(), 2);
    let before = fs::read(dir.path().join("cscope.out")).unwrap();
    fs::write(dir.path().join("broken.c"), "int broken( {\n").unwrap();
    let broken = run(dir.path(), &["-b", "sample.py", "broken.c"]);
    assert!(!broken.status.success());
    assert!(String::from_utf8_lossy(&broken.stderr).contains("C syntax errors near line"));
    assert_eq!(fs::read(dir.path().join("cscope.out")).unwrap(), before);
    assert_eq!(locations(&success(dir.path(), &["-1", "leaf"])).len(), 2);
    fs::write(dir.path().join("wrong.cpp"), "int main() {}\n").unwrap();
    assert!(!run(dir.path(), &["-b", "wrong.cpp"]).status.success());
}

#[test]
fn declarations_complex_calls_and_include_macros_do_not_claim_semantic_resolution() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("complex.c"),
        concat!(
            "typedef int Fn(int);\n",
            "extern int declared(int), *pointer_declared(void);\n",
            "struct Forward;\n",
            "#define HEADER \"generated.h\"\n",
            "#include HEADER\n",
            "int run(int (**table)(int), int (*cb)(int)) {\n",
            "    (table[0])(1);\n",
            "    ((cb))(1);\n",
            "    return cb(2);\n",
            "}\n",
        ),
    )
    .unwrap();
    success(dir.path(), &["-b", "complex.c"]);
    assert_eq!(
        locations(&success(dir.path(), &["-1", "Fn"])),
        vec![("<global>".into(), 1)]
    );
    for name in ["declared", "pointer_declared", "Forward", "cb"] {
        assert!(success(dir.path(), &["-1", name]).is_empty(), "{name}");
    }
    assert_eq!(
        locations(&success(dir.path(), &["-2", "run"])),
        vec![("cb".into(), 8), ("cb".into(), 9)]
    );
    assert_eq!(
        locations(&success(dir.path(), &["-8", "^HEADER$"])),
        vec![("<global>".into(), 5)]
    );
    assert!(success(dir.path(), &["-8", "generated.h"]).is_empty());
}

#[test]
fn existing_lean_python_indexes_remain_readable_without_rebuilding() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("old.py"), "def leaf():\n    pass\n").unwrap();
    success(dir.path(), &["-b", "old.py"]);
    let connection = rusqlite::Connection::open(dir.path().join("cscope.out")).unwrap();
    connection.execute("UPDATE metadata SET value='schema3-python0.25.0-ts0.25.10-extract2' WHERE key='index_version'", []).unwrap();
    assert_eq!(
        locations(&success(dir.path(), &["-1", "leaf"])),
        vec![("<global>".into(), 1)]
    );
    fs::write(dir.path().join("new.c"), "int added(void) { return 1; }\n").unwrap();
    success(dir.path(), &["-b", "old.py", "new.c"]);
    assert_eq!(locations(&success(dir.path(), &["-1", "added"])).len(), 1);
    assert_eq!(locations(&success(dir.path(), &["-1", "leaf"])).len(), 1);
}

#[test]
fn partial_c_build_is_opt_in_persistently_labeled_and_keeps_diagnostics() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("partial.c"),
        concat!(
            "int before(void) { return 1; }\n",
            "void broken(void *p) { obj = container_of(p, struct item, field); }\n",
            "int after(void) { return 2; }\n",
        ),
    )
    .unwrap();
    assert!(!run(dir.path(), &["-b", "partial.c"]).status.success());
    assert!(!dir.path().join("cscope.out").exists());
    let output = run(dir.path(), &["-b", "--allow-parse-errors", "partial.c"]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("PARTIAL INDEX"));
    let connection = rusqlite::Connection::open(dir.path().join("cscope.out")).unwrap();
    let mode: String = connection
        .query_row(
            "SELECT value FROM metadata WHERE key='parse_mode'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(mode, "best_effort");
    let version: String = connection
        .query_row(
            "SELECT value FROM metadata WHERE key='index_version'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(version.ends_with("-partial1"));
    let diagnostic: (i64, i64, String) = connection
        .query_row(
            "SELECT line,end_line,kind FROM parse_diagnostics",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(diagnostic, (2, 2, "ERROR".into()));
    let found = run(dir.path(), &["-1", "after"]);
    assert!(found.status.success());
    assert_eq!(
        locations(&String::from_utf8(found.stdout).unwrap()),
        vec![("<global>".into(), 3)]
    );
    assert!(String::from_utf8_lossy(&found.stderr).contains("PARTIAL INDEX"));
    let absent = run(dir.path(), &["-1", "absent"]);
    assert!(absent.status.success() && absent.stdout.is_empty());
    assert!(String::from_utf8_lossy(&absent.stderr).contains("PARTIAL INDEX"));
    let bytes = fs::read(dir.path().join("cscope.out")).unwrap();
    assert!(!run(dir.path(), &["-b", "partial.c"]).status.success());
    assert_eq!(bytes, fs::read(dir.path().join("cscope.out")).unwrap());
}

#[test]
fn recovery_flag_is_build_only_and_does_not_relax_python_or_io_errors() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("bad.py"), "def broken(:\n").unwrap();
    assert!(
        !run(dir.path(), &["-b", "--allow-parse-errors", "bad.py"])
            .status
            .success()
    );
    assert!(
        !run(dir.path(), &["-b", "--allow-parse-errors", "missing.c"])
            .status
            .success()
    );
    assert!(
        !run(dir.path(), &["--allow-parse-errors", "-1", "x"])
            .status
            .success()
    );
    assert!(!dir.path().join("cscope.out").exists());
    fs::write(dir.path().join("good.c"), "int good(void) { return 1; }\n").unwrap();
    success(dir.path(), &["-b", "--allow-parse-errors", "good.c"]);
    let output = run(dir.path(), &["-1", "good"]);
    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("PARTIAL INDEX"));
}
