use tree_sitter::Node;

/// Calculate cyclomatic complexity for a function node.
/// Starts at 1 and adds 1 for each decision point.
pub fn cyclomatic_complexity(node: Node, source: &[u8]) -> usize {
    let mut complexity = 1;
    count_decision_points(node, source, &mut complexity);
    complexity
}

fn count_decision_points(node: Node, source: &[u8], count: &mut usize) {
    match node.kind() {
        "if_statement"
        | "else_clause"
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
        count_decision_points(child, source, count);
    }
}
