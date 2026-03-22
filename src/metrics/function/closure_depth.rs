use tree_sitter::Node;

const FUNCTION_KINDS: &[&str] = &[
    "arrow_function",
    "function_expression",
    "function_declaration",
    "method_definition",
];

/// Compute the maximum function nesting depth (closure depth) within a node.
///
/// Each function/arrow/method that is defined inside another function adds one
/// level of closure depth.  The outermost function counts as depth 1.
///
/// Examples:
/// - A standalone function: depth = 1
/// - A function containing an arrow function: outer = 1, inner = 2
/// - Three levels deep: outer = 1, middle = 2, innermost = 3
///
/// Returns 0 when `node` is not a function and contains no functions.
pub fn max_closure_depth(node: Node) -> usize {
    max_depth_helper(node, 0)
}

fn max_depth_helper(node: Node, current_depth: usize) -> usize {
    let is_fn = FUNCTION_KINDS.contains(&node.kind());
    let depth = if is_fn {
        current_depth + 1
    } else {
        current_depth
    };

    let mut max = depth;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let d = max_depth_helper(child, depth);
        if d > max {
            max = d;
        }
    }
    max
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_typescript;

    fn depth(src: &str) -> usize {
        let tree = parse_typescript(src).expect("parse failed");
        max_closure_depth(tree.root_node())
    }

    #[test]
    fn test_no_functions_is_zero() {
        assert_eq!(depth("const x = 42;"), 0);
    }

    #[test]
    fn test_single_function_is_one() {
        let src = "function foo() { return 1; }";
        assert_eq!(depth(src), 1);
    }

    #[test]
    fn test_single_arrow_is_one() {
        let src = "const f = () => 1;";
        assert_eq!(depth(src), 1);
    }

    #[test]
    fn test_two_sibling_functions_are_both_depth_one() {
        let src = "function a() {} function b() {}";
        assert_eq!(depth(src), 1);
    }

    #[test]
    fn test_nested_function_expression_is_two() {
        let src = r#"
function outer() {
    const inner = function() { return 1; };
}
"#;
        assert_eq!(depth(src), 2);
    }

    #[test]
    fn test_nested_arrow_is_two() {
        let src = r#"
function outer() {
    const inner = () => 42;
}
"#;
        assert_eq!(depth(src), 2);
    }

    #[test]
    fn test_three_levels_deep() {
        let src = r#"
function outer() {
    function middle() {
        const inner = () => 1;
    }
}
"#;
        assert_eq!(depth(src), 3);
    }

    #[test]
    fn test_method_definition_is_one() {
        let src = r#"
class Foo {
    bar() { return 1; }
}
"#;
        assert_eq!(depth(src), 1);
    }

    #[test]
    fn test_method_with_nested_arrow_is_two() {
        let src = r#"
class Foo {
    bar() {
        const fn = () => 1;
    }
}
"#;
        assert_eq!(depth(src), 2);
    }

    #[test]
    fn test_deeply_nested_callbacks() {
        // Typical callback hell: 4 levels
        let src = r#"
function fetchData() {
    doA(function() {
        doB(function() {
            doC(() => {});
        });
    });
}
"#;
        assert_eq!(depth(src), 4);
    }

    #[test]
    fn test_sibling_branches_max_reported() {
        // Left branch goes 3 deep, right branch goes 2 deep → max is 3
        let src = r#"
function root() {
    function a() {
        const b = () => {};  // depth 3
    }
    const c = () => {};      // depth 2
}
"#;
        assert_eq!(depth(src), 3);
    }
}
