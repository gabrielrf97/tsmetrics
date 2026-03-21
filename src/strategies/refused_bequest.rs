use std::collections::HashMap;

use serde::Serialize;
use tree_sitter::Node;

use crate::metrics::class::dit::compute_dit;
use crate::metrics::class::nom::compute_class_nom;

/// Override-ratio threshold below which a subclass is considered a Refused Bequest.
///
/// A class that overrides fewer than 33% of its own methods is flagged.
/// When a subclass has no methods at all (NOM = 0), the ratio is treated as
/// 0.0, which is always below the threshold.
pub const OVERRIDE_RATIO_THRESHOLD: f64 = 0.33;

/// A detected Refused Bequest smell for a single class.
///
/// A Refused Bequest occurs when a subclass (DIT > 0) inherits from a parent
/// but overrides very few — or none — of its methods.  The class accepts the
/// inheritance link yet largely ignores what the parent provides.
///
/// Reference: *Object-Oriented Metrics in Practice*, Lanza & Marinescu.
#[derive(Debug, Clone, Serialize)]
pub struct RefusedBequest {
    /// Name of the offending class.
    pub class_name: String,
    /// Line number where the class is declared (1-based).
    pub line: usize,
    /// Depth of Inheritance Tree — always ≥ 1 for flagged classes.
    pub dit: usize,
    /// Total number of methods in the class (NOM).
    pub nom: usize,
    /// Number of overriding methods (NOOM).
    pub noom: usize,
    /// `noom / nom`; 0.0 when `nom == 0`.
    pub override_ratio: f64,
}

/// Detect Refused Bequest smells in all classes declared under `root`.
///
/// A class is flagged when:
/// - DIT > 0 (it is a subclass), **and**
/// - `noom / nom < OVERRIDE_RATIO_THRESHOLD` (override ratio is very low).
///
/// Classes that do not appear in the DIT results (e.g. anonymous class
/// expressions) are skipped because their inheritance depth cannot be
/// determined.
pub fn detect_refused_bequest(root: Node, source: &[u8]) -> Vec<RefusedBequest> {
    let dit_results = compute_dit(root, source);
    let nom_results = compute_class_nom(root, source);

    // Build name → DIT lookup.  Named classes are unique within a file in
    // valid TypeScript; if names collide (pathological input) the last wins,
    // which mirrors how DIT itself resolves ambiguity.
    let dit_map: HashMap<&str, usize> = dit_results
        .iter()
        .map(|d| (d.name.as_str(), d.dit))
        .collect();

    nom_results
        .iter()
        .filter_map(|n| {
            let &dit = dit_map.get(n.class_name.as_str())?;

            // Only subclasses can exhibit Refused Bequest.
            if dit == 0 {
                return None;
            }

            let override_ratio = if n.nom == 0 {
                0.0
            } else {
                n.noom as f64 / n.nom as f64
            };

            if override_ratio < OVERRIDE_RATIO_THRESHOLD {
                Some(RefusedBequest {
                    class_name: n.class_name.clone(),
                    line: n.line,
                    dit,
                    nom: n.nom,
                    noom: n.noom,
                    override_ratio,
                })
            } else {
                None
            }
        })
        .collect()
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_typescript;
    use approx::assert_relative_eq;

    fn detect(src: &str) -> Vec<RefusedBequest> {
        let tree = parse_typescript(src).expect("parse failed");
        detect_refused_bequest(tree.root_node(), src.as_bytes())
    }

    fn names(smells: &[RefusedBequest]) -> Vec<&str> {
        smells.iter().map(|s| s.class_name.as_str()).collect()
    }

    // ── No parent — never flagged ─────────────────────────────────────────────

    #[test]
    fn root_class_not_flagged() {
        let smells = detect("class A { foo(): void {} }");
        assert!(smells.is_empty(), "root class must never be flagged");
    }

    #[test]
    fn multiple_root_classes_not_flagged() {
        let src = r#"
class A { foo(): void {} }
class B { bar(): void {} }
class C {}
"#;
        let smells = detect(src);
        assert!(smells.is_empty());
    }

    // ── Subclass that overrides nothing — always flagged ──────────────────────

    #[test]
    fn subclass_no_methods_flagged() {
        // Child inherits from Parent but declares no methods at all.
        let src = r#"
class Parent { speak(): void {} }
class Child extends Parent {}
"#;
        let smells = detect(src);
        assert_eq!(smells.len(), 1);
        let s = &smells[0];
        assert_eq!(s.class_name, "Child");
        assert_eq!(s.dit, 1);
        assert_eq!(s.nom, 0);
        assert_eq!(s.noom, 0);
        assert_relative_eq!(s.override_ratio, 0.0);
    }

    #[test]
    fn subclass_adds_methods_but_no_overrides_flagged() {
        // Child adds new methods but overrides none — ratio = 0/3 = 0.0.
        let src = r#"
class Base {
    a(): void {}
    b(): void {}
}
class Child extends Base {
    c(): void {}
    d(): void {}
    e(): void {}
}
"#;
        let smells = detect(src);
        assert_eq!(names(&smells), vec!["Child"]);
        let s = &smells[0];
        assert_eq!(s.noom, 0);
        assert_eq!(s.nom, 3);
        assert_relative_eq!(s.override_ratio, 0.0);
    }

    // ── Subclass that overrides all methods — not flagged ─────────────────────

    #[test]
    fn subclass_overrides_all_not_flagged() {
        let src = r#"
class Animal {
    speak(): string { return "..."; }
    move(): void {}
}
class Dog extends Animal {
    override speak(): string { return "woof"; }
    override move(): void { console.log("runs"); }
}
"#;
        let smells = detect(src);
        assert!(smells.is_empty(), "full override ratio must not be flagged");
    }

    // ── Threshold boundary ────────────────────────────────────────────────────

    #[test]
    fn exactly_one_third_override_ratio_not_flagged() {
        // 1 override out of 3 methods → ratio = 0.333… ≥ 0.33 → not flagged.
        let src = r#"
class Base {
    a(): void {}
    b(): void {}
    c(): void {}
}
class Child extends Base {
    override a(): void {}
    d(): void {}
    e(): void {}
}
"#;
        let smells = detect(src);
        // ratio = 1/3 ≈ 0.3333, which is ≥ 0.33 → should NOT be flagged
        assert!(
            smells.is_empty(),
            "ratio exactly at or above threshold must not be flagged; got {:?}",
            smells
        );
    }

    #[test]
    fn below_threshold_flagged() {
        // 1 override out of 4 methods → ratio = 0.25 < 0.33 → flagged.
        let src = r#"
class Base { a(): void {} }
class Child extends Base {
    override a(): void {}
    b(): void {}
    c(): void {}
    d(): void {}
}
"#;
        let smells = detect(src);
        assert_eq!(names(&smells), vec!["Child"]);
        let s = &smells[0];
        assert_relative_eq!(s.override_ratio, 0.25, epsilon = 1e-9);
    }

    // ── Deep inheritance chains ───────────────────────────────────────────────

    #[test]
    fn deep_chain_each_level_evaluated_independently() {
        // A → B → C.  B overrides nothing (flagged), C overrides all (not flagged).
        let src = r#"
class A { x(): void {} }
class B extends A { y(): void {} }
class C extends B { override x(): void {} override y(): void {} }
"#;
        let smells = detect(src);
        assert_eq!(names(&smells), vec!["B"], "only B should be flagged");
        let b = &smells[0];
        assert_eq!(b.dit, 1);
    }

    #[test]
    fn dit_value_reflects_chain_depth() {
        let src = r#"
class A {}
class B extends A {}
class C extends B {}
"#;
        // All have 0 methods → ratio 0.0 → all flagged except A.
        let smells = detect(src);
        let b = smells.iter().find(|s| s.class_name == "B").unwrap();
        let c = smells.iter().find(|s| s.class_name == "C").unwrap();
        assert_eq!(b.dit, 1);
        assert_eq!(c.dit, 2);
    }

    // ── External / built-in parent ────────────────────────────────────────────

    #[test]
    fn extends_builtin_no_overrides_flagged() {
        // Error is external; MyError has DIT=1 but no overrides.
        let src = "class MyError extends Error {}";
        let smells = detect(src);
        assert_eq!(smells.len(), 1);
        assert_eq!(smells[0].class_name, "MyError");
        assert_eq!(smells[0].dit, 1);
    }

    #[test]
    fn extends_builtin_with_overrides_not_flagged() {
        let src = r#"
class MyError extends Error {
    override toString(): string { return "MyError"; }
    override message: string = "my error";
}
"#;
        // toString is overridden (1 override / 1 method = 1.0 ratio) → not flagged.
        let smells = detect(src);
        assert!(
            smells.is_empty(),
            "full override ratio must not be flagged; got {:?}",
            smells
        );
    }

    // ── Abstract classes ──────────────────────────────────────────────────────

    #[test]
    fn concrete_child_of_abstract_no_override_flagged() {
        let src = r#"
abstract class Shape { abstract area(): number; }
class Circle extends Shape { radius: number = 1; }
"#;
        let smells = detect(src);
        assert_eq!(names(&smells), vec!["Circle"]);
    }

    // ── Multiple classes in file — only flagged ones returned ─────────────────

    #[test]
    fn mixed_file_only_guilty_classes_returned() {
        let src = r#"
class A { foo(): void {} bar(): void {} }

class B extends A {
    override foo(): void {}
    override bar(): void {}
    baz(): void {}
}

class C extends A {}

class D { standalone(): void {} }
"#;
        // B: 2 overrides / 3 total = 0.667 → not flagged
        // C: 0 overrides / 0 total = 0.0  → flagged
        // D: no parent                    → not flagged
        let smells = detect(src);
        assert_eq!(names(&smells), vec!["C"]);
    }

    // ── Ratio and fields are correct ──────────────────────────────────────────

    #[test]
    fn fields_populated_correctly() {
        // 1 override + 4 new methods = 5 total → ratio 0.2 < 0.33 → flagged
        let src2 = r#"
class Base { a(): void {} }
class Child extends Base {
    override a(): void {}
    b(): void {}
    c(): void {}
    d(): void {}
    e(): void {}
}
"#;
        // 1 override / 5 methods = 0.2 < 0.33 → flagged
        let smells = detect(src2);
        let s = smells.iter().find(|s| s.class_name == "Child").unwrap();
        assert_eq!(s.dit, 1);
        assert_eq!(s.nom, 5);
        assert_eq!(s.noom, 1);
        assert_relative_eq!(s.override_ratio, 0.2, epsilon = 1e-9);
        assert_eq!(s.line, 3); // "class Child" is on line 3
    }
}
