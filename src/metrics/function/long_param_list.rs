use tree_sitter::Node;

/// Default threshold above which a parameter list is considered "long".
///
/// Robert Martin's Clean Code recommends ≤3 parameters; we default to 4
/// (a common practical threshold).
pub const DEFAULT_THRESHOLD: usize = 4;

/// Long Parameter List detection result for a single function.
#[derive(Debug, Clone)]
pub struct LongParamListResult {
    /// The actual number of parameters.
    pub param_count: usize,
    /// Whether this function exceeds the threshold.
    pub is_long: bool,
    /// The threshold used for this detection.
    pub threshold: usize,
}

/// Check whether a function node has a "long" parameter list.
///
/// Uses `DEFAULT_THRESHOLD` (4). A function with > threshold parameters
/// is flagged.
pub fn check_long_param_list(node: Node) -> LongParamListResult {
    check_with_threshold(node, DEFAULT_THRESHOLD)
}

/// Check with a custom threshold.
pub fn check_with_threshold(node: Node, threshold: usize) -> LongParamListResult {
    let param_count = count_params(node);
    LongParamListResult {
        param_count,
        is_long: param_count > threshold,
        threshold,
    }
}

/// Count parameters in a function node.
///
/// Handles:
/// - `formal_parameters` for regular functions and arrow functions with parens
/// - Bare `identifier` for single-param arrow functions (`x => x`)
fn count_params(node: Node) -> usize {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "formal_parameters" => return child.named_child_count(),
            "identifier" if node.kind() == "arrow_function" => return 1,
            _ => {}
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_typescript;

    fn check(src: &str) -> LongParamListResult {
        let tree = parse_typescript(src).expect("parse failed");
        let root = tree.root_node();
        let func = find_first_function(root).expect("no function");
        check_long_param_list(func)
    }

    fn find_first_function(node: Node) -> Option<Node> {
        if matches!(
            node.kind(),
            "function_declaration" | "function_expression" | "arrow_function" | "method_definition"
        ) {
            return Some(node);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(f) = find_first_function(child) {
                return Some(f);
            }
        }
        None
    }

    #[test]
    fn test_zero_params_not_long() {
        let result = check("function foo() { return 1; }");
        assert_eq!(result.param_count, 0);
        assert!(!result.is_long);
    }

    #[test]
    fn test_one_param_not_long() {
        let result = check("function foo(a: number) { return a; }");
        assert_eq!(result.param_count, 1);
        assert!(!result.is_long);
    }

    #[test]
    fn test_four_params_not_long_at_default_threshold() {
        // exactly at threshold — not "long" (threshold is >4, so 4 is not long)
        let result = check("function foo(a: number, b: string, c: boolean, d: number) {}");
        assert_eq!(result.param_count, 4);
        assert!(!result.is_long, "4 params should not be long at threshold 4");
    }

    #[test]
    fn test_five_params_is_long() {
        let result = check("function foo(a: number, b: string, c: boolean, d: number, e: string) {}");
        assert_eq!(result.param_count, 5);
        assert!(result.is_long, "5 params should be long at default threshold 4");
    }

    #[test]
    fn test_arrow_function_with_parens() {
        let src = "const fn = (a: number, b: string, c: boolean, d: number, e: string) => a;";
        let result = check(src);
        assert_eq!(result.param_count, 5);
        assert!(result.is_long);
    }

    #[test]
    fn test_single_param_arrow_no_parens() {
        let src = "const fn = x => x * 2;";
        let result = check(src);
        assert_eq!(result.param_count, 1);
        assert!(!result.is_long);
    }

    #[test]
    fn test_method_definition() {
        let src = r#"
class Foo {
    bar(a: number, b: string, c: boolean, d: number, e: string): void {}
}
"#;
        let result = check(src);
        assert_eq!(result.param_count, 5);
        assert!(result.is_long);
    }

    #[test]
    fn test_custom_threshold_lower() {
        let src = "function foo(a: number, b: string, c: boolean) {}";
        let tree = parse_typescript(src).expect("parse failed");
        let root = tree.root_node();
        let func = find_first_function(root).unwrap();
        // With threshold of 2: 3 params > 2 → is_long
        let result = check_with_threshold(func, 2);
        assert_eq!(result.param_count, 3);
        assert!(result.is_long);
    }

    #[test]
    fn test_custom_threshold_higher() {
        let src = "function foo(a: number, b: string, c: boolean, d: number, e: string) {}";
        let tree = parse_typescript(src).expect("parse failed");
        let root = tree.root_node();
        let func = find_first_function(root).unwrap();
        // With threshold of 6: 5 params ≤ 6 → not long
        let result = check_with_threshold(func, 6);
        assert_eq!(result.param_count, 5);
        assert!(!result.is_long);
    }

    #[test]
    fn test_rest_parameter_counted() {
        let src = "function foo(a: number, b: string, ...rest: string[]) {}";
        let result = check(src);
        // 3 params total (a, b, ...rest)
        assert_eq!(result.param_count, 3);
    }

    #[test]
    fn test_optional_parameter_counted() {
        let src = "function foo(a: number, b?: string, c?: boolean, d?: number, e?: string) {}";
        let result = check(src);
        assert_eq!(result.param_count, 5);
        assert!(result.is_long);
    }

    #[test]
    fn test_destructured_parameter_counts_as_one() {
        // One destructured object param = 1 parameter slot
        let src = "function foo({ a, b, c, d, e }: Config) {}";
        let result = check(src);
        assert_eq!(result.param_count, 1, "destructured param is one param");
        assert!(!result.is_long);
    }

    #[test]
    fn test_threshold_stored_in_result() {
        let result = check("function foo() {}");
        assert_eq!(result.threshold, DEFAULT_THRESHOLD);
    }
}
