use crate::{c, java, python, rust};
use anyhow::{Context, Result, bail};
use std::path::Path;

#[derive(Debug)]
pub struct Record {
    pub name: String,
    pub kind: &'static str,
    pub caller: Option<String>,
    pub line: usize,
    pub column: usize,
}

pub struct Diagnostic {
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
    pub kind: String,
}

pub struct ParsedFile {
    pub records: Vec<Record>,
    pub diagnostics: Vec<Diagnostic>,
}

pub enum Language {
    Python,
    C,
    Rust,
    Java,
}

impl Language {
    pub fn for_path(path: &Path) -> Result<Self> {
        match path.extension().and_then(|ext| ext.to_str()) {
            Some("py") => Ok(Self::Python),
            Some("c" | "h") => Ok(Self::C),
            Some("rs") => Ok(Self::Rust),
            Some("java") => Ok(Self::Java),
            _ => bail!(
                "unsupported input {}; supported extensions are .py, .c, .h, .rs, .java",
                path.display()
            ),
        }
    }

    pub fn parse(&self, source: &str, allow_parse_errors: bool) -> Result<ParsedFile> {
        match self {
            Self::Python => Ok(ParsedFile {
                records: python::parse(source)?,
                diagnostics: Vec::new(),
            }),
            Self::C => c::parse(source, allow_parse_errors),
            Self::Rust => Ok(ParsedFile {
                records: rust::parse(source)?,
                diagnostics: Vec::new(),
            }),
            Self::Java => Ok(ParsedFile {
                records: java::parse(source)?,
                diagnostics: Vec::new(),
            }),
        }
    }
}

/// New languages use strict syntax handling. Recovery remains explicitly C-only.
pub fn parse_tree(
    source: &str,
    grammar: tree_sitter::Language,
    name: &str,
) -> Result<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&grammar)?;
    let tree = parser
        .parse(source, None)
        .with_context(|| format!("{name} parsing cancelled"))?;
    let mut pending = vec![tree.root_node()];
    while let Some(node) = pending.pop() {
        if node.is_error() || node.is_missing() {
            let point = node.start_position();
            bail!(
                "{name} syntax errors near line {}, column {}; index was not replaced",
                point.row + 1,
                point.column + 1
            );
        }
        if node.has_error() {
            let mut cursor = node.walk();
            let children: Vec<_> = node.children(&mut cursor).collect();
            pending.extend(children.into_iter().rev());
        }
    }
    Ok(tree)
}

impl Record {
    pub fn at(
        node: tree_sitter::Node<'_>,
        source: &str,
        kind: &'static str,
        caller: &Option<String>,
    ) -> Self {
        let position = node.start_position();
        Self {
            name: source[node.byte_range()].to_owned(),
            kind,
            caller: caller.clone(),
            line: position.row + 1,
            column: position.column + 1,
        }
    }
}
