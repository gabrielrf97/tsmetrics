use tree_sitter::Node;

/// Count the number of parameters in a function node.
pub fn param_count(node: Node) -> usize {
    // Look for formal_parameters or parameters child
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "formal_parameters" {
            return child.named_child_count();
        }
    }
    0
}
