use tree_sitter::Node;

/// Count the number of interfaces a class explicitly implements.
///
/// Walks the direct children of the class node, looking for an
/// `implements_clause` either directly or nested inside `class_heritage`
/// (the layout emitted by tree-sitter-typescript 0.20+).
/// Returns the number of named children of that clause, which corresponds
/// to the number of types listed after the `implements` keyword.
pub fn count_implemented_interfaces(class_node: Node) -> usize {
    let mut cursor = class_node.walk();
    for child in class_node.children(&mut cursor) {
        match child.kind() {
            // tree-sitter-typescript ≥ 0.20: heritage is wrapped.
            "class_heritage" => {
                let mut hc = child.walk();
                for hchild in child.children(&mut hc) {
                    if hchild.kind() == "implements_clause" {
                        return hchild.named_child_count();
                    }
                }
            }
            // Older grammar: implements_clause directly under the class node.
            "implements_clause" => return child.named_child_count(),
            _ => {}
        }
    }
    0
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_typescript;

    /// Parse `source`, find the first class node (declaration or abstract),
    /// and return its NOI.
    fn noi(source: &str) -> usize {
        let tree = parse_typescript(source).expect("parse failed");
        let root = tree.root_node();
        find_first_class(root)
            .map(count_implemented_interfaces)
            .expect("no class found in source")
    }

    fn find_first_class(node: Node) -> Option<Node> {
        if matches!(
            node.kind(),
            "class_declaration" | "abstract_class_declaration" | "class"
        ) && node.child_by_field_name("body").is_some()
        {
            return Some(node);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = find_first_class(child) {
                return Some(found);
            }
        }
        None
    }

    // ── Spec test cases ───────────────────────────────────────────────────────

    #[test]
    fn no_implements_returns_zero() {
        assert_eq!(noi("class Foo {}"), 0);
    }

    #[test]
    fn one_interface() {
        assert_eq!(noi("class Foo implements Bar {}"), 1);
    }

    #[test]
    fn multiple_interfaces() {
        assert_eq!(noi("class Foo implements Bar, Baz, Qux {}"), 3);
    }

    #[test]
    fn extends_and_implements() {
        assert_eq!(noi("class Foo extends Base implements Bar, Baz {}"), 2);
    }

    #[test]
    fn abstract_class_implements_one() {
        assert_eq!(noi("abstract class Foo implements Bar {}"), 1);
    }

    // ── Additional edge cases ─────────────────────────────────────────────────

    #[test]
    fn abstract_class_no_implements() {
        assert_eq!(noi("abstract class Foo {}"), 0);
    }

    #[test]
    fn abstract_class_multiple_interfaces() {
        assert_eq!(noi("abstract class Foo implements A, B, C {}"), 3);
    }

    #[test]
    fn extends_only_returns_zero() {
        assert_eq!(noi("class Foo extends Base {}"), 0);
    }

    #[test]
    fn implements_generic_interface() {
        // Generic type arguments should not inflate the count — each item in
        // the implements list is still one named child.
        assert_eq!(noi("class Foo implements Iterable<string> {}"), 1);
    }

    #[test]
    fn two_generic_interfaces() {
        assert_eq!(
            noi("class Foo implements Iterable<string>, Serializable<Foo> {}"),
            2
        );
    }
}
