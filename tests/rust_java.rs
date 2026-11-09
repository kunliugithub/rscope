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
            (fields[1].into(), fields[2].parse().unwrap())
        })
        .collect()
}
fn check(dir: &Path, mode: &str, name: &str, expected: &[(&str, usize)]) {
    assert_eq!(
        locations(&success(dir, &[mode, name])),
        expected
            .iter()
            .map(|(s, n)| (s.to_string(), *n))
            .collect::<Vec<_>>(),
        "{mode} {name}"
    );
}
fn project(name: &str, source: &str) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(name), source).unwrap();
    success(dir.path(), &["-b", name]);
    dir
}
#[test]
fn rust_definitions_calls_scopes_and_opaque_macros() {
    let dir = project("navigation.rs", include_str!("fixtures/navigation.rs"));
    let d = dir.path();
    for (name, line, scope) in [
        ("Counter", 4, "<global>"),
        ("value", 4, "<global>"),
        ("Visit", 5, "<global>"),
        ("visit", 5, "<global>"),
        ("LIMIT", 6, "<global>"),
        ("helper", 7, "<global>"),
        ("run", 9, "<global>"),
        ("left", 10, "run"),
        ("right", 10, "run"),
        ("renamed", 19, "run"),
        ("wrap", 3, "<global>"),
    ] {
        check(d, "-1", name, &[(scope, line)]);
    }
    check(
        d,
        "-2",
        "run",
        &[
            ("helper", 10),
            ("helper", 14),
            ("wrap", 16),
            ("helper", 17),
            ("visit", 18),
        ],
    );
    check(
        d,
        "-3",
        "helper",
        &[("run", 10), ("run", 14), ("<closure>", 15), ("run", 17)],
    );
    check(d, "-0", "hidden", &[]);
    check(d, "-3", "hidden", &[]);
    check(d, "-0", "phantom", &[]);
    check(d, "-4", "phantom", &[("<unknown>", 22), ("<unknown>", 23)]);
}
#[test]
fn rust_imports_and_written_targets() {
    let dir = project("navigation.rs", include_str!("fixtures/navigation.rs"));
    let d = dir.path();
    for path in [
        "std::fmt",
        "std::fmt::Debug",
        "std::io::*",
        "std::collections::HashMap",
    ] {
        check(
            d,
            "-8",
            &format!("^{}$", regex::escape(path)),
            &[("<global>", 1)],
        );
    }
    check(d, "-8", "^sibling$", &[("<global>", 2)]);
    check(d, "-8", "^D$", &[]);
    check(d, "-9", "left", &[("run", 10), ("run", 12)]);
    check(d, "-9", "value", &[("run", 11)]);
    check(d, "-9", "values", &[("run", 13), ("run", 14)]);
    check(d, "-9", "renamed", &[("run", 19)]);
    check(d, "-9", "right", &[("run", 10)]);
    check(d, "-9", "helper", &[]);
}
#[test]
fn java_definitions_constructors_calls_and_scope_boundaries() {
    let dir = project("Navigation.java", include_str!("fixtures/Navigation.java"));
    let d = dir.path();
    for (name, line, scope) in [
        ("Visit", 3, "<global>"),
        ("Pair", 4, "<global>"),
        ("first", 4, "<global>"),
        ("helper", 9, "<global>"),
        ("run", 11, "<global>"),
        ("left", 12, "run"),
        ("right", 12, "run"),
        ("value", 6, "<global>"),
        ("nested", 20, "<global>"),
    ] {
        check(d, "-1", name, &[(scope, line)]);
    }
    check(
        d,
        "-1",
        "Navigation",
        &[("<global>", 5), ("<global>", 7), ("<global>", 8)],
    );
    check(d, "-1", "visit", &[("<global>", 3), ("<global>", 10)]);
    check(
        d,
        "-2",
        "run",
        &[
            ("helper", 12),
            ("helper", 16),
            ("Navigation", 18),
            ("visit", 19),
            ("Object", 20),
        ],
    );
    check(
        d,
        "-3",
        "helper",
        &[("run", 12), ("run", 16), ("<lambda>", 17), ("nested", 20)],
    );
    check(d, "-2", "Navigation", &[("this", 7)]);
    check(d, "-0", "phantom", &[]);
    check(d, "-4", "phantom", &[("<unknown>", 22), ("<unknown>", 23)]);
}
#[test]
fn java_imports_and_written_targets() {
    let dir = project("Navigation.java", include_str!("fixtures/Navigation.java"));
    let d = dir.path();
    check(d, "-8", r"^java\.util\.List$", &[("<global>", 1)]);
    check(
        d,
        "-8",
        r"^java\.util\.Collections\.\*$",
        &[("<global>", 2)],
    );
    check(
        d,
        "-9",
        "value",
        &[("<global>", 6), ("Navigation", 8), ("run", 13)],
    );
    check(d, "-9", "left", &[("run", 12), ("run", 14)]);
    check(d, "-9", "values", &[("run", 15), ("run", 16)]);
    check(d, "-9", "right", &[]);
    check(d, "-9", "helper", &[]);
}
#[test]
fn four_languages_share_manifest_and_new_syntax_errors_preserve_index() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    for (name, source) in [
        ("a.py", "def shared(): pass\n"),
        ("a.c", "void shared(void) {}\n"),
        ("a.rs", "fn shared() {}\n"),
        ("A.java", "class A { void shared() {} }\n"),
    ] {
        fs::write(d.join(name), source).unwrap();
    }
    fs::write(d.join("rscope.files"), "a.py\na.c\na.rs\nA.java\n").unwrap();
    success(d, &["-b"]);
    assert_eq!(locations(&success(d, &["-1", "shared"])).len(), 4);
    let before = fs::read(d.join("cscope.out")).unwrap();
    for (name, source, language) in [
        ("bad.rs", "fn broken( {", "Rust"),
        ("Bad.java", "class Bad { void broken( {", "Java"),
    ] {
        fs::write(d.join(name), source).unwrap();
        for args in [vec!["-b", name], vec!["-b", "--allow-parse-errors", name]] {
            let output = run(d, &args);
            assert!(!output.status.success());
            assert!(
                String::from_utf8_lossy(&output.stderr)
                    .contains(&format!("{language} syntax errors near line"))
            );
            assert_eq!(fs::read(d.join("cscope.out")).unwrap(), before);
        }
    }
}

#[test]
fn additional_rust_items_patterns_and_reference_targets() {
    let dir = project(
        "items.rs",
        concat!(
            "enum State { Ready, Done(i32) }\n",
            "union Bits { integer: u32, float: f32 }\n",
            "type Count = u32;\n",
            "trait Items { type Item; const SIZE: usize; fn make(); }\n",
            "static mut TOTAL: i32 = 0;\n",
            "extern crate core as renamed;\n",
            "fn run(mut ptr: &mut i32) {\n",
            "    let [first, second] = [1, 2];\n",
            "    *ptr += first;\n",
            "    let local;\n",
            "    macro_rules! call { () => { hidden() }; }\n",
            "    call!();\n",
            "}\n",
        ),
    );
    let d = dir.path();
    for (name, line) in [
        ("State", 1),
        ("Ready", 1),
        ("Done", 1),
        ("Bits", 2),
        ("integer", 2),
        ("Count", 3),
        ("Items", 4),
        ("Item", 4),
        ("SIZE", 4),
        ("make", 4),
        ("TOTAL", 5),
    ] {
        check(d, "-1", name, &[("<global>", line)]);
    }
    check(d, "-1", "first", &[("run", 8)]);
    check(d, "-1", "second", &[("run", 8)]);
    check(d, "-9", "ptr", &[("run", 9)]);
    check(d, "-1", "local", &[("run", 10)]);
    check(d, "-9", "local", &[]);
    check(d, "-0", "call", &[("run", 11), ("run", 12)]);
    check(d, "-3", "hidden", &[]);
    check(d, "-8", "^core$", &[("<global>", 6)]);
    check(d, "-8", "^renamed$", &[]);
}
#[test]
fn additional_java_types_initializers_generic_constructors_and_references() {
    let dir = project(
        "Items.java",
        concat!(
            "@interface Flag { String text(); }\n",
            "enum State { READY, DONE; }\n",
            "record Pair(int first) { Pair { helper(); } }\n",
            "class Items {\n",
            "    static { helper(); }\n",
            "    void run() {\n",
            "        Object item = new Outer.Inner<String>();\n",
            "        Runnable callback = Items::helper;\n",
            "    }\n",
            "}\n",
        ),
    );
    let d = dir.path();
    for (name, line) in [
        ("Flag", 1),
        ("text", 1),
        ("State", 2),
        ("READY", 2),
        ("DONE", 2),
        ("first", 3),
        ("Items", 4),
    ] {
        check(d, "-1", name, &[("<global>", line)]);
    }
    check(d, "-1", "Pair", &[("<global>", 3)]);
    check(d, "-3", "helper", &[("Pair", 3), ("<initializer>", 5)]);
    check(d, "-2", "run", &[("Inner", 7)]);
    check(
        d,
        "-0",
        "helper",
        &[("Pair", 3), ("<initializer>", 5), ("run", 8)],
    );
}
#[test]
fn prior_c_indexes_remain_readable_and_partial_warnings_survive() {
    let dir = project("old.c", "int old(void) { return 1; }\n");
    let d = dir.path();
    let conn = rusqlite::Connection::open(d.join("cscope.out")).unwrap();
    conn.execute("UPDATE metadata SET value='schema3-ts0.25.10-python0.25.0-c0.24.2-extract3' WHERE key='index_version'", []).unwrap();
    check(d, "-1", "old", &[("<global>", 1)]);
    fs::write(
        d.join("bad.c"),
        "void broken(void *p) { obj = container_of(p, struct item, field); }\nint after(void) {}\n",
    )
    .unwrap();
    success(d, &["-b", "--allow-parse-errors", "bad.c"]);
    let conn = rusqlite::Connection::open(d.join("cscope.out")).unwrap();
    conn.execute("UPDATE metadata SET value='schema3-ts0.25.10-python0.25.0-c0.24.2-partial1' WHERE key='index_version'", []).unwrap();
    let output = run(d, &["-1", "after"]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("PARTIAL INDEX"));
    assert_eq!(
        locations(&String::from_utf8(output.stdout).unwrap()),
        vec![("<global>".into(), 2)]
    );
}
