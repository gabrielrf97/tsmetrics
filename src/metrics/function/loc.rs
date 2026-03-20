use tree_sitter::Node;

/// Count lines of code (including blank lines and comments) within a node.
pub fn count_loc(node: Node, _source: &[u8]) -> usize {
    let start_line = node.start_position().row;
    let end_line = node.end_position().row;
    end_line - start_line + 1
}

/// Count significant lines of code (excluding blank lines and comment-only lines).
pub fn count_sloc(node: Node, source: &[u8]) -> usize {
    let start_byte = node.start_byte();
    let end_byte = node.end_byte();
    let snippet = &source[start_byte..end_byte];
    let text = std::str::from_utf8(snippet).unwrap_or("");

    text.lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with("//") && !trimmed.starts_with("/*") && !trimmed.starts_with('*')
        })
        .count()
}
