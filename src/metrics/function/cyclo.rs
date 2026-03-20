use tree_sitter::Node;

const NESTED_FUNCTION_KINDS: &[&str] =
    &["arrow_function", "function_expression", "function_declaration"];

/// Calculate cyclomatic complexity for a function node.
/// Starts at 1 and adds 1 for each decision point.
/// Does NOT recurse into nested function nodes — their complexity is counted separately.
pub fn cyclomatic_complexity(node: Node, source: &[u8]) -> usize {
    let mut complexity = 1;
    count_decision_points(node, source, &mut complexity);
    complexity
}

fn count_decision_points(node: Node, source: &[u8], count: &mut usize) {
    match node.kind() {
        // `else_clause` is intentionally excluded: it is the complement of `if`, not an
        // independent decision path. An if/else should produce CC=2, not CC=3.
        "if_statement"
        | "while_statement"
        | "do_statement"
        | "for_statement"
        | "for_in_statement"
        | "switch_case"
        | "catch_clause"
        | "ternary_expression"
        | "binary_expression" => {
            if node.kind() == "binary_expression" {
                if let Some(op) = node.child_by_field_name("operator") {
                    let op_text = op.utf8_text(source).unwrap_or("");
                    if op_text == "&&" || op_text == "||" || op_text == "??" {
                        *count += 1;
                    }
                }
            } else {
                *count += 1;
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        // Skip nested functions — they are collected and measured independently.
        if NESTED_FUNCTION_KINDS.contains(&child.kind()) {
            continue;
        }
        count_decision_points(child, source, count);
    }
}
