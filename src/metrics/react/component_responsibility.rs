use tree_sitter::Node;

/// Component Responsibility Score (CRS) — a weighted composite metric for
/// detecting "God Components": React components that are doing too much.
///
/// Score = w_props * prop_count
///       + w_state * state_count
///       + w_effects * effect_count
///       + w_jsx * jsx_element_count
///
/// Default weights (tunable):
///   - props:   1.0  (each prop adds 1 point)
///   - state:   2.0  (each useState/useReducer call adds 2 points — state is costly)
///   - effects: 3.0  (each useEffect adds 3 points — effects are very costly)
///   - jsx:     0.5  (each JSX element adds 0.5 points — raw size indicator)
///
/// A score above 20 indicates a component with too much responsibility.
///
/// This is the React/FP equivalent of the God Class detection strategy.
#[derive(Debug, Clone)]
pub struct ComponentResponsibilityScore {
    /// Number of props (parameters of the component function).
    pub prop_count: usize,
    /// Number of state declarations (useState + useReducer calls).
    pub state_count: usize,
    /// Number of effect hooks (useEffect + useLayoutEffect + useInsertionEffect).
    pub effect_count: usize,
    /// Number of JSX elements (jsx_element + jsx_self_closing_element).
    pub jsx_element_count: usize,
    /// Computed responsibility score.
    pub score: f64,
}

/// Weights for the CRS composite score.
#[derive(Debug, Clone)]
pub struct CrsWeights {
    pub props: f64,
    pub state: f64,
    pub effects: f64,
    pub jsx: f64,
}

impl Default for CrsWeights {
    fn default() -> Self {
        CrsWeights {
            props: 1.0,
            state: 2.0,
            effects: 3.0,
            jsx: 0.5,
        }
    }
}

/// Compute the Component Responsibility Score for a function node.
///
/// `component_node` should be the function or arrow function that represents
/// the React component.
pub fn compute_component_responsibility(
    component_node: Node,
    source: &[u8],
    weights: &CrsWeights,
) -> ComponentResponsibilityScore {
    let prop_count = count_props(component_node, source);
    let (state_count, effect_count) = count_hooks(component_node, source);
    let jsx_element_count = count_jsx_elements(component_node, source);

    let score = weights.props * prop_count as f64
        + weights.state * state_count as f64
        + weights.effects * effect_count as f64
        + weights.jsx * jsx_element_count as f64;

    ComponentResponsibilityScore {
        prop_count,
        state_count,
        effect_count,
        jsx_element_count,
        score,
    }
}

// ── Prop counting ────────────────────────────────────────────────────────────

fn count_props(component_node: Node, source: &[u8]) -> usize {
    // Find the formal_parameters node.
    let mut cursor = component_node.walk();
    for child in component_node.children(&mut cursor) {
        if child.kind() == "formal_parameters" {
            return count_formal_params(child, source);
        }
        // Single-param arrow function: `props => ...`
        if child.kind() == "identifier" && component_node.kind() == "arrow_function" {
            return 1;
        }
    }
    0
}

fn count_formal_params(params_node: Node, source: &[u8]) -> usize {
    let mut count = 0;
    let mut cursor = params_node.walk();
    for child in params_node.children(&mut cursor) {
        match child.kind() {
            "required_parameter" | "optional_parameter" | "rest_parameter" => {
                count += 1;
            }
            "identifier" => {
                // Plain JS params without type annotations
                let _ = source;
                count += 1;
            }
            _ => {}
        }
    }
    count
}

// ── Hook counting ─────────────────────────────────────────────────────────────

const STATE_HOOKS: &[&str] = &["useState", "useReducer"];
const EFFECT_HOOKS: &[&str] = &["useEffect", "useLayoutEffect", "useInsertionEffect"];

/// Count state declarations and effect calls in the component's direct scope.
/// Returns (state_count, effect_count).
fn count_hooks(component_node: Node, source: &[u8]) -> (usize, usize) {
    let mut state = 0usize;
    let mut effects = 0usize;
    if let Some(body) = component_node.child_by_field_name("body") {
        collect_hooks(body, source, &mut state, &mut effects, 1);
    }
    (state, effects)
}

fn collect_hooks(node: Node, source: &[u8], state: &mut usize, effects: &mut usize, fn_depth: usize) {
    if fn_depth >= 2 {
        return;
    }

    if node.kind() == "call_expression" {
        if let Some(func) = node.child_by_field_name("function") {
            if func.kind() == "identifier" {
                let name = func.utf8_text(source).unwrap_or("");
                if STATE_HOOKS.contains(&name) {
                    *state += 1;
                } else if EFFECT_HOOKS.contains(&name) {
                    *effects += 1;
                }
            }
        }
    }

    let is_fn = matches!(
        node.kind(),
        "arrow_function" | "function_expression" | "function_declaration" | "method_definition"
    );
    let child_depth = if is_fn { fn_depth + 1 } else { fn_depth };

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_hooks(child, source, state, effects, child_depth);
    }
}

// ── JSX element counting ──────────────────────────────────────────────────────

fn count_jsx_elements(component_node: Node, source: &[u8]) -> usize {
    let _ = source;
    count_jsx_in(component_node)
}

fn count_jsx_in(node: Node) -> usize {
    let mut count = 0;
    if matches!(
        node.kind(),
        "jsx_element" | "jsx_self_closing_element" | "jsx_fragment"
    ) {
        count += 1;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        count += count_jsx_in(child);
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_tsx;

    fn crs(src: &str) -> ComponentResponsibilityScore {
        let tree = parse_tsx(src).expect("parse failed");
        let root = tree.root_node();
        let component = find_first_function(root).expect("no function found");
        compute_component_responsibility(component, src.as_bytes(), &CrsWeights::default())
    }

    fn find_first_function(node: Node) -> Option<Node> {
        if matches!(
            node.kind(),
            "function_declaration" | "function_expression" | "arrow_function"
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
    fn test_empty_component_zero_score() {
        let src = "function Empty() { return null; }";
        let result = crs(src);
        assert_eq!(result.prop_count, 0);
        assert_eq!(result.state_count, 0);
        assert_eq!(result.effect_count, 0);
        assert_eq!(result.jsx_element_count, 0);
        assert_eq!(result.score, 0.0);
    }

    #[test]
    fn test_props_counted() {
        let src = r#"
function Comp({ name, age, onClick }: Props) {
    return null;
}
"#;
        let result = crs(src);
        // One destructured object parameter
        assert_eq!(result.prop_count, 1);
    }

    #[test]
    fn test_state_counted() {
        let src = r#"
function Counter() {
    const [count, setCount] = useState(0);
    const [name, setName] = useState('');
    return null;
}
"#;
        let result = crs(src);
        assert_eq!(result.state_count, 2);
    }

    #[test]
    fn test_effects_counted() {
        let src = r#"
function App() {
    useEffect(() => {}, []);
    useEffect(() => {}, []);
    return null;
}
"#;
        let result = crs(src);
        assert_eq!(result.effect_count, 2);
    }

    #[test]
    fn test_jsx_elements_counted() {
        let src = r#"
function Card() {
    return (
        <div>
            <h1>Title</h1>
            <p>Body</p>
        </div>
    );
}
"#;
        let result = crs(src);
        // div, h1, p
        assert_eq!(result.jsx_element_count, 3);
    }

    #[test]
    fn test_score_formula() {
        // props=1(w=1), state=2(w=2), effects=1(w=3), jsx=4(w=0.5)
        // score = 1*1 + 2*2 + 1*3 + 4*0.5 = 1 + 4 + 3 + 2 = 10
        let src = r#"
function Widget({ title }: Props) {
    const [x, setX] = useState(0);
    const [y, setY] = useState(0);
    useEffect(() => {}, []);
    return (
        <div>
            <span>{x}</span>
            <span>{y}</span>
            <button>click</button>
        </div>
    );
}
"#;
        let result = crs(src);
        assert_eq!(result.prop_count, 1);
        assert_eq!(result.state_count, 2);
        assert_eq!(result.effect_count, 1);
        // div + span + span + button = 4 jsx elements
        assert_eq!(result.jsx_element_count, 4);
        let expected = 1.0 * 1.0 + 2.0 * 2.0 + 3.0 * 1.0 + 0.5 * 4.0;
        assert!((result.score - expected).abs() < 1e-9);
    }

    #[test]
    fn test_use_reducer_counted_as_state() {
        let src = r#"
function Store() {
    const [state, dispatch] = useReducer(reducer, initial);
    return null;
}
"#;
        let result = crs(src);
        assert_eq!(result.state_count, 1);
    }

    #[test]
    fn test_use_layout_effect_counted() {
        let src = r#"
function Modal() {
    useLayoutEffect(() => {}, []);
    return null;
}
"#;
        let result = crs(src);
        assert_eq!(result.effect_count, 1);
    }

    #[test]
    fn test_nested_effect_not_counted() {
        // useEffect inside a handler is not a direct component effect
        let src = r#"
function Component() {
    const handler = () => {
        useEffect(() => {}, []);
    };
    return null;
}
"#;
        let result = crs(src);
        assert_eq!(result.effect_count, 0);
    }

    #[test]
    fn test_custom_weights() {
        let src = r#"
function Comp() {
    const [x, setX] = useState(0);
    return null;
}
"#;
        let weights = CrsWeights {
            props: 0.0,
            state: 5.0,
            effects: 0.0,
            jsx: 0.0,
        };
        let tree = parse_tsx(src).expect("parse failed");
        let root = tree.root_node();
        let component = find_first_function(root).unwrap();
        let result = compute_component_responsibility(component, src.as_bytes(), &weights);
        assert_eq!(result.state_count, 1);
        assert!((result.score - 5.0).abs() < 1e-9);
    }
}
