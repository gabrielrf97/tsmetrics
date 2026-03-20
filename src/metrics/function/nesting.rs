use tree_sitter::Node;

/// Calculate the maximum nesting depth within a function node.
pub fn max_nesting(node: Node) -> usize {
    calculate_nesting(node, 0)
}

fn calculate_nesting(node: Node, current_depth: usize) -> usize {
    let is_nesting_node = matches!(
        node.kind(),
        "if_statement"
            | "else_clause"
            | "while_statement"
            | "do_statement"
            | "for_statement"
            | "for_in_statement"
            | "switch_statement"
            | "try_statement"
            | "catch_clause"
            | "arrow_function"
            | "function_declaration"
            | "function_expression"
    );

    let next_depth = if is_nesting_node {
        current_depth + 1
    } else {
        current_depth
    };

    let mut max = next_depth;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let child_max = calculate_nesting(child, next_depth);
        if child_max > max {
            max = child_max;
        }
    }
    max
}
