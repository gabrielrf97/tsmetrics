//! Number of Used Components (NUC) metric (S-164) — React component level.
//!
//! Definition:
//!   NUC = number of distinct React component references (PascalCase JSX tags)
//!         in a component's return value or render method, excluding HTML
//!         intrinsics (lowercase tags such as `div`, `span`, `input`).
//!
//! Scope:
//!   - Function components: `function_declaration`, `function_expression`, or
//!     `arrow_function` whose name starts with an uppercase letter.
//!   - Class components: any class containing a `render()` method.
//!
//! Counting rules:
//!   - Each unique PascalCase tag name counts once regardless of how many
//!     times it appears.
//!   - Traversal stops at nested `function_declaration`, `function_expression`,
//!     and PascalCase-assigned `arrow_function` nodes so that inner component
//!     definitions do not contribute their JSX to the outer component's NUC.
//!   - Inline `arrow_function` callbacks (e.g. inside `.map(…)`) are traversed
//!     because they are part of the render scope and are not assigned to a
//!     PascalCase variable.

use std::collections::HashSet;

use serde::Serialize;
use tree_sitter::Node;

// ── Public types ─────────────────────────────────────────────────────────────

/// NUC result for a single React component.
#[derive(Debug, Clone, Serialize)]
pub struct ComponentNuc {
    /// Component name, or `<anonymous>` when unnamed.
    pub component_name: String,
    /// 1-based line where the component starts.
    pub line: usize,
    /// Number of distinct React component references used in the render output.
    pub nuc: usize,
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Compute NUC for every React component found under `root`.
///
/// Both function components (PascalCase named functions / arrow functions) and
/// class components (classes that have a `render()` method) are detected.
pub fn compute_used_components(root: Node, source: &[u8]) -> Vec<ComponentNuc> {
    let mut results = Vec::new();
    collect_components(root, source, &mut results);
    results
}

// ── Component detection ───────────────────────────────────────────────────────

fn collect_components(node: Node, source: &[u8], out: &mut Vec<ComponentNuc>) {
    match node.kind() {
        // Named function declaration: `function MyComponent() { … }`
        "function_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = name_node.utf8_text(source).unwrap_or("").to_string();
                if is_pascal_case(&name) {
                    out.push(ComponentNuc {
                        component_name: name,
                        line: node.start_position().row + 1,
                        nuc: count_jsx_component_refs(node, source),
                    });
                }
            }
        }

        // Arrow function / function expression assigned to a PascalCase variable:
        //   `const MyComp = () => …`  or  `const MyComp = function() { … }`
        "variable_declarator" => {
            if let (Some(name_node), Some(value_node)) = (
                node.child_by_field_name("name"),
                node.child_by_field_name("value"),
            ) {
                let name = name_node.utf8_text(source).unwrap_or("").to_string();
                if is_pascal_case(&name)
                    && matches!(value_node.kind(), "arrow_function" | "function_expression")
                {
                    out.push(ComponentNuc {
                        component_name: name,
                        line: node.start_position().row + 1,
                        nuc: count_jsx_component_refs(value_node, source),
                    });
                    // Still recurse so that inner definitions are picked up too.
                }
            }
        }

        // Class component: any class that has a `render()` method.
        "class_declaration" | "class" => {
            if let Some(render_body) = find_render_body(node, source) {
                let name = node
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("<anonymous>")
                    .to_string();
                out.push(ComponentNuc {
                    component_name: name,
                    line: node.start_position().row + 1,
                    nuc: count_jsx_component_refs(render_body, source),
                });
            }
        }

        _ => {}
    }

    // Always recurse so that nested / multiple components in the same file
    // are all discovered.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_components(child, source, out);
    }
}

// ── JSX reference counting ────────────────────────────────────────────────────

/// Count distinct PascalCase JSX tag references under `root`.
///
/// Descends into the full subtree but **skips** nested `function_declaration`,
/// `function_expression`, and `arrow_function` nodes that are assigned to a
/// PascalCase variable (i.e. inner arrow-function components like
/// `const Inner = () => …`) to avoid attributing a nested component's JSX to
/// the outer component.  Inline `arrow_function` callbacks (render callbacks
/// such as `.map(item => <ListItem />)`) are traversed normally because they
/// are not assigned to a PascalCase variable.
fn count_jsx_component_refs(root: Node, source: &[u8]) -> usize {
    let mut seen: HashSet<String> = HashSet::new();
    // is_root=true so the root node itself is never skipped.
    collect_jsx_refs(root, source, &mut seen, true);
    seen.len()
}

fn collect_jsx_refs(node: Node, source: &[u8], seen: &mut HashSet<String>, is_root: bool) {
    // Stop at nested named function definitions (but never skip the root
    // component node itself, which may be a function_declaration).
    if !is_root && matches!(node.kind(), "function_declaration" | "function_expression") {
        return;
    }

    // Stop at arrow functions that are assigned to a PascalCase variable
    // (i.e. inner arrow-function components like `const Inner = () => …`).
    // Inline arrow callbacks (e.g. inside `.map(…)`) are NOT assigned to a
    // PascalCase variable_declarator and continue to be traversed normally.
    if !is_root && node.kind() == "arrow_function" {
        if let Some(parent) = node.parent() {
            if parent.kind() == "variable_declarator" {
                if let Some(name_node) = parent.child_by_field_name("name") {
                    let name = name_node.utf8_text(source).unwrap_or("");
                    if is_pascal_case(name) {
                        return;
                    }
                }
            }
        }
    }

    match node.kind() {
        // Opening tags (<Button …>) and self-closing tags (<Icon />)
        "jsx_opening_element" | "jsx_self_closing_element" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = name_node.utf8_text(source).unwrap_or("");
                if is_pascal_case(name) {
                    seen.insert(name.to_string());
                }
            }
            // Fall through to recurse into children (handles <A><B /></A>).
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_jsx_refs(child, source, seen, false);
    }
}

// ── Class component helpers ───────────────────────────────────────────────────

/// Return the statement body of the `render()` method if one exists in `class_node`.
fn find_render_body<'a>(class_node: Node<'a>, source: &[u8]) -> Option<Node<'a>> {
    let body = class_node.child_by_field_name("body")?;
    let mut cursor = body.walk();
    for member in body.children(&mut cursor) {
        if member.kind() == "method_definition" {
            let is_render = member
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .map(|n| n == "render")
                .unwrap_or(false);
            if is_render {
                return member.child_by_field_name("body");
            }
        }
    }
    None
}

// ── Utilities ─────────────────────────────────────────────────────────────────

/// Returns `true` when `name` starts with an ASCII uppercase letter (PascalCase
/// React convention).  Empty strings and HTML intrinsics (lowercase) return
/// `false`.
fn is_pascal_case(name: &str) -> bool {
    name.chars()
        .next()
        .map(|c| c.is_ascii_uppercase())
        .unwrap_or(false)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_tsx;

    fn nuc_for(src: &str) -> Vec<ComponentNuc> {
        let tree = parse_tsx(src).expect("parse failed");
        compute_used_components(tree.root_node(), src.as_bytes())
    }

    fn first(src: &str) -> ComponentNuc {
        let mut v = nuc_for(src);
        assert!(!v.is_empty(), "no component found");
        v.remove(0)
    }

    fn find_component<'a>(results: &'a [ComponentNuc], name: &str) -> &'a ComponentNuc {
        results
            .iter()
            .find(|c| c.component_name == name)
            .unwrap_or_else(|| panic!("component '{}' not found", name))
    }

    // ── Multiple React components used ───────────────────────────────────────

    #[test]
    fn test_multiple_react_components_counted() {
        let src = r#"
function Dashboard() {
  return (
    <div>
      <Header />
      <Sidebar />
      <MainContent />
    </div>
  );
}
"#;
        let c = first(src);
        assert_eq!(c.component_name, "Dashboard");
        assert_eq!(c.nuc, 3, "Header, Sidebar, MainContent");
    }

    // ── Only HTML intrinsics — NUC is zero ───────────────────────────────────

    #[test]
    fn test_only_html_tags_nuc_zero() {
        let src = r#"
function Layout() {
  return (
    <div>
      <header><h1>Title</h1></header>
      <main><p>Body</p></main>
      <footer><span>Footer</span></footer>
    </div>
  );
}
"#;
        let c = first(src);
        assert_eq!(c.component_name, "Layout");
        assert_eq!(c.nuc, 0, "all lowercase intrinsics");
    }

    // ── Mixed HTML and React components ──────────────────────────────────────

    #[test]
    fn test_mixed_html_and_components() {
        let src = r#"
function Page() {
  return (
    <div>
      <Navbar />
      <main>
        <ArticleList />
      </main>
      <Footer />
    </div>
  );
}
"#;
        let c = first(src);
        assert_eq!(c.component_name, "Page");
        assert_eq!(c.nuc, 3, "Navbar, ArticleList, Footer");
    }

    // ── No JSX at all — NUC is zero ──────────────────────────────────────────

    #[test]
    fn test_no_jsx_nuc_zero() {
        let src = r#"
function calculateTotal(items: number[]): number {
  return items.reduce((sum, item) => sum + item, 0);
}
"#;
        // Not a React component (lowercase name), so no entry in results.
        let v = nuc_for(src);
        assert!(v.is_empty(), "non-component function should not appear");
    }

    // ── Duplicate component references count only once ────────────────────────

    #[test]
    fn test_duplicate_refs_counted_once() {
        let src = r#"
function List() {
  return (
    <ul>
      <ListItem />
      <ListItem />
      <ListItem />
    </ul>
  );
}
"#;
        let c = first(src);
        assert_eq!(c.nuc, 1, "ListItem appears three times but counts once");
    }

    // ── Arrow function component ──────────────────────────────────────────────

    #[test]
    fn test_arrow_function_component() {
        let src = r#"
const Card = () => (
  <div>
    <CardHeader />
    <CardBody />
  </div>
);
"#;
        let c = first(src);
        assert_eq!(c.component_name, "Card");
        assert_eq!(c.nuc, 2, "CardHeader and CardBody");
    }

    // ── Class component (render method) ──────────────────────────────────────

    #[test]
    fn test_class_component_render_method() {
        let src = r#"
class UserProfile extends React.Component {
  render() {
    return (
      <div>
        <Avatar />
        <UserInfo />
      </div>
    );
  }
}
"#;
        let c = first(src);
        assert_eq!(c.component_name, "UserProfile");
        assert_eq!(c.nuc, 2, "Avatar and UserInfo");
    }

    // ── Class without render — not detected ──────────────────────────────────

    #[test]
    fn test_class_without_render_not_detected() {
        let src = r#"
class UtilityClass {
  static helper(): string { return "help"; }
}
"#;
        let v = nuc_for(src);
        assert!(v.is_empty(), "class without render() is not a React component");
    }

    // ── Multiple components in one file ──────────────────────────────────────

    #[test]
    fn test_multiple_components_independent() {
        let src = r#"
function Header() {
  return <nav><Logo /></nav>;
}

function Footer() {
  return <footer><CopyrightNotice /><SocialLinks /></footer>;
}
"#;
        let results = nuc_for(src);
        assert_eq!(results.len(), 2);
        assert_eq!(find_component(&results, "Header").nuc, 1);
        assert_eq!(find_component(&results, "Footer").nuc, 2);
    }

    // ── Self-closing and opening tags both counted ────────────────────────────

    #[test]
    fn test_self_closing_and_opening_tags() {
        let src = r#"
function Form() {
  return (
    <FormWrapper>
      <TextInput />
      <SelectInput />
      <SubmitButton />
    </FormWrapper>
  );
}
"#;
        let c = first(src);
        // FormWrapper (opening), TextInput, SelectInput, SubmitButton (all self-closing)
        assert_eq!(c.nuc, 4);
    }

    // ── Render callback arrow functions are traversed ─────────────────────────

    #[test]
    fn test_jsx_in_render_callback_counted() {
        let src = r#"
function ItemList() {
  return (
    <ul>
      {items.map(item => <ListItem key={item.id} />)}
    </ul>
  );
}
"#;
        let c = first(src);
        assert_eq!(c.component_name, "ItemList");
        assert_eq!(c.nuc, 1, "ListItem inside arrow callback counts");
    }

    // ── Nested arrow-function component JSX excluded ──────────────────────────

    #[test]
    fn test_nested_arrow_function_component_excluded() {
        let src = r#"
function Outer() {
  const Inner = () => <Button />;
  return <Inner />;
}
"#;
        let results = nuc_for(src);
        let outer = find_component(&results, "Outer");
        // Only <Inner /> counts for Outer; <Button /> is inside Inner's body.
        assert_eq!(outer.nuc, 1, "Inner is used by Outer; Button is not");

        let inner = find_component(&results, "Inner");
        assert_eq!(inner.nuc, 1, "Inner uses Button");
    }

    // ── Nested function declaration JSX excluded ──────────────────────────────

    #[test]
    fn test_nested_function_declaration_excluded() {
        let src = r#"
function Outer() {
  function Inner() { return <Button />; }
  return <Inner />;
}
"#;
        let results = nuc_for(src);
        let outer = find_component(&results, "Outer");
        // Only <Inner /> counts for Outer; <Button /> is inside Inner's body.
        assert_eq!(outer.nuc, 1, "Inner is used by Outer; Button is not");

        let inner = find_component(&results, "Inner");
        assert_eq!(inner.nuc, 1, "Inner uses Button");
    }

    // ── is_pascal_case helper ─────────────────────────────────────────────────

    #[test]
    fn test_is_pascal_case() {
        assert!(is_pascal_case("MyComponent"));
        assert!(is_pascal_case("Button"));
        assert!(is_pascal_case("A"));
        assert!(!is_pascal_case("div"));
        assert!(!is_pascal_case("span"));
        assert!(!is_pascal_case(""));
        assert!(!is_pascal_case("myComponent"));
    }
}
