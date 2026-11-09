use crate::language::{Record, parse_tree};
use anyhow::Result;
use tree_sitter::Node;

fn text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}

fn leaf_name(mut node: Node<'_>, write: bool) -> Option<Node<'_>> {
    loop {
        match node.kind() {
            "identifier" | "field_identifier" => return Some(node),
            "scoped_identifier" | "scoped_type_identifier" => {
                return node.child_by_field_name("name");
            }
            "field_expression" => {
                return node
                    .child_by_field_name("field")
                    .filter(|n| n.kind() == "field_identifier");
            }
            "generic_function" => node = node.child_by_field_name("function")?,
            "parenthesized_expression" | "unary_expression" => {
                let mut cursor = node.walk();
                node = node.named_children(&mut cursor).next()?;
            }
            "index_expression" if write => {
                let mut cursor = node.walk();
                node = node.named_children(&mut cursor).next()?;
            }
            _ => return None,
        }
    }
}

fn bindings(pattern: Node<'_>) -> Vec<Node<'_>> {
    let mut result = Vec::new();
    let mut pending = vec![pattern];
    while let Some(node) = pending.pop() {
        match node.kind() {
            "identifier" | "shorthand_field_identifier" => result.push(node),
            "field_pattern" => pending.extend(
                node.child_by_field_name("pattern")
                    .or_else(|| node.child_by_field_name("name")),
            ),
            "struct_pattern" | "tuple_struct_pattern" => {
                let ty = node.child_by_field_name("type");
                let mut cursor = node.walk();
                pending.extend(node.named_children(&mut cursor).filter(|n| Some(*n) != ty));
            }
            "tuple_pattern" | "slice_pattern" | "mut_pattern" | "ref_pattern"
            | "reference_pattern" | "captured_pattern" | "or_pattern" => {
                let mut cursor = node.walk();
                pending.extend(node.named_children(&mut cursor));
            }
            _ => {}
        }
    }
    result
}

fn use_paths(argument: Node<'_>, source: &str, caller: &Option<String>, records: &mut Vec<Record>) {
    let mut pending = vec![(argument, String::new())];
    while let Some((node, prefix)) = pending.pop() {
        let join = |suffix: &str| {
            if prefix.is_empty() {
                suffix.to_owned()
            } else if suffix == "self" {
                prefix.clone()
            } else {
                format!("{prefix}::{suffix}")
            }
        };
        match node.kind() {
            "scoped_use_list" => {
                if let (Some(path), Some(list)) = (
                    node.child_by_field_name("path"),
                    node.child_by_field_name("list"),
                ) {
                    let path: String = text(path, source)
                        .chars()
                        .filter(|c| !c.is_whitespace())
                        .collect();
                    pending.push((list, join(&path)));
                }
            }
            "use_list" => {
                let mut cursor = node.walk();
                pending.extend(
                    node.named_children(&mut cursor)
                        .map(|n| (n, prefix.clone())),
                );
            }
            "use_as_clause" => {
                if let Some(path) = node.child_by_field_name("path") {
                    pending.push((path, prefix));
                }
            }
            _ => {
                let raw: String = text(node, source)
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect();
                let mut record = Record::at(node, source, "import", caller);
                record.name = join(&raw);
                records.push(record);
            }
        }
    }
}

pub fn parse(source: &str) -> Result<Vec<Record>> {
    let tree = parse_tree(source, tree_sitter_rust::LANGUAGE.into(), "Rust")?;
    let mut records = Vec::new();
    let mut pending = vec![(tree.root_node(), None::<String>)];
    while let Some((node, caller)) = pending.pop() {
        if matches!(
            node.kind(),
            "line_comment"
                | "block_comment"
                | "string_literal"
                | "raw_string_literal"
                | "char_literal"
                | "token_tree"
                | "attribute_item"
                | "inner_attribute_item"
        ) {
            continue;
        }
        if matches!(
            node.kind(),
            "identifier" | "type_identifier" | "field_identifier" | "shorthand_field_identifier"
        ) {
            records.push(Record::at(node, source, "identifier", &caller));
        }
        if matches!(
            node.kind(),
            "function_item"
                | "function_signature_item"
                | "struct_item"
                | "enum_item"
                | "enum_variant"
                | "union_item"
                | "trait_item"
                | "type_item"
                | "associated_type"
                | "const_item"
                | "static_item"
                | "mod_item"
                | "field_declaration"
                | "macro_definition"
        ) && let Some(name) = node.child_by_field_name("name")
        {
            records.push(Record::at(name, source, "definition", &caller));
        }
        match node.kind() {
            "let_declaration" => {
                if let Some(pattern) = node.child_by_field_name("pattern") {
                    for binding in bindings(pattern) {
                        records.push(Record::at(binding, source, "definition", &caller));
                        if node.child_by_field_name("value").is_some() {
                            records.push(Record::at(binding, source, "assignment", &caller));
                        }
                    }
                }
            }
            "static_item" | "const_item" => {
                if node.child_by_field_name("value").is_some()
                    && let Some(name) = node.child_by_field_name("name")
                {
                    records.push(Record::at(name, source, "assignment", &caller));
                }
            }
            "assignment_expression" | "compound_assignment_expr" => {
                if let Some(target) = node
                    .child_by_field_name("left")
                    .and_then(|n| leaf_name(n, true))
                {
                    records.push(Record::at(target, source, "assignment", &caller));
                }
            }
            "call_expression" | "macro_invocation" => {
                let target = node
                    .child_by_field_name("function")
                    .or_else(|| node.child_by_field_name("macro"));
                if let Some(target) = target.and_then(|n| leaf_name(n, false)) {
                    records.push(Record::at(target, source, "call", &caller));
                }
            }
            "use_declaration" => {
                if let Some(argument) = node.child_by_field_name("argument") {
                    use_paths(argument, source, &caller, &mut records);
                }
            }
            "extern_crate_declaration" => {
                if let Some(name) = node.child_by_field_name("name") {
                    records.push(Record::at(name, source, "import", &caller));
                }
            }
            "mod_item" if node.child_by_field_name("body").is_none() => {
                if let Some(name) = node.child_by_field_name("name") {
                    records.push(Record::at(name, source, "import", &caller));
                }
            }
            _ => {}
        }
        if node.kind() == "macro_definition" {
            if let Some(name) = node.child_by_field_name("name") {
                records.push(Record::at(name, source, "identifier", &caller));
            }
            continue;
        }
        let body = node.child_by_field_name("body");
        let mut cursor = node.walk();
        let children: Vec<_> = node.named_children(&mut cursor).collect();
        for child in children.into_iter().rev() {
            let mut scope = caller.clone();
            if Some(child) == body {
                match node.kind() {
                    "function_item" => {
                        scope = node
                            .child_by_field_name("name")
                            .map(|n| text(n, source).to_owned())
                    }
                    "closure_expression" => scope = Some("<closure>".into()),
                    "mod_item" | "impl_item" | "trait_item" | "struct_item" | "enum_item"
                    | "union_item" => scope = None,
                    _ => {}
                }
            }
            pending.push((child, scope));
        }
    }
    Ok(records)
}
