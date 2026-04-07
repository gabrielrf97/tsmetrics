pub mod class;
pub mod file;
pub mod function;
pub mod module;
pub mod react;

use crate::structs::FileMetrics;
use function::loc::count_sloc_str;
use tree_sitter::Node;

/// Count top-level import statements in a file.
pub fn count_imports(root: Node, _source: &[u8]) -> usize {
    let mut count = 0;
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "import_statement" {
            count += 1;
        }
    }
    count
}

/// Count class declarations in a file.
pub fn count_classes(root: Node) -> usize {
    let mut count = 0;
    count_nodes_of_kind(root, "class_declaration", &mut count);
    count
}

fn count_nodes_of_kind(node: Node, kind: &str, count: &mut usize) {
    if node.kind() == kind {
        *count += 1;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        count_nodes_of_kind(child, kind, count);
    }
}

/// Helper used by tests: parse TypeScript source and compute file metrics.
#[cfg(test)]
fn parse_and_compute(source: &str, path: &str) -> FileMetrics {
    let tree = crate::parse::parse_file(source, path).expect("parse failed");
    compute_file_metrics(tree.root_node(), source.as_bytes(), path)
}

/// Compute all metrics for a parsed file.
pub fn compute_file_metrics(root: Node, source: &[u8], path: &str) -> FileMetrics {
    let source_str = std::str::from_utf8(source).unwrap_or("");

    // ── Function metrics ────────────────────────────────────────────────────
    let functions = function::extract_functions(root, source, path);

    // ── Class metrics ───────────────────────────────────────────────────────
    let mut classes = class::extract_classes(root, source, path);

    // Enrich classes with additional OO metrics (computed at file level)
    let dit_results    = class::dit::compute_dit(root, source);
    let nom_results    = class::nom::compute_class_nom(root, source);
    let tcc_results    = class::tcc::compute_class_tcc(root, source);
    let cbo_results    = class::cbo::compute_class_cbo(root, source);
    let rfc_results    = class::rfc::compute_class_rfc(root, source);
    let woc_results    = class::woc::compute_class_woc(root, source);

    for cm in &mut classes {
        // Match by class name (best-effort; unique names assumed within a file)
        if let Some(d) = dit_results.iter().find(|d| d.name == cm.name) {
            cm.dit = d.dit;
        }
        if let Some(n) = nom_results.iter().find(|n| n.class_name == cm.name) {
            cm.nom  = n.nom;
            cm.noam = n.noam;
            cm.noom = n.noom;
        }
        if let Some(t) = tcc_results.iter().find(|t| t.class_name == cm.name) {
            cm.tcc = t.tcc;
        }
        if let Some(c) = cbo_results.iter().find(|c| c.class_name == cm.name) {
            cm.cbo = c.cbo;
        }
        if let Some(r) = rfc_results.iter().find(|r| r.class_name == cm.name) {
            cm.rfc = r.rfc;
        }
        if let Some(w) = woc_results.iter().find(|w| w.class_name == cm.name) {
            cm.woc = w.woc;
        }
    }

    // ── File-level metrics ─────────────────────────────────────────────────
    let debt         = file::technical_debt::compute(source_str);
    let cohesion     = module::cohesion::compute_module_cohesion(root, source);
    let coupling     = module::coupling::compute_module_coupling(root, source, path);
    let purity       = module::purity::compute_module_purity(root, source, path);

    let total_loc  = root.end_position().row + 1;
    let total_sloc = count_sloc_str(source_str);

    let is_tsx = path.ends_with(".tsx");

    let maintainability_index = if functions.is_empty() {
        100.0
    } else {
        let total_fn_loc: usize = functions.iter().map(|f| f.loc.max(1)).sum();
        if total_fn_loc == 0 {
            100.0
        } else {
            functions.iter()
                .map(|f| f.maintainability_index * f.loc.max(1) as f64)
                .sum::<f64>()
                / total_fn_loc as f64
        }
    };

    // File-level logic MI: LOC-weighted average of function MI(L) values
    let maintainability_index_logic = {
        let total_weight: usize = functions.iter().map(|f| f.loc.max(1)).sum();
        if total_weight > 0 {
            let weighted_sum: f64 = functions
                .iter()
                .map(|f| f.maintainability_index_logic * f.loc.max(1) as f64)
                .sum();
            weighted_sum / total_weight as f64
        } else {
            100.0
        }
    };

    FileMetrics {
        path: path.to_string(),
        total_loc,
        total_sloc,
        function_count: functions.len(),
        class_count: count_classes(root),
        import_count: count_imports(root, source),
        functions,
        classes,
        tech_debt_total: debt.total,
        tech_debt_per_100_sloc: debt.per_100_sloc,
        module_cohesion: cohesion.mc,
        module_fan_out: coupling.fan_out,
        pure_fn_ratio: purity.ratio,
        maintainability_index,
        is_tsx,
        maintainability_index_logic,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    const EPSILON: f64 = 0.01;

    // ── 1. Empty file (no functions) => MI should be 100.0 ──────────────────

    #[test]
    fn empty_file_mi_is_100() {
        let source = "";
        let fm = parse_and_compute(source, "empty.ts");
        assert_eq!(fm.function_count, 0);
        assert_relative_eq!(fm.maintainability_index, 100.0, epsilon = EPSILON);
    }

    #[test]
    fn file_with_only_imports_mi_is_100() {
        let source = r#"
            import { foo } from './foo';
            import { bar } from './bar';
            const x: number = 42;
        "#;
        let fm = parse_and_compute(source, "imports_only.ts");
        assert_eq!(fm.function_count, 0);
        assert_relative_eq!(fm.maintainability_index, 100.0, epsilon = EPSILON);
    }

    // ── 2. Single function => file MI equals that function's MI ─────────────

    #[test]
    fn single_function_file_mi_equals_function_mi() {
        let source = r#"
            function add(a: number, b: number): number {
                return a + b;
            }
        "#;
        let fm = parse_and_compute(source, "single.ts");
        assert_eq!(fm.function_count, 1);
        let fn_mi = fm.functions[0].maintainability_index;
        assert_relative_eq!(fm.maintainability_index, fn_mi, epsilon = EPSILON);
    }

    #[test]
    fn single_complex_function_file_mi_equals_function_mi() {
        let source = r#"
            function process(data: number[]): number {
                let result = 0;
                for (let i = 0; i < data.length; i++) {
                    if (data[i] > 0) {
                        result += data[i] * 2;
                    } else if (data[i] < -10) {
                        result -= data[i];
                    } else {
                        result += 1;
                    }
                }
                return result;
            }
        "#;
        let fm = parse_and_compute(source, "complex_single.ts");
        assert_eq!(fm.function_count, 1);
        let fn_mi = fm.functions[0].maintainability_index;
        assert_relative_eq!(fm.maintainability_index, fn_mi, epsilon = EPSILON);
    }

    // ── 3. Multiple functions with different LOC => LOC-weighted average ─────

    #[test]
    fn multiple_functions_loc_weighted_average() {
        // We create two functions with clearly different sizes.
        // The large function should dominate the file-level MI.
        let source = r#"
            function tiny(x: number): number { return x; }

            function large(data: number[]): number {
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
        let fm = parse_and_compute(source, "multi.ts");
        assert_eq!(fm.function_count, 2);

        let tiny = &fm.functions.iter().find(|f| f.name == "tiny").unwrap();
        let large = &fm.functions.iter().find(|f| f.name == "large").unwrap();

        // Manually compute expected LOC-weighted average
        let tiny_loc = tiny.loc.max(1) as f64;
        let large_loc = large.loc.max(1) as f64;
        let expected_mi = (tiny.maintainability_index * tiny_loc
            + large.maintainability_index * large_loc)
            / (tiny_loc + large_loc);

        assert_relative_eq!(fm.maintainability_index, expected_mi, epsilon = EPSILON);

        // The file MI should be closer to large's MI since it has more LOC
        let simple_avg = (tiny.maintainability_index + large.maintainability_index) / 2.0;
        let diff_from_large = (fm.maintainability_index - large.maintainability_index).abs();
        let diff_from_simple_avg = (fm.maintainability_index - simple_avg).abs();
        assert!(
            diff_from_large < diff_from_simple_avg,
            "file MI ({:.2}) should be closer to the large function's MI ({:.2}) than the simple average ({:.2})",
            fm.maintainability_index,
            large.maintainability_index,
            simple_avg
        );
    }

    // ── 4. File with only trivial functions (LOC=1) ─────────────────────────

    #[test]
    fn trivial_one_line_functions_compute_correctly() {
        let source = r#"
            const a = () => 1;
            const b = () => 2;
            const c = () => 3;
        "#;
        let fm = parse_and_compute(source, "trivial.ts");
        assert!(fm.function_count >= 3, "expected at least 3 functions, got {}", fm.function_count);

        // All functions have LOC=1 (or very small), so the weighted average
        // should be close to the simple average.
        let total_weight: f64 = fm.functions.iter().map(|f| f.loc.max(1) as f64).sum();
        let weighted_sum: f64 = fm.functions.iter()
            .map(|f| f.maintainability_index * f.loc.max(1) as f64)
            .sum();
        let expected = weighted_sum / total_weight;

        assert_relative_eq!(fm.maintainability_index, expected, epsilon = EPSILON);
        // All trivial functions should have high MI, so file MI should be high
        assert!(
            fm.maintainability_index > 50.0,
            "trivial functions should have reasonable MI, got {:.2}",
            fm.maintainability_index
        );
    }

    #[test]
    fn trivial_functions_equal_loc_gives_simple_average() {
        // When all functions have the same LOC, weighted average = simple average
        let source = r#"
            function f1(x: number): number { return x + 1; }
            function f2(x: number): number { return x * 2; }
        "#;
        let fm = parse_and_compute(source, "equal_loc.ts");
        assert_eq!(fm.function_count, 2);

        // If both have same LOC, the file MI should be the simple average
        if fm.functions[0].loc == fm.functions[1].loc {
            let simple_avg = (fm.functions[0].maintainability_index
                + fm.functions[1].maintainability_index) / 2.0;
            assert_relative_eq!(fm.maintainability_index, simple_avg, epsilon = EPSILON);
        }
    }

    // ── maintainability_index_logic follows same pattern ─────────────────────

    #[test]
    fn empty_file_mi_logic_is_100() {
        let source = "";
        let fm = parse_and_compute(source, "empty2.ts");
        assert_relative_eq!(fm.maintainability_index_logic, 100.0, epsilon = EPSILON);
    }

    #[test]
    fn single_function_file_mi_logic_equals_function_mi_logic() {
        let source = r#"
            function add(a: number, b: number): number {
                return a + b;
            }
        "#;
        let fm = parse_and_compute(source, "single_logic.ts");
        assert_eq!(fm.function_count, 1);
        let fn_mi_logic = fm.functions[0].maintainability_index_logic;
        assert_relative_eq!(fm.maintainability_index_logic, fn_mi_logic, epsilon = EPSILON);
    }
}
