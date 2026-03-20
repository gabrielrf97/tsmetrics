use tree_sitter::Node;

/// Count the number of parameters in a function node.
///
/// Handles three forms:
/// - `(a, b) => expr`  — `formal_parameters` child with named children
/// - `x => expr`       — bare `identifier` child (no parentheses)
/// - `function f(a, b)` — same `formal_parameters` path
pub fn param_count(node: Node) -> usize {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "formal_parameters" => return child.named_child_count(),
            // Single-parameter arrow function without parentheses: `x => expr`
            "identifier" if node.kind() == "arrow_function" => return 1,
            _ => {}
        }
    }
    0
}
