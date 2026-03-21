use tree_sitter::Node;

use crate::metrics::function::cyclo::cyclomatic_complexity;

/// Compute the Weighted Methods per Class (WMC) for a class node.
///
/// WMC = Σ CC(mᵢ) for all methods mᵢ in the class.
///
/// Each method's cyclomatic complexity is computed via the shared `cyclo` module.
/// Nested functions inside methods are not counted as separate methods.
/// If the class has no methods, WMC = 0.
pub fn compute_wmc(class_node: Node, source: &[u8]) -> usize {
    match find_class_body(class_node) {
        None => 0,
        Some(body) => iter_methods(body)
            .map(|method| cyclomatic_complexity(method, source))
            .sum(),
    }
}

/// Count the number of methods (including constructor, getters, setters) in a class.
pub fn count_methods(class_node: Node) -> usize {
    match find_class_body(class_node) {
        None => 0,
        Some(body) => iter_methods(body).count(),
    }
}

/// Extract the class name from a class declaration/expression node.
pub fn extract_class_name(class_node: Node, source: &[u8]) -> String {
    if let Some(name_node) = class_node.child_by_field_name("name") {
        return name_node
            .utf8_text(source)
            .unwrap_or("<anonymous>")
            .to_string();
    }
    "<anonymous>".to_string()
}

fn find_class_body(class_node: Node) -> Option<Node> {
    let mut cursor = class_node.walk();
    for child in class_node.children(&mut cursor) {
        if child.kind() == "class_body" {
            return Some(child);
        }
    }
    None
}

/// Iterate over immediate method children of a `class_body` node.
/// Matches both `method_definition` (concrete methods) and
/// `abstract_method_signature` (abstract methods in abstract classes).
fn iter_methods(body: Node) -> impl Iterator<Item = Node> {
    let children: Vec<Node> = {
        let mut out = Vec::new();
        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            if matches!(child.kind(), "method_definition" | "abstract_method_signature") {
                out.push(child);
            }
        }
        out
    };
    children.into_iter()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_typescript;

    fn parse(source: &str) -> tree_sitter::Tree {
        parse_typescript(source).expect("parse failed")
    }

    /// Walk the AST and return the first node matching one of the class kinds.
    /// tree-sitter-typescript uses "class_declaration" for `class Foo {}`,
    /// "abstract_class_declaration" for `abstract class Foo {}`, and
    /// "class" for class expressions like `const x = class {}`.
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

    // ── Empty class ────────────────────────────────────────────────────────────

    #[test]
    fn test_empty_class_wmc_is_zero() {
        let src = "class Empty {}";
        let tree = parse(src);
        let class = find_first_class(tree.root_node()).expect("no class found");
        assert_eq!(compute_wmc(class, src.as_bytes()), 0);
    }

    #[test]
    fn test_empty_class_method_count_is_zero() {
        let src = "class Empty {}";
        let tree = parse(src);
        let class = find_first_class(tree.root_node()).expect("no class found");
        assert_eq!(count_methods(class), 0);
    }

    // ── Constructor only ───────────────────────────────────────────────────────

    #[test]
    fn test_class_with_constructor_only_wmc_is_one() {
        // Constructor with no branches: CC = 1 (baseline), WMC = 1.
        let src = r#"
class Service {
    constructor(private name: string) {
        this.name = name;
    }
}
"#;
        let tree = parse(src);
        let class = find_first_class(tree.root_node()).expect("no class found");
        assert_eq!(count_methods(class), 1);
        assert_eq!(compute_wmc(class, src.as_bytes()), 1);
    }

    // ── Simple methods ─────────────────────────────────────────────────────────

    #[test]
    fn test_class_with_three_simple_methods() {
        // Each method has no decision points → CC = 1 each → WMC = 3.
        let src = r#"
class Simple {
    foo(): void { console.log("foo"); }
    bar(): void { console.log("bar"); }
    baz(): void { console.log("baz"); }
}
"#;
        let tree = parse(src);
        let class = find_first_class(tree.root_node()).expect("no class found");
        assert_eq!(count_methods(class), 3);
        assert_eq!(compute_wmc(class, src.as_bytes()), 3);
    }

    // ── Complex methods ────────────────────────────────────────────────────────

    #[test]
    fn test_class_with_complex_methods() {
        // validate: 1 (base) + if(x>0) + if(x<100) = 3
        // process:  1 (base) + while = 2
        // WMC = 5
        let src = r#"
class Processor {
    validate(x: number): boolean {
        if (x > 0) {
            if (x < 100) {
                return true;
            }
        }
        return false;
    }
    process(items: number[]): void {
        let i = 0;
        while (i < items.length) {
            i++;
        }
    }
}
"#;
        let tree = parse(src);
        let class = find_first_class(tree.root_node()).expect("no class found");
        assert_eq!(count_methods(class), 2);
        assert_eq!(compute_wmc(class, src.as_bytes()), 5);
    }

    // ── Getters and setters ────────────────────────────────────────────────────

    #[test]
    fn test_class_with_getters_and_setters() {
        // getter: CC = 1, setter: CC = 1 → WMC = 2.
        let src = r#"
class Counter {
    private _count: number = 0;
    get count(): number { return this._count; }
    set count(v: number) { this._count = v; }
}
"#;
        let tree = parse(src);
        let class = find_first_class(tree.root_node()).expect("no class found");
        assert_eq!(count_methods(class), 2);
        assert_eq!(compute_wmc(class, src.as_bytes()), 2);
    }

    // ── Nested functions do not inflate WMC ───────────────────────────────────

    #[test]
    fn test_nested_functions_not_counted_as_methods() {
        // Only 1 method_definition; the arrow function inside is not a class method.
        // compute: CC = 1 (base) + 1 (if) = 2
        // WMC = 2
        let src = r#"
class WithNested {
    compute(x: number): number {
        const transform = (v: number) => v * 2;
        if (x > 0) {
            return transform(x);
        }
        return 0;
    }
}
"#;
        let tree = parse(src);
        let class = find_first_class(tree.root_node()).expect("no class found");
        assert_eq!(count_methods(class), 1);
        assert_eq!(compute_wmc(class, src.as_bytes()), 2);
    }

    // ── Name extraction ────────────────────────────────────────────────────────

    #[test]
    fn test_extract_class_name() {
        let src = "class MyClass {}";
        let tree = parse(src);
        let class = find_first_class(tree.root_node()).expect("no class found");
        assert_eq!(extract_class_name(class, src.as_bytes()), "MyClass");
    }

    #[test]
    fn test_extract_anonymous_class_name() {
        let src = "const x = class {};";
        let tree = parse(src);
        let class = find_first_class(tree.root_node()).expect("no class found");
        assert_eq!(extract_class_name(class, src.as_bytes()), "<anonymous>");
    }

    // ── Abstract classes ───────────────────────────────────────────────────────

    #[test]
    fn test_abstract_class_with_abstract_methods() {
        // Two abstract methods (no body, CC = 1 each) + one concrete method (CC = 1).
        // method_count = 3, WMC = 3.
        let src = r#"
abstract class Shape {
    abstract area(): number;
    abstract perimeter(): number;
    describe(): string { return "shape"; }
}
"#;
        let tree = parse(src);
        let class = find_first_class(tree.root_node()).expect("no class found");
        assert_eq!(count_methods(class), 3);
        assert_eq!(compute_wmc(class, src.as_bytes()), 3);
    }

    #[test]
    fn test_abstract_class_only_abstract_methods() {
        // Three abstract methods, no concrete methods.
        // method_count = 3, WMC = 3.
        let src = r#"
abstract class Animal {
    abstract speak(): void;
    abstract move(): void;
    abstract eat(): void;
}
"#;
        let tree = parse(src);
        let class = find_first_class(tree.root_node()).expect("no class found");
        assert_eq!(count_methods(class), 3);
        assert_eq!(compute_wmc(class, src.as_bytes()), 3);
    }
}
