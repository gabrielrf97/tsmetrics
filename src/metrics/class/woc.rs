use serde::Serialize;
use tree_sitter::Node;

/// Weight of Class (WOC) metrics for a single class.
///
/// WOC = public_attributes / (public_attributes + public_methods)
///
/// A high WOC (→ 1.0) indicates a data-heavy, DTO-like class that exposes
/// mostly state. A low WOC (→ 0.0) indicates a behaviour-heavy class that
/// encapsulates state and exposes operations instead.
///
/// Reference: Object-Oriented Metrics in Practice (Lanza & Marinescu, 2006)
#[derive(Debug, Clone, Serialize)]
pub struct ClassWoc {
    /// Name of the class (`<anonymous>` if the class is unnamed).
    pub class_name: String,
    /// Line number where the class starts (1-based).
    pub line: usize,
    /// Number of explicitly or implicitly public fields (attributes).
    pub public_attributes: usize,
    /// Number of explicitly or implicitly public methods (including
    /// constructor, getters, and setters).
    pub public_methods: usize,
    /// WOC score in [0.0, 1.0].  0.0 for an empty class.
    pub woc: f64,
}

/// Compute WOC for every class found in `root`.
pub fn compute_class_woc(root: Node, source: &[u8]) -> Vec<ClassWoc> {
    let mut results = Vec::new();
    collect_classes(root, source, &mut results);
    results
}

fn collect_classes(node: Node, source: &[u8], out: &mut Vec<ClassWoc>) {
    let is_class = match node.kind() {
        "class_declaration" => true,
        // Anonymous class expression: `const X = class { … }`.
        // The bare `class` keyword token (which is a leaf child of
        // class_declaration) also has kind "class", so guard against it by
        // requiring a body field.
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

fn measure_class(class_node: Node, source: &[u8]) -> ClassWoc {
    let class_name = class_node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("<anonymous>")
        .to_string();

    let line = class_node.start_position().row + 1;

    let mut public_attributes = 0usize;
    let mut public_methods = 0usize;

    if let Some(body) = class_node.child_by_field_name("body") {
        let mut cursor = body.walk();
        for member in body.children(&mut cursor) {
            match member.kind() {
                "public_field_definition" if is_public(member, source) => {
                    public_attributes += 1;
                }
                "method_definition" if is_public(member, source) => {
                    public_methods += 1;
                }
                _ => {}
            }
        }
    }

    let total = public_attributes + public_methods;
    let woc = if total == 0 {
        0.0
    } else {
        public_attributes as f64 / total as f64
    };

    ClassWoc {
        class_name,
        line,
        public_attributes,
        public_methods,
        woc,
    }
}

/// A member is public when it carries no accessibility modifier (implicitly
/// public in TypeScript) or its modifier is explicitly `"public"`.
fn is_public(node: Node, source: &[u8]) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "accessibility_modifier" {
            return child.utf8_text(source).unwrap_or("") == "public";
        }
    }
    true // no modifier → implicitly public
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_typescript;

    fn woc_for(src: &str) -> Vec<ClassWoc> {
        let tree = parse_typescript(src).expect("parse failed");
        compute_class_woc(tree.root_node(), src.as_bytes())
    }

    /// Data-heavy class: many public fields, few public methods.
    /// Expected WOC = 4 / (4 + 1) = 0.8
    #[test]
    fn test_data_heavy_class() {
        let src = r#"
class DataDto {
    public name: string;
    public age: number;
    public email: string;
    public address: string;

    public getId(): string { return this.name; }
}
"#;
        let results = woc_for(src);
        assert_eq!(results.len(), 1);
        let c = &results[0];
        assert_eq!(c.class_name, "DataDto");
        assert_eq!(c.public_attributes, 4);
        assert_eq!(c.public_methods, 1);
        assert!((c.woc - 0.8).abs() < 1e-9, "expected 0.8, got {}", c.woc);
    }

    /// Behaviour-heavy class: no public fields, several public methods.
    /// Expected WOC = 0 / (0 + 4) = 0.0
    #[test]
    fn test_behavior_heavy_class() {
        let src = r#"
class BehaviorService {
    private _state: string;

    public doA(): void {}
    public doB(): void {}
    public doC(): void {}
    public doD(): void {}
}
"#;
        let results = woc_for(src);
        assert_eq!(results.len(), 1);
        let c = &results[0];
        assert_eq!(c.class_name, "BehaviorService");
        assert_eq!(c.public_attributes, 0);
        assert_eq!(c.public_methods, 4);
        assert!((c.woc - 0.0).abs() < 1e-9, "expected 0.0, got {}", c.woc);
    }

    /// Balanced class: equal public fields and methods.
    /// Expected WOC = 2 / (2 + 2) = 0.5
    #[test]
    fn test_balanced_class() {
        let src = r#"
class Balanced {
    public x: number;
    public y: number;

    public getX(): number { return this.x; }
    public getY(): number { return this.y; }
}
"#;
        let results = woc_for(src);
        assert_eq!(results.len(), 1);
        let c = &results[0];
        assert_eq!(c.class_name, "Balanced");
        assert_eq!(c.public_attributes, 2);
        assert_eq!(c.public_methods, 2);
        assert!((c.woc - 0.5).abs() < 1e-9, "expected 0.5, got {}", c.woc);
    }

    /// Class with only getters/setters — they are methods, not attributes.
    /// Expected WOC = 0 / (0 + 2) = 0.0
    #[test]
    fn test_only_getters_setters() {
        let src = r#"
class Accessor {
    private _name: string = "";

    get name(): string { return this._name; }
    set name(value: string) { this._name = value; }
}
"#;
        let results = woc_for(src);
        assert_eq!(results.len(), 1);
        let c = &results[0];
        assert_eq!(c.class_name, "Accessor");
        assert_eq!(c.public_attributes, 0, "private field must not be counted");
        assert_eq!(c.public_methods, 2, "getter and setter are public methods");
        assert!((c.woc - 0.0).abs() < 1e-9, "expected 0.0, got {}", c.woc);
    }

    /// Empty class has no public interface.
    /// Expected WOC = 0.0
    #[test]
    fn test_empty_class() {
        let src = "class Empty {}";
        let results = woc_for(src);
        assert_eq!(results.len(), 1);
        let c = &results[0];
        assert_eq!(c.class_name, "Empty");
        assert_eq!(c.public_attributes, 0);
        assert_eq!(c.public_methods, 0);
        assert!((c.woc - 0.0).abs() < 1e-9, "expected 0.0, got {}", c.woc);
    }

    /// Implicit public (no accessibility modifier) members are counted.
    #[test]
    fn test_implicit_public_members() {
        let src = r#"
class Implicit {
    name: string;
    age: number;

    greet(): string { return "hi"; }
}
"#;
        let results = woc_for(src);
        assert_eq!(results.len(), 1);
        let c = &results[0];
        assert_eq!(c.public_attributes, 2);
        assert_eq!(c.public_methods, 1);
        // WOC = 2 / 3
        assert!((c.woc - 2.0 / 3.0).abs() < 1e-9, "expected 2/3, got {}", c.woc);
    }

    /// Private members must not inflate public counts.
    #[test]
    fn test_private_members_excluded() {
        let src = r#"
class Encapsulated {
    private secret: string;
    protected inner: number;
    public label: string;

    private doInternal(): void {}
    public doExternal(): void {}
}
"#;
        let results = woc_for(src);
        assert_eq!(results.len(), 1);
        let c = &results[0];
        assert_eq!(c.public_attributes, 1, "only `label` is public");
        assert_eq!(c.public_methods, 1, "only `doExternal` is public");
        assert!((c.woc - 0.5).abs() < 1e-9, "expected 0.5, got {}", c.woc);
    }

    /// Multiple classes in one file are all measured independently.
    #[test]
    fn test_multiple_classes() {
        let src = r#"
class A {
    public x: number;
    public doA(): void {}
}

class B {
    public doB(): void {}
    public doC(): void {}
}
"#;
        let results = woc_for(src);
        assert_eq!(results.len(), 2);

        let a = results.iter().find(|c| c.class_name == "A").unwrap();
        assert_eq!(a.public_attributes, 1);
        assert_eq!(a.public_methods, 1);
        assert!((a.woc - 0.5).abs() < 1e-9);

        let b = results.iter().find(|c| c.class_name == "B").unwrap();
        assert_eq!(b.public_attributes, 0);
        assert_eq!(b.public_methods, 2);
        assert!((b.woc - 0.0).abs() < 1e-9);
    }
}
