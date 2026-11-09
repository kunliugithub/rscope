use rusqlite::Connection;
use std::{
    fs,
    path::Path,
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

fn run(dir: &Path, args: &[&str]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rscope"))
        .current_dir(dir)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let started = Instant::now();
    loop {
        if child.try_wait().unwrap().is_some() {
            return child.wait_with_output().unwrap();
        }
        if started.elapsed() > Duration::from_secs(15) {
            child.kill().unwrap();
            panic!("indexer did not terminate: {args:?}");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn success(dir: &Path, args: &[&str]) -> Output {
    let out = run(dir, args);
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

fn records(path: &Path) -> Vec<(String, String, String, Option<String>, i64, i64)> {
    let conn = Connection::open(path).unwrap();
    assert_eq!(
        conn.query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    conn.prepare("SELECT f.path,o.name,o.kind,o.caller,o.line,o.column_number FROM occurrences o JOIN files f ON f.id=o.file_id ORDER BY f.path,o.name,o.kind,o.caller,o.line,o.column_number")
        .unwrap().query_map([], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).unwrap().map(Result::unwrap).collect()
}

#[test]
fn mixed_language_records_and_all_queries_match_across_workers() {
    let dir = tempfile::tempdir().unwrap();
    for (name, source) in [
        ("sample.py", include_str!("fixtures/navigation.py")),
        ("sample.c", include_str!("fixtures/navigation.c")),
        ("sample.rs", include_str!("fixtures/navigation.rs")),
        ("Navigation.java", include_str!("fixtures/Navigation.java")),
    ] {
        fs::write(dir.path().join(name), source).unwrap();
    }
    fs::write(
        dir.path().join("inputs"),
        "sample.py sample.c sample.rs Navigation.java ./sample.py",
    )
    .unwrap();
    success(
        dir.path(),
        &["-b", "-i", "inputs", "-f", "serial.db", "--jobs", "1"],
    );
    let expected = records(&dir.path().join("serial.db"));
    for jobs in ["2", "4", "8"] {
        for capacity in ["0", "1"] {
            let out = success(
                dir.path(),
                &[
                    "-b",
                    "-i",
                    "inputs",
                    "-f",
                    "parallel.db",
                    "--jobs",
                    jobs,
                    "--queue-capacity",
                    capacity,
                    "--profile",
                ],
            );
            assert!(out.stdout.is_empty());
            assert!(String::from_utf8_lossy(&out.stderr).contains("rscope-profile"));
            assert_eq!(expected, records(&dir.path().join("parallel.db")));
            let conn = Connection::open(dir.path().join("parallel.db")).unwrap();
            assert_eq!(
                conn.query_row("SELECT COUNT(*) FROM files", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                4
            );
            for (flag, pattern) in [
                ("-0", "leaf"),
                ("-1", "leaf"),
                ("-2", "outer"),
                ("-3", "leaf"),
                ("-4", "leaf"),
                ("-6", "leaf|outer"),
                ("-7", "sample"),
                ("-8", ".*"),
                ("-9", "value"),
            ] {
                assert_eq!(
                    success(dir.path(), &["-dL", "-f", "serial.db", flag, pattern]).stdout,
                    success(dir.path(), &["-dL", "-f", "parallel.db", flag, pattern]).stdout
                );
            }
        }
    }
}

#[test]
fn parallel_errors_unblock_workers_and_preserve_database() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("good.py"), "def leaf():\n    return 1\n").unwrap();
    success(dir.path(), &["-b", "good.py"]);
    let original = fs::read(dir.path().join("cscope.out")).unwrap();
    for (name, content) in [
        ("bad.py", b"def broken(\n".as_slice()),
        ("utf8.py", b"\xff".as_slice()),
    ] {
        fs::write(dir.path().join(name), content).unwrap();
        let mut inputs = vec![name.to_owned()];
        for index in 0..24 {
            let path = format!("good{index}.py");
            fs::write(dir.path().join(&path), "def leaf():\n    return 1\n").unwrap();
            inputs.push(path);
        }
        fs::write(dir.path().join("inputs"), inputs.join("\n")).unwrap();
        for capacity in ["0", "1"] {
            let out = run(
                dir.path(),
                &[
                    "-b",
                    "-i",
                    "inputs",
                    "--jobs",
                    "8",
                    "--queue-capacity",
                    capacity,
                ],
            );
            assert!(!out.status.success());
            assert_eq!(original, fs::read(dir.path().join("cscope.out")).unwrap());
        }
    }
}

#[test]
fn partial_c_diagnostics_survive_parallel_indexing() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("bad.c"), "int broken( {\n").unwrap();
    fs::write(dir.path().join("good.c"), "int leaf(void) { return 1; }\n").unwrap();
    for (db, jobs) in [("serial.db", "1"), ("parallel.db", "4")] {
        success(
            dir.path(),
            &[
                "-b",
                "--allow-parse-errors",
                "--jobs",
                jobs,
                "-f",
                db,
                "bad.c",
                "good.c",
            ],
        );
    }
    assert_eq!(
        records(&dir.path().join("serial.db")),
        records(&dir.path().join("parallel.db"))
    );
    let diagnostics = |db: &str| {
        let conn = Connection::open(dir.path().join(db)).unwrap();
        conn.prepare("SELECT f.path,d.line,d.column_number,d.end_line,d.end_column,d.kind FROM parse_diagnostics d JOIN files f ON f.id=d.file_id ORDER BY f.path,d.line,d.column_number,d.end_line,d.end_column,d.kind")
            .unwrap().query_map([], |r| Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?,r.get::<_,i64>(2)?,r.get::<_,i64>(3)?,r.get::<_,i64>(4)?,r.get::<_,String>(5)?))).unwrap().map(Result::unwrap).collect::<Vec<_>>()
    };
    assert!(!diagnostics("serial.db").is_empty());
    assert_eq!(diagnostics("serial.db"), diagnostics("parallel.db"));
    let out = success(dir.path(), &["-dL", "-f", "parallel.db", "-1", "leaf"]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("PARTIAL"));
}

#[test]
fn worker_arguments_are_build_only_and_jobs_must_be_positive() {
    let dir = tempfile::tempdir().unwrap();
    for args in [
        vec!["-b", "--jobs", "0", "missing.py"],
        vec!["--jobs", "2", "-1", "leaf"],
        vec!["--queue-capacity", "1", "-1", "leaf"],
        vec!["--profile", "-1", "leaf"],
    ] {
        assert!(!run(dir.path(), &args).status.success());
    }
}
