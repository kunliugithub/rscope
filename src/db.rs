use crate::{
    indexer::{self, Prepared},
    language::Language,
    source::{Source, Stamp},
};
use anyhow::{Context, Result, bail};
use regex::Regex;
use rusqlite::{Connection, OpenFlags, params};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tempfile::NamedTempFile;

// Change when schema, grammar, or extraction semantics require rebuilding.
const OLD_INDEX_VERSION: &str = "schema3-ts0.25.10-python0.25.0-c0.24.2-extract3";
const INDEX_VERSION: &str = "schema3-ts0.25.10-python0.25.0-c0.24.2-rust0.24.2-java0.23.5-extract1";
// Older executables must reject experimental partial indexes rather than
// silently presenting them as complete.
const OLD_PARTIAL_INDEX_VERSION: &str = "schema3-ts0.25.10-python0.25.0-c0.24.2-partial1";
const PARTIAL_INDEX_VERSION: &str =
    "schema3-ts0.25.10-python0.25.0-c0.24.2-rust0.24.2-java0.23.5-partial1";
// Adding C does not change the schema or extraction semantics of lean Python
// indexes. Older snapshot formats remain incompatible.
const PYTHON_INDEX_VERSION: &str = "schema3-python0.25.0-ts0.25.10-extract2";
const APPLICATION_ID: i64 = 0x52534350;

pub struct Row {
    pub path: String,
    pub context: String,
    pub line: i64,
    pub snippet: String,
}

fn check_existing(database: &Path) -> Result<()> {
    if database.exists() {
        let connection = Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let id: i64 = connection.pragma_query_value(None, "application_id", |r| r.get(0))?;
        if id != APPLICATION_ID {
            bail!("refusing to replace a database not identified as an rscope index");
        }
        let journal: String = connection.pragma_query_value(None, "journal_mode", |r| r.get(0))?;
        if journal == "wal" {
            bail!(
                "cannot replace a WAL-mode index; close other connections and restore journal_mode=DELETE first"
            );
        }
    }
    Ok(())
}

pub struct BuildOptions {
    pub allow_parse_errors: bool,
    pub jobs: usize,
    pub queue_capacity: usize,
    pub profile: bool,
}

pub fn build(database: &Path, files: Vec<PathBuf>, options: BuildOptions) -> Result<usize> {
    let started = Instant::now();
    let database_path = if database.exists() {
        fs::canonicalize(database)?
    } else {
        let parent = database
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::canonicalize(parent)?.join(database.file_name().context("invalid index path")?)
    };
    check_existing(&database_path)?;
    let mut selected = Vec::new();
    let mut seen = HashSet::new();
    for file in files {
        let file =
            fs::canonicalize(&file).with_context(|| format!("opening {}", file.display()))?;
        if file == database_path {
            bail!("index path must not be a source file");
        }
        Language::for_path(&file)?;
        let path = file.to_str().context("source paths must be UTF-8")?;
        if path.chars().any(char::is_whitespace) {
            bail!("source path contains whitespace, unsupported by current xcscope output: {path}");
        }
        if seen.insert(file.clone()) {
            selected.push(file);
        }
    }
    let selection_time = started.elapsed();
    let setup_started = Instant::now();
    // Build alongside the destination, then atomically replace it. A failed
    // file leaves the old index intact, and obsolete text pages are discarded.
    let temporary = NamedTempFile::new_in(
        database_path
            .parent()
            .context("index needs a parent directory")?,
    )?;
    let mut connection = Connection::open(temporary.path())?;
    connection.execute_batch(
        "PRAGMA cache_size=-8192; PRAGMA temp_store=FILE; PRAGMA foreign_keys=ON;",
    )?;
    let transaction = connection.transaction()?;
    transaction.pragma_update(None, "application_id", APPLICATION_ID)?;
    transaction.execute_batch(
        "
        CREATE TABLE metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL);
        CREATE TABLE files (
            id INTEGER PRIMARY KEY, path TEXT NOT NULL UNIQUE,
            byte_len INTEGER NOT NULL, modified_ns TEXT NOT NULL
        );
        CREATE TABLE occurrences (
            file_id INTEGER NOT NULL REFERENCES files(id),
            name TEXT NOT NULL, kind TEXT NOT NULL, caller TEXT,
            line INTEGER NOT NULL, column_number INTEGER NOT NULL
        );
        CREATE TABLE parse_diagnostics (
            file_id INTEGER NOT NULL REFERENCES files(id),
            line INTEGER NOT NULL, column_number INTEGER NOT NULL,
            end_line INTEGER NOT NULL, end_column INTEGER NOT NULL, kind TEXT NOT NULL
        );
    ",
    )?;
    transaction.execute(
        "INSERT INTO metadata VALUES ('index_version', ?1)",
        [INDEX_VERSION],
    )?;
    let setup_time = setup_started.elapsed();
    let pipeline_started = Instant::now();
    let mut read_time = Duration::ZERO;
    let mut parse_time = Duration::ZERO;
    let mut insert_time = Duration::ZERO;
    let mut count = 0;
    let mut partial_files = 0;
    let mut error_regions = 0;
    {
        let mut insert_file = transaction
            .prepare("INSERT INTO files(path,byte_len,modified_ns) VALUES (?1,?2,?3)")?;
        let mut insert =
            transaction.prepare("INSERT INTO occurrences VALUES (?1,?2,?3,?4,?5,?6)")?;
        let mut insert_diagnostic =
            transaction.prepare("INSERT INTO parse_diagnostics VALUES (?1,?2,?3,?4,?5,?6)")?;
        indexer::run(
            &selected,
            options.jobs,
            options.queue_capacity,
            options.allow_parse_errors,
            |prepared| {
                let Prepared {
                    path: file,
                    stamp,
                    parsed,
                    read_time: read,
                    parse_time: parse,
                } = prepared;
                read_time += read;
                parse_time += parse;
                let insert_started = Instant::now();
                let path = file.to_str().context("source paths must be UTF-8")?;
                insert_file.execute(params![path, stamp.byte_len, stamp.modified_ns])?;
                let file_id = transaction.last_insert_rowid();
                if !parsed.diagnostics.is_empty() {
                    partial_files += 1;
                    error_regions += parsed.diagnostics.len();
                    eprintln!(
                        "rscope: partial parse: {path}: {} error regions; locations saved in parse_diagnostics",
                        parsed.diagnostics.len()
                    );
                }
                for diagnostic in parsed.diagnostics {
                    insert_diagnostic.execute(params![
                        file_id,
                        diagnostic.line as i64,
                        diagnostic.column as i64,
                        diagnostic.end_line as i64,
                        diagnostic.end_column as i64,
                        diagnostic.kind
                    ])?;
                }
                for record in parsed.records {
                    insert.execute(params![
                        file_id,
                        record.name,
                        record.kind,
                        record.caller,
                        record.line as i64,
                        record.column as i64
                    ])?;
                }
                // source/records are released after each file, not after the tree.
                count += 1;
                insert_time += insert_started.elapsed();
                Ok(())
            },
        )?;
    }
    let pipeline_time = pipeline_started.elapsed();
    let index_started = Instant::now();
    for (key, value) in [
        (
            "parse_mode",
            if options.allow_parse_errors {
                "best_effort".to_owned()
            } else {
                "strict".to_owned()
            },
        ),
        ("partial_files", partial_files.to_string()),
        ("error_regions", error_regions.to_string()),
    ] {
        transaction.execute("INSERT INTO metadata VALUES (?1,?2)", params![key, value])?;
    }
    if partial_files > 0 {
        transaction.execute(
            "UPDATE metadata SET value=?1 WHERE key='index_version'",
            [PARTIAL_INDEX_VERSION],
        )?;
    }
    transaction.execute_batch(
        "
        CREATE INDEX occurrences_name ON occurrences(name,kind);
        CREATE INDEX occurrences_caller ON occurrences(caller,kind);
        CREATE INDEX occurrences_kind ON occurrences(kind);
    ",
    )?;
    let index_time = index_started.elapsed();
    let publish_started = Instant::now();
    transaction.commit()?;
    connection.close().map_err(|(_, error)| error)?;
    temporary.as_file().sync_all()?;
    check_existing(&database_path)?;
    temporary
        .persist(&database_path)
        .with_context(|| format!("replacing index {}", database_path.display()))?;
    if partial_files > 0 {
        eprintln!(
            "rscope: PARTIAL INDEX: {partial_files} files with {error_regions} parse-error regions; definitions and relationships may be missing or misidentified"
        );
    }
    if options.profile {
        eprintln!(
            "rscope-profile jobs={} queue_capacity={} files={} selection_s={:.6} setup_s={:.6} pipeline_s={:.6} read_sum_s={:.6} parse_sum_s={:.6} insert_s={:.6} indexes_s={:.6} publish_s={:.6} total_s={:.6}",
            options.jobs,
            options.queue_capacity,
            count,
            selection_time.as_secs_f64(),
            setup_time.as_secs_f64(),
            pipeline_time.as_secs_f64(),
            read_time.as_secs_f64(),
            parse_time.as_secs_f64(),
            insert_time.as_secs_f64(),
            index_time.as_secs_f64(),
            publish_started.elapsed().as_secs_f64(),
            started.elapsed().as_secs_f64()
        );
    }
    Ok(count)
}

pub fn query(
    database: &Path,
    mode: u8,
    name: &str,
    mut emit: impl FnMut(Row) -> Result<()>,
) -> Result<()> {
    let connection = Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| {
            format!(
                "opening index {}; build it with -b first",
                database.display()
            )
        })?;
    let id: i64 = connection.pragma_query_value(None, "application_id", |r| r.get(0))?;
    if id != APPLICATION_ID {
        bail!("not an rscope index; choose a separate path with -f");
    }
    let version: String = connection
        .query_row(
            "SELECT value FROM metadata WHERE key='index_version'",
            [],
            |r| r.get(0),
        )
        .context("unrecognized index; rebuild with -b")?;
    if version != OLD_INDEX_VERSION
        && version != OLD_PARTIAL_INDEX_VERSION
        && version != INDEX_VERSION
        && version != PYTHON_INDEX_VERSION
        && version != PARTIAL_INDEX_VERSION
    {
        bail!("incompatible index version; rebuild with -b");
    }
    if version == PARTIAL_INDEX_VERSION || version == OLD_PARTIAL_INDEX_VERSION {
        let affected: String = connection.query_row(
            "SELECT value FROM metadata WHERE key='partial_files'",
            [],
            |r| r.get(0),
        )?;
        eprintln!(
            "rscope: PARTIAL INDEX ({affected} files with parse errors): results may be missing or misidentified; see parse_diagnostics"
        );
    }
    // Keep one database generation across the query, including its manifest.
    connection.execute_batch("PRAGMA cache_size=-8192; PRAGMA temp_store=FILE; BEGIN;")?;
    if matches!(mode, 4 | 6 | 7) {
        return search_files(&connection, mode, name, &mut emit);
    }
    search_locations(&connection, mode, name, &mut emit)
}

fn search_files(
    connection: &Connection,
    mode: u8,
    pattern: &str,
    emit: &mut impl FnMut(Row) -> Result<()>,
) -> Result<()> {
    let pattern = if mode == 4 {
        regex::escape(pattern)
    } else {
        pattern.to_owned()
    };
    let matcher = Regex::new(&pattern).context("invalid search regex")?;
    let mut statement = connection.prepare("SELECT path FROM files ORDER BY path")?;
    let mut files = statement.query([])?;
    while let Some(file) = files.next()? {
        let path: String = file.get(0)?;
        if mode == 7 && !matcher.is_match(&path) {
            continue;
        }
        let mut source = Source::open(&path, None)?;
        if mode == 7 {
            emit(Row {
                path,
                context: "<unknown>".into(),
                line: 1,
                snippet: String::new(),
            })?;
        } else {
            while source.next_line()? {
                if matcher.is_match(&source.text) {
                    emit(Row {
                        path: path.clone(),
                        context: "<unknown>".into(),
                        line: source.line,
                        snippet: source.text.trim().to_owned(),
                    })?;
                }
            }
        }
        source.verify_unchanged()?;
    }
    Ok(())
}

fn search_locations(
    connection: &Connection,
    mode: u8,
    name: &str,
    emit: &mut impl FnMut(Row) -> Result<()>,
) -> Result<()> {
    let import_pattern = if mode == 8 {
        Some(Regex::new(name).context("invalid search regex")?)
    } else {
        None
    };
    let condition = match mode {
        0 => "o.name=?1 AND kind IN ('identifier','definition')",
        1 => "o.name=?1 AND kind='definition'",
        2 => "caller=?1 AND kind='call'",
        3 => "o.name=?1 AND kind='call'",
        8 => "kind='import'",
        9 => "o.name=?1 AND kind='assignment'",
        _ => bail!("unsupported query mode"),
    };
    let context = if mode == 2 {
        "o.name"
    } else {
        "COALESCE(caller,'<global>')"
    };
    let match_name = if mode == 8 { "o.name" } else { "''" };
    let sql = format!("SELECT DISTINCT f.id,f.path,line,{context} AS result_context,byte_len,modified_ns,{match_name} AS match_name
        FROM occurrences o JOIN files f ON f.id=o.file_id WHERE {condition}
        ORDER BY f.path,line,result_context,match_name");
    let mut statement = connection.prepare(&sql)?;
    let mut rows = if mode == 8 {
        statement.query([])?
    } else {
        statement.query([name])?
    };
    let mut current: Option<(i64, Source)> = None;
    let mut previous = None;
    while let Some(row) = rows.next()? {
        if let Some(pattern) = &import_pattern {
            let module: String = row.get(6)?;
            if !pattern.is_match(&module) {
                continue;
            }
        }
        let id: i64 = row.get(0)?;
        let path: String = row.get(1)?;
        let line: i64 = row.get(2)?;
        let context: String = row.get(3)?;
        let key = (id, line, context.clone());
        if previous.as_ref() == Some(&key) {
            continue;
        }
        previous = Some(key);
        if current.as_ref().is_none_or(|(file_id, _)| *file_id != id) {
            if let Some((_, source)) = current.take() {
                source.verify_unchanged()?;
            }
            let stamp = Stamp {
                byte_len: row.get(4)?,
                modified_ns: row.get(5)?,
            };
            current = Some((id, Source::open(&path, Some(&stamp))?));
        }
        let snippet = current
            .as_mut()
            .expect("source opened above")
            .1
            .snippet_at(line)?;
        emit(Row {
            path,
            context,
            line,
            snippet,
        })?;
    }
    if let Some((_, source)) = current {
        source.verify_unchanged()?;
    }
    Ok(())
}
