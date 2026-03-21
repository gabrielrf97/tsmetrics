//! Maintainability Index metric (S-154) — function / method level.
//!
//! Definition (SEI / Visual Studio variant, normalized to 0-100):
//!
//! ```text
//!   MI_raw = 171 - 5.2 × ln(HV) - 0.23 × CC - 16.2 × ln(LOC)
//!   MI     = max(0, (MI_raw / 171) × 100)
//! ```
//!
//! where:
//!   HV  = Halstead Volume (see `halstead.rs`)
//!   CC  = Cyclomatic Complexity (see `cyclo.rs`)
//!   LOC = Lines of Code (total, including blank/comment lines)
//!
//! Edge-case handling:
//!   - HV = 0  →  ln(HV) term is treated as 0 (no volume ⟹ no penalty).
//!   - LOC = 0 →  treat as 1 to avoid ln(0) = -∞.
//!   - MI_raw < 0 → clamped to 0 after normalization.

use tree_sitter::{Node, Parser};

use super::cyclo::cyclomatic_complexity;
use super::halstead::compute as halstead_compute;
use super::loc::count_loc;

// ── public types ─────────────────────────────────────────────────────────────

/// Maintainability Index result for a single function or method.
#[derive(Debug, Clone, PartialEq)]
pub struct MaintainabilityMetrics {
    /// Function / method name, or `"<anonymous>"` for unnamed arrow functions.
    pub name: String,
    /// Normalized Maintainability Index in the range [0, 100].
    pub mi: f64,
    /// Raw (un-normalized) MI value, may be negative for very complex functions.
    pub mi_raw: f64,
    /// Halstead Volume used in the calculation.
    pub halstead_volume: f64,
    /// Cyclomatic Complexity used in the calculation.
    pub cyclomatic_complexity: usize,
    /// Lines of Code used in the calculation.
    pub loc: usize,
}

// ── public API ────────────────────────────────────────────────────────────────

/// Parse TypeScript `source` and return Maintainability Index metrics for every
/// function declaration, function expression, arrow function,
/// generator function, or method definition found at any nesting level.
///
/// Nested functions are treated as *separate* units, consistent with the
/// Halstead and Cyclomatic Complexity implementations.
pub fn compute(source: &str) -> Vec<MaintainabilityMetrics> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .expect("Error loading TypeScript grammar");
    let tree = parser.parse(source, None).expect("Failed to parse source");

    // Collect Halstead volumes keyed by function name (first match wins for
    // duplicate names — consistent with how other metrics work).
    let halstead_results = halstead_compute(source);

    let mut results = Vec::new();
    collect_functions(
        tree.root_node(),
        source.as_bytes(),
        &halstead_results,
        &mut results,
    );
    results
}

// ── private helpers ───────────────────────────────────────────────────────────

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

fn collect_functions(
    node: Node<'_>,
    source: &[u8],
    halstead_results: &[super::halstead::FunctionHalstead],
    out: &mut Vec<MaintainabilityMetrics>,
) {
    if is_function_node(node) {
        let name = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("<anonymous>")
            .to_string();

        let loc = count_loc(node, source);
        let cc = cyclomatic_complexity(node, source);

        // Find the matching Halstead volume by name (same order as halstead::compute).
        let hv = halstead_results
            .iter()
            .find(|fh| fh.name == name)
            .map(|fh| fh.metrics.volume)
            .unwrap_or(0.0);

        let metrics = maintainability_index(&name, hv, cc, loc);
        out.push(metrics);

        // Recurse into children to find nested functions (treated separately).
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            collect_functions(child, source, halstead_results, out);
        }
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_functions(child, source, halstead_results, out);
    }
}

/// Compute the Maintainability Index for a single function given its raw inputs.
///
/// Exported for use in tests and potential future direct callers.
pub fn maintainability_index(
    name: &str,
    halstead_volume: f64,
    cc: usize,
    loc: usize,
) -> MaintainabilityMetrics {
    // Guard against ln(0): treat 0-volume as no penalty (ln term → 0).
    let ln_hv = if halstead_volume > 0.0 {
        halstead_volume.ln()
    } else {
        0.0
    };

    // Guard against ln(0) for LOC: treat empty body as 1 line.
    let effective_loc = loc.max(1) as f64;
    let ln_loc = effective_loc.ln();

    let mi_raw = 171.0 - 5.2 * ln_hv - 0.23 * cc as f64 - 16.2 * ln_loc;

    // Normalize to [0, 100].
    let mi = (mi_raw / 171.0 * 100.0).clamp(0.0, 100.0);

    MaintainabilityMetrics {
        name: name.to_string(),
        mi,
        mi_raw,
        halstead_volume,
        cyclomatic_complexity: cc,
        loc,
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    const EPSILON: f64 = 0.01;

    /// Helper: compute and return first function's metrics.
    fn first(src: &str) -> MaintainabilityMetrics {
        let results = compute(src);
        assert!(!results.is_empty(), "No function found in: {src}");
        results.into_iter().next().unwrap()
    }

    // ── formula unit tests ───────────────────────────────────────────────────

    #[test]
    fn formula_simple_values() {
        // HV=100, CC=3, LOC=20
        // MI_raw = 171 - 5.2*ln(100) - 0.23*3 - 16.2*ln(20)
        //        = 171 - 5.2*4.60517 - 0.69 - 16.2*2.99573
        //        = 171 - 23.947 - 0.69 - 48.531
        //        ≈ 97.832
        // MI = (97.832 / 171) * 100 ≈ 57.21
        let m = maintainability_index("f", 100.0, 3, 20);
        assert_relative_eq!(m.mi_raw, 97.832, epsilon = 0.01);
        assert_relative_eq!(m.mi, 57.21, epsilon = 0.01);
    }

    #[test]
    fn formula_zero_volume() {
        // HV=0 → ln(HV) term is 0
        // MI_raw = 171 - 0 - 0.23*1 - 16.2*ln(1)
        //        = 171 - 0.23 - 0
        //        = 170.77
        // MI = (170.77 / 171) * 100 ≈ 99.87
        let m = maintainability_index("f", 0.0, 1, 1);
        assert_relative_eq!(m.mi_raw, 170.77, epsilon = EPSILON);
        assert_relative_eq!(m.mi, 99.87, epsilon = EPSILON);
    }

    #[test]
    fn formula_zero_loc_uses_one() {
        // LOC=0 is clamped to 1 so ln(LOC)=0
        let m_zero = maintainability_index("f", 0.0, 1, 0);
        let m_one = maintainability_index("f", 0.0, 1, 1);
        assert_relative_eq!(m_zero.mi, m_one.mi, epsilon = 1e-9);
    }

    #[test]
    fn very_complex_function_is_clamped_to_zero() {
        // Extremely high CC and LOC → raw MI goes deeply negative → normalized to 0.
        let m = maintainability_index("f", 100_000.0, 200, 50_000);
        assert_relative_eq!(m.mi, 0.0, epsilon = 1e-9);
        assert!(m.mi_raw < 0.0, "raw MI should be negative for very complex fn");
    }

    #[test]
    fn perfect_function_cannot_exceed_100() {
        // HV=0 (no operators/operands), CC=1, LOC=1 → maximum possible MI.
        let m = maintainability_index("f", 0.0, 1, 1);
        assert!(m.mi <= 100.0, "MI must not exceed 100");
    }

    // ── AST-driven integration tests ─────────────────────────────────────────

    #[test]
    fn simple_function_has_high_mi() {
        // A short, straight-line function with no branches should have high MI.
        let src = r#"
            function add(a: number, b: number): number {
                return a + b;
            }
        "#;
        let m = first(src);
        assert!(
            m.mi >= 60.0,
            "simple function expected MI >= 60, got {:.2}",
            m.mi
        );
    }

    #[test]
    fn complex_function_has_lower_mi() {
        // A function with many branches, long body, and complex expressions.
        let src = r#"
            function processData(data: number[]): number {
                let result = 0;
                for (let i = 0; i < data.length; i++) {
                    if (data[i] > 0) {
                        if (data[i] % 2 === 0) {
                            result += data[i] * 2;
                        } else if (data[i] % 3 === 0) {
                            result += data[i] * 3;
                        } else {
                            result += data[i];
                        }
                    } else if (data[i] < -10) {
                        result -= data[i];
                    } else {
                        result += data[i] > -5 ? data[i] * -1 : 0;
                    }
                }
                return result;
            }
        "#;
        let m = first(src);
        assert!(
            m.mi < 70.0,
            "complex function expected MI < 70, got {:.2}",
            m.mi
        );
    }

    #[test]
    fn empty_function_body_returns_valid_mi() {
        // Empty function: no operators, one operand (name), 1 LOC → near-maximum MI.
        let src = "function noop() {}";
        let m = first(src);
        assert!(m.mi >= 90.0, "empty function expected MI >= 90, got {:.2}", m.mi);
        assert!(m.mi <= 100.0);
    }

    #[test]
    fn single_line_function_has_high_mi() {
        let src = "const double = (x: number) => x * 2;";
        let m = first(src);
        assert!(
            m.mi >= 60.0,
            "single-line fn expected MI >= 60, got {:.2}",
            m.mi
        );
    }

    #[test]
    fn name_is_reported_correctly() {
        let src = "function myFunc(x: number): number { return x + 1; }";
        let m = first(src);
        assert_eq!(m.name, "myFunc");
    }

    #[test]
    fn multiple_functions_all_collected() {
        let src = r#"
            function foo() { return 1; }
            function bar() { return 2; }
        "#;
        let results = compute(src);
        assert_eq!(results.len(), 2);
        let names: Vec<&str> = results.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"foo"));
        assert!(names.contains(&"bar"));
    }

    #[test]
    fn mi_fields_are_consistent() {
        let src = "function f(x: number): number { return x * x; }";
        let m = first(src);
        // Recompute expected MI_raw from fields.
        let ln_hv = if m.halstead_volume > 0.0 { m.halstead_volume.ln() } else { 0.0 };
        let ln_loc = (m.loc.max(1) as f64).ln();
        let expected_raw = 171.0 - 5.2 * ln_hv - 0.23 * m.cyclomatic_complexity as f64 - 16.2 * ln_loc;
        assert_relative_eq!(m.mi_raw, expected_raw, epsilon = 1e-9);
        let expected_mi = (expected_raw / 171.0 * 100.0).clamp(0.0, 100.0);
        assert_relative_eq!(m.mi, expected_mi, epsilon = 1e-9);
    }
}
