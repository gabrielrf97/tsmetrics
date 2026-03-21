use std::collections::HashSet;

use serde::Serialize;
use tree_sitter::Node;

/// NOM/NOAM/NOOM metrics for a single class.
///
/// - NOM  (Number of Methods): total declared `method_definition` nodes, including
///   constructor, getters, setters, static methods, and abstract methods.
/// - NOAM (Number of Added Methods): methods that are new to this class —
///   i.e. they do NOT carry an explicit `override` modifier.
/// - NOOM (Number of Overriding Methods): methods that are marked with the
///   TypeScript `override` keyword, signalling they redefine a method from
///   a superclass or implemented interface.
///
/// Identity: NOM = NOAM + NOOM.
#[derive(Debug, Clone, Serialize)]
pub struct ClassNom {
    /// Name of the class (`<anonymous>` if unnamed).
    pub class_name: String,
    /// Line number where the class starts (1-based).
    pub line: usize,
    /// Total methods (NOM).
    pub nom: usize,
    /// Methods added by this class (NOAM).
    pub noam: usize,
    /// Methods overriding a parent (NOOM).
    pub noom: usize,
}

/// Compute NOM/NOAM/NOOM for every class found under `root`.
pub fn compute_class_nom(root: Node, source: &[u8]) -> Vec<ClassNom> {
    let mut results = Vec::new();
    collect_classes(root, source, &mut results);
    results
}

fn collect_classes(node: Node, source: &[u8], out: &mut Vec<ClassNom>) {
    let is_class = match node.kind() {
        "class_declaration" | "abstract_class_declaration" => true,
        // Class expressions: the bare "class" keyword token (leaf) also has
        // kind "class", so guard by requiring a body field.
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

fn measure_class(class_node: Node, source: &[u8]) -> ClassNom {
    let class_name = class_node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("<anonymous>")
        .to_string();

    let line = class_node.start_position().row + 1;

    let (nom, noom) = match class_node.child_by_field_name("body") {
        None => (0, 0),
        Some(body) => count_methods(body),
    };

    ClassNom {
        class_name,
        line,
        nom,
        noam: nom - noom,
        noom,
    }
}

/// Returns `(nom, noom)` by walking the immediate children of a `class_body`.
///
/// Both `method_definition` (concrete methods) and `abstract_method_signature`
/// (abstract method declarations) count toward NOM.  Only concrete methods can
/// carry `override`, so abstract methods always contribute to NOAM.
fn count_methods(body: Node) -> (usize, usize) {
    let mut nom = 0usize;
    let mut noom = 0usize;
    let mut cursor = body.walk();
    for member in body.children(&mut cursor) {
        match member.kind() {
            "method_definition" => {
                nom += 1;
                if has_override_modifier(member) {
                    noom += 1;
                }
            }
            "abstract_method_signature" => {
                nom += 1;
                // Abstract methods cannot be overrides of another abstract method
                // in the same class hierarchy direction; they are always new.
            }
            _ => {}
        }
    }
    (nom, noom)
}

/// Returns `true` when the method carries an explicit `override` modifier.
///
/// In tree-sitter-typescript v0.23 the `override` keyword is wrapped in an
/// `override_modifier` node that is a direct child of `method_definition`.
fn has_override_modifier(method: Node) -> bool {
    let mut cursor = method.walk();
    for child in method.children(&mut cursor) {
        if child.kind() == "override_modifier" {
            return true;
        }
    }
    false
}

/// Collect the set of names declared as `abstract` methods in the class at `class_node`.
///
/// Used by the Refused Bequest strategy to identify abstract method signatures
/// so that child-class implementations (which may omit the `override` keyword)
/// can still be counted as overriding methods.
pub fn abstract_method_names(class_node: Node, source: &[u8]) -> HashSet<String> {
    let mut names = HashSet::new();
    let body = match class_node.child_by_field_name("body") {
        Some(b) => b,
        None => return names,
    };
    let mut cursor = body.walk();
    for member in body.children(&mut cursor) {
        if member.kind() == "abstract_method_signature" {
            if let Some(name_node) = member.child_by_field_name("name") {
                if let Ok(text) = name_node.utf8_text(source) {
                    names.insert(text.to_string());
                }
            }
        }
    }
    names
}

/// Walk the AST collecting every named class node (declarations and class expressions),
/// keyed by the class name, pairing it with the abstract method names it declares.
///
/// This is used by Refused Bequest detection to resolve abstract method implementations
/// in child classes that lack the `override` keyword.
pub fn collect_abstract_methods_by_class(
    root: Node,
    source: &[u8],
) -> std::collections::HashMap<String, HashSet<String>> {
    let mut map = std::collections::HashMap::new();
    collect_abstract_methods_walk(root, source, &mut map);
    map
}

fn collect_abstract_methods_walk(
    node: Node,
    source: &[u8],
    out: &mut std::collections::HashMap<String, HashSet<String>>,
) {
    let kind = node.kind();
    let is_class = match kind {
        "abstract_class_declaration" => true,
        "class_declaration" | "class" => node.child_by_field_name("body").is_some(),
        _ => false,
    };
    if is_class {
        let class_name = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("<anonymous>")
            .to_string();
        let names = abstract_method_names(node, source);
        if !names.is_empty() {
            out.insert(class_name, names);
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_abstract_methods_walk(child, source, out);
    }
}

/// Collect the names of concrete methods in `class_node`'s body that do NOT carry
/// an `override` modifier.
///
/// Used by Refused Bequest detection to identify methods that implement abstract
/// parent methods without using the `override` keyword.
pub fn concrete_non_override_method_names(class_node: Node, source: &[u8]) -> HashSet<String> {
    let mut names = HashSet::new();
    let body = match class_node.child_by_field_name("body") {
        Some(b) => b,
        None => return names,
    };
    let mut cursor = body.walk();
    for member in body.children(&mut cursor) {
        if member.kind() == "method_definition" && !has_override_modifier(member) {
            if let Some(name_node) = member.child_by_field_name("name") {
                if let Ok(text) = name_node.utf8_text(source) {
                    names.insert(text.to_string());
                }
            }
        }
    }
    names
}

/// Per-class data needed by Refused Bequest to compute implicit overrides.
#[derive(Debug, Clone)]
pub struct ClassMethodInfo {
    /// Class name.
    pub class_name: String,
    /// Names of abstract methods declared in this class.
    pub abstract_method_names: HashSet<String>,
    /// Names of concrete methods that do NOT carry `override`.
    pub concrete_non_override_names: HashSet<String>,
}

/// Collect `ClassMethodInfo` for every named class in the file.
///
/// This is used by the Refused Bequest strategy to find abstract method
/// implementations in child classes that omit the `override` keyword.
pub fn collect_class_method_info(root: Node, source: &[u8]) -> Vec<ClassMethodInfo> {
    let mut out = Vec::new();
    collect_class_method_info_walk(root, source, &mut out);
    out
}

fn collect_class_method_info_walk(node: Node, source: &[u8], out: &mut Vec<ClassMethodInfo>) {
    let kind = node.kind();
    let is_class = match kind {
        "class_declaration" | "abstract_class_declaration" => true,
        "class" => node.child_by_field_name("body").is_some(),
        _ => false,
    };
    if is_class {
        let class_name = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("<anonymous>")
            .to_string();
        let info = ClassMethodInfo {
            class_name,
            abstract_method_names: abstract_method_names(node, source),
            concrete_non_override_names: concrete_non_override_method_names(node, source),
        };
        out.push(info);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_class_method_info_walk(child, source, out);
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_typescript;

    fn nom_for(src: &str) -> Vec<ClassNom> {
        let tree = parse_typescript(src).expect("parse failed");
        compute_class_nom(tree.root_node(), src.as_bytes())
    }


    fn first(src: &str) -> ClassNom {
        let mut v = nom_for(src);
        assert!(!v.is_empty(), "no class found");
        v.remove(0)
    }

    // ── Empty class ──────────────────────────────────────────────────────────

    #[test]
    fn test_empty_class_all_zero() {
        let c = first("class Empty {}");
        assert_eq!(c.nom, 0);
        assert_eq!(c.noam, 0);
        assert_eq!(c.noom, 0);
    }

    // ── Constructor counts as a method ───────────────────────────────────────

    #[test]
    fn test_constructor_counts_as_method() {
        let src = r#"
class Service {
    constructor(private name: string) {
        this.name = name;
    }
}
"#;
        let c = first(src);
        assert_eq!(c.nom, 1);
        assert_eq!(c.noam, 1);
        assert_eq!(c.noom, 0);
    }

    // ── Regular, static, getter, setter ─────────────────────────────────────

    #[test]
    fn test_various_method_types() {
        let src = r#"
class Counter {
    private _count: number = 0;

    constructor() { this._count = 0; }
    increment(): void { this._count++; }
    static create(): Counter { return new Counter(); }
    get count(): number { return this._count; }
    set count(v: number) { this._count = v; }
}
"#;
        let c = first(src);
        assert_eq!(c.nom, 5, "constructor + regular + static + getter + setter");
        assert_eq!(c.noam, 5, "none are overrides");
        assert_eq!(c.noom, 0);
    }

    // ── Override methods ─────────────────────────────────────────────────────

    #[test]
    fn test_override_methods_counted_in_noom() {
        let src = r#"
class Animal {
    speak(): string { return "..."; }
    move(): void {}
}

class Dog extends Animal {
    override speak(): string { return "woof"; }
    override move(): void { console.log("runs"); }
    fetch(): void {}
}
"#;
        let results = nom_for(src);
        assert_eq!(results.len(), 2);

        let animal = results.iter().find(|c| c.class_name == "Animal").unwrap();
        assert_eq!(animal.nom, 2);
        assert_eq!(animal.noam, 2);
        assert_eq!(animal.noom, 0);

        let dog = results.iter().find(|c| c.class_name == "Dog").unwrap();
        assert_eq!(dog.nom, 3);
        assert_eq!(dog.noom, 2, "speak + move are overrides");
        assert_eq!(dog.noam, 1, "only fetch is new");
    }

    // ── Class with only overrides ────────────────────────────────────────────

    #[test]
    fn test_only_overrides() {
        let src = r#"
class Base {
    foo(): void {}
    bar(): void {}
}

class Child extends Base {
    override foo(): void {}
    override bar(): void {}
}
"#;
        let results = nom_for(src);
        let child = results.iter().find(|c| c.class_name == "Child").unwrap();
        assert_eq!(child.nom, 2);
        assert_eq!(child.noom, 2);
        assert_eq!(child.noam, 0);
    }

    // ── Abstract class ───────────────────────────────────────────────────────

    #[test]
    fn test_abstract_class_methods() {
        let src = r#"
abstract class Shape {
    abstract area(): number;
    abstract perimeter(): number;
    describe(): string { return "shape"; }
}
"#;
        let c = first(src);
        assert_eq!(c.nom, 3, "abstract methods count too");
        assert_eq!(c.noam, 3);
        assert_eq!(c.noom, 0);
    }

    // ── NOM = NOAM + NOOM identity ───────────────────────────────────────────

    #[test]
    fn test_nom_equals_noam_plus_noom() {
        let src = r#"
class Base {
    a(): void {}
    b(): void {}
    c(): void {}
}

class Child extends Base {
    override a(): void {}
    d(): void {}
    override b(): void {}
    e(): void {}
}
"#;
        let results = nom_for(src);
        for c in &results {
            assert_eq!(c.nom, c.noam + c.noom, "NOM = NOAM + NOOM for {}", c.class_name);
        }
        let child = results.iter().find(|c| c.class_name == "Child").unwrap();
        assert_eq!(child.nom, 4);
        assert_eq!(child.noom, 2);
        assert_eq!(child.noam, 2);
    }

    // ── Multiple classes in one file ─────────────────────────────────────────

    #[test]
    fn test_multiple_classes_independent() {
        let src = r#"
class A {
    foo(): void {}
}

class B {
    bar(): void {}
    baz(): void {}
}
"#;
        let results = nom_for(src);
        assert_eq!(results.len(), 2);
        let a = results.iter().find(|c| c.class_name == "A").unwrap();
        assert_eq!(a.nom, 1);
        let b = results.iter().find(|c| c.class_name == "B").unwrap();
        assert_eq!(b.nom, 2);
    }

    // ── Line number ──────────────────────────────────────────────────────────

    #[test]
    fn test_line_number() {
        let src = "class Foo {\n    method(): void {}\n}";
        let c = first(src);
        assert_eq!(c.class_name, "Foo");
        assert_eq!(c.line, 1);
    }
}
