use std::collections::HashSet;
use tree_sitter::Node;

/// Tight Class Cohesion (TCC) metrics for a single class.
///
/// TCC = directly_connected_pairs / total_pairs
///
/// Two methods are "directly connected" if they both access at least one
/// common instance field via `this.field`.  For a class with N methods,
/// total_pairs = N * (N - 1) / 2.
///
/// Special cases:
/// - 0 or 1 method → TCC = 1.0  (vacuously cohesive)
///
/// Reference: Bieman & Kang, "Cohesion and reuse in an object-oriented
/// system", ACM SIGSOFT Software Engineering Notes, 1995.
#[derive(Debug, Clone)]
pub struct ClassTcc {
    /// Name of the class (`<anonymous>` if unnamed).
    pub class_name: String,
    /// 1-based line where the class starts.
    pub line: usize,
    /// Total number of methods (including constructor, getters, setters).
    pub method_count: usize,
    /// Number of method pairs that share at least one `this.field` access.
    pub connected_pairs: usize,
    /// Total number of method pairs: method_count * (method_count - 1) / 2.
    pub total_pairs: usize,
    /// TCC score in [0.0, 1.0].
    pub tcc: f64,
}

/// Compute TCC for every class found in `root`.
pub fn compute_class_tcc(root: Node, source: &[u8]) -> Vec<ClassTcc> {
    let mut results = Vec::new();
    collect_classes(root, source, &mut results);
    results
}

fn collect_classes(node: Node, source: &[u8], out: &mut Vec<ClassTcc>) {
    let is_class = match node.kind() {
        "class_declaration" => true,
        // Guard against the bare `class` keyword token (leaf child of
        // class_declaration) by requiring a body field.
        "class" => node.child_by_field_name("body").is_some(),
        _ => false,
    };
    if is_class {
        out.push(measure_class(node, source));
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_classes(child, source, out);
    }
}

fn measure_class(class_node: Node, source: &[u8]) -> ClassTcc {
    let class_name = class_node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("<anonymous>")
        .to_string();

    let line = class_node.start_position().row + 1;

    let method_fields = collect_method_fields(class_node, source);
    let method_count = method_fields.len();

    let total_pairs = if method_count < 2 {
        0
    } else {
        method_count * (method_count - 1) / 2
    };

    let connected_pairs = if total_pairs == 0 {
        0
    } else {
        count_connected_pairs(&method_fields)
    };

    let tcc = if method_count <= 1 {
        1.0
    } else if total_pairs == 0 {
        1.0
    } else {
        connected_pairs as f64 / total_pairs as f64
    };

    ClassTcc {
        class_name,
        line,
        method_count,
        connected_pairs,
        total_pairs,
        tcc,
    }
}

/// Returns one `HashSet<String>` per method, containing every distinct field
/// name accessed via `this.field` anywhere within that method's body.
fn collect_method_fields(class_node: Node, source: &[u8]) -> Vec<HashSet<String>> {
    let body = match class_node.child_by_field_name("body") {
        Some(b) => b,
        None => return Vec::new(),
    };

    let mut result = Vec::new();
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() == "method_definition" {
            result.push(this_field_accesses(child, source));
        }
    }
    result
}

/// Recursively collect all distinct property names from `this.<name>`
/// expressions within `node`.
fn this_field_accesses(node: Node, source: &[u8]) -> HashSet<String> {
    let mut fields = HashSet::new();
    collect_this_accesses(node, source, &mut fields);
    fields
}

fn collect_this_accesses(node: Node, source: &[u8], fields: &mut HashSet<String>) {
    if node.kind() == "member_expression" {
        if let Some(obj) = node.child_by_field_name("object") {
            if obj.kind() == "this" {
                if let Some(prop) = node.child_by_field_name("property") {
                    if let Ok(name) = prop.utf8_text(source) {
                        fields.insert(name.to_string());
                    }
                }
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_this_accesses(child, source, fields);
    }
}

fn count_connected_pairs(method_fields: &[HashSet<String>]) -> usize {
    let n = method_fields.len();
    let mut count = 0;
    for i in 0..n {
        for j in (i + 1)..n {
            if method_fields[i]
                .intersection(&method_fields[j])
                .next()
                .is_some()
            {
                count += 1;
            }
        }
    }
    count
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_typescript;

    fn tcc_for(src: &str) -> Vec<ClassTcc> {
        let tree = parse_typescript(src).expect("parse failed");
        compute_class_tcc(tree.root_node(), src.as_bytes())
    }

    // ── Fully cohesive (TCC = 1.0) ─────────────────────────────────────────

    /// All three methods access the same field `x`.
    /// Pairs: (getX,setX) ✓  (getX,doubleX) ✓  (setX,doubleX) ✓
    /// connected=3, total=3, TCC=1.0
    #[test]
    fn test_fully_cohesive_class() {
        let src = r#"
class Cohesive {
    private x: number;
    getX(): number { return this.x; }
    setX(v: number): void { this.x = v; }
    doubleX(): number { return this.x * 2; }
}
"#;
        let results = tcc_for(src);
        assert_eq!(results.len(), 1);
        let c = &results[0];
        assert_eq!(c.class_name, "Cohesive");
        assert_eq!(c.method_count, 3);
        assert_eq!(c.total_pairs, 3);
        assert_eq!(c.connected_pairs, 3);
        assert!((c.tcc - 1.0).abs() < 1e-9, "expected TCC=1.0, got {}", c.tcc);
    }

    // ── Non-cohesive (TCC = 0.0) ───────────────────────────────────────────

    /// getA accesses only `a`; getB accesses only `b` — no shared field.
    /// connected=0, total=1, TCC=0.0
    #[test]
    fn test_non_cohesive_class() {
        let src = r#"
class NonCohesive {
    private a: number;
    private b: number;
    getA(): number { return this.a; }
    getB(): number { return this.b; }
}
"#;
        let results = tcc_for(src);
        assert_eq!(results.len(), 1);
        let c = &results[0];
        assert_eq!(c.class_name, "NonCohesive");
        assert_eq!(c.method_count, 2);
        assert_eq!(c.total_pairs, 1);
        assert_eq!(c.connected_pairs, 0);
        assert!((c.tcc - 0.0).abs() < 1e-9, "expected TCC=0.0, got {}", c.tcc);
    }

    // ── Partially cohesive (TCC = 2/3) ────────────────────────────────────

    /// getX:{x}, getY:{y}, getSum:{x,y}
    /// Pairs: (getX,getY) ✗  (getX,getSum) ✓  (getY,getSum) ✓
    /// connected=2, total=3, TCC=2/3
    #[test]
    fn test_partially_cohesive_class() {
        let src = r#"
class Partial {
    private x: number;
    private y: number;
    getX(): number { return this.x; }
    getY(): number { return this.y; }
    getSum(): number { return this.x + this.y; }
}
"#;
        let results = tcc_for(src);
        assert_eq!(results.len(), 1);
        let c = &results[0];
        assert_eq!(c.class_name, "Partial");
        assert_eq!(c.method_count, 3);
        assert_eq!(c.total_pairs, 3);
        assert_eq!(c.connected_pairs, 2);
        assert!(
            (c.tcc - 2.0 / 3.0).abs() < 1e-9,
            "expected TCC=2/3, got {}",
            c.tcc
        );
    }

    // ── Single method → TCC = 1.0 ─────────────────────────────────────────

    /// A class with exactly one method has no pairs, so TCC is defined as 1.0.
    #[test]
    fn test_single_method_class() {
        let src = r#"
class Single {
    private val: number;
    getValue(): number { return this.val; }
}
"#;
        let results = tcc_for(src);
        assert_eq!(results.len(), 1);
        let c = &results[0];
        assert_eq!(c.class_name, "Single");
        assert_eq!(c.method_count, 1);
        assert_eq!(c.total_pairs, 0);
        assert_eq!(c.connected_pairs, 0);
        assert!((c.tcc - 1.0).abs() < 1e-9, "expected TCC=1.0, got {}", c.tcc);
    }

    // ── Empty class → TCC = 1.0 ───────────────────────────────────────────

    #[test]
    fn test_empty_class() {
        let src = "class Empty {}";
        let results = tcc_for(src);
        assert_eq!(results.len(), 1);
        let c = &results[0];
        assert_eq!(c.method_count, 0);
        assert_eq!(c.total_pairs, 0);
        assert!((c.tcc - 1.0).abs() < 1e-9, "expected TCC=1.0, got {}", c.tcc);
    }

    // ── Getters and setters sharing fields ────────────────────────────────

    /// getter and setter both access `_count`.
    /// connected=1, total=1, TCC=1.0
    #[test]
    fn test_getters_setters_sharing_field() {
        let src = r#"
class Counter {
    private _count: number = 0;
    get count(): number { return this._count; }
    set count(v: number) { this._count = v; }
}
"#;
        let results = tcc_for(src);
        assert_eq!(results.len(), 1);
        let c = &results[0];
        assert_eq!(c.class_name, "Counter");
        assert_eq!(c.method_count, 2);
        assert_eq!(c.total_pairs, 1);
        assert_eq!(c.connected_pairs, 1);
        assert!((c.tcc - 1.0).abs() < 1e-9, "expected TCC=1.0, got {}", c.tcc);
    }

    // ── Constructor participates in cohesion calculation ──────────────────

    /// constructor writes `this.x`; getter reads `this.x` → connected.
    /// connected=1, total=1, TCC=1.0
    #[test]
    fn test_constructor_counts_as_method() {
        let src = r#"
class Widget {
    private x: number;
    constructor(x: number) { this.x = x; }
    getX(): number { return this.x; }
}
"#;
        let results = tcc_for(src);
        assert_eq!(results.len(), 1);
        let c = &results[0];
        assert_eq!(c.method_count, 2);
        assert_eq!(c.connected_pairs, 1);
        assert!((c.tcc - 1.0).abs() < 1e-9, "expected TCC=1.0, got {}", c.tcc);
    }

    // ── Multiple classes in one file ──────────────────────────────────────

    #[test]
    fn test_multiple_classes_measured_independently() {
        let src = r#"
class A {
    private v: number;
    getV(): number { return this.v; }
    setV(x: number): void { this.v = x; }
}
class B {
    private p: string;
    private q: string;
    getP(): string { return this.p; }
    getQ(): string { return this.q; }
}
"#;
        let results = tcc_for(src);
        assert_eq!(results.len(), 2);

        let a = results.iter().find(|c| c.class_name == "A").unwrap();
        assert_eq!(a.method_count, 2);
        assert_eq!(a.connected_pairs, 1);
        assert!((a.tcc - 1.0).abs() < 1e-9, "A: expected TCC=1.0");

        let b = results.iter().find(|c| c.class_name == "B").unwrap();
        assert_eq!(b.method_count, 2);
        assert_eq!(b.connected_pairs, 0);
        assert!((b.tcc - 0.0).abs() < 1e-9, "B: expected TCC=0.0");
    }
}
