//! Read-only grammar diagnosis. This does not call rscope extraction or build a DB.
use anyhow::{Context, Result};
use clap::Parser as Arguments;
use std::{
    fs,
    io::{BufWriter, Write},
    path::PathBuf,
    time::Instant,
};
use tree_sitter::{Node, Parser};

#[derive(Arguments)]
struct Args {
    #[arg(short = 'i', long)]
    file_list: Option<PathBuf>,
    #[arg(short = 'o', long)]
    output: PathBuf,
    /// Write full syntax trees; intended for small reproduction cases only
    #[arg(long)]
    dump_ast: bool,
    files: Vec<PathBuf>,
}

fn escaped(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}

fn excerpt(node: Node<'_>, source: &str) -> String {
    escaped(
        &source[node.byte_range()]
            .chars()
            .take(240)
            .collect::<String>(),
    )
}

fn run() -> Result<()> {
    let args = Args::parse();
    let mut files = args.files;
    if let Some(list) = args.file_list {
        files.extend(
            shell_words::split(&fs::read_to_string(list)?)?
                .into_iter()
                .map(PathBuf::from),
        );
    }
    fs::create_dir_all(&args.output)?;
    let mut summary = BufWriter::new(fs::File::create(args.output.join("files.tsv"))?);
    let mut errors = BufWriter::new(fs::File::create(args.output.join("errors.tsv"))?);
    writeln!(
        summary,
        "path\tstatus\tbytes\tlines\tparse_us\terror_regions"
    )?;
    writeln!(
        errors,
        "path\tstart_line\tstart_column\tend_line\tend_column\tkind\tparent_kind\ttext\tline_text\tparent_text"
    )?;
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_c::LANGUAGE.into())?;
    let started = Instant::now();
    let mut failed_files = 0;
    let mut read_failures = 0;
    let mut regions = 0;
    for (index, file) in files.iter().enumerate() {
        let path = escaped(&file.to_string_lossy());
        let source = match fs::read_to_string(file) {
            Ok(source) => source,
            Err(error) => {
                read_failures += 1;
                writeln!(
                    summary,
                    "{path}\tread_error:{}\t0\t0\t0\t0",
                    escaped(&error.to_string())
                )?;
                continue;
            }
        };
        let lines: Vec<_> = source.lines().collect();
        let parse_start = Instant::now();
        let tree = parser.parse(&source, None).context("C parsing cancelled")?;
        let parse_us = parse_start.elapsed().as_micros();
        let mut count = 0;
        let mut pending = vec![tree.root_node()];
        while let Some(node) = pending.pop() {
            if node.is_error() || node.is_missing() {
                let start = node.start_position();
                let end = node.end_position();
                let parent = node.parent().unwrap_or(node);
                let kind = if node.is_missing() {
                    format!("MISSING {}", node.kind())
                } else {
                    "ERROR".into()
                };
                writeln!(
                    errors,
                    "{path}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    start.row + 1,
                    start.column + 1,
                    end.row + 1,
                    end.column + 1,
                    escaped(&kind),
                    escaped(parent.kind()),
                    excerpt(node, &source),
                    escaped(lines.get(start.row).unwrap_or(&"")),
                    excerpt(parent, &source)
                )?;
                count += 1;
                // Count outermost error regions, not cascaded errors inside them.
                continue;
            }
            if node.has_error() {
                let mut cursor = node.walk();
                let children: Vec<_> = node.children(&mut cursor).collect();
                pending.extend(children.into_iter().rev());
            }
        }
        if tree.root_node().has_error() {
            failed_files += 1;
        }
        regions += count;
        let status = if tree.root_node().has_error() {
            "parse_error"
        } else {
            "clean"
        };
        writeln!(
            summary,
            "{path}\t{status}\t{}\t{}\t{parse_us}\t{count}",
            source.len(),
            lines.len()
        )?;
        if args.dump_ast {
            let name = file.file_name().unwrap_or_default().to_string_lossy();
            fs::write(
                args.output.join(format!("{index:04}-{name}.sexp")),
                tree.root_node().to_sexp(),
            )?;
        }
    }
    summary.flush()?;
    errors.flush()?;
    println!(
        "Scanned {} files: {} clean, {failed_files} with parse errors, {read_failures} read failures; {regions} outermost error regions; elapsed {:.3}s. No index built.",
        files.len(),
        files.len() - failed_files - read_failures,
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

fn main() -> Result<()> {
    run()
}
