use crate::structs::FunctionMetrics;

/// Thresholds that define a "Brain Method".
///
/// A function is a Brain Method when ALL three conditions hold:
///   - LOC > `loc_threshold`
///   - cyclomatic complexity > `cc_threshold`
///   - max nesting depth > `nesting_threshold`
///
/// Defaults match the reference values from *Object-Oriented Metrics in Practice*
/// (Lanza & Marinescu 2006): LOC > 65, CC > 5, nesting > 3.
#[derive(Debug, Clone)]
pub struct BrainMethodConfig {
    pub loc_threshold: usize,
    pub cc_threshold: usize,
    pub nesting_threshold: usize,
}

impl Default for BrainMethodConfig {
    fn default() -> Self {
        Self {
            loc_threshold: 65,
            cc_threshold: 5,
            nesting_threshold: 3,
        }
    }
}

/// A function detected as a Brain Method.
#[derive(Debug, Clone, PartialEq)]
pub struct BrainMethodResult {
    pub name: String,
    pub file: String,
    pub line: usize,
    pub loc: usize,
    pub cyclomatic_complexity: usize,
    pub max_nesting: usize,
}

/// Detect Brain Methods in a slice of function metrics.
///
/// Returns one `BrainMethodResult` per function that exceeds *all three*
/// configured thresholds simultaneously.
pub fn detect_brain_methods(
    functions: &[FunctionMetrics],
    config: &BrainMethodConfig,
) -> Vec<BrainMethodResult> {
    functions
        .iter()
        .filter(|f| {
            f.sloc > config.loc_threshold
                && f.cyclomatic_complexity > config.cc_threshold
                && f.max_nesting > config.nesting_threshold
        })
        .map(|f| BrainMethodResult {
            name: f.name.clone(),
            file: f.file.clone(),
            line: f.line,
            loc: f.loc,
            cyclomatic_complexity: f.cyclomatic_complexity,
            max_nesting: f.max_nesting,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_fn(
        name: &str,
        loc: usize,
        cyclomatic_complexity: usize,
        max_nesting: usize,
    ) -> FunctionMetrics {
        // sloc is always less than loc to simulate blank/comment lines.
        // This ensures tests can distinguish between the two fields.
        let sloc = if loc > 0 { loc - 1 } else { 0 };
        FunctionMetrics {
            name: name.to_string(),
            file: "test.ts".to_string(),
            line: 1,
            loc,
            sloc,
            cyclomatic_complexity,
            max_nesting,
            param_count: 0,
            ..FunctionMetrics::default()
        }
    }

    // ── Brain Method (all thresholds exceeded) ─────────────────────────────────

    #[test]
    fn test_clearly_a_brain_method() {
        let config = BrainMethodConfig::default();
        let functions = vec![make_fn("processOrders", 100, 10, 5)];
        let results = detect_brain_methods(&functions, &config);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "processOrders");
        assert_eq!(results[0].loc, 100);
        assert_eq!(results[0].cyclomatic_complexity, 10);
        assert_eq!(results[0].max_nesting, 5);
    }

    // ── Simple method — not flagged ────────────────────────────────────────────

    #[test]
    fn test_simple_method_not_flagged() {
        let config = BrainMethodConfig::default();
        let functions = vec![make_fn("getName", 5, 1, 0)];
        let results = detect_brain_methods(&functions, &config);
        assert!(results.is_empty());
    }

    // ── High CC but low LOC — not flagged ─────────────────────────────────────

    #[test]
    fn test_high_cc_low_loc_not_flagged() {
        let config = BrainMethodConfig::default();
        // CC = 10, nesting = 5, but LOC is only 20 (≤ 65)
        let functions = vec![make_fn("compactBranchy", 20, 10, 5)];
        let results = detect_brain_methods(&functions, &config);
        assert!(results.is_empty());
    }

    // ── High LOC but low CC — not flagged ─────────────────────────────────────

    #[test]
    fn test_high_loc_low_cc_not_flagged() {
        let config = BrainMethodConfig::default();
        // LOC = 100, nesting = 5, but CC = 2 (≤ 5)
        let functions = vec![make_fn("longButSimple", 100, 2, 5)];
        let results = detect_brain_methods(&functions, &config);
        assert!(results.is_empty());
    }

    // ── High LOC + high CC but low nesting — not flagged ──────────────────────

    #[test]
    fn test_high_loc_high_cc_low_nesting_not_flagged() {
        let config = BrainMethodConfig::default();
        // LOC = 100, CC = 10, but nesting = 2 (≤ 3)
        let functions = vec![make_fn("flatButComplex", 100, 10, 2)];
        let results = detect_brain_methods(&functions, &config);
        assert!(results.is_empty());
    }

    // ── Exactly at thresholds — not flagged (thresholds are strict >) ─────────

    #[test]
    fn test_exactly_at_thresholds_not_flagged() {
        let config = BrainMethodConfig::default(); // LOC>65, CC>5, nesting>3
        let functions = vec![make_fn("atThreshold", 65, 5, 3)];
        let results = detect_brain_methods(&functions, &config);
        assert!(results.is_empty());
    }

    // ── One above each threshold — flagged ─────────────────────────────────────

    #[test]
    fn test_one_above_all_thresholds_flagged() {
        let config = BrainMethodConfig::default(); // SLOC>65, CC>5, nesting>3
        // loc=67 → sloc=66, which is just above the threshold of 65
        let functions = vec![make_fn("justOver", 67, 6, 4)];
        let results = detect_brain_methods(&functions, &config);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "justOver");
    }

    // ── Mixed batch — only offending functions returned ────────────────────────

    #[test]
    fn test_mixed_batch_only_brain_methods_returned() {
        let config = BrainMethodConfig::default();
        let functions = vec![
            make_fn("brainMethod", 100, 10, 5),
            make_fn("simpleHelper", 5, 1, 0),
            make_fn("anotherBrain", 80, 8, 4),
            make_fn("highCcOnly", 20, 10, 5),
        ];
        let results = detect_brain_methods(&functions, &config);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].name, "brainMethod");
        assert_eq!(results[1].name, "anotherBrain");
    }

    // ── loc > threshold but sloc <= threshold — must NOT be flagged ───────────
    //
    // This test would fail if the filter used `f.loc` instead of `f.sloc`.

    #[test]
    fn test_high_loc_but_low_sloc_not_flagged() {
        let config = BrainMethodConfig::default(); // SLOC>65, CC>5, nesting>3
        // loc=80 exceeds the threshold (80 > 65), but sloc=60 does not (60 ≤ 65).
        // A filter on f.loc would incorrectly flag this; f.sloc must be used.
        let func = FunctionMetrics {
            name: "bloatedComments".to_string(),
            file: "test.ts".to_string(),
            line: 1,
            loc: 80,
            sloc: 60,
            cyclomatic_complexity: 10,
            max_nesting: 5,
            param_count: 0,
            ..FunctionMetrics::default()
        };
        let results = detect_brain_methods(&[func], &config);
        assert!(results.is_empty());
    }

    // ── Empty input ────────────────────────────────────────────────────────────

    #[test]
    fn test_empty_input_returns_empty() {
        let config = BrainMethodConfig::default();
        let results = detect_brain_methods(&[], &config);
        assert!(results.is_empty());
    }

    // ── Custom thresholds ──────────────────────────────────────────────────────

    #[test]
    fn test_custom_thresholds() {
        let config = BrainMethodConfig {
            loc_threshold: 10,
            cc_threshold: 2,
            nesting_threshold: 1,
        };
        // Would not trigger with defaults but triggers with stricter thresholds
        let functions = vec![make_fn("mediumMethod", 15, 3, 2)];
        let results = detect_brain_methods(&functions, &config);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "mediumMethod");
    }

    // ── Result fields are correct ──────────────────────────────────────────────

    #[test]
    fn test_result_fields_populated_correctly() {
        let config = BrainMethodConfig::default();
        let mut func = make_fn("myFunc", 100, 10, 5);
        func.file = "src/foo.ts".to_string();
        func.line = 42;
        let results = detect_brain_methods(&[func], &config);
        assert_eq!(results.len(), 1);
        let r = &results[0];
        assert_eq!(r.name, "myFunc");
        assert_eq!(r.file, "src/foo.ts");
        assert_eq!(r.line, 42);
        assert_eq!(r.loc, 100);
        assert_eq!(r.cyclomatic_complexity, 10);
        assert_eq!(r.max_nesting, 5);
    }
}
