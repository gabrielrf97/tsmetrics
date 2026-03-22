use tree_sitter::Node;

/// Effect Density result for a React component function.
///
/// `Effect Density = useEffect_count / component_loc`
///
/// A high effect density indicates a component whose side-effects dominate its
/// rendering logic, suggesting it should be split or that effects should be
/// extracted to custom hooks.
#[derive(Debug, Clone)]
pub struct EffectDensity {
    /// Number of `useEffect` (and `useLayoutEffect` / `useInsertionEffect`) calls.
    pub effect_count: usize,
    /// Lines of code in the component's function body (blank lines excluded from SLOC).
    pub component_sloc: usize,
    /// `effect_count / component_sloc`. Returns `0.0` when `component_sloc == 0`.
    pub density: f64,
}

/// Compute effect density for a function body node.
///
/// `body_node` should be the body of a React component function
/// (typically a `statement_block`).
pub fn compute_effect_density(body_node: Node, source: &[u8]) -> EffectDensity {
    // Start at fn_depth=1 because body_node is already the interior of the component
    // function, so any function encountered inside it is at nesting depth 2 (nested).
    let effect_count = count_effect_calls(body_node, source, 1);
    let component_sloc = count_sloc(body_node, source);

    let density = if component_sloc == 0 {
        0.0
    } else {
        effect_count as f64 / component_sloc as f64
    };

    EffectDensity {
        effect_count,
        component_sloc,
        density,
    }
}

const EFFECT_HOOKS: &[&str] = &["useEffect", "useLayoutEffect", "useInsertionEffect"];

/// `fn_depth`: 0 = top level, 1 = inside component function, ≥2 = nested function (stop).
fn count_effect_calls(node: Node, source: &[u8], fn_depth: usize) -> usize {
    if fn_depth >= 2 {
        return 0;
    }

    let mut count = 0;

    if node.kind() == "call_expression" {
        if let Some(func) = node.child_by_field_name("function") {
            if func.kind() == "identifier" {
                let name = func.utf8_text(source).unwrap_or("");
                if EFFECT_HOOKS.contains(&name) {
                    count += 1;
                }
            }
        }
    }

    let is_fn_node = matches!(
        node.kind(),
        "arrow_function" | "function_expression" | "function_declaration" | "method_definition"
    );
    let child_depth = if is_fn_node { fn_depth + 1 } else { fn_depth };

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        count += count_effect_calls(child, source, child_depth);
    }
    count
}

/// Count source lines of code (non-blank, non-comment lines) in a node.
fn count_sloc(node: Node, source: &[u8]) -> usize {
    let start = node.start_position().row;
    let end = node.end_position().row;
    if end < start {
        return 0;
    }

    let source_str = std::str::from_utf8(source).unwrap_or("");
    source_str
        .lines()
        .enumerate()
        .filter(|(i, line)| {
            *i >= start && *i <= end && !line.trim().is_empty()
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_typescript;

    fn effect_density(src: &str) -> EffectDensity {
        let tree = parse_typescript(src).expect("parse failed");
        // Find the first function body in the AST
        let body = find_first_function_body(tree.root_node()).expect("no function body found");
        compute_effect_density(body, src.as_bytes())
    }

    fn find_first_function_body(node: Node) -> Option<Node> {
        if matches!(
            node.kind(),
            "function_declaration" | "function_expression" | "arrow_function" | "method_definition"
        ) {
            if let Some(body) = node.child_by_field_name("body") {
                return Some(body);
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(body) = find_first_function_body(child) {
                return Some(body);
            }
        }
        None
    }

    #[test]
    fn test_no_effects_zero_density() {
        let src = r#"
function Component() {
    const [x, setX] = useState(0);
    return x;
}
"#;
        let ed = effect_density(src);
        assert_eq!(ed.effect_count, 0);
        assert_eq!(ed.density, 0.0);
    }

    #[test]
    fn test_single_effect() {
        let src = r#"
function Component() {
    useEffect(() => {
        document.title = "hello";
    }, []);
    return null;
}
"#;
        let ed = effect_density(src);
        assert_eq!(ed.effect_count, 1);
        assert!(ed.component_sloc > 0);
        assert!(ed.density > 0.0);
    }

    #[test]
    fn test_multiple_effects() {
        let src = r#"
function Dashboard() {
    useEffect(() => { fetchUser(); }, []);
    useEffect(() => { fetchPosts(); }, []);
    useEffect(() => { fetchComments(); }, []);
    return null;
}
"#;
        let ed = effect_density(src);
        assert_eq!(ed.effect_count, 3);
    }

    #[test]
    fn test_use_layout_effect_counted() {
        let src = r#"
function Modal() {
    useLayoutEffect(() => {
        focusRef.current?.focus();
    }, []);
    return null;
}
"#;
        let ed = effect_density(src);
        assert_eq!(ed.effect_count, 1);
    }

    #[test]
    fn test_effect_inside_nested_function_not_counted() {
        let src = r#"
function Component() {
    const handler = () => {
        useEffect(() => {}, []);
    };
    return null;
}
"#;
        let ed = effect_density(src);
        // The useEffect is inside an arrow function, should not be counted
        assert_eq!(ed.effect_count, 0);
    }

    #[test]
    fn test_density_ratio() {
        // 2 effects in a component with known SLOC
        let src = r#"function Comp() {
    useEffect(() => {}, []);
    useEffect(() => {}, []);
    return null;
}"#;
        let ed = effect_density(src);
        assert_eq!(ed.effect_count, 2);
        assert!(ed.component_sloc > 0);
        assert!((ed.density - 2.0 / ed.component_sloc as f64).abs() < 1e-9);
    }

    #[test]
    fn test_empty_function_zero_density() {
        let src = "function Comp() {}";
        let tree = parse_typescript(src).expect("parse failed");
        let body = find_first_function_body(tree.root_node()).expect("no body");
        let ed = compute_effect_density(body, src.as_bytes());
        assert_eq!(ed.effect_count, 0);
        assert_eq!(ed.density, 0.0);
    }
}
