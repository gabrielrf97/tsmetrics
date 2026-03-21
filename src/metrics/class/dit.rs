use std::collections::HashMap;
use tree_sitter::Node;

/// The inheritance info for a single class found in a file.
#[derive(Debug, Clone)]
pub struct ClassDit {
    /// Class name.
    pub name: String,
    /// DIT value: 0 for root classes, 1 for direct subclasses, etc.
    pub dit: usize,
    /// Line where the class is declared (1-based).
    pub line: usize,
}

/// Return the direct parent name (from `extends`) for each named class in the file.
///
/// The map contains an entry for every class found (declarations and named class
/// expressions).  The value is `None` when the class has no `extends` clause.
pub fn collect_parent_map(root: Node, source: &[u8]) -> HashMap<String, Option<String>> {
    collect_raw_classes(root, source)
        .into_iter()
        .map(|(name, parent, _line)| (name, parent))
        .collect()
}

/// Compute the Depth of Inheritance Tree (DIT) for every class declared in a file.
///
/// DIT is the maximum number of edges in the inheritance path from a class to the
/// root of its hierarchy.  A class with no parent has DIT = 0.
/// When the parent is not found in the same file (e.g. a built-in or an imported
/// type) it is treated as a root, so that child has DIT = 1.
pub fn compute_dit(root: Node, source: &[u8]) -> Vec<ClassDit> {
    let raw = collect_raw_classes(root, source);

    // Build a name → parent lookup for in-file classes.
    let parent_map: HashMap<&str, Option<&str>> = raw
        .iter()
        .map(|(name, parent, _)| (name.as_str(), parent.as_deref()))
        .collect();

    let mut cache: HashMap<String, usize> = HashMap::new();

    raw.iter()
        .map(|(name, _, line)| {
            let dit = resolve_dit(name, &parent_map, &mut cache, &mut vec![]);
            ClassDit {
                name: name.clone(),
                dit,
                line: *line,
            }
        })
        .collect()
}

/// Recursively resolve DIT, memoising results and guarding against cycles.
fn resolve_dit(
    name: &str,
    parent_map: &HashMap<&str, Option<&str>>,
    cache: &mut HashMap<String, usize>,
    visiting: &mut Vec<String>,
) -> usize {
    if let Some(&cached) = cache.get(name) {
        return cached;
    }
    // Cycle guard (shouldn't occur in valid TypeScript).
    if visiting.iter().any(|v| v == name) {
        return 0;
    }
    let dit = match parent_map.get(name) {
        // Class not declared in this file (built-in / imported) → treat as root.
        None => 0,
        // Class declared but has no `extends`.
        Some(None) => 0,
        // Class extends another.
        Some(Some(parent)) => {
            visiting.push(name.to_string());
            let parent_dit = resolve_dit(parent, parent_map, cache, visiting);
            visiting.pop();
            parent_dit + 1
        }
    };
    cache.insert(name.to_string(), dit);
    dit
}

// ── AST helpers ──────────────────────────────────────────────────────────────

/// Returns (class_name, Option<parent_name>, line) for every class declaration.
fn collect_raw_classes(root: Node, source: &[u8]) -> Vec<(String, Option<String>, usize)> {
    let mut out = Vec::new();
    walk_classes(root, source, &mut out);
    out
}

fn walk_classes(node: Node, source: &[u8], out: &mut Vec<(String, Option<String>, usize)>) {
    let kind = node.kind();
    let is_class = match kind {
        "class_declaration" | "abstract_class_declaration" => true,
        // Named class expressions: `const X = class ClassName extends Base {}`.
        // The bare "class" keyword leaf also has kind "class", so require a body.
        "class" => node.child_by_field_name("body").is_some(),
        _ => false,
    };
    if is_class {
        if let Some(entry) = extract_class_entry(node, source) {
            out.push(entry);
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_classes(child, source, out);
    }
}

fn extract_class_entry(node: Node, source: &[u8]) -> Option<(String, Option<String>, usize)> {
    let name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())?
        .to_string();
    let line = node.start_position().row + 1;
    let parent = find_extends_name(node, source);
    Some((name, parent, line))
}

/// Walk the direct children of a class node to find the parent name from
/// an `extends_clause`, handling both flat and `class_heritage`-wrapped layouts.
fn find_extends_name(class_node: Node, source: &[u8]) -> Option<String> {
    let mut cursor = class_node.walk();
    for child in class_node.children(&mut cursor) {
        match child.kind() {
            "class_heritage" => {
                // Newer grammar: extends_clause is nested inside class_heritage.
                let mut c2 = child.walk();
                for hc in child.children(&mut c2) {
                    if hc.kind() == "extends_clause" {
                        return extract_from_extends_clause(hc, source);
                    }
                }
            }
            "extends_clause" => {
                // Older grammar: extends_clause directly under class_declaration.
                return extract_from_extends_clause(child, source);
            }
            _ => {}
        }
    }
    None
}

/// Pull the parent class name out of an `extends_clause` node.
///
/// The clause looks like `extends Foo<T>` or `extends A.B`.  We want the
/// base identifier before any generic type arguments.
fn extract_from_extends_clause(extends_node: Node, source: &[u8]) -> Option<String> {
    // Try the `value` field first (present in most grammar versions).
    if let Some(value) = extends_node.child_by_field_name("value") {
        return Some(base_name(value.utf8_text(source).ok()?));
    }
    // Fallback: skip the `extends` keyword and return the first real child.
    let mut cursor = extends_node.walk();
    for child in extends_node.children(&mut cursor) {
        if child.kind() == "extends" || child.is_extra() {
            continue;
        }
        if let Ok(text) = child.utf8_text(source) {
            return Some(base_name(text));
        }
    }
    None
}

/// Strip generic type arguments from a name, e.g. `Array<string>` → `Array`.
fn base_name(text: &str) -> String {
    text.split('<').next().unwrap_or(text).trim().to_string()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_typescript;

    fn dit_map(source: &str) -> HashMap<String, usize> {
        let tree = parse_typescript(source).expect("parse failed");
        compute_dit(tree.root_node(), source.as_bytes())
            .into_iter()
            .map(|c| (c.name, c.dit))
            .collect()
    }

    // ── Basic cases ───────────────────────────────────────────────────────────

    #[test]
    fn no_parent_dit_is_zero() {
        let m = dit_map("class A {}");
        assert_eq!(m["A"], 0);
    }

    #[test]
    fn single_inheritance_dit_is_one() {
        let m = dit_map("class A {}\nclass B extends A {}");
        assert_eq!(m["A"], 0);
        assert_eq!(m["B"], 1);
    }

    #[test]
    fn three_level_chain() {
        let src = "class A {}\nclass B extends A {}\nclass C extends B {}";
        let m = dit_map(src);
        assert_eq!(m["A"], 0);
        assert_eq!(m["B"], 1);
        assert_eq!(m["C"], 2);
    }

    #[test]
    fn four_level_chain() {
        let src = "class A {}\nclass B extends A {}\nclass C extends B {}\nclass D extends C {}";
        let m = dit_map(src);
        assert_eq!(m["D"], 3);
    }

    // ── Built-in / external parent ────────────────────────────────────────────

    #[test]
    fn extends_builtin_dit_is_one() {
        // Error is a built-in; it is not declared in the file, so DIT = 1.
        let m = dit_map("class MyError extends Error {}");
        assert_eq!(m["MyError"], 1);
    }

    #[test]
    fn extends_builtin_with_generics() {
        // Array<string> — the generic parameter must be stripped.
        let m = dit_map("class MyList extends Array<string> {}");
        assert_eq!(m["MyList"], 1);
    }

    // ── Multi-class files ─────────────────────────────────────────────────────

    #[test]
    fn multiple_independent_classes() {
        let src = "class A {}\nclass B {}\nclass C {}";
        let m = dit_map(src);
        assert_eq!(m["A"], 0);
        assert_eq!(m["B"], 0);
        assert_eq!(m["C"], 0);
    }

    #[test]
    fn sibling_subclasses() {
        let src = "class Base {}\nclass X extends Base {}\nclass Y extends Base {}";
        let m = dit_map(src);
        assert_eq!(m["Base"], 0);
        assert_eq!(m["X"], 1);
        assert_eq!(m["Y"], 1);
    }

    #[test]
    fn deep_hierarchy_with_branches() {
        let src = r#"
            class A {}
            class B extends A {}
            class C extends B {}
            class D extends A {}
        "#;
        let m = dit_map(src);
        assert_eq!(m["A"], 0);
        assert_eq!(m["B"], 1);
        assert_eq!(m["C"], 2);
        assert_eq!(m["D"], 1);
    }

    // ── Line numbers ──────────────────────────────────────────────────────────

    #[test]
    fn line_numbers_are_correct() {
        let src = "class A {}\nclass B extends A {}";
        let tree = parse_typescript(src).unwrap();
        let classes = compute_dit(tree.root_node(), src.as_bytes());
        let a = classes.iter().find(|c| c.name == "A").unwrap();
        let b = classes.iter().find(|c| c.name == "B").unwrap();
        assert_eq!(a.line, 1);
        assert_eq!(b.line, 2);
    }

    // ── Abstract classes ──────────────────────────────────────────────────────

    #[test]
    fn abstract_class_no_parent() {
        let m = dit_map("abstract class A {}");
        assert_eq!(m["A"], 0);
    }

    #[test]
    fn abstract_class_extends_concrete() {
        let src = "class Base {}\nabstract class Mid extends Base {}\nclass Leaf extends Mid {}";
        let m = dit_map(src);
        assert_eq!(m["Base"], 0);
        assert_eq!(m["Mid"], 1);
        assert_eq!(m["Leaf"], 2);
    }

    // ── Named class expressions ───────────────────────────────────────────────
    // Regression: `const X = class Foo extends Base {}` was silently dropped
    // because walk_classes only visited `class_declaration` nodes.

    #[test]
    fn named_class_expression_no_parent_dit_is_zero() {
        let m = dit_map("const Foo = class FooClass {}");
        assert_eq!(m.get("FooClass").copied(), Some(0));
    }

    #[test]
    fn named_class_expression_extends_dit_is_one() {
        let src = "class Base {}\nconst Foo = class FooClass extends Base {}";
        let m = dit_map(src);
        assert_eq!(m["Base"], 0);
        assert_eq!(m["FooClass"], 1);
    }

    #[test]
    fn named_class_expression_in_deep_chain() {
        let src = "class A {}\nclass B extends A {}\nconst C = class CClass extends B {}";
        let m = dit_map(src);
        assert_eq!(m["A"], 0);
        assert_eq!(m["B"], 1);
        assert_eq!(m["CClass"], 2);
    }
}
