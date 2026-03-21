//! Technical Debt metric (S-163) — file level.
//!
//! Definition (DCM approach):
//!
//! For each function/method in the file:
//! ```text
//!   debt_f = max(0, 1 − MI_f / 100) × √(HV_f + 1)
//! ```
//!
//! Aggregated to file level:
//! ```text
//!   total_debt   = Σ debt_f
//!   per_100_sloc = total_debt / max(1, SLOC) × 100
//! ```
//!
//! Where:
//!   MI_f  = Maintainability Index of function f (normalized, 0–100).
//!   HV_f  = Halstead Volume of function f.
//!   SLOC  = Source Lines of Code in the file (blank/comment lines excluded).
//!
//! Rationale:
//!   - `(1 − MI / 100)` represents the fraction of quality lost relative to a
//!     perfect (MI = 100) function; it is zero when MI ≥ 100 and one when MI = 0.
//!   - `√(HV + 1)` scales the penalty by the effort needed to comprehend the
//!     function, preventing tiny functions from dominating the score.
//!   - Summing across all functions gives a file-level cost signal.
//!   - Normalizing by SLOC enables fair comparison across files of different sizes.
//!
//! Files without any functions (e.g. pure type declaration files) return a
//! zero debt score, which is correct: there is no procedural logic to become
//! a maintenance burden.

use crate::metrics::function::{loc::count_sloc_str, maintainability::compute as mi_compute};

// ── public types ──────────────────────────────────────────────────────────────

/// Technical Debt contribution of a single function or method.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDebt {
    /// Function / method name, or `"<anonymous>"` for unnamed arrow functions.
    pub name: String,
    /// Technical debt score for this function (raw, unbounded above).
    pub debt: f64,
    /// Maintainability Index used in the calculation (0–100).
    pub maintainability_index: f64,
    /// Halstead Volume used in the calculation.
    pub halstead_volume: f64,
}

/// File-level Technical Debt result.
#[derive(Debug, Clone, PartialEq)]
pub struct FileTechnicalDebt {
    /// Sum of per-function debt scores.
    pub total: f64,
    /// Total debt normalized per 100 source lines of code.
    pub per_100_sloc: f64,
    /// Per-function breakdown, in source order.
    pub functions: Vec<FunctionDebt>,
}

// ── public API ────────────────────────────────────────────────────────────────

/// Parse TypeScript `source` and compute the file-level Technical Debt score.
///
/// Files with no function definitions return a zero-debt result.
pub fn compute(source: &str) -> FileTechnicalDebt {
    let mi_results = mi_compute(source);
    let sloc = count_sloc_str(source);

    let functions: Vec<FunctionDebt> = mi_results
        .into_iter()
        .map(|m| {
            let debt = function_debt(m.mi, m.halstead_volume);
            FunctionDebt {
                name: m.name,
                debt,
                maintainability_index: m.mi,
                halstead_volume: m.halstead_volume,
            }
        })
        .collect();

    let total: f64 = functions.iter().map(|f| f.debt).sum();
    let per_100_sloc = total / sloc.max(1) as f64 * 100.0;

    FileTechnicalDebt {
        total,
        per_100_sloc,
        functions,
    }
}

// ── private helpers ───────────────────────────────────────────────────────────

/// Compute the technical debt contribution of a single function.
///
/// `mi`              — Maintainability Index in [0, 100].
/// `halstead_volume` — Halstead Volume (≥ 0).
pub fn function_debt(mi: f64, halstead_volume: f64) -> f64 {
    let deficit = (1.0 - mi / 100.0).max(0.0);
    deficit * (halstead_volume + 1.0).sqrt()
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    const EPSILON: f64 = 1e-9;

    // ── unit tests for function_debt formula ─────────────────────────────────

    #[test]
    fn perfect_function_has_zero_debt() {
        // MI = 100 → deficit = 0 → debt = 0, regardless of HV.
        assert_relative_eq!(function_debt(100.0, 0.0), 0.0, epsilon = EPSILON);
        assert_relative_eq!(function_debt(100.0, 500.0), 0.0, epsilon = EPSILON);
        assert_relative_eq!(function_debt(100.0, 1_000_000.0), 0.0, epsilon = EPSILON);
    }

    #[test]
    fn zero_mi_debt_equals_sqrt_hv_plus_one() {
        // MI = 0 → deficit = 1 → debt = √(HV + 1).
        assert_relative_eq!(function_debt(0.0, 0.0), 1.0, epsilon = EPSILON);
        assert_relative_eq!(function_debt(0.0, 3.0), 2.0, epsilon = EPSILON);
        assert_relative_eq!(function_debt(0.0, 99.0), 10.0, epsilon = EPSILON);
        assert_relative_eq!(function_debt(0.0, 24.0), 5.0, epsilon = EPSILON);
    }

    #[test]
    fn half_maintainability_half_debt() {
        // MI = 50 → deficit = 0.5 → debt = 0.5 × √(HV + 1).
        assert_relative_eq!(function_debt(50.0, 3.0), 1.0, epsilon = EPSILON);
        assert_relative_eq!(function_debt(50.0, 99.0), 5.0, epsilon = EPSILON);
    }

    #[test]
    fn mi_above_100_is_clamped_to_zero_debt() {
        // Clamp ensures MI > 100 still gives zero debt.
        assert_relative_eq!(function_debt(110.0, 500.0), 0.0, epsilon = EPSILON);
    }

    #[test]
    fn debt_increases_with_lower_mi() {
        // Monotonicity: lower MI → more debt.
        let hv = 100.0;
        let d90 = function_debt(90.0, hv);
        let d50 = function_debt(50.0, hv);
        let d10 = function_debt(10.0, hv);
        assert!(d90 < d50, "debt at MI=90 ({d90:.4}) should be < MI=50 ({d50:.4})");
        assert!(d50 < d10, "debt at MI=50 ({d50:.4}) should be < MI=10 ({d10:.4})");
    }

    #[test]
    fn debt_increases_with_higher_hv() {
        // Monotonicity: higher HV → more debt.
        let mi = 50.0;
        let d10 = function_debt(mi, 10.0);
        let d100 = function_debt(mi, 100.0);
        let d1000 = function_debt(mi, 1000.0);
        assert!(d10 < d100, "debt at HV=10 ({d10:.4}) should be < HV=100 ({d100:.4})");
        assert!(d100 < d1000, "debt at HV=100 ({d100:.4}) should be < HV=1000 ({d1000:.4})");
    }

    // ── integration tests (compute from TypeScript source) ───────────────────

    /// Helper: compute and return the single function's debt (panics if not exactly one).
    fn only_fn(src: &str) -> FunctionDebt {
        let result = compute(src);
        assert_eq!(
            result.functions.len(),
            1,
            "expected exactly one function, got {}: {src}",
            result.functions.len()
        );
        result.functions.into_iter().next().unwrap()
    }

    #[test]
    fn empty_file_has_zero_debt() {
        let result = compute("");
        assert_relative_eq!(result.total, 0.0, epsilon = EPSILON);
        assert_relative_eq!(result.per_100_sloc, 0.0, epsilon = EPSILON);
        assert!(result.functions.is_empty());
    }

    #[test]
    fn file_with_no_functions_has_zero_debt() {
        let src = r#"
            const PI = 3.14159;
            type Point = { x: number; y: number };
            interface Shape { area(): number; }
        "#;
        let result = compute(src);
        assert_relative_eq!(result.total, 0.0, epsilon = EPSILON);
    }

    #[test]
    fn simple_function_has_low_debt() {
        // A minimal, straight-line function should have very low debt.
        let src = "function add(a: number, b: number): number { return a + b; }";
        let result = compute(src);
        assert!(
            result.total < 5.0,
            "simple function debt should be low, got {:.4}",
            result.total
        );
    }

    #[test]
    fn complex_function_has_higher_debt_than_simple() {
        let simple = r#"function identity(x: number): number { return x; }"#;
        let complex = r#"
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
        let simple_debt = compute(simple).total;
        let complex_debt = compute(complex).total;
        assert!(
            complex_debt > simple_debt,
            "complex fn debt {complex_debt:.4} should exceed simple fn debt {simple_debt:.4}"
        );
    }

    #[test]
    fn function_name_is_preserved() {
        let src = "function myFunc(x: number): number { return x + 1; }";
        let fd = only_fn(src);
        assert_eq!(fd.name, "myFunc");
    }

    #[test]
    fn anonymous_arrow_function_name() {
        let src = "const fn = () => 42;";
        let fd = only_fn(src);
        assert_eq!(fd.name, "<anonymous>");
    }

    #[test]
    fn function_debt_field_matches_formula() {
        // Verify the stored FunctionDebt.debt equals function_debt(mi, hv).
        let src = "function f(a: number, b: number): number { return a * b + a; }";
        let fd = only_fn(src);
        let expected = function_debt(fd.maintainability_index, fd.halstead_volume);
        assert_relative_eq!(fd.debt, expected, epsilon = EPSILON);
    }

    #[test]
    fn total_equals_sum_of_function_debts() {
        let src = r#"
            function foo() { return 1; }
            function bar(x: number, y: number) { return x > y ? x : y; }
        "#;
        let result = compute(src);
        let sum: f64 = result.functions.iter().map(|f| f.debt).sum();
        assert_relative_eq!(result.total, sum, epsilon = EPSILON);
    }

    #[test]
    fn per_100_sloc_is_normalized_total() {
        let src = r#"
            function alpha() { return 1; }
            function beta(x: number) { return x * 2; }
        "#;
        let result = compute(src);

        let sloc = crate::metrics::function::loc::count_sloc_str(src) as f64;
        let expected = result.total / sloc.max(1.0) * 100.0;
        assert_relative_eq!(result.per_100_sloc, expected, epsilon = EPSILON);
    }

    #[test]
    fn multiple_functions_all_collected() {
        let src = r#"
            function a() { return 1; }
            function b() { return 2; }
            function c() { return 3; }
        "#;
        let result = compute(src);
        assert_eq!(result.functions.len(), 3);
    }

    #[test]
    fn more_functions_can_raise_total_debt() {
        // Adding an extra function with some debt raises the total.
        let one_fn = "function foo() { return 1; }";
        let two_fns = r#"
            function foo() { return 1; }
            function bar(a: number, b: number, c: number) {
                if (a > b) { return a + c; }
                else if (b > c) { return b - a; }
                return c * a + b;
            }
        "#;
        let debt1 = compute(one_fn).total;
        let debt2 = compute(two_fns).total;
        assert!(
            debt2 >= debt1,
            "two-fn debt {debt2:.4} should be >= one-fn debt {debt1:.4}"
        );
    }

    #[test]
    fn mi_and_hv_fields_are_non_negative() {
        let src = r#"
            function complex(data: number[]): number {
                let sum = 0;
                for (const x of data) { if (x > 0) sum += x; }
                return sum;
            }
        "#;
        let result = compute(src);
        for fd in &result.functions {
            assert!(
                fd.maintainability_index >= 0.0,
                "MI must be >= 0, got {}",
                fd.maintainability_index
            );
            assert!(
                fd.halstead_volume >= 0.0,
                "HV must be >= 0, got {}",
                fd.halstead_volume
            );
        }
    }
}
