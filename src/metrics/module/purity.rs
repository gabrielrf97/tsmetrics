use std::collections::HashSet;

use tree_sitter::Node;

const NESTED_FUNCTION_KINDS: &[&str] = &[
    "arrow_function",
    "function_expression",
    "function_declaration",
    "method_definition",
];

const IMPURE_GLOBAL_CALLS: &[&str] =
    &["fetch", "setTimeout", "setInterval", "alert", "prompt"];

const MUTATION_METHODS: &[&str] = &["push", "splice", "sort"];

/// A reason why a function is considered impure.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ImpurityReason {
    UsesThis,
    CallsImpureApi(String),
    MutatesParameter(String),
    WritesToOuterScope(String),
    UsesAwait,
    CallsMutationMethod(String),
    UsesDeleteOperator,
}

impl ImpurityReason {
    pub fn description(&self) -> String {
        match self {
            ImpurityReason::UsesThis => "accesses `this`".to_string(),
            ImpurityReason::CallsImpureApi(name) => format!("calls impure API `{}`", name),
            ImpurityReason::MutatesParameter(name) => {
                format!("mutates parameter `{}`", name)
            }
            ImpurityReason::WritesToOuterScope(name) => {
                format!("writes to outer-scope variable `{}`", name)
            }
            ImpurityReason::UsesAwait => "uses `await` (I/O by convention)".to_string(),
            ImpurityReason::CallsMutationMethod(name) => {
                format!("calls mutation method `.{}()`", name)
            }
            ImpurityReason::UsesDeleteOperator => "uses `delete` operator".to_string(),
        }
    }
}

/// Purity information for a single function.
#[derive(Debug, Clone)]
pub struct FunctionPurity {
    pub name: String,
    pub line: usize,
    pub is_pure: bool,
    pub reasons: Vec<ImpurityReason>,
}

/// Pure Function Ratio (PFR) analysis for a file.
///
/// `PFR = pure_functions / total_functions`
///
/// A function is considered pure when none of the following impurity heuristics apply:
/// - Accesses `this`
/// - Calls known impure APIs (`console.*`, `fetch`, `Math.random`, `Date.now`,
///   `setTimeout`, `setInterval`, `alert`, `prompt`)
/// - Mutates a parameter's properties (`param.x = ...`)
/// - Writes to a variable declared outside the function scope
/// - Contains `await` (I/O by convention)
/// - Calls array mutation methods (`.push()`, `.splice()`, `.sort()`)
/// - Uses the `delete` operator
#[derive(Debug, Clone)]
pub struct ModulePurityResult {
    pub file: String,
    pub total_functions: usize,
    pub pure_functions: usize,
    /// `PFR = pure_functions / total_functions`. Returns `1.0` when there are no functions.
    pub ratio: f64,
    pub functions: Vec<FunctionPurity>,
}

impl ModulePurityResult {
    pub fn impure_functions(&self) -> impl Iterator<Item = &FunctionPurity> {
        self.functions.iter().filter(|f| !f.is_pure)
    }
}

/// Compute the Pure Function Ratio for a file's AST.
pub fn compute_module_purity(root: Node, source: &[u8], file: &str) -> ModulePurityResult {
    let mut functions = Vec::new();
    collect_function_purity(root, source, &mut functions);

    let total = functions.len();
    let pure = functions.iter().filter(|f| f.is_pure).count();
    let ratio = if total == 0 {
        1.0
    } else {
        pure as f64 / total as f64
    };

    ModulePurityResult {
        file: file.to_string(),
        total_functions: total,
        pure_functions: pure,
        ratio,
        functions,
    }
}

fn collect_function_purity(node: Node, source: &[u8], out: &mut Vec<FunctionPurity>) {
    let kind = node.kind();
    let is_function = matches!(
        kind,
        "function_declaration" | "function_expression" | "arrow_function" | "method_definition"
    );

    if is_function {
        let name = extract_function_name(node, source);
        let line = node.start_position().row + 1;
        let params = collect_param_names(node, source);
        let locals = collect_local_names(node, source);
        let mut reasons = Vec::new();

        if let Some(body) = node.child_by_field_name("body") {
            scan_for_impurity(body, source, &params, &locals, &mut reasons);
        }

        let is_pure = reasons.is_empty();
        out.push(FunctionPurity {
            name,
            line,
            is_pure,
            reasons,
        });
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_function_purity(child, source, out);
    }
}

/// Recursively scan a node for impurity markers.
/// Skips recursing into nested function bodies — those are analysed independently.
fn scan_for_impurity(
    node: Node,
    source: &[u8],
    params: &HashSet<String>,
    locals: &HashSet<String>,
    reasons: &mut Vec<ImpurityReason>,
) {
    match node.kind() {
        "this" => {
            push_unique(reasons, ImpurityReason::UsesThis);
        }
        "await_expression" => {
            push_unique(reasons, ImpurityReason::UsesAwait);
        }
        "unary_expression" => {
            // Detect the `delete` operator by checking the node text prefix.
            let text = node.utf8_text(source).unwrap_or("");
            if text.starts_with("delete ") || text == "delete" {
                push_unique(reasons, ImpurityReason::UsesDeleteOperator);
            }
        }
        "call_expression" => {
            check_call_expression(node, source, reasons);
        }
        "assignment_expression" | "augmented_assignment_expression" => {
            check_assignment(node, source, params, locals, reasons);
        }
        _ => {}
    }

    // Do not recurse into nested function bodies — they are analysed separately.
    if NESTED_FUNCTION_KINDS.contains(&node.kind()) {
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        scan_for_impurity(child, source, params, locals, reasons);
    }
}

fn check_call_expression(node: Node, source: &[u8], reasons: &mut Vec<ImpurityReason>) {
    let Some(func_node) = node.child_by_field_name("function") else {
        return;
    };

    match func_node.kind() {
        "identifier" => {
            let name = func_node.utf8_text(source).unwrap_or("");
            if IMPURE_GLOBAL_CALLS.contains(&name) {
                push_unique(reasons, ImpurityReason::CallsImpureApi(name.to_string()));
            }
        }
        "member_expression" => {
            let object_text = func_node
                .child_by_field_name("object")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("");
            let property_text = func_node
                .child_by_field_name("property")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("");

            if object_text == "console" {
                push_unique(
                    reasons,
                    ImpurityReason::CallsImpureApi(format!("console.{}", property_text)),
                );
            } else if object_text == "Math" && property_text == "random" {
                push_unique(
                    reasons,
                    ImpurityReason::CallsImpureApi("Math.random".to_string()),
                );
            } else if object_text == "Date" && property_text == "now" {
                push_unique(
                    reasons,
                    ImpurityReason::CallsImpureApi("Date.now".to_string()),
                );
            }

            if MUTATION_METHODS.contains(&property_text) {
                push_unique(
                    reasons,
                    ImpurityReason::CallsMutationMethod(property_text.to_string()),
                );
            }
        }
        _ => {}
    }
}

fn check_assignment(
    node: Node,
    source: &[u8],
    params: &HashSet<String>,
    locals: &HashSet<String>,
    reasons: &mut Vec<ImpurityReason>,
) {
    let Some(left) = node.child_by_field_name("left") else {
        return;
    };

    match left.kind() {
        "member_expression" => {
            // Detect `param.property = value` (parameter property mutation).
            if let Some(object) = left.child_by_field_name("object") {
                if object.kind() == "identifier" {
                    let obj_text = object.utf8_text(source).unwrap_or("").to_string();
                    if params.contains(&obj_text) {
                        push_unique(reasons, ImpurityReason::MutatesParameter(obj_text));
                    }
                }
            }
        }
        "identifier" => {
            // Detect writes to variables declared outside the function scope.
            let name = left.utf8_text(source).unwrap_or("").to_string();
            if !name.is_empty() && !params.contains(&name) && !locals.contains(&name) {
                push_unique(reasons, ImpurityReason::WritesToOuterScope(name));
            }
        }
        _ => {}
    }
}

// ── Parameter and local variable collection ─────────────────────────────────

fn collect_param_names(func_node: Node, source: &[u8]) -> HashSet<String> {
    let mut params = HashSet::new();
    let mut cursor = func_node.walk();
    for child in func_node.children(&mut cursor) {
        match child.kind() {
            "formal_parameters" => {
                collect_identifiers_from_formal_params(child, source, &mut params);
                break;
            }
            // Single-parameter arrow function without parentheses: `x => expr`
            "identifier" if func_node.kind() == "arrow_function" => {
                if let Ok(text) = child.utf8_text(source) {
                    params.insert(text.to_string());
                }
                break;
            }
            _ => {}
        }
    }
    params
}

fn collect_identifiers_from_formal_params(
    params_node: Node,
    source: &[u8],
    out: &mut HashSet<String>,
) {
    let mut cursor = params_node.walk();
    for child in params_node.children(&mut cursor) {
        match child.kind() {
            "required_parameter" | "optional_parameter" | "rest_parameter" => {
                if let Some(pattern) = child.child_by_field_name("pattern") {
                    collect_binding_pattern(pattern, source, out);
                }
            }
            "identifier" => {
                if let Ok(text) = child.utf8_text(source) {
                    out.insert(text.to_string());
                }
            }
            _ => {}
        }
    }
}

/// Recursively extract bound identifier names from a destructuring or plain pattern.
fn collect_binding_pattern(node: Node, source: &[u8], out: &mut HashSet<String>) {
    match node.kind() {
        "identifier" => {
            if let Ok(text) = node.utf8_text(source) {
                out.insert(text.to_string());
            }
        }
        // Skip type annotations so type names are not collected as parameter names.
        "type_annotation" => {}
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_binding_pattern(child, source, out);
            }
        }
    }
}

fn collect_local_names(func_node: Node, source: &[u8]) -> HashSet<String> {
    let mut locals = HashSet::new();
    if let Some(body) = func_node.child_by_field_name("body") {
        collect_declarations_in_scope(body, source, &mut locals);
    }
    locals
}

/// Walk a scope body and collect all locally declared variable names.
/// Does NOT recurse into nested function bodies.
fn collect_declarations_in_scope(node: Node, source: &[u8], out: &mut HashSet<String>) {
    if NESTED_FUNCTION_KINDS.contains(&node.kind()) {
        return;
    }

    if matches!(node.kind(), "lexical_declaration" | "variable_declaration") {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "variable_declarator" {
                if let Some(name_node) = child.child_by_field_name("name") {
                    collect_binding_pattern(name_node, source, out);
                }
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_declarations_in_scope(child, source, out);
    }
}

fn extract_function_name(node: Node, source: &[u8]) -> String {
    if let Some(name_node) = node.child_by_field_name("name") {
        return name_node
            .utf8_text(source)
            .unwrap_or("<anonymous>")
            .to_string();
    }
    if let Some(parent) = node.parent() {
        let name_field = match parent.kind() {
            "variable_declarator" | "public_field_definition" | "assignment_expression" => {
                parent.child_by_field_name("name")
            }
            "pair" => parent.child_by_field_name("key"),
            _ => None,
        };
        if let Some(n) = name_field {
            return n.utf8_text(source).unwrap_or("<anonymous>").to_string();
        }
    }
    "<anonymous>".to_string()
}

fn push_unique(reasons: &mut Vec<ImpurityReason>, reason: ImpurityReason) {
    if !reasons.contains(&reason) {
        reasons.push(reason);
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_typescript;

    fn analyse(src: &str) -> ModulePurityResult {
        let tree = parse_typescript(src).expect("parse failed");
        compute_module_purity(tree.root_node(), src.as_bytes(), "test.ts")
    }

    fn first_fn(result: &ModulePurityResult) -> &FunctionPurity {
        result.functions.first().expect("no functions found")
    }

    // ── Pure functions ─────────────────────────────────────────────────────────

    #[test]
    fn test_pure_math_function() {
        let src = "function add(a: number, b: number): number { return a + b; }";
        let result = analyse(src);
        assert_eq!(result.total_functions, 1);
        assert_eq!(result.pure_functions, 1);
        assert_eq!(result.ratio, 1.0);
        assert!(first_fn(&result).is_pure);
    }

    #[test]
    fn test_pure_string_function() {
        let src = r#"function greet(name: string): string { return "Hello, " + name; }"#;
        let result = analyse(src);
        assert!(first_fn(&result).is_pure);
    }

    #[test]
    fn test_pure_arrow_function_expression_body() {
        let src = "const double = (x: number): number => x * 2;";
        let result = analyse(src);
        assert_eq!(result.total_functions, 1);
        assert!(first_fn(&result).is_pure, "reasons: {:?}", first_fn(&result).reasons);
    }

    #[test]
    fn test_pure_arrow_function_block_body() {
        let src = r#"
const add = (a: number, b: number): number => {
    return a + b;
};
"#;
        let result = analyse(src);
        assert_eq!(result.total_functions, 1);
        assert!(first_fn(&result).is_pure, "reasons: {:?}", first_fn(&result).reasons);
    }

    #[test]
    fn test_empty_function_is_pure() {
        let src = "function noop() {}";
        let result = analyse(src);
        assert_eq!(result.total_functions, 1);
        assert!(first_fn(&result).is_pure);
    }

    // ── Impure: this ───────────────────────────────────────────────────────────

    #[test]
    fn test_impure_reads_this() {
        let src = r#"
function getName(): string {
    return this.name;
}
"#;
        let result = analyse(src);
        let f = first_fn(&result);
        assert!(!f.is_pure);
        assert!(f.reasons.contains(&ImpurityReason::UsesThis));
    }

    #[test]
    fn test_impure_mutates_this_property() {
        let src = r#"
function setX(): void {
    this.x = 10;
}
"#;
        let result = analyse(src);
        let f = first_fn(&result);
        assert!(!f.is_pure);
        assert!(f.reasons.contains(&ImpurityReason::UsesThis));
    }

    // ── Impure: console calls ──────────────────────────────────────────────────

    #[test]
    fn test_impure_console_log() {
        let src = r#"
function log(msg: string): void {
    console.log(msg);
}
"#;
        let result = analyse(src);
        let f = first_fn(&result);
        assert!(!f.is_pure);
        assert!(f
            .reasons
            .iter()
            .any(|r| matches!(r, ImpurityReason::CallsImpureApi(s) if s == "console.log")));
    }

    #[test]
    fn test_impure_console_warn() {
        let src = r#"function warn(msg: string): void { console.warn(msg); }"#;
        let result = analyse(src);
        assert!(!first_fn(&result).is_pure);
    }

    // ── Impure: fetch + await ──────────────────────────────────────────────────

    #[test]
    fn test_impure_await_fetch() {
        let src = r#"
async function loadData(url: string) {
    const res = await fetch(url);
    return res.json();
}
"#;
        let result = analyse(src);
        let f = first_fn(&result);
        assert!(!f.is_pure);
        assert!(f.reasons.contains(&ImpurityReason::UsesAwait));
        assert!(f
            .reasons
            .iter()
            .any(|r| matches!(r, ImpurityReason::CallsImpureApi(s) if s == "fetch")));
    }

    // ── Impure: Math.random ────────────────────────────────────────────────────

    #[test]
    fn test_impure_math_random() {
        let src = r#"
function randomValue(): number {
    return Math.random();
}
"#;
        let result = analyse(src);
        let f = first_fn(&result);
        assert!(!f.is_pure);
        assert!(f
            .reasons
            .iter()
            .any(|r| matches!(r, ImpurityReason::CallsImpureApi(s) if s == "Math.random")));
    }

    // ── Impure: Date.now ──────────────────────────────────────────────────────

    #[test]
    fn test_impure_date_now() {
        let src = r#"
function timestamp(): number {
    return Date.now();
}
"#;
        let result = analyse(src);
        let f = first_fn(&result);
        assert!(!f.is_pure);
        assert!(f
            .reasons
            .iter()
            .any(|r| matches!(r, ImpurityReason::CallsImpureApi(s) if s == "Date.now")));
    }

    // ── Impure: setTimeout / setInterval ──────────────────────────────────────

    #[test]
    fn test_impure_set_timeout() {
        let src = r#"
function delayed(cb: () => void): void {
    setTimeout(cb, 1000);
}
"#;
        let result = analyse(src);
        let f = first_fn(&result);
        assert!(!f.is_pure);
        assert!(f
            .reasons
            .iter()
            .any(|r| matches!(r, ImpurityReason::CallsImpureApi(s) if s == "setTimeout")));
    }

    #[test]
    fn test_impure_set_interval() {
        let src = r#"function poll(cb: () => void): void { setInterval(cb, 500); }"#;
        let result = analyse(src);
        assert!(!first_fn(&result).is_pure);
    }

    // ── Impure: alert / prompt ────────────────────────────────────────────────

    #[test]
    fn test_impure_alert() {
        let src = r#"function notify(msg: string): void { alert(msg); }"#;
        let result = analyse(src);
        assert!(!first_fn(&result).is_pure);
    }

    // ── Impure: parameter mutation ─────────────────────────────────────────────

    #[test]
    fn test_impure_mutates_parameter_property() {
        let src = r#"
function mutateParam(obj: { x: number }): void {
    obj.x = 42;
}
"#;
        let result = analyse(src);
        let f = first_fn(&result);
        assert!(!f.is_pure);
        assert!(f
            .reasons
            .iter()
            .any(|r| matches!(r, ImpurityReason::MutatesParameter(s) if s == "obj")));
    }

    // ── Impure: outer scope write ──────────────────────────────────────────────

    #[test]
    fn test_impure_outer_scope_write() {
        let src = r#"
let counter = 0;
function increment(): void {
    counter = counter + 1;
}
"#;
        let result = analyse(src);
        let f = result
            .functions
            .iter()
            .find(|f| f.name == "increment")
            .expect("no increment fn");
        assert!(!f.is_pure);
        assert!(f
            .reasons
            .iter()
            .any(|r| matches!(r, ImpurityReason::WritesToOuterScope(s) if s == "counter")));
    }

    #[test]
    fn test_pure_local_variable_reassignment_is_ok() {
        // Reassigning a locally declared variable is NOT an outer-scope write.
        let src = r#"
function count(): number {
    let x = 0;
    x = x + 1;
    return x;
}
"#;
        let result = analyse(src);
        let f = first_fn(&result);
        assert!(f.is_pure, "expected pure, reasons: {:?}", f.reasons);
    }

    // ── Impure: array mutation methods ────────────────────────────────────────

    #[test]
    fn test_impure_array_push() {
        let src = r#"
function addItem(arr: number[], item: number): void {
    arr.push(item);
}
"#;
        let result = analyse(src);
        let f = first_fn(&result);
        assert!(!f.is_pure);
        assert!(f
            .reasons
            .iter()
            .any(|r| matches!(r, ImpurityReason::CallsMutationMethod(s) if s == "push")));
    }

    #[test]
    fn test_impure_array_splice() {
        let src = r#"
function removeFirst(arr: number[]): void {
    arr.splice(0, 1);
}
"#;
        let result = analyse(src);
        let f = first_fn(&result);
        assert!(!f.is_pure);
        assert!(f
            .reasons
            .iter()
            .any(|r| matches!(r, ImpurityReason::CallsMutationMethod(s) if s == "splice")));
    }

    #[test]
    fn test_impure_array_sort() {
        let src = r#"
function sortInPlace(arr: number[]): void {
    arr.sort();
}
"#;
        let result = analyse(src);
        let f = first_fn(&result);
        assert!(!f.is_pure);
        assert!(f
            .reasons
            .iter()
            .any(|r| matches!(r, ImpurityReason::CallsMutationMethod(s) if s == "sort")));
    }

    // ── Impure: delete operator ────────────────────────────────────────────────

    #[test]
    fn test_impure_delete_property() {
        let src = r#"
function removeKey(obj: any): void {
    delete obj.key;
}
"#;
        let result = analyse(src);
        let f = first_fn(&result);
        assert!(!f.is_pure);
        assert!(f.reasons.contains(&ImpurityReason::UsesDeleteOperator));
    }

    #[test]
    fn test_impure_delete_index() {
        let src = r#"
function removeIndex(obj: any, key: string): void {
    delete obj[key];
}
"#;
        let result = analyse(src);
        let f = first_fn(&result);
        assert!(!f.is_pure);
        assert!(f.reasons.contains(&ImpurityReason::UsesDeleteOperator));
    }

    // ── Module-level ratio ─────────────────────────────────────────────────────

    #[test]
    fn test_module_all_pure() {
        let src = r#"
function add(a: number, b: number): number { return a + b; }
function sub(a: number, b: number): number { return a - b; }
function mul(a: number, b: number): number { return a * b; }
"#;
        let result = analyse(src);
        assert_eq!(result.total_functions, 3);
        assert_eq!(result.pure_functions, 3);
        assert_eq!(result.ratio, 1.0);
    }

    #[test]
    fn test_module_all_impure() {
        let src = r#"
function a(): void { console.log("a"); }
function b(): void { console.log("b"); }
"#;
        let result = analyse(src);
        assert_eq!(result.total_functions, 2);
        assert_eq!(result.pure_functions, 0);
        assert_eq!(result.ratio, 0.0);
    }

    #[test]
    fn test_module_mixed_ratio() {
        let src = r#"
function add(a: number, b: number): number { return a + b; }
function log(msg: string): void { console.log(msg); }
"#;
        let result = analyse(src);
        assert_eq!(result.total_functions, 2);
        assert_eq!(result.pure_functions, 1);
        assert_eq!(result.ratio, 0.5);
    }

    #[test]
    fn test_module_no_functions_ratio_is_one() {
        let src = "const x = 42;";
        let result = analyse(src);
        assert_eq!(result.total_functions, 0);
        assert_eq!(result.pure_functions, 0);
        assert_eq!(result.ratio, 1.0);
    }

    // ── Nested functions are analysed independently ────────────────────────────

    #[test]
    fn test_outer_pure_inner_impure() {
        // The outer function just defines and returns an arrow function — no direct
        // side effects of its own. The inner function calls console.log.
        let src = r#"
function outer(): () => void {
    const inner = () => { console.log("x"); };
    return inner;
}
"#;
        let result = analyse(src);
        assert_eq!(result.total_functions, 2);

        let outer = result
            .functions
            .iter()
            .find(|f| f.name == "outer")
            .expect("no outer");
        assert!(outer.is_pure, "outer should be pure; reasons: {:?}", outer.reasons);

        let inner = result
            .functions
            .iter()
            .find(|f| f.name == "inner")
            .expect("no inner");
        assert!(!inner.is_pure);
    }

    // ── impure_functions iterator ──────────────────────────────────────────────

    #[test]
    fn test_impure_functions_iterator() {
        let src = r#"
function pure(x: number): number { return x * 2; }
function impure(): void { console.log("side effect"); }
"#;
        let result = analyse(src);
        let impure_names: Vec<&str> = result.impure_functions().map(|f| f.name.as_str()).collect();
        assert_eq!(impure_names, vec!["impure"]);
    }

    // ── ImpurityReason::description() ─────────────────────────────────────────

    #[test]
    fn test_reason_descriptions_are_non_empty() {
        let reasons = [
            ImpurityReason::UsesThis,
            ImpurityReason::CallsImpureApi("console.log".to_string()),
            ImpurityReason::MutatesParameter("obj".to_string()),
            ImpurityReason::WritesToOuterScope("global".to_string()),
            ImpurityReason::UsesAwait,
            ImpurityReason::CallsMutationMethod("push".to_string()),
            ImpurityReason::UsesDeleteOperator,
        ];
        for r in &reasons {
            assert!(!r.description().is_empty(), "empty description for {:?}", r);
        }
    }
}
