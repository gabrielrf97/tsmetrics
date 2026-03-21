use serde::Serialize;
use tree_sitter::Node;

use crate::metrics::class::wmc::compute_wmc;
use crate::metrics::class::woc::compute_class_woc;

/// Thresholds used to decide whether a class is a Data Class.
///
/// Default values follow the recommendations in *Object-Oriented Metrics in
/// Practice* (Lanza & Marinescu, 2006):
/// - `min_woc`: WOC must be **above** this value (class exposes more data than
///   behaviour). Default: 0.5.
/// - `max_wmc`: WMC must be **below** this value (low method complexity).
///   Default: 10.
#[derive(Debug, Clone)]
pub struct DataClassThresholds {
    /// Minimum WOC for a class to be considered a Data Class (exclusive).
    pub min_woc: f64,
    /// Maximum WMC for a class to be considered a Data Class (exclusive).
    pub max_wmc: usize,
}

impl Default for DataClassThresholds {
    fn default() -> Self {
        Self {
            min_woc: 0.5,
            max_wmc: 10,
        }
    }
}

/// Detection result for a single class.
#[derive(Debug, Clone, Serialize)]
pub struct DataClassResult {
    /// Name of the class (`<anonymous>` if unnamed).
    pub class_name: String,
    /// 1-based line number where the class starts.
    pub line: usize,
    /// Weighted Methods per Class score.
    pub wmc: usize,
    /// Weight of Class score (fraction of public interface that is data).
    pub woc: f64,
    /// `true` when the class meets the Data Class detection criteria.
    pub is_data_class: bool,
}

/// Detect Data Classes in `root` using the provided thresholds.
///
/// A class is flagged as a Data Class when:
/// - `woc > thresholds.min_woc`  (more data than behaviour in public interface)
/// - `wmc < thresholds.max_wmc`  (low method complexity)
pub fn detect_data_classes(
    root: Node,
    source: &[u8],
    thresholds: &DataClassThresholds,
) -> Vec<DataClassResult> {
    let woc_results = compute_class_woc(root, source);
    let mut results = Vec::with_capacity(woc_results.len());

    for woc_entry in woc_results {
        // Re-locate the class node to compute WMC.  We walk from the root and
        // match by class name + line so we can support multiple classes per
        // file without re-parsing.
        let wmc = find_wmc_for_line(root, source, woc_entry.line);

        let is_data_class =
            woc_entry.woc > thresholds.min_woc && wmc < thresholds.max_wmc;

        results.push(DataClassResult {
            class_name: woc_entry.class_name,
            line: woc_entry.line,
            wmc,
            woc: woc_entry.woc,
            is_data_class,
        });
    }

    results
}

/// Walk the AST and compute WMC for the class node that starts at `line`
/// (1-based).  Returns 0 if no matching class is found.
fn find_wmc_for_line(node: Node, source: &[u8], line: usize) -> usize {
    if matches!(node.kind(), "class_declaration" | "class")
        && node.child_by_field_name("body").is_some()
        && node.start_position().row + 1 == line
    {
        return compute_wmc(node, source);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let wmc = find_wmc_for_line(child, source, line);
        if wmc > 0 || (matches!(child.kind(), "class_declaration" | "class")
            && child.child_by_field_name("body").is_some()
            && child.start_position().row + 1 == line)
        {
            // Re-check: if the child is the class at that line, return its wmc
            // (even if it was 0 for a class with no methods).
            if matches!(child.kind(), "class_declaration" | "class")
                && child.child_by_field_name("body").is_some()
                && child.start_position().row + 1 == line
            {
                return compute_wmc(child, source);
            }
            return wmc;
        }
    }
    0
}

/// Convenience wrapper using default thresholds.
pub fn detect_data_classes_default(root: Node, source: &[u8]) -> Vec<DataClassResult> {
    detect_data_classes(root, source, &DataClassThresholds::default())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_typescript;

    fn detect(src: &str) -> Vec<DataClassResult> {
        let tree = parse_typescript(src).expect("parse failed");
        detect_data_classes_default(tree.root_node(), src.as_bytes())
    }

    // ── DTO-like class ───────────────────────────────────────────────────────

    /// Many public fields, few methods → high WOC, low WMC → IS a Data Class.
    #[test]
    fn test_dto_like_class_flagged() {
        let src = r#"
class UserDto {
    public id: number;
    public name: string;
    public email: string;
    public createdAt: Date;

    public getId(): number { return this.id; }
}
"#;
        let results = detect(src);
        assert_eq!(results.len(), 1);
        let r = &results[0];
        assert_eq!(r.class_name, "UserDto");
        // WOC = 4 / (4 + 1) = 0.8  → > 0.5 ✓
        assert!(r.woc > 0.5, "WOC should be > 0.5, got {}", r.woc);
        // WMC = 1 (one simple getter) → < 10 ✓
        assert!(r.wmc < 10, "WMC should be < 10, got {}", r.wmc);
        assert!(r.is_data_class, "DTO-like class must be flagged as Data Class");
    }

    // ── Behaviour-heavy class ────────────────────────────────────────────────

    /// Private state, many complex public methods → low WOC → NOT a Data Class.
    #[test]
    fn test_behavior_heavy_class_not_flagged() {
        let src = r#"
class OrderService {
    private orders: string[] = [];

    public add(order: string): void {
        if (order) { this.orders.push(order); }
    }
    public remove(order: string): void {
        const idx = this.orders.indexOf(order);
        if (idx >= 0) { this.orders.splice(idx, 1); }
    }
    public process(): void {
        for (const o of this.orders) {
            if (o.length > 0) { console.log(o); }
        }
    }
    public validate(order: string): boolean {
        return order.length > 0 && order.length < 100;
    }
}
"#;
        let results = detect(src);
        assert_eq!(results.len(), 1);
        let r = &results[0];
        // WOC = 0 / (0 + 4) = 0.0 → NOT > 0.5
        assert!(
            !r.is_data_class,
            "behaviour-heavy class must NOT be flagged; WOC={}, WMC={}",
            r.woc,
            r.wmc
        );
    }

    // ── Balanced class ───────────────────────────────────────────────────────

    /// Equal public fields and methods → WOC = 0.5 → NOT strictly > 0.5.
    #[test]
    fn test_balanced_class_not_flagged() {
        let src = r#"
class Point {
    public x: number;
    public y: number;

    public distanceTo(other: Point): number {
        return Math.sqrt((this.x - other.x) ** 2 + (this.y - other.y) ** 2);
    }
    public toString(): string { return `(${this.x}, ${this.y})`; }
}
"#;
        let results = detect(src);
        assert_eq!(results.len(), 1);
        let r = &results[0];
        // WOC = 2 / (2 + 2) = 0.5 → NOT > 0.5 (boundary excluded)
        assert!(
            (r.woc - 0.5).abs() < 1e-9,
            "expected WOC = 0.5, got {}",
            r.woc
        );
        assert!(
            !r.is_data_class,
            "balanced class (WOC = 0.5) must NOT be flagged"
        );
    }

    // ── Class with only getters/setters ──────────────────────────────────────

    /// Only getters/setters count as methods (not attributes) → WOC = 0.0.
    #[test]
    fn test_only_getters_setters_not_flagged() {
        let src = r#"
class Temperature {
    private _celsius: number = 0;

    get celsius(): number { return this._celsius; }
    set celsius(value: number) { this._celsius = value; }
    get fahrenheit(): number { return this._celsius * 9 / 5 + 32; }
    set fahrenheit(value: number) { this._celsius = (value - 32) * 5 / 9; }
}
"#;
        let results = detect(src);
        assert_eq!(results.len(), 1);
        let r = &results[0];
        // All getters/setters are methods → WOC = 0 / (0 + 4) = 0.0
        assert_eq!(r.woc, 0.0, "WOC must be 0.0 for getter/setter-only class");
        assert!(
            !r.is_data_class,
            "getter/setter-only class must NOT be flagged as Data Class"
        );
    }

    // ── High WMC disqualifies even a data-heavy class ────────────────────────

    /// A class with many public fields BUT also high WMC should NOT be flagged.
    #[test]
    fn test_high_wmc_prevents_flagging() {
        // Build a class that has many public fields (high WOC) but also has
        // a very complex method (WMC ≥ 10).
        let src = r#"
class ComplexData {
    public a: number;
    public b: number;
    public c: number;
    public d: number;
    public e: number;
    public f: number;

    public compute(x: number): number {
        if (x > 0) {
            if (x > 10) {
                if (x > 100) {
                    if (x > 1000) {
                        if (x > 10000) {
                            if (x > 100000) {
                                if (x > 1000000) {
                                    if (x > 10000000) {
                                        if (x > 100000000) {
                                            return x;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        return 0;
    }
}
"#;
        let results = detect(src);
        assert_eq!(results.len(), 1);
        let r = &results[0];
        assert!(r.wmc >= 10, "WMC should be >= 10, got {}", r.wmc);
        assert!(
            !r.is_data_class,
            "high-WMC class must NOT be flagged even with high WOC"
        );
    }

    // ── Custom thresholds ────────────────────────────────────────────────────

    #[test]
    fn test_custom_thresholds_stricter_woc() {
        let src = r#"
class UserDto {
    public id: number;
    public name: string;
    public email: string;

    public getId(): number { return this.id; }
    public getName(): string { return this.name; }
    public getEmail(): string { return this.email; }
}
"#;
        // WOC = 3 / (3 + 3) = 0.5
        // With min_woc = 0.5: NOT flagged (exclusive boundary)
        // With min_woc = 0.4: flagged
        let tree = parse_typescript(src).expect("parse failed");
        let root = tree.root_node();
        let source = src.as_bytes();

        let strict = DataClassThresholds { min_woc: 0.5, max_wmc: 10 };
        let results_strict = detect_data_classes(root, source, &strict);
        assert!(!results_strict[0].is_data_class, "WOC=0.5 must not exceed min_woc=0.5");

        let lenient = DataClassThresholds { min_woc: 0.4, max_wmc: 10 };
        let results_lenient = detect_data_classes(root, source, &lenient);
        assert!(results_lenient[0].is_data_class, "WOC=0.5 must exceed min_woc=0.4");
    }

    // ── Multiple classes ─────────────────────────────────────────────────────

    #[test]
    fn test_multiple_classes_independent() {
        let src = r#"
class PersonDto {
    public firstName: string;
    public lastName: string;
    public age: number;

    public getFullName(): string { return `${this.firstName} ${this.lastName}`; }
}

class PersonService {
    private persons: string[] = [];

    public add(name: string): void { this.persons.push(name); }
    public remove(name: string): void {
        const idx = this.persons.indexOf(name);
        if (idx >= 0) { this.persons.splice(idx, 1); }
    }
    public find(name: string): boolean {
        return this.persons.some(p => p === name);
    }
}
"#;
        let results = detect(src);
        assert_eq!(results.len(), 2);

        let dto = results.iter().find(|r| r.class_name == "PersonDto").unwrap();
        assert!(dto.is_data_class, "PersonDto should be flagged as Data Class");

        let svc = results.iter().find(|r| r.class_name == "PersonService").unwrap();
        assert!(!svc.is_data_class, "PersonService must NOT be flagged");
    }
}
