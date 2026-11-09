use crate::language::Record;
use anyhow::{Context, Result, bail};
use tree_sitter::{Node, Parser};

fn text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}

// Only assignment targets are traversed here. Receivers and subscript indices
// are reads; obj.field and mapping[key] write field and mapping, respectively.
fn assignment_targets(node: Node<'_>) -> Vec<Node<'_>> {
    let mut targets = Vec::new();
    let mut pending = vec![node];
    while let Some(node) = pending.pop() {
        match node.kind() {
            "identifier" => targets.push(node),
            "attribute" => targets.extend(node.child_by_field_name("attribute")),
            "subscript" => pending.extend(node.child_by_field_name("value")),
            "pattern_list"
            | "tuple_pattern"
            | "list_pattern"
            | "list_splat_pattern"
            | "parenthesized_expression" => {
                let mut cursor = node.walk();
                pending.extend(node.named_children(&mut cursor));
            }
            _ => {}
        }
    }
    targets
}

pub fn parse(source: &str) -> Result<Vec<Record>> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_python::LANGUAGE.into())?;
    let tree = parser
        .parse(source, None)
        .context("Python parsing cancelled")?;
    if tree.root_node().has_error() {
        bail!("Python syntax errors; index was not replaced");
    }
    let mut records = Vec::new();
    // Iterative traversal avoids overflowing the Rust stack on deeply nested code.
    let mut pending = vec![(tree.root_node(), None::<String>)];
    while let Some((node, caller)) = pending.pop() {
        if node.kind() == "comment" {
            continue;
        }
        let mut add = |target: Node<'_>, kind| {
            let position = target.start_position();
            records.push(Record {
                name: if kind == "import" {
                    text(target, source)
                        .chars()
                        .filter(|c| !c.is_whitespace() && *c != '\\')
                        .collect()
                } else {
                    text(target, source).to_owned()
                },
                kind,
                caller: caller.clone(),
                line: position.row + 1,
                column: position.column + 1,
            });
        };
        if node.kind() == "identifier" {
            let definition = node.parent().is_some_and(|parent| {
                matches!(parent.kind(), "function_definition" | "class_definition")
                    && parent.child_by_field_name("name") == Some(node)
            });
            add(
                node,
                if definition {
                    "definition"
                } else {
                    "identifier"
                },
            );
        }
        if node.kind() == "call"
            && let Some(function) = node.child_by_field_name("function")
        {
            let target = match function.kind() {
                "identifier" => Some(function),
                "attribute" => function.child_by_field_name("attribute"),
                _ => None,
            };
            if let Some(target) = target {
                add(target, "call");
            }
        }
        let target = match node.kind() {
            "assignment" if node.child_by_field_name("right").is_some() => {
                node.child_by_field_name("left")
            }
            "augmented_assignment" => node.child_by_field_name("left"),
            "named_expression" => node.child_by_field_name("name"),
            _ => None,
        };
        if let Some(target) = target {
            for target in assignment_targets(target) {
                add(target, "assignment");
            }
        }
        // For `from package import item`, only package is known to be a module.
        // Imported members and aliases are not assumed to be module names.
        let modules = match node.kind() {
            "import_statement" => {
                let mut cursor = node.walk();
                node.children_by_field_name("name", &mut cursor)
                    .map(|n| n.child_by_field_name("name").unwrap_or(n))
                    .collect::<Vec<_>>()
            }
            "import_from_statement" => node
                .child_by_field_name("module_name")
                .into_iter()
                .collect(),
            _ => Vec::new(),
        };
        for module in modules {
            add(module, "import");
        }
        let mut cursor = node.walk();
        let children: Vec<_> = node.named_children(&mut cursor).collect();
        // Field lookup may scan siblings. Do it once, not once per child of a
        // module with thousands of top-level statements/comments.
        let body = node.child_by_field_name("body");
        for child in children.into_iter().rev() {
            let mut scope = caller.clone();
            if body == Some(child) {
                match node.kind() {
                    "function_definition" => {
                        scope = node
                            .child_by_field_name("name")
                            .map(|n| text(n, source).to_owned());
                    }
                    "class_definition" => scope = None,
                    "lambda" => scope = Some("<lambda>".into()),
                    _ => {}
                }
            }
            pending.push((child, scope));
        }
    }
    Ok(records)
}
