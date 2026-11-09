use crate::language::{Diagnostic, ParsedFile, Record};
use anyhow::{Context, Result, bail};
use tree_sitter::{Node, Parser};

fn text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}

/// Follow the declaration's own spine, excluding parameter lists, array sizes,
/// and attributes. Handles functions returning pointers and function pointers.
fn declarator_name(mut node: Node<'_>) -> Option<Node<'_>> {
    loop {
        match node.kind() {
            "identifier" | "field_identifier" | "type_identifier" => return Some(node),
            "parenthesized_declarator" | "attributed_declarator" => {
                let mut cursor = node.walk();
                node = node
                    .named_children(&mut cursor)
                    .find(|n| !matches!(n.kind(), "attribute_declaration" | "ms_call_modifier"))?;
            }
            _ => node = node.child_by_field_name("declarator")?,
        }
    }
}

fn function_declaration(name: Node<'_>) -> bool {
    let mut parent = name.parent();
    while let Some(node) = parent {
        match node.kind() {
            "parenthesized_declarator" | "attributed_declarator" => parent = node.parent(),
            "function_declarator" => return true,
            _ => return false,
        }
    }
    false
}

fn written_target(mut node: Node<'_>, allow_subscript: bool) -> Option<Node<'_>> {
    loop {
        match node.kind() {
            "identifier" | "field_identifier" => return Some(node),
            "field_expression" => return node.child_by_field_name("field"),
            "pointer_expression" => node = node.child_by_field_name("argument")?,
            "subscript_expression" if allow_subscript => {
                node = node.child_by_field_name("argument")?
            }
            "parenthesized_expression" => {
                let mut cursor = node.walk();
                node = node.named_children(&mut cursor).next()?;
            }
            _ => return None,
        }
    }
}

pub fn parse(source: &str, allow_parse_errors: bool) -> Result<ParsedFile> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_c::LANGUAGE.into())?;
    let tree = parser.parse(source, None).context("C parsing cancelled")?;
    let mut diagnostics = Vec::new();
    if tree.root_node().has_error() {
        let mut pending = vec![tree.root_node()];
        while let Some(node) = pending.pop() {
            if node.is_error() || node.is_missing() {
                let point = node.start_position();
                if !allow_parse_errors {
                    bail!(
                        "C syntax errors near line {}, column {}; unsupported macros/extensions may need grammar work; index was not replaced",
                        point.row + 1,
                        point.column + 1
                    );
                }
                let end = node.end_position();
                diagnostics.push(Diagnostic {
                    line: point.row + 1,
                    column: point.column + 1,
                    end_line: end.row + 1,
                    end_column: end.column + 1,
                    kind: if node.is_missing() {
                        format!("MISSING {}", node.kind())
                    } else {
                        "ERROR".into()
                    },
                });
                continue;
            }
            if node.has_error() {
                let mut cursor = node.walk();
                let children: Vec<_> = node.children(&mut cursor).collect();
                pending.extend(children.into_iter().rev());
            }
        }
        if !allow_parse_errors {
            bail!("C syntax errors; index was not replaced");
        }
    }
    let mut records = Vec::new();
    let mut pending = vec![(tree.root_node(), None::<String>)];
    while let Some((node, caller)) = pending.pop() {
        // Experimental recovery: error regions (including their descendants)
        // are excluded. Enclosing valid nodes can still be recovered incorrectly.
        if node.is_error() || node.is_missing() {
            continue;
        }
        if matches!(
            node.kind(),
            "comment" | "string_literal" | "char_literal" | "system_lib_string" | "preproc_arg"
        ) {
            continue;
        }
        let mut add = |target: Node<'_>, kind: &'static str, name: Option<String>| {
            let point = target.start_position();
            records.push(Record {
                name: name.unwrap_or_else(|| text(target, source).to_owned()),
                kind,
                caller: caller.clone(),
                line: point.row + 1,
                column: point.column + 1,
            });
        };
        let function_name = if node.kind() == "function_definition" {
            node.child_by_field_name("declarator")
                .and_then(declarator_name)
        } else {
            None
        };
        if let Some(name) = function_name {
            add(name, "definition", None);
        }
        if matches!(
            node.kind(),
            "identifier" | "type_identifier" | "field_identifier" | "statement_identifier"
        ) {
            add(node, "identifier", None);
        }
        match node.kind() {
            "declaration" | "field_declaration" | "type_definition" => {
                let mut cursor = node.walk();
                let is_extern = node.named_children(&mut cursor).any(|child| {
                    child.kind() == "storage_class_specifier" && text(child, source) == "extern"
                });
                let mut cursor = node.walk();
                for declarator in node.children_by_field_name("declarator", &mut cursor) {
                    if let Some(name) = declarator_name(declarator) {
                        let typedef = node.kind() == "type_definition";
                        let initialized = declarator.kind() == "init_declarator";
                        if typedef || (!function_declaration(name) && (!is_extern || initialized)) {
                            add(name, "definition", None);
                        }
                    }
                }
            }
            "struct_specifier" | "union_specifier" | "enum_specifier" => {
                if node.child_by_field_name("body").is_some()
                    && let Some(name) = node.child_by_field_name("name")
                {
                    add(name, "definition", None);
                }
            }
            "enumerator" | "preproc_def" | "preproc_function_def" => {
                if let Some(name) = node.child_by_field_name("name") {
                    add(name, "definition", None);
                }
            }
            "preproc_include" => {
                if let Some(path) = node.child_by_field_name("path") {
                    let raw = text(path, source);
                    let name = raw
                        .strip_prefix('"')
                        .and_then(|s| s.strip_suffix('"'))
                        .or_else(|| raw.strip_prefix('<').and_then(|s| s.strip_suffix('>')))
                        .unwrap_or(raw);
                    add(path, "import", Some(name.to_owned()));
                }
            }
            "call_expression" => {
                if let Some(function) = node.child_by_field_name("function") {
                    // Array dispatch has no single written callee name.
                    if let Some(target) = written_target(function, false) {
                        add(target, "call", None);
                    }
                }
            }
            "assignment_expression" | "update_expression" => {
                let target = node
                    .child_by_field_name("left")
                    .or_else(|| node.child_by_field_name("argument"));
                if let Some(target) = target.and_then(|node| written_target(node, true)) {
                    add(target, "assignment", None);
                }
            }
            "init_declarator" => {
                if let Some(target) = declarator_name(node) {
                    add(target, "assignment", None);
                }
            }
            _ => {}
        }
        let body = if node.kind() == "function_definition" {
            node.child_by_field_name("body")
        } else {
            None
        };
        let mut cursor = node.walk();
        let children: Vec<_> = node.named_children(&mut cursor).collect();
        for child in children.into_iter().rev() {
            let scope = if Some(child) == body {
                function_name.map(|name| text(name, source).to_owned())
            } else {
                caller.clone()
            };
            pending.push((child, scope));
        }
    }
    Ok(ParsedFile {
        records,
        diagnostics,
    })
}
