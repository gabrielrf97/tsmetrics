//! God Class detection strategy.
//!
//! A God Class is a class that has grown too large and too central,
//! accumulating data and behaviour that belongs elsewhere.  Three metrics
//! are combined to flag it:
//!
//! - **WMC** (Weighted Methods per Class) > 47  — complex enough to be a brain
//! - **TCC** (Tight Class Cohesion) < 0.33      — low internal cohesion;
//!   the class is not a cohesive unit
//! - **ATFD** (Access To Foreign Data) > 5      — excessively dependent on
//!   other classes' data
//!
//! Reference: *Object-Oriented Metrics in Practice*, Lanza & Marinescu 2006.

use tree_sitter::Node;

use crate::metrics::class::tcc::compute_tcc_for_class;
use crate::metrics::class::wmc::{compute_wmc, extract_class_name};

/// WMC threshold: a God Class must have strictly more than this many weighted methods.
pub const WMC_THRESHOLD: usize = 47;

/// TCC threshold: a God Class must have strictly less cohesion than this.
pub const TCC_THRESHOLD: f64 = 0.33;

/// ATFD threshold: a God Class must access strictly more foreign data items than this.
pub const ATFD_THRESHOLD: usize = 5;

/// Thresholds that define a "God Class".
///
/// A class is a God Class when ALL three conditions hold:
///   - WMC > `wmc_threshold`
///   - TCC < `tcc_threshold`
///   - ATFD > `atfd_threshold`
///
/// Defaults match the reference values from *Object-Oriented Metrics in Practice*
/// (Lanza & Marinescu 2006): WMC > 47, TCC < 0.33, ATFD > 5.
#[derive(Debug, Clone)]
pub struct GodClassConfig {
    pub wmc_threshold: usize,
    pub tcc_threshold: f64,
    pub atfd_threshold: usize,
}

impl Default for GodClassConfig {
    fn default() -> Self {
        Self {
            wmc_threshold: WMC_THRESHOLD,
            tcc_threshold: TCC_THRESHOLD,
            atfd_threshold: ATFD_THRESHOLD,
        }
    }
}

/// A class detected as a God Class.
#[derive(Debug, Clone)]
pub struct GodClassResult {
    pub class_name: String,
    /// 1-based line where the class is declared.
    pub line: usize,
    /// Weighted Methods per Class.
    pub wmc: usize,
    /// Tight Class Cohesion.
    pub tcc: f64,
    /// Access To Foreign Data.
    pub atfd: usize,
}

/// Detect God Classes in all classes declared under `root`.
///
/// Returns one `GodClassResult` per class that exceeds *all three* configured
/// thresholds simultaneously.  Both `class_declaration` and
/// `abstract_class_declaration` nodes are considered.
pub fn detect_god_classes(
    root: Node,
    source: &[u8],
    config: &GodClassConfig,
) -> Vec<GodClassResult> {
    let mut results = Vec::new();
    collect_god_classes(root, source, config, &mut results);
    results
}

fn collect_god_classes(
    node: Node,
    source: &[u8],
    config: &GodClassConfig,
    out: &mut Vec<GodClassResult>,
) {
    let is_class = matches!(
        node.kind(),
        "class_declaration" | "abstract_class_declaration"
    ) || (node.kind() == "class" && node.child_by_field_name("body").is_some());

    if is_class {
        let class_name = extract_class_name(node, source);
        let wmc = compute_wmc(node, source);
        let tcc = compute_tcc_for_class(node, source);
        let atfd = compute_atfd(node, source);

        if wmc > config.wmc_threshold && tcc < config.tcc_threshold && atfd > config.atfd_threshold
        {
            out.push(GodClassResult {
                class_name,
                line: node.start_position().row + 1,
                wmc,
                tcc,
                atfd,
            });
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_god_classes(child, source, config, out);
    }
}

/// Compute ATFD (Access To Foreign Data) for a class node.
///
/// ATFD counts member expressions inside the class body's methods where the
/// base object is a plain identifier that is neither `this` nor `super`.
///
/// Only the *base* access is counted to avoid double-counting chains:
/// `a.b.c` contributes 1 (for `a.b`), not 2.
///
/// Examples counted:    `repo.data`, `svc.execute()`, `obj.name`
/// Examples not counted: `this.field`, `super.method()`
pub fn compute_atfd(class_node: Node, source: &[u8]) -> usize {
    let body = match class_node.child_by_field_name("body") {
        Some(b) => b,
        None => return 0,
    };

    let mut count = 0;
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if matches!(
            child.kind(),
            "method_definition" | "abstract_method_signature"
        ) {
            count_foreign_member_accesses(child, source, &mut count);
        }
    }
    count
}

fn count_foreign_member_accesses(node: Node, source: &[u8], count: &mut usize) {
    if node.kind() == "member_expression" {
        if let Some(obj) = node.child_by_field_name("object") {
            // Only count when the base object is a simple identifier.
            // `this` has kind "this" and `super` has kind "super" in tree-sitter,
            // so the kind check already excludes them; the text check is a safety net.
            if obj.kind() == "identifier" {
                if let Ok(text) = obj.utf8_text(source) {
                    if text != "this" && text != "super" {
                        *count += 1;
                    }
                }
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        // Do not descend into nested class definitions — their foreign accesses
        // belong to the inner class, not the enclosing method.
        match child.kind() {
            "class_declaration" | "abstract_class_declaration" | "class" => {}
            _ => count_foreign_member_accesses(child, source, count),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_typescript;

    fn detect(src: &str) -> Vec<GodClassResult> {
        let tree = parse_typescript(src).expect("parse failed");
        detect_god_classes(tree.root_node(), src.as_bytes(), &GodClassConfig::default())
    }

    fn atfd_for(src: &str) -> usize {
        let tree = parse_typescript(src).expect("parse failed");
        match find_first_class(tree.root_node()) {
            Some(cls) => compute_atfd(cls, src.as_bytes()),
            None => 0,
        }
    }

    fn find_first_class(node: Node) -> Option<Node> {
        if matches!(
            node.kind(),
            "class_declaration" | "abstract_class_declaration" | "class"
        ) {
            return Some(node);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(n) = find_first_class(child) {
                return Some(n);
            }
        }
        None
    }

    /// Generate source for a clearly-over-threshold God Class.
    ///
    /// - 48 methods accessing unique `this.fN` → WMC = 54, TCC = 0.0
    /// - 6 methods accessing foreign params    → ATFD = 6
    fn god_class_src() -> String {
        let mut src = String::from("class GodClass {\n");
        for i in 0..48 {
            src.push_str(&format!("    m{i}(): number {{ return this.f{i}; }}\n"));
        }
        src.push_str("    fetchA(repo: any) { return repo.data; }\n");
        src.push_str("    fetchB(svc: any) { return svc.value; }\n");
        src.push_str("    fetchC(obj: any) { return obj.name; }\n");
        src.push_str("    fetchD(store: any) { return store.items; }\n");
        src.push_str("    fetchE(api: any) { return api.response; }\n");
        src.push_str("    fetchF(db: any) { return db.records; }\n");
        src.push_str("}\n");
        src
    }

    // ── ATFD unit tests ──────────────────────────────────────────────────────

    #[test]
    fn test_atfd_no_foreign_accesses() {
        let src = r#"
class Pure {
    getValue(): number { return this.value; }
    setValue(v: number): void { this.value = v; }
}
"#;
        assert_eq!(atfd_for(src), 0);
    }

    #[test]
    fn test_atfd_single_foreign_access() {
        let src = r#"
class Borrower {
    process(dep: any): string { return dep.name; }
}
"#;
        assert_eq!(atfd_for(src), 1);
    }

    #[test]
    fn test_atfd_this_not_counted() {
        let src = r#"
class Own {
    greet(): string { return this.name; }
}
"#;
        assert_eq!(atfd_for(src), 0);
    }

    #[test]
    fn test_atfd_super_not_counted() {
        let src = r#"
class Child extends Parent {
    greet(): string { return super.name; }
}
"#;
        assert_eq!(atfd_for(src), 0);
    }

    #[test]
    fn test_atfd_multiple_foreign_accesses() {
        let src = r#"
class Multi {
    work(a: any, b: any, c: any): void {
        const x = a.data;
        const y = b.value;
        const z = c.name;
    }
}
"#;
        assert_eq!(atfd_for(src), 3);
    }

    #[test]
    fn test_atfd_chained_access_counted_once() {
        // `obj.a.b` → tree-sitter: (obj.a).b
        // Inner access `obj.a` (identifier base) is counted.
        // Outer access `(obj.a).b` (member_expression base) is not counted.
        let src = r#"
class Chained {
    work(obj: any): any { return obj.a.b; }
}
"#;
        assert_eq!(atfd_for(src), 1);
    }

    #[test]
    fn test_atfd_method_call_on_foreign_counted() {
        let src = r#"
class Caller {
    work(svc: any): void { svc.execute(); }
}
"#;
        assert_eq!(atfd_for(src), 1);
    }

    // ── God Class detection tests ─────────────────────────────────────────────

    // ── Clearly a God Class (all thresholds exceeded) ─────────────────────────

    #[test]
    fn test_clearly_a_god_class() {
        // WMC = 54 > 47, TCC = 0.0 < 0.33, ATFD = 6 > 5
        let src = god_class_src();
        let results = detect(&src);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].class_name, "GodClass");
        assert!(results[0].wmc > 47, "expected WMC > 47, got {}", results[0].wmc);
        assert!(results[0].tcc < 0.33, "expected TCC < 0.33, got {}", results[0].tcc);
        assert!(results[0].atfd > 5, "expected ATFD > 5, got {}", results[0].atfd);
    }

    // ── ATFD too low — not flagged ─────────────────────────────────────────────

    #[test]
    fn test_not_flagged_atfd_too_low() {
        // WMC > 47, TCC < 0.33, but ATFD = 0 (no foreign accesses at all)
        let mut src = String::from("class HighWmcNoAtfd {\n");
        for i in 0..54 {
            src.push_str(&format!("    m{i}(): number {{ return this.f{i}; }}\n"));
        }
        src.push_str("}\n");
        let results = detect(&src);
        assert!(results.is_empty(), "ATFD = 0 must not be flagged");
    }

    // ── WMC too low — not flagged ──────────────────────────────────────────────

    #[test]
    fn test_not_flagged_wmc_too_low() {
        let src = r#"
class SmallClass {
    getA(): number { return this.a; }
    getB(): number { return this.b; }
    fetchX(ext: any): any { return ext.data; }
}
"#;
        let results = detect(src);
        assert!(results.is_empty(), "WMC = 3, too low to be flagged");
    }

    // ── TCC too high — not flagged ─────────────────────────────────────────────

    #[test]
    fn test_not_flagged_tcc_too_high() {
        // All 48 methods share this.x → TCC ≈ 0.79 ≥ 0.33
        let mut src = String::from("class HighTcc {\n");
        for i in 0..48 {
            src.push_str(&format!("    m{i}(): number {{ return this.x; }}\n"));
        }
        for i in 0..6 {
            src.push_str(&format!(
                "    fetch{i}(ext: any): any {{ return ext.data; }}\n"
            ));
        }
        src.push_str("}\n");
        let results = detect(&src);
        assert!(
            results.is_empty(),
            "TCC too high (all methods share this.x)"
        );
    }

    // ── WMC exactly at threshold — not flagged (strict >) ─────────────────────

    #[test]
    fn test_wmc_exactly_at_threshold_not_flagged() {
        // 41 unique-field methods + 6 foreign-access methods = 47 total → WMC = 47 (not > 47)
        let mut src = String::from("class WmcAtThreshold {\n");
        for i in 0..41 {
            src.push_str(&format!("    m{i}(): number {{ return this.f{i}; }}\n"));
        }
        src.push_str("    fetchA(repo: any) { return repo.data; }\n");
        src.push_str("    fetchB(svc: any) { return svc.value; }\n");
        src.push_str("    fetchC(obj: any) { return obj.name; }\n");
        src.push_str("    fetchD(store: any) { return store.items; }\n");
        src.push_str("    fetchE(api: any) { return api.response; }\n");
        src.push_str("    fetchF(db: any) { return db.records; }\n");
        src.push_str("}\n");
        // WMC = 47 (= threshold, not > 47), TCC = 0.0, ATFD = 6
        let results = detect(&src);
        assert!(results.is_empty(), "WMC = 47 must not be flagged (strict >)");
    }

    // ── ATFD exactly at threshold — not flagged (strict >) ────────────────────

    #[test]
    fn test_atfd_exactly_at_threshold_not_flagged() {
        // 48 unique-field methods + 5 foreign-access methods → WMC = 53, ATFD = 5 (not > 5)
        let mut src = String::from("class AtfdAtThreshold {\n");
        for i in 0..48 {
            src.push_str(&format!("    m{i}(): number {{ return this.f{i}; }}\n"));
        }
        src.push_str("    fetchA(repo: any) { return repo.data; }\n");
        src.push_str("    fetchB(svc: any) { return svc.value; }\n");
        src.push_str("    fetchC(obj: any) { return obj.name; }\n");
        src.push_str("    fetchD(store: any) { return store.items; }\n");
        src.push_str("    fetchE(api: any) { return api.response; }\n");
        src.push_str("}\n");
        // WMC = 53, TCC = 0.0, ATFD = 5 (= threshold, not > 5)
        let results = detect(&src);
        assert!(results.is_empty(), "ATFD = 5 must not be flagged (strict >)");
    }

    // ── Abstract class detected as God Class ──────────────────────────────────

    #[test]
    fn test_abstract_class_detected_as_god_class() {
        let mut src = String::from("abstract class AbstractGod {\n");
        for i in 0..48 {
            src.push_str(&format!("    m{i}(): number {{ return this.f{i}; }}\n"));
        }
        src.push_str("    fetchA(repo: any) { return repo.data; }\n");
        src.push_str("    fetchB(svc: any) { return svc.value; }\n");
        src.push_str("    fetchC(obj: any) { return obj.name; }\n");
        src.push_str("    fetchD(store: any) { return store.items; }\n");
        src.push_str("    fetchE(api: any) { return api.response; }\n");
        src.push_str("    fetchF(db: any) { return db.records; }\n");
        src.push_str("}\n");
        let results = detect(&src);
        assert_eq!(results.len(), 1, "abstract God Class must be detected");
        assert_eq!(results[0].class_name, "AbstractGod");
    }

    // ── Empty source — returns empty ───────────────────────────────────────────

    #[test]
    fn test_empty_source_returns_empty() {
        let results = detect("");
        assert!(results.is_empty());
    }

    // ── Custom thresholds ──────────────────────────────────────────────────────

    #[test]
    fn test_custom_thresholds() {
        // Lower thresholds: WMC > 2, TCC < 1.0, ATFD > 0
        let config = GodClassConfig {
            wmc_threshold: 2,
            tcc_threshold: 1.0,
            atfd_threshold: 0,
        };
        let src = r#"
class SmallGod {
    getA(): number { return this.a; }
    getB(): number { return this.b; }
    getC(): number { return this.c; }
    fetch(ext: any): any { return ext.data; }
}
"#;
        let tree = parse_typescript(src).expect("parse failed");
        let results = detect_god_classes(tree.root_node(), src.as_bytes(), &config);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].class_name, "SmallGod");
    }

    // ── Result fields are populated correctly ──────────────────────────────────

    #[test]
    fn test_result_fields_populated_correctly() {
        let src = god_class_src();
        let tree = parse_typescript(&src).expect("parse failed");
        let results =
            detect_god_classes(tree.root_node(), src.as_bytes(), &GodClassConfig::default());
        assert_eq!(results.len(), 1);
        let r = &results[0];
        assert_eq!(r.class_name, "GodClass");
        assert_eq!(r.line, 1); // no leading newline in god_class_src()
        assert!(r.wmc > 47);
        assert!(r.tcc < 0.33);
        assert!(r.atfd > 5);
    }
}
