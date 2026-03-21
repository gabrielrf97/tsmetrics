use serde::Serialize;
use tree_sitter::Node;

/// ATFD threshold: a method must access **more than** this many foreign
/// attributes to be a Feature Envy candidate.
///
/// Reference: *Object-Oriented Metrics in Practice*, Lanza & Marinescu 2006.
pub const ATFD_THRESHOLD: usize = 5;

/// A detected Feature Envy smell for a single method.
///
/// Feature Envy occurs when a method is more interested in the data of other
/// classes than in the data of its own class.  The method "envies" a foreign
/// class and arguably belongs there.
///
/// A method is flagged when **both** conditions hold:
/// - `atfd > ATFD_THRESHOLD` (accesses more than 5 foreign attributes), **and**
/// - `atfd > local_accesses` (more foreign accesses than own `this.x` accesses).
///
/// ATFD (Access To Foreign Data) counts `obj.property` expressions inside the
/// method body where `obj` is a plain identifier other than `this`.  Method
/// calls (`obj.method()`) are **not** counted — only data accesses.  Similarly,
/// `this.method()` calls are excluded from the local count; only `this.property`
/// reads/writes are counted.
///
/// Reference: *Object-Oriented Metrics in Practice*, Lanza & Marinescu 2006.
#[derive(Debug, Clone, Serialize)]
pub struct FeatureEnvyResult {
    /// Name of the method exhibiting Feature Envy.
    pub method_name: String,
    /// Name of the class that contains the method.
    pub class_name: String,
    /// Line number where the method is declared (1-based).
    pub line: usize,
    /// Number of foreign attribute accesses (ATFD).
    pub atfd: usize,
    /// Number of local `this.x` attribute accesses.
    pub local_accesses: usize,
}

/// Detect Feature Envy smells in all methods of all classes under `root`.
///
/// Walks every `class_declaration`, `abstract_class_declaration`, and named
/// class expression.  For each `method_definition` inside a class body the
/// method body is scanned for attribute accesses (see [`FeatureEnvyResult`]).
pub fn detect_feature_envy(root: Node, source: &[u8]) -> Vec<FeatureEnvyResult> {
    let mut results = Vec::new();
    collect_classes(root, source, &mut results);
    results
}

// ---------------------------------------------------------------------------
// AST traversal
// ---------------------------------------------------------------------------

fn collect_classes(node: Node, source: &[u8], out: &mut Vec<FeatureEnvyResult>) {
    let is_class = matches!(
        node.kind(),
        "class_declaration" | "abstract_class_declaration"
    ) || (node.kind() == "class" && node.child_by_field_name("body").is_some());

    if is_class {
        let class_name = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("<anonymous>")
            .to_string();

        if let Some(body) = node.child_by_field_name("body") {
            check_class_methods(body, &class_name, source, out);
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_classes(child, source, out);
    }
}

fn check_class_methods(
    body: Node,
    class_name: &str,
    source: &[u8],
    out: &mut Vec<FeatureEnvyResult>,
) {
    let mut cursor = body.walk();
    for member in body.children(&mut cursor) {
        if member.kind() == "method_definition" {
            let method_name = member
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("<anonymous>")
                .to_string();

            let line = member.start_position().row + 1;

            if let Some(method_body) = member.child_by_field_name("body") {
                let (atfd, local_accesses) = count_accesses(method_body, source);

                if atfd > ATFD_THRESHOLD && atfd > local_accesses {
                    out.push(FeatureEnvyResult {
                        method_name,
                        class_name: class_name.to_string(),
                        line,
                        atfd,
                        local_accesses,
                    });
                }
            }
        }
    }
}

/// Count foreign (ATFD) and local attribute accesses in a subtree.
///
/// Returns `(atfd, local_accesses)`.
fn count_accesses(node: Node, source: &[u8]) -> (usize, usize) {
    let mut foreign = 0usize;
    let mut local = 0usize;
    walk_accesses(node, source, false, &mut foreign, &mut local);
    (foreign, local)
}

/// Recursively walk the AST and count member-expression accesses.
///
/// `is_call_function` is `true` when this node is the callee of a
/// `call_expression` — in that case the node is a method call, not a data
/// access, and must not be counted.
fn walk_accesses(
    node: Node,
    source: &[u8],
    is_call_function: bool,
    foreign: &mut usize,
    local: &mut usize,
) {
    if node.kind() == "member_expression" && !is_call_function {
        if let Some(obj) = node.child_by_field_name("object") {
            match obj.kind() {
                "this" => *local += 1,
                "identifier" => *foreign += 1,
                _ => {}
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        // Mark a child as a call-function only when the current node is a
        // call_expression and the child is its `function` field.
        let child_is_call_fn = node.kind() == "call_expression"
            && node
                .child_by_field_name("function")
                .map(|f| f.id() == child.id())
                .unwrap_or(false);

        walk_accesses(child, source, child_is_call_fn, foreign, local);
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_typescript;

    fn detect(src: &str) -> Vec<FeatureEnvyResult> {
        let tree = parse_typescript(src).expect("parse failed");
        detect_feature_envy(tree.root_node(), src.as_bytes())
    }

    fn method_names(smells: &[FeatureEnvyResult]) -> Vec<&str> {
        smells.iter().map(|s| s.method_name.as_str()).collect()
    }

    // ── Empty / no accesses — never flagged ───────────────────────────────────

    #[test]
    fn empty_class_not_flagged() {
        let smells = detect("class Empty {}");
        assert!(smells.is_empty());
    }

    #[test]
    fn method_with_no_accesses_not_flagged() {
        let src = r#"
class A {
    doNothing(): void {}
}
"#;
        let smells = detect(src);
        assert!(smells.is_empty());
    }

    // ── Local-only accesses — not flagged ─────────────────────────────────────

    #[test]
    fn method_with_only_local_accesses_not_flagged() {
        // 7 this.x accesses, 0 foreign → not flagged
        let src = r#"
class A {
    compute(): number {
        return this.a + this.b + this.c + this.d + this.e + this.f + this.g;
    }
}
"#;
        let smells = detect(src);
        assert!(smells.is_empty(), "only local accesses must not be flagged");
    }

    // ── Feature Envy: ATFD > 5 AND atfd > local ───────────────────────────────

    #[test]
    fn method_with_six_foreign_and_zero_local_flagged() {
        // 6 foreign data accesses (a.x a.y a.z b.x b.y b.z), 0 local
        let src = r#"
class Report {
    generate(): number {
        return a.x + a.y + a.z + b.x + b.y + b.z;
    }
}
"#;
        let smells = detect(src);
        assert_eq!(
            smells.len(),
            1,
            "method with 6 foreign accesses and 0 local must be flagged"
        );
        assert_eq!(smells[0].method_name, "generate");
        assert_eq!(smells[0].atfd, 6);
        assert_eq!(smells[0].local_accesses, 0);
    }

    #[test]
    fn method_with_six_foreign_and_fewer_local_flagged() {
        // 6 foreign, 1 local → 6 > 5 AND 6 > 1 → flagged
        let src = r#"
class Processor {
    process(): void {
        const v1 = repo.data;
        const v2 = repo.status;
        const v3 = repo.count;
        const v4 = logger.level;
        const v5 = logger.enabled;
        const v6 = cache.hits;
        this.result = v1;
    }
}
"#;
        let smells = detect(src);
        assert_eq!(smells.len(), 1);
        assert_eq!(smells[0].atfd, 6);
        assert_eq!(smells[0].local_accesses, 1);
    }

    // ── Exactly 5 foreign — not flagged (threshold is strict >) ──────────────

    #[test]
    fn method_with_exactly_five_foreign_not_flagged() {
        // ATFD = 5, not > 5 → not flagged
        let src = r#"
class A {
    compute(): number {
        return a.x + a.y + a.z + b.x + b.y;
    }
}
"#;
        let smells = detect(src);
        assert!(
            smells.is_empty(),
            "exactly 5 foreign accesses must not be flagged (threshold is > 5)"
        );
    }

    // ── More local than foreign — not flagged ─────────────────────────────────

    #[test]
    fn method_with_more_local_than_foreign_not_flagged() {
        // 6 foreign but 10 local → ATFD > 5 but atfd NOT > local → not flagged
        let src = r#"
class A {
    compute(): number {
        return this.a + this.b + this.c + this.d + this.e
             + this.f + this.g + this.h + this.i + this.j
             + b.x + b.y + b.z + c.x + c.y + c.z;
    }
}
"#;
        let smells = detect(src);
        assert!(
            smells.is_empty(),
            "more local than foreign accesses must not be flagged"
        );
    }

    // ── Equal foreign and local — not flagged (strict >) ─────────────────────

    #[test]
    fn method_with_equal_foreign_and_local_not_flagged() {
        // 6 foreign, 6 local → ATFD > 5 but atfd NOT > local (equal) → not flagged
        let src = r#"
class A {
    compute(): number {
        return this.a + this.b + this.c + this.d + this.e + this.f
             + b.x + b.y + b.z + c.x + c.y + c.z;
    }
}
"#;
        let smells = detect(src);
        assert!(
            smells.is_empty(),
            "equal foreign and local must not be flagged (strict >)"
        );
    }

    // ── Method calls are NOT counted as ATFD ──────────────────────────────────

    #[test]
    fn method_calls_not_counted_as_atfd() {
        // obj.method() calls — these are call_expressions, not data accesses
        let src = r#"
class A {
    doWork(): void {
        obj.method1();
        obj.method2();
        obj.method3();
        obj.method4();
        obj.method5();
        obj.method6();
    }
}
"#;
        let smells = detect(src);
        assert!(
            smells.is_empty(),
            "method calls must not count as ATFD; got {:?}",
            smells
        );
    }

    // ── this.method() calls NOT counted as local accesses ────────────────────

    #[test]
    fn this_method_calls_not_counted_as_local() {
        // this.step1() and this.step2() are calls, not data accesses
        // foreign = 6, local = 0 → 6 > 5 AND 6 > 0 → flagged
        let src = r#"
class A {
    orchestrate(): void {
        this.step1();
        this.step2();
        const x = repo.a + repo.b + repo.c + repo.d + repo.e + repo.f;
    }
}
"#;
        let smells = detect(src);
        assert_eq!(
            smells.len(),
            1,
            "this.method() calls must not count as local accesses"
        );
        assert_eq!(smells[0].local_accesses, 0, "no local data accesses expected");
        assert_eq!(smells[0].atfd, 6);
    }

    // ── Multiple methods — only guilty ones returned ──────────────────────────

    #[test]
    fn only_guilty_methods_returned_per_class() {
        let src = r#"
class MyClass {
    clean(): void {
        this.x + this.y;
    }

    envious(): void {
        const v = repo.a + repo.b + repo.c + repo.d + repo.e + repo.f;
        this.x + this.y;
    }
}
"#;
        let smells = detect(src);
        assert_eq!(smells.len(), 1);
        assert_eq!(smells[0].method_name, "envious");
    }

    // ── Multiple classes — only guilty methods returned ───────────────────────

    #[test]
    fn mixed_file_only_guilty_methods_returned() {
        let src = r#"
class Clean {
    simple(): void {
        this.x + repo.a + repo.b;
    }
}

class Smelly {
    envious(): void {
        const v = repo.a + repo.b + repo.c + repo.d + repo.e + repo.f;
        this.x + this.y;
    }

    alsoClean(): void {
        this.a + this.b;
    }
}
"#;
        let smells = detect(src);
        assert_eq!(smells.len(), 1);
        assert_eq!(smells[0].class_name, "Smelly");
        assert_eq!(smells[0].method_name, "envious");
    }

    // ── Abstract class methods are checked ────────────────────────────────────

    #[test]
    fn abstract_class_methods_checked() {
        let src = r#"
abstract class AbstractProcessor {
    process(): void {
        const v = repo.a + repo.b + repo.c + repo.d + repo.e + repo.f;
    }
}
"#;
        let smells = detect(src);
        assert_eq!(smells.len(), 1);
        assert_eq!(smells[0].class_name, "AbstractProcessor");
        assert_eq!(smells[0].method_name, "process");
    }

    // ── Fields populated correctly ─────────────────────────────────────────────

    #[test]
    fn fields_populated_correctly() {
        let src = r#"
class MyClass {
    myMethod(): void {
        const a1 = repo.data;
        const a2 = repo.status;
        const a3 = cache.hits;
        const a4 = cache.misses;
        const a5 = logger.level;
        const a6 = logger.enabled;
        this.result = a1;
        this.count = a6;
    }
}
"#;
        let smells = detect(src);
        assert_eq!(smells.len(), 1);
        let s = &smells[0];
        assert_eq!(s.class_name, "MyClass");
        assert_eq!(s.method_name, "myMethod");
        assert_eq!(s.atfd, 6);
        assert_eq!(s.local_accesses, 2);
        assert!(s.line > 0);
    }

    // ── Line number is the method declaration line ─────────────────────────────

    #[test]
    fn line_number_is_method_start() {
        // Line 1: empty, Line 2: class A {, Line 3: method declaration
        let src = "\nclass A {\n    envious(): void {\n        const x = r.a + r.b + r.c + r.d + r.e + r.f;\n    }\n}\n";
        let smells = detect(src);
        assert_eq!(smells.len(), 1);
        assert_eq!(smells[0].line, 3, "line should be the method declaration line");
    }

    // ── Empty source ───────────────────────────────────────────────────────────

    #[test]
    fn empty_input_returns_empty() {
        let smells = detect("");
        assert!(smells.is_empty());
    }

    // ── Multiple guilty methods in one class ──────────────────────────────────

    #[test]
    fn multiple_guilty_methods_all_reported() {
        let src = r#"
class GodClass {
    methodA(): void {
        const x = a.p + a.q + a.r + b.p + b.q + b.r;
    }

    methodB(): void {
        const y = c.x + c.y + c.z + d.x + d.y + d.z;
    }

    clean(): void {
        this.data + this.count;
    }
}
"#;
        let smells = detect(src);
        let names = method_names(&smells);
        assert_eq!(smells.len(), 2);
        assert!(names.contains(&"methodA"), "methodA must be flagged");
        assert!(names.contains(&"methodB"), "methodB must be flagged");
        assert!(!names.contains(&"clean"), "clean must not be flagged");
    }

    // ── Class expression (named) is also checked ──────────────────────────────

    #[test]
    fn named_class_expression_methods_checked() {
        let src = r#"
const Foo = class FooClass {
    envious(): void {
        const x = r.a + r.b + r.c + r.d + r.e + r.f;
    }
};
"#;
        let smells = detect(src);
        assert_eq!(smells.len(), 1);
        assert_eq!(smells[0].class_name, "FooClass");
    }
}
