use serde::Serialize;
use std::collections::HashSet;
use tree_sitter::Node;

/// RFC (Response For Class) metric for a single class.
///
/// RFC = NOM + |RS|
///
/// where NOM is the number of methods defined in the class and RS (Response Set)
/// is the set of *unique* method/function callees invoked across all method bodies.
///
/// TypeScript adaptation: a "callee" is the text of the `function` field of every
/// `call_expression` node found while walking method bodies, plus the `constructor`
/// field of every `new_expression` node (constructor calls).  Deduplication is
/// performed on the full callee text so that `this.foo()` and `foo()` are counted
/// separately, matching real-world coupling semantics.
#[derive(Debug, Clone, Serialize)]
pub struct ClassRfc {
    /// Name of the class (`<anonymous>` if unnamed).
    pub class_name: String,
    /// Line number where the class starts (1-based).
    pub line: usize,
    /// Number of methods defined in the class (NOM component).
    pub nom: usize,
    /// Number of unique callees found across all method bodies (|RS| component).
    pub unique_callees: usize,
    /// RFC = nom + unique_callees.
    pub rfc: usize,
}

/// Compute RFC for every class found under `root`.
pub fn compute_class_rfc(root: Node, source: &[u8]) -> Vec<ClassRfc> {
    let mut results = Vec::new();
    collect_classes(root, source, &mut results);
    results
}

fn collect_classes(node: Node, source: &[u8], out: &mut Vec<ClassRfc>) {
    let is_class = match node.kind() {
        "class_declaration" | "abstract_class_declaration" => true,
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

fn measure_class(class_node: Node, source: &[u8]) -> ClassRfc {
    let class_name = class_node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("<anonymous>")
        .to_string();

    let line = class_node.start_position().row + 1;

    let body = match class_node.child_by_field_name("body") {
        Some(b) => b,
        None => {
            return ClassRfc {
                class_name,
                line,
                nom: 0,
                unique_callees: 0,
                rfc: 0,
            }
        }
    };

    let nom = count_methods(body);
    let callees = collect_callees(body, source);
    let unique_callees = callees.len();
    let rfc = nom + unique_callees;

    ClassRfc {
        class_name,
        line,
        nom,
        unique_callees,
        rfc,
    }
}

/// Count `method_definition` children of a `class_body`.
fn count_methods(body: Node) -> usize {
    let mut cursor = body.walk();
    body.children(&mut cursor)
        .filter(|n| n.kind() == "method_definition")
        .count()
}

/// Walk all method bodies in the class and collect unique callee strings.
///
/// Callees come from two node kinds:
/// - `call_expression`  → the `function` field text  (e.g. `"this.foo"`, `"bar"`)
/// - `new_expression`   → the `constructor` field text (e.g. `"MyService"`)
fn collect_callees(body: Node, source: &[u8]) -> HashSet<String> {
    let mut callees = HashSet::new();
    let mut cursor = body.walk();
    for member in body.children(&mut cursor) {
        if member.kind() == "method_definition" {
            if let Some(method_body) = member.child_by_field_name("body") {
                walk_for_calls(method_body, source, &mut callees);
            }
        }
    }
    callees
}

/// Recursively walk `node`, inserting callee texts into `callees`.
fn walk_for_calls(node: Node, source: &[u8], callees: &mut HashSet<String>) {
    match node.kind() {
        "call_expression" => {
            if let Some(func) = node.child_by_field_name("function") {
                if let Ok(text) = func.utf8_text(source) {
                    callees.insert(text.to_string());
                }
            }
            // Also recurse into argument list to catch nested calls.
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                walk_for_calls(child, source, callees);
            }
        }
        "new_expression" => {
            if let Some(ctor) = node.child_by_field_name("constructor") {
                if let Ok(text) = ctor.utf8_text(source) {
                    callees.insert(text.to_string());
                }
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                walk_for_calls(child, source, callees);
            }
        }
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                walk_for_calls(child, source, callees);
            }
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_typescript;

    fn rfc_for(src: &str) -> Vec<ClassRfc> {
        let tree = parse_typescript(src).expect("parse failed");
        compute_class_rfc(tree.root_node(), src.as_bytes())
    }

    fn first(src: &str) -> ClassRfc {
        let mut v = rfc_for(src);
        assert!(!v.is_empty(), "no class found");
        v.remove(0)
    }

    // ── Empty class ──────────────────────────────────────────────────────────

    #[test]
    fn test_empty_class_rfc_is_zero() {
        let c = first("class Empty {}");
        assert_eq!(c.nom, 0);
        assert_eq!(c.unique_callees, 0);
        assert_eq!(c.rfc, 0);
    }

    // ── Class with methods but no calls ─────────────────────────────────────

    #[test]
    fn test_class_with_no_method_calls() {
        let src = r#"
class Pure {
    add(a: number, b: number): number { return a + b; }
    negate(x: number): number { return -x; }
}
"#;
        let c = first(src);
        assert_eq!(c.nom, 2);
        assert_eq!(c.unique_callees, 0, "no call expressions in bodies");
        assert_eq!(c.rfc, 2);
    }

    // ── Class calling its own (internal) methods ─────────────────────────────

    #[test]
    fn test_class_calling_internal_methods() {
        let src = r#"
class Formatter {
    private trim(s: string): string { return s.trim(); }
    private upper(s: string): string { return s.toUpperCase(); }
    format(s: string): string {
        return this.upper(this.trim(s));
    }
}
"#;
        let c = first(src);
        assert_eq!(c.nom, 3);
        // `this.upper`, `this.trim`, `s.trim`, `s.toUpperCase` are 4 unique callees.
        assert_eq!(c.unique_callees, 4);
        assert_eq!(c.rfc, 7);
    }

    // ── Class calling external methods ───────────────────────────────────────

    #[test]
    fn test_class_calling_external_methods() {
        let src = r#"
class Logger {
    log(msg: string): void {
        console.log(msg);
        console.error(msg);
        console.log(msg);
    }
}
"#;
        let c = first(src);
        assert_eq!(c.nom, 1);
        // `console.log` and `console.error` — deduplicated to 2 unique callees.
        assert_eq!(c.unique_callees, 2);
        assert_eq!(c.rfc, 3);
    }

    // ── Class with constructor (new) calls ───────────────────────────────────

    #[test]
    fn test_class_with_constructor_calls() {
        let src = r#"
class Factory {
    createA(): ServiceA { return new ServiceA(); }
    createB(): ServiceB { return new ServiceB(); }
    createDuplicate(): ServiceA { return new ServiceA(); }
}
"#;
        let c = first(src);
        assert_eq!(c.nom, 3);
        // `ServiceA` and `ServiceB` — `ServiceA` deduped → 2 unique callees.
        assert_eq!(c.unique_callees, 2);
        assert_eq!(c.rfc, 5);
    }

    // ── Same callee in multiple methods is counted once ──────────────────────

    #[test]
    fn test_duplicate_callees_across_methods_deduplicated() {
        let src = r#"
class Svc {
    a(): void { helper(); }
    b(): void { helper(); }
    c(): void { helper(); }
}
"#;
        let c = first(src);
        assert_eq!(c.nom, 3);
        assert_eq!(c.unique_callees, 1, "`helper` is the same callee in all three methods");
        assert_eq!(c.rfc, 4);
    }

    // ── Mixed internal and external callees ──────────────────────────────────

    #[test]
    fn test_mixed_callees() {
        let src = r#"
class Controller {
    private repo: Repo;
    find(id: number) {
        validate(id);
        return this.repo.findById(id);
    }
    save(data: unknown) {
        validate(data);
        this.repo.save(data);
        console.log("saved");
    }
}
"#;
        let c = first(src);
        assert_eq!(c.nom, 2);
        // `validate`, `this.repo.findById`, `this.repo.save`, `console.log` → 4
        assert_eq!(c.unique_callees, 4);
        assert_eq!(c.rfc, 6);
    }

    // ── RFC = NOM when there are no calls ────────────────────────────────────

    #[test]
    fn test_rfc_equals_nom_when_no_calls() {
        let src = r#"
class Trivial {
    foo(): void {}
    bar(): void {}
    baz(): void {}
}
"#;
        let c = first(src);
        assert_eq!(c.rfc, c.nom, "RFC must equal NOM when response set is empty");
    }

    // ── Multiple classes in one file ─────────────────────────────────────────

    #[test]
    fn test_multiple_classes_independent() {
        let src = r#"
class A {
    fn(): void { helper(); }
}
class B {
    fn(): void {}
}
"#;
        let results = rfc_for(src);
        assert_eq!(results.len(), 2);
        let a = results.iter().find(|c| c.class_name == "A").unwrap();
        assert_eq!(a.rfc, 2); // 1 method + 1 callee
        let b = results.iter().find(|c| c.class_name == "B").unwrap();
        assert_eq!(b.rfc, 1); // 1 method + 0 callees
    }

    // ── Line number and name ─────────────────────────────────────────────────

    #[test]
    fn test_class_name_and_line() {
        let src = "class MyClass {\n    foo(): void {}\n}";
        let c = first(src);
        assert_eq!(c.class_name, "MyClass");
        assert_eq!(c.line, 1);
    }
}
