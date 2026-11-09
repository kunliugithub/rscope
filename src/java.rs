use crate::language::{Record, parse_tree};
use anyhow::Result;
use tree_sitter::Node;

fn text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}

fn type_name(mut node: Node<'_>) -> Option<Node<'_>> {
    loop {
        if node.kind() == "type_identifier" {
            return Some(node);
        }
        let mut cursor = node.walk();
        node = node
            .named_children(&mut cursor)
            .filter(|n| {
                matches!(
                    n.kind(),
                    "type_identifier" | "scoped_type_identifier" | "generic_type"
                )
            })
            .last()?;
    }
}

fn written_target(mut node: Node<'_>) -> Option<Node<'_>> {
    loop {
        match node.kind() {
            "identifier" => return Some(node),
            "field_access" => return node.child_by_field_name("field"),
            "array_access" => node = node.child_by_field_name("array")?,
            "parenthesized_expression" => {
                let mut cursor = node.walk();
                node = node.named_children(&mut cursor).next()?;
            }
            _ => return None,
        }
    }
}

pub fn parse(source: &str) -> Result<Vec<Record>> {
    let tree = parse_tree(source, tree_sitter_java::LANGUAGE.into(), "Java")?;
    let mut records = Vec::new();
    let mut pending = vec![(tree.root_node(), None::<String>)];
    while let Some((node, caller)) = pending.pop() {
        if matches!(
            node.kind(),
            "line_comment" | "block_comment" | "string_literal" | "character_literal"
        ) {
            continue;
        }
        if matches!(node.kind(), "identifier" | "type_identifier") {
            records.push(Record::at(node, source, "identifier", &caller));
        }
        if matches!(
            node.kind(),
            "class_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "record_declaration"
                | "annotation_type_declaration"
                | "enum_constant"
                | "method_declaration"
                | "constructor_declaration"
                | "compact_constructor_declaration"
                | "annotation_type_element_declaration"
                | "variable_declarator"
        ) && let Some(name) = node.child_by_field_name("name")
        {
            records.push(Record::at(name, source, "definition", &caller));
        }
        match node.kind() {
            "record_declaration" => {
                if let Some(parameters) = node.child_by_field_name("parameters") {
                    let mut cursor = parameters.walk();
                    for parameter in parameters.named_children(&mut cursor) {
                        if let Some(name) = parameter.child_by_field_name("name") {
                            records.push(Record::at(name, source, "definition", &caller));
                        }
                    }
                }
            }
            "method_invocation" => {
                if let Some(name) = node.child_by_field_name("name") {
                    records.push(Record::at(name, source, "call", &caller));
                }
            }
            "object_creation_expression" => {
                if let Some(name) = node.child_by_field_name("type").and_then(type_name) {
                    records.push(Record::at(name, source, "call", &caller));
                }
            }
            "explicit_constructor_invocation" => {
                if let Some(name) = node.child_by_field_name("constructor") {
                    records.push(Record::at(name, source, "call", &caller));
                }
            }
            "variable_declarator" if node.child_by_field_name("value").is_some() => {
                if let Some(name) = node.child_by_field_name("name") {
                    records.push(Record::at(name, source, "assignment", &caller));
                }
            }
            "assignment_expression" | "update_expression" => {
                let mut cursor = node.walk();
                let target = node
                    .child_by_field_name("left")
                    .or_else(|| node.named_children(&mut cursor).next());
                if let Some(name) = target.and_then(written_target) {
                    records.push(Record::at(name, source, "assignment", &caller));
                }
            }
            "import_declaration" => {
                let mut cursor = node.walk();
                let children: Vec<_> = node.named_children(&mut cursor).collect();
                if let Some(path) = children
                    .iter()
                    .find(|n| matches!(n.kind(), "identifier" | "scoped_identifier"))
                {
                    let mut record = Record::at(*path, source, "import", &caller);
                    record.name.retain(|c| !c.is_whitespace());
                    if children.iter().any(|n| n.kind() == "asterisk") {
                        record.name.push_str(".*");
                    }
                    records.push(record);
                }
            }
            _ => {}
        }
        let body = node.child_by_field_name("body");
        let mut cursor = node.walk();
        let children: Vec<_> = node.named_children(&mut cursor).collect();
        for child in children.into_iter().rev() {
            let mut scope = if matches!(node.kind(), "class_body" | "interface_body" | "enum_body")
            {
                None
            } else {
                caller.clone()
            };
            if Some(child) == body {
                match node.kind() {
                    "method_declaration"
                    | "constructor_declaration"
                    | "compact_constructor_declaration" => {
                        scope = node
                            .child_by_field_name("name")
                            .map(|n| text(n, source).to_owned())
                    }
                    "lambda_expression" => scope = Some("<lambda>".into()),
                    _ => {}
                }
            }
            if node.kind() == "static_initializer" {
                scope = Some("<initializer>".into());
            }
            pending.push((child, scope));
        }
    }
    Ok(records)
}
