use tree_sitter::Node;

/// Count lines of code (including blank lines and comments) within a node.
pub fn count_loc(node: Node, _source: &[u8]) -> usize {
    let start_line = node.start_position().row;
    let end_line = node.end_position().row;
    end_line - start_line + 1
}

/// Count significant lines of code (excluding blank lines and comment-only lines).
///
/// Uses a stateful scan to correctly handle multi-line `/* */` blocks even when
/// body lines don't start with `*`.
pub fn count_sloc(node: Node, source: &[u8]) -> usize {
    let start_byte = node.start_byte();
    let end_byte = node.end_byte();
    let snippet = &source[start_byte..end_byte];
    let text = std::str::from_utf8(snippet).unwrap_or("");
    count_sloc_str(text)
}

/// Shared SLOC logic: stateful scan that correctly handles `/* ... */` blocks.
pub fn count_sloc_str(text: &str) -> usize {
    let mut in_block = false;
    let mut count = 0;
    for line in text.lines() {
        let t = line.trim();
        if in_block {
            if t.contains("*/") {
                in_block = false;
            }
            continue;
        }
        if t.starts_with("/*") {
            if !t.contains("*/") {
                in_block = true;
            }
            continue;
        }
        if !t.is_empty() && !t.starts_with("//") {
            count += 1;
        }
    }
    count
}
