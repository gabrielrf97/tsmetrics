use tree_sitter::Node;

/// Check whether a node (function or class) has a preceding suppression comment.
///
/// Supported comment forms:
/// - `// tsmetrics-disable-next-function` (before function declarations)
/// - `// tsmetrics-disable-next-class` (before class declarations)
/// - `// tsmetrics-disable-module` (at file top-level, suppresses all violations in the file)
///
/// For functions wrapped in `export_statement` or `lexical_declaration` (e.g.
/// `export const foo = () => {}`), we walk up the parent chain to find the
/// outermost wrapper and check *its* preceding siblings.
pub fn has_suppression_comment(node: Node, source: &[u8], keyword: &str) -> bool {
    // First check directly on this node's preceding siblings
    if check_prev_siblings_for(node, source, keyword) {
        return true;
    }

    // Walk up through wrapper nodes (export_statement, lexical_declaration,
    // variable_declarator) and check their preceding siblings too.
    let mut current = node;
    while let Some(parent) = current.parent() {
        match parent.kind() {
            "export_statement" | "lexical_declaration" | "variable_declarator" => {
                if check_prev_siblings_for(parent, source, keyword) {
                    return true;
                }
                current = parent;
            }
            _ => break,
        }
    }

    false
}

/// Check whether any file-level comment contains `tsmetrics-disable-module`.
pub fn has_module_suppression(root: Node, source: &[u8]) -> bool {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "comment" {
            let text = child.utf8_text(source).unwrap_or("");
            if extract_directive(text, "tsmetrics-disable-module") {
                return true;
            }
        }
    }
    false
}

/// Walk preceding siblings of `node`, checking comment nodes for the directive.
/// Skips over other comments (e.g. JSDoc) and decorators to find the directive.
fn check_prev_siblings_for(node: Node, source: &[u8], keyword: &str) -> bool {
    let mut prev = node.prev_named_sibling();
    while let Some(sib) = prev {
        match sib.kind() {
            "comment" => {
                let text = sib.utf8_text(source).unwrap_or("");
                if extract_directive(text, keyword) {
                    return true;
                }
            }
            // Skip decorators — the suppression comment may be above them
            "decorator" => {}
            // Any other node means we've gone past where the comment could be
            _ => break,
        }
        prev = sib.prev_named_sibling();
    }
    false
}

/// Check if a comment's text contains the given directive keyword.
fn extract_directive(comment_text: &str, keyword: &str) -> bool {
    // Strip comment delimiters: // or /* ... */
    let trimmed = comment_text
        .trim_start_matches("//")
        .trim_start_matches("/*")
        .trim_end_matches("*/")
        .trim();
    trimmed.starts_with(keyword)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_typescript;

    fn is_function_suppressed(src: &str) -> bool {
        let tree = parse_typescript(src).expect("parse failed");
        let root = tree.root_node();
        find_and_check(root, src.as_bytes(), &["function_declaration", "arrow_function", "function_expression", "method_definition"], "tsmetrics-disable-next-function")
    }

    fn is_class_suppressed(src: &str) -> bool {
        let tree = parse_typescript(src).expect("parse failed");
        let root = tree.root_node();
        find_and_check(root, src.as_bytes(), &["class_declaration", "abstract_class_declaration"], "tsmetrics-disable-next-class")
    }

    fn is_module_suppressed(src: &str) -> bool {
        let tree = parse_typescript(src).expect("parse failed");
        has_module_suppression(tree.root_node(), src.as_bytes())
    }

    /// Helper: find the first node of the given kinds and check suppression.
    fn find_and_check(node: Node, source: &[u8], kinds: &[&str], keyword: &str) -> bool {
        if kinds.contains(&node.kind()) {
            return has_suppression_comment(node, source, keyword);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if find_and_check(child, source, kinds, keyword) {
                return true;
            }
        }
        false
    }

    // ── Function suppression ──────────────────────────────────────────────

    #[test]
    fn suppress_function_declaration() {
        assert!(is_function_suppressed(
            "// tsmetrics-disable-next-function\nfunction foo() {}"
        ));
    }

    #[test]
    fn no_suppression_without_comment() {
        assert!(!is_function_suppressed("function foo() {}"));
    }

    #[test]
    fn suppress_exported_function() {
        assert!(is_function_suppressed(
            "// tsmetrics-disable-next-function\nexport function foo() {}"
        ));
    }

    #[test]
    fn suppress_const_arrow() {
        assert!(is_function_suppressed(
            "// tsmetrics-disable-next-function\nconst foo = () => {}"
        ));
    }

    #[test]
    fn suppress_export_const_arrow() {
        assert!(is_function_suppressed(
            "// tsmetrics-disable-next-function\nexport const foo = () => {}"
        ));
    }

    #[test]
    fn suppress_with_jsdoc_between() {
        assert!(is_function_suppressed(
            "// tsmetrics-disable-next-function\n/** JSDoc comment */\nfunction foo() {}"
        ));
    }

    #[test]
    fn suppress_block_comment() {
        assert!(is_function_suppressed(
            "/* tsmetrics-disable-next-function */\nfunction foo() {}"
        ));
    }

    #[test]
    fn unrelated_comment_does_not_suppress() {
        assert!(!is_function_suppressed(
            "// some other comment\nfunction foo() {}"
        ));
    }

    // ── Class suppression ─────────────────────────────────────────────────

    #[test]
    fn suppress_class_declaration() {
        assert!(is_class_suppressed(
            "// tsmetrics-disable-next-class\nclass Foo {}"
        ));
    }

    #[test]
    fn suppress_exported_class() {
        assert!(is_class_suppressed(
            "// tsmetrics-disable-next-class\nexport class Foo {}"
        ));
    }

    #[test]
    fn no_class_suppression_without_comment() {
        assert!(!is_class_suppressed("class Foo {}"));
    }

    // ── Module suppression ────────────────────────────────────────────────

    #[test]
    fn suppress_module() {
        assert!(is_module_suppressed(
            "// tsmetrics-disable-module\nfunction foo() {}\nclass Bar {}"
        ));
    }

    #[test]
    fn no_module_suppression_without_comment() {
        assert!(!is_module_suppressed("function foo() {}"));
    }
}
