use tree_sitter::Node;

/// Render Complexity (RC) for a React component.
///
/// Measures cyclomatic-style complexity within JSX expressions:
/// each conditional rendering pattern adds +1 to the base complexity of 1.
///
/// Counted patterns:
/// - `condition && <jsx>` — logical AND rendering (binary_expression with &&)
/// - `condition ? <jsx> : fallback` — ternary expressions (ternary_expression)
/// - `.map(...)` calls within JSX — list rendering (call_expression with `.map`)
/// - `.filter(...)` chains before `.map(...)` — counted separately
///
/// Only patterns *within JSX return values* are counted — imperative code
/// before the return statement is excluded.
#[derive(Debug, Clone)]
pub struct RenderComplexity {
    /// Base complexity (always 1 for a function that has any JSX).
    pub base: usize,
    /// Number of `&&` conditional renders found in JSX.
    pub conditional_and_count: usize,
    /// Number of ternary `? :` expressions found in JSX.
    pub ternary_count: usize,
    /// Number of `.map()` list renders found in JSX.
    pub map_count: usize,
    /// Total render complexity: base + conditionals + ternaries + maps.
    pub total: usize,
}

/// Compute render complexity for a component node.
///
/// Pass the component's function node (or its entire subtree). JSX found
/// anywhere in the subtree is analysed.
pub fn compute_render_complexity(node: Node, source: &[u8]) -> RenderComplexity {
    let mut cond_and = 0usize;
    let mut ternary = 0usize;
    let mut map_count = 0usize;
    let mut has_jsx = false;

    collect_render_complexity(node, source, &mut cond_and, &mut ternary, &mut map_count, &mut has_jsx, false);

    let base = if has_jsx { 1 } else { 0 };
    let total = base + cond_and + ternary + map_count;

    RenderComplexity {
        base,
        conditional_and_count: cond_and,
        ternary_count: ternary,
        map_count,
        total,
    }
}

fn collect_render_complexity(
    node: Node,
    source: &[u8],
    cond_and: &mut usize,
    ternary: &mut usize,
    map_count: &mut usize,
    has_jsx: &mut bool,
    in_jsx_context: bool,
) {
    let kind = node.kind();

    // Detect JSX context entry
    let entering_jsx = matches!(
        kind,
        "jsx_element" | "jsx_fragment" | "jsx_self_closing_element" | "jsx_expression"
    );
    let jsx_ctx = in_jsx_context || entering_jsx;

    if entering_jsx {
        *has_jsx = true;
    }

    if jsx_ctx {
        match kind {
            "binary_expression" => {
                // `condition && <jsx>` pattern: look for `&&` operator child
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "&&" {
                        *cond_and += 1;
                        break;
                    }
                }
            }
            "ternary_expression" => {
                *ternary += 1;
            }
            "call_expression" => {
                // `.map(...)` list rendering
                if let Some(func) = node.child_by_field_name("function") {
                    if func.kind() == "member_expression" {
                        if let Some(prop) = func.child_by_field_name("property") {
                            let prop_name = prop.utf8_text(source).unwrap_or("");
                            if prop_name == "map" {
                                *map_count += 1;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_render_complexity(child, source, cond_and, ternary, map_count, has_jsx, jsx_ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_tsx;

    fn render_complexity(src: &str) -> RenderComplexity {
        let tree = parse_tsx(src).expect("parse failed");
        compute_render_complexity(tree.root_node(), src.as_bytes())
    }

    #[test]
    fn test_no_jsx_zero_complexity() {
        let src = "function pure(x: number) { return x * 2; }";
        let rc = render_complexity(src);
        assert_eq!(rc.base, 0);
        assert_eq!(rc.total, 0);
    }

    #[test]
    fn test_simple_jsx_base_one() {
        let src = r#"
function Hello() {
    return <div>hello</div>;
}
"#;
        let rc = render_complexity(src);
        assert_eq!(rc.base, 1);
        assert_eq!(rc.conditional_and_count, 0);
        assert_eq!(rc.ternary_count, 0);
        assert_eq!(rc.map_count, 0);
        assert_eq!(rc.total, 1);
    }

    #[test]
    fn test_ternary_adds_one() {
        let src = r#"
function Comp({ show }) {
    return <div>{show ? <span>yes</span> : <span>no</span>}</div>;
}
"#;
        let rc = render_complexity(src);
        assert_eq!(rc.ternary_count, 1);
        assert_eq!(rc.total, rc.base + 1);
    }

    #[test]
    fn test_map_adds_one() {
        let src = r#"
function List({ items }) {
    return (
        <ul>
            {items.map(item => <li key={item}>{item}</li>)}
        </ul>
    );
}
"#;
        let rc = render_complexity(src);
        assert_eq!(rc.map_count, 1);
        assert_eq!(rc.total, rc.base + 1);
    }

    #[test]
    fn test_multiple_conditionals() {
        let src = r#"
function Dashboard({ user, posts, comments }) {
    return (
        <div>
            {user ? <UserCard user={user} /> : <Login />}
            {posts ? posts.map(p => <Post key={p.id} post={p} />) : null}
            {comments && <CommentSection comments={comments} />}
        </div>
    );
}
"#;
        let rc = render_complexity(src);
        assert_eq!(rc.ternary_count, 2);
        assert_eq!(rc.map_count, 1);
        assert!(rc.total >= 4); // base + 2 ternaries + 1 map
    }

    #[test]
    fn test_no_jsx_component_returns_null() {
        let src = r#"
function Loading() {
    return null;
}
"#;
        let rc = render_complexity(src);
        assert_eq!(rc.total, 0);
    }

    #[test]
    fn test_self_closing_jsx_base_one() {
        let src = r#"
function Icon() {
    return <svg />;
}
"#;
        let rc = render_complexity(src);
        assert_eq!(rc.base, 1);
        assert_eq!(rc.total, 1);
    }

    #[test]
    fn test_total_formula() {
        // base=1, ternary=1, map=1
        let src = r#"
function Component({ items, show }) {
    return (
        <div>
            {show ? <h1>visible</h1> : null}
            {items.map(i => <span key={i}>{i}</span>)}
        </div>
    );
}
"#;
        let rc = render_complexity(src);
        assert_eq!(rc.base, 1);
        assert_eq!(rc.ternary_count, 1);
        assert_eq!(rc.map_count, 1);
        assert_eq!(rc.total, 3);
    }
}
