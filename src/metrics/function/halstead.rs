//! Halstead Volume metric (S-153) — function / method level.
//!
//! Definition (Lanza & Marinescu, *Object-Oriented Metrics in Practice*):
//!
//! ```text
//!   η   = η₁ + η₂          (vocabulary)
//!   N   = N₁ + N₂          (program length)
//!   V   = N × log₂(η)      (volume)
//! ```
//!
//! where η₁ = distinct operators, η₂ = distinct operands,
//!       N₁ = total operators,    N₂ = total operands.
//!
//! Operands are identifiers and literals.
//! Operators are binary/unary/assignment symbols and keyword operators
//! (`return`, `if`, `new`, …).  Structural punctuation (`(`, `)`, `{`, `}`,
//! `,`, `;`) is ignored.

use std::collections::HashMap;
use tree_sitter::{Node, Parser};

// ── token classification ─────────────────────────────────────────────────────

/// Anonymous leaf-node kinds that count as **operators**.
static OPERATOR_KINDS: &[&str] = &[
    // arithmetic
    "+", "-", "*", "/", "%", "**",
    // equality
    "===", "!==", "==", "!=",
    // comparison
    "<", ">", "<=", ">=",
    // logical
    "&&", "||", "??", "!",
    // bitwise
    "&", "|", "^", "~", "<<", ">>", ">>>",
    // assignment
    "=", "+=", "-=", "*=", "/=", "%=", "**=",
    "&&=", "||=", "??=", "&=", "|=", "^=", "<<=", ">>=", ">>>=",
    // update
    "++", "--",
    // member access
    ".", "?.",
    // spread / rest
    "...",
    // arrow function
    "=>",
    // control flow keywords
    "if", "else", "while", "for", "in", "of",
    "switch", "case", "default",
    // keyword operators
    "return", "new", "typeof", "instanceof", "delete", "void",
    // async / generator
    "await", "yield",
    // jump
    "throw", "break", "continue",
    // ternary (`?` only — `:` skipped to avoid type-annotation colons)
    "?",
];

/// Named node kinds that count as **operands**.
static OPERAND_KINDS: &[&str] = &[
    "identifier",
    "property_identifier",
    "shorthand_property_identifier",
    "shorthand_property_identifier_pattern",
    "private_property_identifier",
    // literals
    "number",
    "string",
    "template_string",
    "true",
    "false",
    "null",
    "regex",
    // special
    "this",
    "super",
];

// ── public types ─────────────────────────────────────────────────────────────

/// Halstead metrics for a single function or method.
#[derive(Debug, Clone, PartialEq)]
pub struct HalsteadMetrics {
    /// η₁ — number of distinct operators.
    pub distinct_operators: usize,
    /// η₂ — number of distinct operands.
    pub distinct_operands: usize,
    /// N₁ — total operator occurrences.
    pub total_operators: usize,
    /// N₂ — total operand occurrences.
    pub total_operands: usize,
    /// η = η₁ + η₂ — vocabulary.
    pub vocabulary: usize,
    /// N = N₁ + N₂ — program length.
    pub length: usize,
    /// V = N × log₂(η) — program volume.
    ///
    /// Returns `0.0` when vocabulary ≤ 1 (log₂ undefined / zero).
    pub volume: f64,
}

impl HalsteadMetrics {
    fn from_maps(operators: &HashMap<String, usize>, operands: &HashMap<String, usize>) -> Self {
        let distinct_operators = operators.len();
        let distinct_operands = operands.len();
        let total_operators: usize = operators.values().sum();
        let total_operands: usize = operands.values().sum();
        let vocabulary = distinct_operators + distinct_operands;
        let length = total_operators + total_operands;
        let volume = if vocabulary <= 1 {
            0.0
        } else {
            length as f64 * (vocabulary as f64).log2()
        };
        HalsteadMetrics {
            distinct_operators,
            distinct_operands,
            total_operators,
            total_operands,
            vocabulary,
            length,
            volume,
        }
    }
}

/// Halstead result for one function: its name (or `"<anonymous>"`) and metrics.
#[derive(Debug, Clone)]
pub struct FunctionHalstead {
    /// Function / method name, or `"<anonymous>"` for unnamed arrow functions.
    pub name: String,
    /// Computed Halstead metrics.
    pub metrics: HalsteadMetrics,
}

// ── public API ────────────────────────────────────────────────────────────────

/// Compute Halstead metrics for a single already-parsed function `node`.
///
/// Unlike [`compute`], this does not re-parse the source — it uses the AST
/// node directly.  Tokens belonging to nested functions are excluded so that
/// each function is measured independently.
pub fn compute_for_node(node: Node, source: &[u8]) -> HalsteadMetrics {
    let mut operators: HashMap<String, usize> = HashMap::new();
    let mut operands: HashMap<String, usize> = HashMap::new();
    walk(node, source, &mut operators, &mut operands, 0);
    HalsteadMetrics::from_maps(&operators, &operands)
}

/// Parse TypeScript `source` and return Halstead metrics for every
/// function declaration, function expression, arrow function,
/// generator function, or method definition found at any nesting level.
///
/// Nested functions are treated as *separate* units; their tokens do **not**
/// contribute to the enclosing function's metrics.
pub fn compute(source: &str) -> Vec<FunctionHalstead> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .expect("Error loading TypeScript grammar");
    let tree = parser.parse(source, None).expect("Failed to parse source");
    let mut results = Vec::new();
    collect_functions(tree.root_node(), source.as_bytes(), &mut results);
    results
}

// ── private helpers ───────────────────────────────────────────────────────────

/// Returns `true` for **named** AST nodes that introduce a function scope.
///
/// The `node.is_named()` guard is critical: the anonymous `"function"` keyword
/// token inside every `function_declaration` also has kind `"function"`, so
/// without the guard it would be mistakenly treated as a function expression.
fn is_function_node(node: Node<'_>) -> bool {
    node.is_named()
        && matches!(
            node.kind(),
            "function_declaration"
                | "function"
                | "arrow_function"
                | "generator_function"
                | "generator_function_declaration"
                | "method_definition"
        )
}

/// Collect one `FunctionHalstead` per function node, then recurse into
/// siblings/children but **not** into nested function bodies (they become
/// their own entries).
fn collect_functions(node: Node<'_>, source: &[u8], results: &mut Vec<FunctionHalstead>) {
    if is_function_node(node) {
        let name = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("<anonymous>")
            .to_string();

        let mut operators: HashMap<String, usize> = HashMap::new();
        let mut operands: HashMap<String, usize> = HashMap::new();
        walk(node, source, &mut operators, &mut operands, 0);

        results.push(FunctionHalstead {
            name,
            metrics: HalsteadMetrics::from_maps(&operators, &operands),
        });

        // Recurse into children to find *nested* functions (treated separately).
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            collect_functions(child, source, results);
        }
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_functions(child, source, results);
    }
}

/// Recursively walk `node`, accumulating operator and operand counts.
///
/// `depth` is 0 for the root function node itself; children increment it.
/// Nested function-scope nodes at `depth > 0` are skipped so that their
/// tokens are not double-counted inside the enclosing function.
fn walk(
    node: Node<'_>,
    source: &[u8],
    operators: &mut HashMap<String, usize>,
    operands: &mut HashMap<String, usize>,
    depth: usize,
) {
    let kind = node.kind();

    // Stop at nested function boundaries (depth > 0 means we already entered
    // the root function; a new named function node would be a separate scope).
    if depth > 0 && is_function_node(node) {
        return;
    }

    // Operator — anonymous leaf token (e.g., "+", "return", "if").
    if OPERATOR_KINDS.contains(&kind) {
        *operators.entry(kind.to_string()).or_insert(0) += 1;
        return; // operators are atomic; no need to recurse
    }

    // Operand — named node representing a value or name.
    if OPERAND_KINDS.contains(&kind) {
        let text = node.utf8_text(source).unwrap_or(kind).to_string();
        *operands.entry(text).or_insert(0) += 1;
        return; // treat the whole node as a single operand token
    }

    // Otherwise, recurse into children.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, source, operators, operands, depth + 1);
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    /// Convenience: compute and return the first function's metrics.
    fn first(src: &str) -> HalsteadMetrics {
        let results = compute(src);
        assert!(!results.is_empty(), "No function found in: {src}");
        results.into_iter().next().unwrap().metrics
    }

    // ── trivial / edge cases ─────────────────────────────────────────────────

    #[test]
    fn empty_function_has_zero_volume() {
        // noop() has one operand ("noop"), zero operators → vocabulary = 1
        // log₂(1) = 0 → volume = 0.
        let m = first("function noop() {}");
        assert_eq!(m.total_operators, 0, "no operators in empty fn");
        assert_eq!(m.distinct_operands, 1, "only 'noop' as operand");
        assert_relative_eq!(m.volume, 0.0, epsilon = 1e-9);
    }

    #[test]
    fn function_with_only_return_has_positive_volume() {
        // function f() { return 1; }
        // Operators: return(×1)  → η₁=1, N₁=1
        // Operands:  f(×1), 1(×1) → η₂=2, N₂=2
        // η=3, N=3, V = 3×log₂(3)
        let m = first("function f() { return 1; }");
        assert_eq!(m.distinct_operators, 1);
        assert_eq!(m.distinct_operands, 2);
        assert_eq!(m.total_operators, 1);
        assert_eq!(m.total_operands, 2);
        assert_eq!(m.vocabulary, 3);
        assert_eq!(m.length, 3);
        assert_relative_eq!(m.volume, 3.0 * 3_f64.log2(), epsilon = 1e-9);
    }

    // ── named function declarations ─────────────────────────────────────────

    #[test]
    fn simple_addition() {
        // function add(a, b) { return a + b; }
        // Operators: return(×1), +(×1)  → η₁=2, N₁=2
        // Operands:  add(×1), a(×2), b(×2) → η₂=3, N₂=5
        // η=5, N=7, V=7×log₂(5)
        let m = first("function add(a, b) { return a + b; }");
        assert_eq!(m.distinct_operators, 2, "return and +");
        assert_eq!(m.distinct_operands, 3, "add, a, b");
        assert_eq!(m.total_operators, 2);
        assert_eq!(m.total_operands, 5);
        assert_eq!(m.vocabulary, 5);
        assert_eq!(m.length, 7);
        assert_relative_eq!(m.volume, 7.0 * 5_f64.log2(), epsilon = 1e-9);
    }

    #[test]
    fn factorial_recursive() {
        // function factorial(n) {
        //   if (n <= 1) { return 1; }
        //   return n * factorial(n - 1);
        // }
        // Operators: if(×1), <=(×1), return(×2), *(×1), -(×1) → η₁=5, N₁=6
        // Operands:  factorial(fn-name + call=×2), n(param + cond + body + arg=×4), 1(cond + ret + arg=×3)
        //            → η₂=3, N₂=9
        // η=5+3=8, N=6+9=15, V=15×log₂(8)=15×3=45
        let src = "function factorial(n) { if (n <= 1) { return 1; } return n * factorial(n - 1); }";
        let m = first(src);
        assert_eq!(m.distinct_operators, 5, "if <= return * -");
        assert_eq!(m.distinct_operands, 3, "factorial n 1");
        assert_eq!(m.total_operators, 6);
        assert_eq!(m.total_operands, 9);
        assert_relative_eq!(m.volume, 45.0, epsilon = 1e-6);
    }

    #[test]
    fn function_name_reported_correctly() {
        let results = compute("function greet(name) { return name; }");
        assert_eq!(results[0].name, "greet");
    }

    // ── arrow functions ─────────────────────────────────────────────────────

    #[test]
    fn arrow_function_concise_body() {
        // const multiply = (x, y) => x * y;
        // Arrow function node starts at "arrow_function":
        // Operators: =>(×1), *(×1)  → η₁=2, N₁=2
        // Operands:  x(×2),  y(×2)  → η₂=2, N₂=4
        // η=4, N=6, V=6×log₂(4)=12
        let src = "const multiply = (x, y) => x * y;";
        let results = compute(src);
        assert!(!results.is_empty(), "arrow function should be found");
        let m = &results[0].metrics;
        assert_eq!(m.distinct_operators, 2, "=> and *");
        assert_eq!(m.distinct_operands, 2, "x and y");
        assert_eq!(m.total_operators, 2);
        assert_eq!(m.total_operands, 4);
        assert_relative_eq!(m.volume, 12.0, epsilon = 1e-9);
    }

    #[test]
    fn arrow_function_block_body() {
        // const square = (n) => { return n * n; };
        // Operators: =>(×1), return(×1), *(×1) → η₁=3, N₁=3
        // Operands:  n(×3) → η₂=1, N₂=3 ... wait
        // Actually n appears in params (×1) and body (×2) = 3 total.
        // η=4, N=6, V=6×log₂(4)=12
        let src = "const square = (n) => { return n * n; };";
        let results = compute(src);
        assert!(!results.is_empty());
        let m = &results[0].metrics;
        assert_eq!(m.distinct_operands, 1, "only 'n'");
        assert!(m.volume > 0.0);
    }

    #[test]
    fn arrow_function_anonymous_name() {
        let src = "const fn = () => 42;";
        let results = compute(src);
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "<anonymous>");
    }

    // ── class methods ────────────────────────────────────────────────────────

    #[test]
    fn class_method_metrics() {
        // class C { add(a, b) { return a + b; } }
        // Same token set as function add(a,b) { return a+b; }
        // Operators: return(×1), +(×1) → η₁=2, N₁=2
        // Operands:  add(×1), a(×2), b(×2) → η₂=3, N₂=5
        // V = 7×log₂(5)
        let src = "class C { add(a, b) { return a + b; } }";
        let results = compute(src);
        assert!(!results.is_empty());
        let m = &results[0].metrics;
        assert_eq!(m.distinct_operators, 2);
        assert_eq!(m.distinct_operands, 3);
        assert_relative_eq!(m.volume, 7.0 * 5_f64.log2(), epsilon = 1e-9);
    }

    #[test]
    fn class_method_name_reported() {
        let src = "class Calculator { multiply(x, y) { return x * y; } }";
        let results = compute(src);
        assert_eq!(results[0].name, "multiply");
    }

    // ── generator functions ─────────────────────────────────────────────────

    #[test]
    fn generator_function_counted() {
        let src = "function* counter() { yield 1; yield 2; }";
        let m = first(src);
        // Operators: *(from function*, kind "*" matches arithmetic *)×1, yield×2
        //            → η₁=2, N₁=3
        // Operands:  counter(×1), 1(×1), 2(×1) → η₂=3, N₂=3
        // Note: the "*" in "function*" is a known token-level simplification —
        // tree-sitter emits it as kind "*", indistinguishable from multiply.
        assert_eq!(m.distinct_operators, 2, "* (generator) and yield");
        assert_eq!(m.total_operators, 3, "*(×1) + yield(×2)");
        assert_eq!(m.distinct_operands, 3);
        assert!(m.volume > 0.0);
    }

    // ── multiple functions ────────────────────────────────────────────────────

    #[test]
    fn multiple_functions_all_collected() {
        let src = "function a() { return 1; } function b() { return 2; }";
        let results = compute(src);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].name, "a");
        assert_eq!(results[1].name, "b");
    }

    #[test]
    fn nested_function_produces_separate_entry() {
        let src = r#"
            function outer(x) {
                function inner(y) { return y + 1; }
                return inner(x);
            }
        "#;
        let results = compute(src);
        // Both outer and inner should be entries.
        assert_eq!(results.len(), 2);
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"outer"));
        assert!(names.contains(&"inner"));
    }

    #[test]
    fn nested_tokens_not_double_counted() {
        // The "return" and "1" inside inner should NOT inflate outer's metrics.
        let src = r#"
            function outer() {
                function inner() { return 1; }
                return 2;
            }
        "#;
        let results = compute(src);
        let outer = results.iter().find(|r| r.name == "outer").unwrap();
        let inner = results.iter().find(|r| r.name == "inner").unwrap();

        // outer has: return(×1), 2(×1), outer(×1)  — NOT inner's tokens
        assert_eq!(outer.metrics.total_operators, 1, "outer has one return");
        // inner has: return(×1), inner(×1), 1(×1)
        assert_eq!(inner.metrics.total_operators, 1, "inner has one return");
    }

    // ── monotonicity ─────────────────────────────────────────────────────────

    #[test]
    fn more_complex_function_has_higher_volume() {
        let simple = first("function f() { return 1; }");
        let complex = first(
            "function g(a, b, c) { if (a > 0) { return a + b; } return b * c; }",
        );
        assert!(
            complex.volume > simple.volume,
            "complex V={} should exceed simple V={}",
            complex.volume,
            simple.volume
        );
    }

    // ── Halstead invariant ────────────────────────────────────────────────────

    #[test]
    fn vocabulary_equals_eta1_plus_eta2() {
        let m = first("function add(a, b) { return a + b; }");
        assert_eq!(m.vocabulary, m.distinct_operators + m.distinct_operands);
    }

    #[test]
    fn length_equals_n1_plus_n2() {
        let m = first("function add(a, b) { return a + b; }");
        assert_eq!(m.length, m.total_operators + m.total_operands);
    }

    #[test]
    fn volume_formula_holds() {
        let m = first("function add(a, b) { return a + b; }");
        let expected = m.length as f64 * (m.vocabulary as f64).log2();
        assert_relative_eq!(m.volume, expected, epsilon = 1e-9);
    }
}
