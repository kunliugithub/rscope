mod c;
mod db;
mod indexer;
mod java;
mod language;
mod python;
mod rust;
mod source;

use anyhow::{Context, Result, bail};
use clap::{ArgGroup, Parser};
use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
};

#[derive(Parser, Debug)]
#[command(version, about, group(ArgGroup::new("query").args([
    "symbol", "definition", "callees", "callers", "text", "regex", "file", "imports", "assignments"
])))]
struct Args {
    /// Build a complete replacement index from the selected files
    #[arg(short = 'b')]
    build: bool,
    /// Experimental: retain recovered C results and skip parse-error subtrees
    #[arg(long, requires = "build")]
    allow_parse_errors: bool,
    /// Parser workers (1 uses the serial path)
    #[arg(long, requires = "build", value_parser = clap::value_parser!(u16).range(1..), default_value = "1")]
    jobs: u16,
    /// Maximum queued parsed files (default: number of workers; 0 is rendezvous)
    #[arg(long, requires = "build")]
    queue_capacity: Option<usize>,
    /// Print indexing phase timings to stderr (worker read/parse times are summed)
    #[arg(long, requires = "build")]
    profile: bool,
    /// Query the existing index (queries currently never update it)
    #[arg(short = 'd')]
    existing: bool,
    /// Single query with cscope-compatible line output
    #[arg(short = 'L')]
    lines: bool,
    /// SQLite index path; this is not a legacy cscope database
    #[arg(short = 'f', default_value = "cscope.out")]
    database: PathBuf,
    /// Read a shell-quoted file list (defaults to rscope.files during a build)
    #[arg(short = 'i')]
    file_list: Option<PathBuf>,
    /// Find exact identifier occurrences
    #[arg(short = '0', value_name = "NAME")]
    symbol: Option<String>,
    /// Find definitions (language-specific kinds)
    #[arg(short = '1', value_name = "NAME")]
    definition: Option<String>,
    /// Find call sites within functions with this name
    #[arg(short = '2', value_name = "NAME")]
    callees: Option<String>,
    /// Find call sites with this written target name
    #[arg(short = '3', value_name = "NAME")]
    callers: Option<String>,
    /// Find a literal substring in current contents of the indexed files
    #[arg(short = '4', value_name = "TEXT", allow_hyphen_values = true)]
    text: Option<String>,
    /// Find a Rust regex pattern in current contents of the indexed files
    #[arg(short = '6', value_name = "PATTERN", allow_hyphen_values = true)]
    regex: Option<String>,
    /// Find indexed file paths matching a Rust regex pattern
    #[arg(short = '7', value_name = "PATTERN", allow_hyphen_values = true)]
    file: Option<String>,
    /// Find written imports/includes matching a Rust regex
    #[arg(short = '8', value_name = "PATTERN", allow_hyphen_values = true)]
    imports: Option<String>,
    /// Find explicit assignments to this identifier, attribute, or container name
    #[arg(short = '9', value_name = "NAME")]
    assignments: Option<String>,
    /// Python, C, Rust, and Java files to index; directories are not scanned yet
    files: Vec<PathBuf>,
}

fn run(args: Args) -> Result<()> {
    let query = args
        .symbol
        .as_ref()
        .map(|s| (0, s))
        .or(args.definition.as_ref().map(|s| (1, s)))
        .or(args.callees.as_ref().map(|s| (2, s)))
        .or(args.callers.as_ref().map(|s| (3, s)))
        .or(args.text.as_ref().map(|s| (4, s)))
        .or(args.regex.as_ref().map(|s| (6, s)))
        .or(args.file.as_ref().map(|s| (7, s)))
        .or(args.imports.as_ref().map(|s| (8, s)))
        .or(args.assignments.as_ref().map(|s| (9, s)));
    if args.build {
        if query.is_some() || args.existing || args.lines {
            bail!("-b cannot be combined with query options");
        }
        let mut files = args.files;
        let list = args.file_list.or_else(|| {
            (files.is_empty() && PathBuf::from("rscope.files").is_file())
                .then(|| PathBuf::from("rscope.files"))
        });
        if let Some(list) = list {
            let text = fs::read_to_string(&list)
                .with_context(|| format!("reading file list {}", list.display()))?;
            files.extend(shell_words::split(&text)?.into_iter().map(PathBuf::from));
        }
        if files.is_empty() {
            bail!("select source files with -b FILE..., -b -i LIST, or a local rscope.files list");
        }
        let count = db::build(
            &args.database,
            files,
            db::BuildOptions {
                allow_parse_errors: args.allow_parse_errors,
                jobs: usize::from(args.jobs),
                queue_capacity: args.queue_capacity.unwrap_or(usize::from(args.jobs)),
                profile: args.profile,
            },
        )?;
        eprintln!(
            "Indexed {count} source files into {}",
            args.database.display()
        );
    } else {
        if !args.files.is_empty() || args.file_list.is_some() {
            bail!("file selection requires -b");
        }
        let (mode, name) =
            query.context("choose -b or a query (-0 through -4, -6 through -9); see --help")?;
        let stdout = io::stdout();
        let mut output = io::BufWriter::new(stdout.lock());
        db::query(&args.database, mode, name, |row| {
            writeln!(
                output,
                "{} {} {} {}",
                row.path, row.context, row.line, row.snippet
            )?;
            Ok(())
        })?;
        output.flush()?;
    }
    Ok(())
}

fn main() {
    if let Err(error) = run(Args::parse()) {
        eprintln!("rscope: {error:#}");
        std::process::exit(1);
    }
}
