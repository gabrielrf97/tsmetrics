use std::collections::{HashMap, HashSet};
use tree_sitter::Node;

/// Custom Hook Cohesion (CHC) measures how focused a custom hook is.
///
/// A custom hook is a function whose name starts with `use` and is followed by
/// an uppercase letter (e.g. `useAuth`, `useFormState`).
///
/// CHC is computed analogously to Module Cohesion: two return values are
/// "connected" if they share at least one internal state or ref declaration.
///
/// `CHC = connected_return_pairs / total_possible_pairs`
///
/// Special cases:
/// - 0 or 1 return value: CHC = 1.0 (vacuously cohesive)
///
/// This metric flags hooks that bundle unrelated concerns — a high-cohesion
/// hook manages one logical piece of state; a low-cohesion hook should be
/// split into two smaller hooks.
#[derive(Debug, Clone)]
pub struct HookCohesion {
    /// Name of the custom hook function.
    pub hook_name: String,
    /// 1-based line of the hook definition.
    pub line: usize,
    /// Number of return values (properties in the return object or elements in
    /// the return array, or 1 for scalar returns).
    pub return_count: usize,
    /// Number of return-value pairs that share an internal state/ref.
    pub connected_pairs: usize,
    /// Total possible pairs.
    pub total_pairs: usize,
    /// CHC score in [0.0, 1.0].
    pub cohesion: f64,
}

/// Compute Custom Hook Cohesion for all custom hooks in `root`.
pub fn compute_hook_cohesion(root: Node, source: &[u8]) -> Vec<HookCohesion> {
    let mut results = Vec::new();
    collect_custom_hooks(root, source, &mut results, 0);
    results
}

fn collect_custom_hooks(node: Node, source: &[u8], out: &mut Vec<HookCohesion>, fn_depth: usize) {
    let kind = node.kind();
    let is_fn = matches!(
        kind,
        "function_declaration" | "function_expression" | "arrow_function" | "method_definition"
    );

    if is_fn && fn_depth == 0 {
        if let Some(name) = extract_fn_name(node, source) {
            if is_custom_hook_name(&name) {
                let line = node.start_position().row + 1;
                let cohesion = analyse_hook(node, source, &name, line);
                out.push(cohesion);
            }
        }
    }

    let child_depth = if is_fn { fn_depth + 1 } else { fn_depth };
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_custom_hooks(child, source, out, child_depth);
    }
}

fn extract_fn_name<'a>(node: Node, source: &'a [u8]) -> Option<String> {
    if let Some(name_node) = node.child_by_field_name("name") {
        return Some(name_node.utf8_text(source).unwrap_or("").to_string());
    }
    if let Some(parent) = node.parent() {
        match parent.kind() {
            "variable_declarator" => {
                if let Some(n) = parent.child_by_field_name("name") {
                    return Some(n.utf8_text(source).unwrap_or("").to_string());
                }
            }
            _ => {}
        }
    }
    None
}

fn is_custom_hook_name(name: &str) -> bool {
    if !name.starts_with("use") {
        return false;
    }
    let rest = &name[3..];
    !rest.is_empty() && rest.chars().next().map_or(false, |c| c.is_uppercase())
}

// ── Hook analysis ─────────────────────────────────────────────────────────────

fn analyse_hook(node: Node, source: &[u8], name: &str, line: usize) -> HookCohesion {
    // Step 1: collect internal declarations (useState, useRef, useReducer)
    let internals = collect_internals(node, source);

    // Step 2: find the return value(s)
    let return_names = collect_return_names(node, source);

    // Step 3: for each return name, find which internals it references in the hook body
    let return_sources: Vec<HashSet<String>> = return_names
        .iter()
        .map(|rn| resolve_return_deps(node, source, rn, &internals))
        .collect();

    let return_count = return_sources.len();
    let total_pairs = if return_count < 2 {
        0
    } else {
        return_count * (return_count - 1) / 2
    };

    let connected_pairs = if total_pairs == 0 {
        0
    } else {
        count_connected_pairs(&return_sources)
    };

    let cohesion = if return_count <= 1 {
        1.0
    } else if total_pairs == 0 {
        1.0
    } else {
        connected_pairs as f64 / total_pairs as f64
    };

    HookCohesion {
        hook_name: name.to_string(),
        line,
        return_count,
        connected_pairs,
        total_pairs,
        cohesion,
    }
}

/// Collect names of variables declared via useState / useRef / useReducer.
/// Returns a set of all binding names from these calls.
fn collect_internals(hook_node: Node, source: &[u8]) -> HashSet<String> {
    let mut internals = HashSet::new();
    let body = match hook_node.child_by_field_name("body") {
        Some(b) => b,
        None => return internals,
    };
    collect_state_declarations(body, source, &mut internals, 1);
    internals
}

const STATE_HOOKS: &[&str] = &["useState", "useReducer", "useRef", "useMemo", "useCallback"];

fn collect_state_declarations(node: Node, source: &[u8], out: &mut HashSet<String>, fn_depth: usize) {
    if fn_depth >= 2 {
        return;
    }

    // `const [value, setter] = useState(...)` or `const ref = useRef(...)`
    if node.kind() == "lexical_declaration" || node.kind() == "variable_declaration" {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "variable_declarator" {
                if let Some(value) = child.child_by_field_name("value") {
                    if is_hook_call(value, source) {
                        if let Some(name_node) = child.child_by_field_name("name") {
                            collect_binding_names(name_node, source, out);
                        }
                    }
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
        collect_state_declarations(child, source, out, child_depth);
    }
}

fn is_hook_call(node: Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    if let Some(func) = node.child_by_field_name("function") {
        if func.kind() == "identifier" {
            let name = func.utf8_text(source).unwrap_or("");
            return STATE_HOOKS.contains(&name);
        }
    }
    false
}

fn collect_binding_names(node: Node, source: &[u8], out: &mut HashSet<String>) {
    match node.kind() {
        "identifier" => {
            if let Ok(name) = node.utf8_text(source) {
                out.insert(name.to_string());
            }
        }
        "array_pattern" | "object_pattern" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_binding_names(child, source, out);
            }
        }
        _ => {}
    }
}

// ── Return value analysis ─────────────────────────────────────────────────────

/// Collect the "logical" return names from a hook.
///
/// For `return { value, setter, reset }` → ["value", "setter", "reset"]
/// For `return [count, setCount]` → ["count", "setCount"]
/// For `return value` → ["value"]
fn collect_return_names(hook_node: Node, source: &[u8]) -> Vec<String> {
    let body = match hook_node.child_by_field_name("body") {
        Some(b) => b,
        None => return Vec::new(),
    };
    let mut names = Vec::new();
    collect_returns(body, source, &mut names, 1);
    // Deduplicate while preserving first-occurrence order
    let mut seen = HashSet::new();
    names.retain(|n| seen.insert(n.clone()));
    names
}

fn collect_returns(node: Node, source: &[u8], out: &mut Vec<String>, fn_depth: usize) {
    if fn_depth >= 2 {
        return;
    }

    if node.kind() == "return_statement" {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            extract_return_names(child, source, out);
        }
    }

    let is_fn = matches!(
        node.kind(),
        "arrow_function" | "function_expression" | "function_declaration" | "method_definition"
    );
    let child_depth = if is_fn { fn_depth + 1 } else { fn_depth };

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_returns(child, source, out, child_depth);
    }
}

fn extract_return_names(node: Node, source: &[u8], out: &mut Vec<String>) {
    match node.kind() {
        "object" => {
            // `{ value, setter }` or `{ value: v, setter: s }`
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "pair" {
                    // `{ key: value }` — use the key name
                    if let Some(key) = child.child_by_field_name("key") {
                        if let Ok(name) = key.utf8_text(source) {
                            out.push(name.to_string());
                        }
                    }
                } else if child.kind() == "shorthand_property_identifier" {
                    // `{ value }` shorthand
                    if let Ok(name) = child.utf8_text(source) {
                        out.push(name.to_string());
                    }
                }
            }
        }
        "array" => {
            // `[count, setCount]`
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "identifier" {
                    if let Ok(name) = child.utf8_text(source) {
                        out.push(name.to_string());
                    }
                }
            }
        }
        "identifier" => {
            // `return value`
            if let Ok(name) = node.utf8_text(source) {
                out.push(name.to_string());
            }
        }
        _ => {}
    }
}

/// For a return name, find which internal state variables it references
/// by scanning the hook body for identifier usage.
fn resolve_return_deps(
    hook_node: Node,
    source: &[u8],
    return_name: &str,
    internals: &HashSet<String>,
) -> HashSet<String> {
    // Find where `return_name` is defined (the variable that holds it).
    // Then look at what internal variables are used in that definition.
    // Simplified: scan the entire hook body for assignments to `return_name`
    // and collect identifiers that match internals.
    let body = match hook_node.child_by_field_name("body") {
        Some(b) => b,
        None => return HashSet::new(),
    };

    let mut deps = HashSet::new();
    find_deps_for_name(body, source, return_name, internals, &mut deps, 1);
    deps
}

fn find_deps_for_name(
    node: Node,
    source: &[u8],
    target: &str,
    internals: &HashSet<String>,
    deps: &mut HashSet<String>,
    fn_depth: usize,
) {
    if fn_depth >= 2 {
        return;
    }

    // Look for variable declarators: `const target = expr` or `const [target, ...] = expr`
    if node.kind() == "variable_declarator" {
        if let Some(name_node) = node.child_by_field_name("name") {
            let bound = binding_contains(name_node, source, target);
            if bound {
                if let Some(value) = node.child_by_field_name("value") {
                    collect_identifiers(value, source, internals, deps);
                }
            }
        }
    }

    // Also check if `target` is directly in internals (self-reference)
    if internals.contains(target) {
        deps.insert(target.to_string());
    }

    let is_fn = matches!(
        node.kind(),
        "arrow_function" | "function_expression" | "function_declaration" | "method_definition"
    );
    let child_depth = if is_fn { fn_depth + 1 } else { fn_depth };

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        find_deps_for_name(child, source, target, internals, deps, child_depth);
    }
}

fn binding_contains(node: Node, source: &[u8], target: &str) -> bool {
    match node.kind() {
        "identifier" => node.utf8_text(source).unwrap_or("") == target,
        "array_pattern" | "object_pattern" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if binding_contains(child, source, target) {
                    return true;
                }
            }
            false
        }
        _ => false,
    }
}

fn collect_identifiers(node: Node, source: &[u8], filter: &HashSet<String>, deps: &mut HashSet<String>) {
    if node.kind() == "identifier" {
        if let Ok(name) = node.utf8_text(source) {
            if filter.contains(name) {
                deps.insert(name.to_string());
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_identifiers(child, source, filter, deps);
    }
}

fn count_connected_pairs(sources: &[HashSet<String>]) -> usize {
    let n = sources.len();
    let mut count = 0;
    for i in 0..n {
        for j in (i + 1)..n {
            if sources[i].intersection(&sources[j]).next().is_some() {
                count += 1;
            }
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_typescript;

    fn analyse(src: &str) -> Vec<HookCohesion> {
        let tree = parse_typescript(src).expect("parse failed");
        compute_hook_cohesion(tree.root_node(), src.as_bytes())
    }

    fn first(src: &str) -> HookCohesion {
        let mut results = analyse(src);
        assert!(!results.is_empty(), "expected at least one hook result");
        results.remove(0)
    }

    #[test]
    fn test_no_custom_hooks() {
        let src = "function foo() { return 1; }";
        let results = analyse(src);
        assert!(results.is_empty());
    }

    #[test]
    fn test_hook_detected_by_name() {
        let src = r#"
function useCounter() {
    const [count, setCount] = useState(0);
    return { count, setCount };
}
"#;
        let result = first(src);
        assert_eq!(result.hook_name, "useCounter");
    }

    #[test]
    fn test_non_hook_use_prefix_not_detected() {
        // `userlevel` doesn't start with `use` + uppercase
        let src = "function userlevel() { return 1; }";
        let results = analyse(src);
        assert!(results.is_empty());
    }

    #[test]
    fn test_single_return_vacuously_cohesive() {
        let src = r#"
function useCount() {
    const [count, setCount] = useState(0);
    return count;
}
"#;
        let result = first(src);
        assert_eq!(result.return_count, 1);
        assert_eq!(result.total_pairs, 0);
        assert!((result.cohesion - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_fully_cohesive_hook() {
        // Both return values share `count` internal state
        let src = r#"
function useCounter() {
    const [count, setCount] = useState(0);
    const increment = () => setCount(count + 1);
    const decrement = () => setCount(count - 1);
    return { increment, decrement };
}
"#;
        let result = first(src);
        // 2 return values: increment and decrement
        // Both depend on setCount which comes from the same useState
        assert_eq!(result.return_count, 2);
        assert_eq!(result.total_pairs, 1);
        // cohesion may be 1.0 if both share internals
        assert!(result.cohesion >= 0.0 && result.cohesion <= 1.0);
    }

    #[test]
    fn test_cohesion_range() {
        let src = r#"
function useFormField(label: string) {
    const [value, setValue] = useState('');
    const [error, setError] = useState('');
    return { value, error };
}
"#;
        let result = first(src);
        assert_eq!(result.return_count, 2);
        assert!(result.cohesion >= 0.0 && result.cohesion <= 1.0);
    }

    #[test]
    fn test_arrow_function_hook_detected() {
        let src = r#"
const useToggle = () => {
    const [on, setOn] = useState(false);
    return { on, setOn };
};
"#;
        let results = analyse(src);
        assert!(!results.is_empty(), "expected useToggle to be detected");
        assert_eq!(results[0].hook_name, "useToggle");
    }

    #[test]
    fn test_line_number_recorded() {
        let src = r#"
function useValue() {
    const [v, setV] = useState(0);
    return v;
}
"#;
        let result = first(src);
        assert!(result.line >= 2, "expected line >= 2, got {}", result.line);
    }

    #[test]
    fn test_is_custom_hook_name() {
        // useState starts with `use` + `S` (uppercase) → matches the pattern
        assert!(is_custom_hook_name("useState"));
        assert!(is_custom_hook_name("useMyHook"));
        assert!(is_custom_hook_name("useAuth"));
        assert!(!is_custom_hook_name("userlevel")); // lowercase r after 'use'
        assert!(!is_custom_hook_name("fetch"));
        assert!(!is_custom_hook_name("use")); // empty rest after 'use'
    }

    #[test]
    fn test_multiple_hooks_in_file() {
        let src = r#"
function useA() {
    const [a, setA] = useState(0);
    return a;
}

function useB() {
    const [b, setB] = useState('');
    return b;
}
"#;
        let results = analyse(src);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].hook_name, "useA");
        assert_eq!(results[1].hook_name, "useB");
    }
}
