use tree_sitter::Node;

/// React hook names that are recognized as built-in hooks.
const REACT_HOOKS: &[&str] = &[
    "useState",
    "useEffect",
    "useContext",
    "useReducer",
    "useCallback",
    "useMemo",
    "useRef",
    "useImperativeHandle",
    "useLayoutEffect",
    "useDebugValue",
    "useId",
    "useTransition",
    "useDeferredValue",
    "useSyncExternalStore",
    "useInsertionEffect",
];

/// Hook complexity result for a single React component function.
#[derive(Debug, Clone)]
pub struct HookComplexity {
    /// Total number of hook calls found in the component.
    pub hook_count: usize,
    /// Number of distinct hook types called (e.g. 3 useEffect calls = 1 distinct type).
    pub distinct_hooks: usize,
    /// Breakdown by hook name (sorted by name).
    pub hook_breakdown: Vec<(String, usize)>,
}

/// Compute hook complexity for an AST node (typically a component function's body).
///
/// Counts all direct hook calls: built-in React hooks (`useState`, `useEffect`, …)
/// and any custom hooks (identifiers matching `use[A-Z]…`).
///
/// Does NOT recurse into nested function bodies — hook calls inside nested
/// callbacks are not attributed to the enclosing component.
pub fn compute_hook_complexity(node: Node, source: &[u8]) -> HookComplexity {
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    collect_hook_calls(node, source, &mut counts, 0);

    let hook_count = counts.values().sum();
    let distinct_hooks = counts.len();
    let mut hook_breakdown: Vec<(String, usize)> = counts.into_iter().collect();
    hook_breakdown.sort_by(|a, b| a.0.cmp(&b.0));

    HookComplexity {
        hook_count,
        distinct_hooks,
        hook_breakdown,
    }
}

/// `fn_depth` tracks how many function scopes we've descended into:
///   0 = top level (e.g. program root, before any function)
///   1 = inside the component function we're analysing
///   ≥2 = nested function inside the component — stop recursing
fn collect_hook_calls(
    node: Node,
    source: &[u8],
    counts: &mut std::collections::HashMap<String, usize>,
    fn_depth: usize,
) {
    // We're inside a nested function — do not count hooks here.
    if fn_depth >= 2 {
        return;
    }

    if node.kind() == "call_expression" {
        if let Some(func) = node.child_by_field_name("function") {
            if func.kind() == "identifier" {
                let name = func.utf8_text(source).unwrap_or("").to_string();
                if is_hook_name(&name) {
                    *counts.entry(name).or_insert(0) += 1;
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
        collect_hook_calls(child, source, counts, child_depth);
    }
}

/// Returns true if `name` looks like a React hook identifier.
/// A hook name starts with `use` followed by an uppercase letter or nothing
/// (e.g. `use`, `useState`, `useFoo`).
fn is_hook_name(name: &str) -> bool {
    if !name.starts_with("use") {
        return false;
    }
    // Built-in hooks are always valid
    if REACT_HOOKS.contains(&name) {
        return true;
    }
    // Custom hooks: `use` followed by uppercase letter (useMyHook) or just `use`
    let rest = &name[3..];
    rest.is_empty() || rest.chars().next().map_or(false, |c| c.is_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_typescript;

    fn hook_complexity(src: &str) -> HookComplexity {
        let tree = parse_typescript(src).expect("parse failed");
        compute_hook_complexity(tree.root_node(), src.as_bytes())
    }

    #[test]
    fn test_no_hooks() {
        let src = "function Foo() { return null; }";
        let h = hook_complexity(src);
        assert_eq!(h.hook_count, 0);
        assert_eq!(h.distinct_hooks, 0);
    }

    #[test]
    fn test_single_use_state() {
        let src = r#"
function Counter() {
    const [count, setCount] = useState(0);
    return count;
}
"#;
        let h = hook_complexity(src);
        assert_eq!(h.hook_count, 1);
        assert_eq!(h.distinct_hooks, 1);
        assert!(h.hook_breakdown.iter().any(|(name, count)| name == "useState" && *count == 1));
    }

    #[test]
    fn test_multiple_same_hooks() {
        let src = r#"
function Form() {
    const [a, setA] = useState('');
    const [b, setB] = useState(0);
    const [c, setC] = useState(false);
    return a;
}
"#;
        let h = hook_complexity(src);
        assert_eq!(h.hook_count, 3);
        assert_eq!(h.distinct_hooks, 1);
        assert_eq!(h.hook_breakdown, vec![("useState".to_string(), 3)]);
    }

    #[test]
    fn test_mixed_hooks() {
        let src = r#"
function App() {
    const [x, setX] = useState(0);
    const ref = useRef(null);
    useEffect(() => {}, [x]);
    return x;
}
"#;
        let h = hook_complexity(src);
        assert_eq!(h.hook_count, 3);
        assert_eq!(h.distinct_hooks, 3);
    }

    #[test]
    fn test_custom_hook() {
        let src = r#"
function Component() {
    const data = useMyData();
    const status = useFetchStatus();
    return data;
}
"#;
        let h = hook_complexity(src);
        assert_eq!(h.hook_count, 2);
        assert_eq!(h.distinct_hooks, 2);
    }

    #[test]
    fn test_hook_inside_nested_function_not_counted() {
        // useEffect's callback (arrow function) contains a hook — should NOT be counted
        // as part of the outer component's hook count at the outer level.
        // Only the useEffect itself is counted.
        let src = r#"
function Component() {
    useEffect(() => {
        const inner = useState(0);  // inside callback, not counted
    }, []);
}
"#;
        let h = hook_complexity(src);
        // Only useEffect is counted (the outer call); useState is inside an arrow function
        assert_eq!(h.hook_count, 1);
        assert!(h.hook_breakdown.iter().any(|(name, _)| name == "useEffect"));
    }

    #[test]
    fn test_use_context_counted() {
        let src = r#"
function Themed() {
    const theme = useContext(ThemeContext);
    return theme;
}
"#;
        let h = hook_complexity(src);
        assert_eq!(h.hook_count, 1);
        assert!(h.hook_breakdown.iter().any(|(name, _)| name == "useContext"));
    }

    #[test]
    fn test_hook_breakdown_is_sorted() {
        let src = r#"
function Component() {
    useEffect(() => {}, []);
    const ref = useRef(null);
    const [x, setX] = useState(0);
}
"#;
        let h = hook_complexity(src);
        let names: Vec<&str> = h.hook_breakdown.iter().map(|(n, _)| n.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted, "breakdown should be sorted alphabetically");
    }

    #[test]
    fn test_use_memo_and_use_callback() {
        let src = r#"
function Expensive() {
    const value = useMemo(() => compute(), []);
    const handler = useCallback(() => {}, []);
    return value;
}
"#;
        let h = hook_complexity(src);
        assert_eq!(h.hook_count, 2);
        assert_eq!(h.distinct_hooks, 2);
    }

    #[test]
    fn test_use_reducer() {
        let src = r#"
function Store() {
    const [state, dispatch] = useReducer(reducer, initialState);
    return state;
}
"#;
        let h = hook_complexity(src);
        assert_eq!(h.hook_count, 1);
        assert_eq!(h.distinct_hooks, 1);
    }

    #[test]
    fn test_is_hook_name_builtin() {
        assert!(is_hook_name("useState"));
        assert!(is_hook_name("useEffect"));
        assert!(is_hook_name("useContext"));
    }

    #[test]
    fn test_is_hook_name_custom() {
        assert!(is_hook_name("useMyHook"));
        assert!(is_hook_name("useData"));
    }

    #[test]
    fn test_is_hook_name_not_a_hook() {
        assert!(!is_hook_name("userlevel"));
        assert!(!is_hook_name("used"));
        assert!(!is_hook_name("render"));
        assert!(!is_hook_name("fetch"));
    }
}
