//! JSX Nesting Level metric (S-165) — component level.
//!
//! Definition: Maximum depth of nested JSX elements within a component's
//! return statement (or any JSX expression at module scope).
//!
//! Depth counting rules:
//! - Every `jsx_element` or `jsx_fragment` increments the depth by 1 as we
//!   descend into it, regardless of whether it contains children.
//! - `jsx_self_closing_element` nodes are leaves — they do not open a new level.
//! - Depth starts at 1 for the outermost JSX element found.
//! - JSX inside `{expression}` children is counted normally.
//!
//! A result of 0 means no nested JSX was found (no element contains another);
//! this includes the case where no JSX is present at all, as well as source
//! that contains only self-closing elements.
//!
//! High nesting (typically > 4–5) signals a component that should be
//! decomposed into smaller sub-components.

use tree_sitter::{Node, Parser};

// ── public API ────────────────────────────────────────────────────────────────

/// Parse TSX `source` and return the maximum JSX nesting depth found anywhere
/// in the file.
///
/// Returns `0` when no nested JSX is present (no element contains another,
/// or no JSX is present at all).
pub fn max_jsx_nesting(source: &str) -> usize {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TSX.into())
        .expect("Error loading TSX grammar");
    let tree = parser.parse(source, None).expect("Failed to parse TSX source");
    max_depth(tree.root_node(), 0)
}

// ── private helpers ───────────────────────────────────────────────────────────

/// Returns `true` for AST nodes that open a JSX nesting level.
/// Self-closing elements are **not** included because they have no children.
#[inline]
fn is_jsx_container(node: Node<'_>) -> bool {
    matches!(node.kind(), "jsx_element" | "jsx_fragment")
}

/// Recursively compute the maximum JSX depth rooted at `node`.
///
/// `current_depth` is the depth of `node` itself (0 for the tree root).
/// When we enter a `jsx_element` or `jsx_fragment` we increment the depth for
/// all children.
fn max_depth(node: Node<'_>, current_depth: usize) -> usize {
    let child_depth = if is_jsx_container(node) {
        current_depth + 1
    } else {
        current_depth
    };

    let mut max = child_depth;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let d = max_depth(child, child_depth);
        if d > max {
            max = d;
        }
    }
    max
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── single / flat ─────────────────────────────────────────────────────────

    #[test]
    fn no_jsx_returns_zero() {
        let src = "const x = 1;";
        assert_eq!(max_jsx_nesting(src), 0);
    }

    #[test]
    fn single_self_closing_element_is_depth_zero() {
        // A self-closing element has no children, so it never opens a container
        // level. The result is 0 because `jsx_self_closing_element` is not a
        // `jsx_element` / `jsx_fragment` container.
        let src = "const el = <img />;";
        assert_eq!(max_jsx_nesting(src), 0);
    }

    #[test]
    fn single_element_with_no_children_is_depth_one() {
        // <div></div> — one jsx_element, no nested JSX children.
        let src = "const el = <div></div>;";
        assert_eq!(max_jsx_nesting(src), 1);
    }

    // ── flat JSX ─────────────────────────────────────────────────────────────

    #[test]
    fn flat_siblings_are_depth_one() {
        // All children are self-closing; the root wrapper is depth 1.
        let src = r#"
            const el = (
                <div>
                    <span />
                    <span />
                    <span />
                </div>
            );
        "#;
        assert_eq!(max_jsx_nesting(src), 1);
    }

    // ── nested JSX ────────────────────────────────────────────────────────────

    #[test]
    fn two_levels_of_nesting() {
        let src = r#"
            const el = (
                <div>
                    <span></span>
                </div>
            );
        "#;
        assert_eq!(max_jsx_nesting(src), 2);
    }

    #[test]
    fn deeply_nested_jsx_tree() {
        // 5 levels: div > section > article > ul > li
        let src = r#"
            function Component() {
                return (
                    <div>
                        <section>
                            <article>
                                <ul>
                                    <li>item</li>
                                </ul>
                            </article>
                        </section>
                    </div>
                );
            }
        "#;
        assert_eq!(max_jsx_nesting(src), 5);
    }

    #[test]
    fn max_is_taken_across_branches() {
        // left branch: 2 levels; right branch: 3 levels — max = 4
        let src = r#"
            const el = (
                <div>
                    <span><em /></span>
                    <section>
                        <article>
                            <p></p>
                        </article>
                    </section>
                </div>
            );
        "#;
        assert_eq!(max_jsx_nesting(src), 4);
    }

    // ── fragments ─────────────────────────────────────────────────────────────

    #[test]
    fn fragment_counts_as_one_level() {
        // <>...</> is a jsx_fragment — counts as a nesting level.
        let src = r#"
            const el = (
                <>
                    <div></div>
                </>
            );
        "#;
        assert_eq!(max_jsx_nesting(src), 2);
    }

    #[test]
    fn nested_fragment_inside_element() {
        let src = r#"
            const el = (
                <div>
                    <>
                        <span></span>
                    </>
                </div>
            );
        "#;
        // div(1) > fragment(2) > span(3)
        assert_eq!(max_jsx_nesting(src), 3);
    }

    // ── conditional rendering ─────────────────────────────────────────────────

    #[test]
    fn conditional_rendering_counts_nested_jsx() {
        // The JSX inside the ternary is still reachable in the AST.
        let src = r#"
            function Component({ flag }) {
                return (
                    <div>
                        {flag ? (
                            <section>
                                <p>yes</p>
                            </section>
                        ) : (
                            <span>no</span>
                        )}
                    </div>
                );
            }
        "#;
        // div(1) > section(2) > p(3) — deeper branch wins
        assert_eq!(max_jsx_nesting(src), 3);
    }

    #[test]
    fn logical_and_conditional() {
        let src = r#"
            function Component({ show }) {
                return (
                    <div>
                        {show && (
                            <article>
                                <p></p>
                            </article>
                        )}
                    </div>
                );
            }
        "#;
        // div(1) > article(2) > p(3)
        assert_eq!(max_jsx_nesting(src), 3);
    }

    // ── multiple components in a file ─────────────────────────────────────────

    #[test]
    fn takes_max_across_multiple_components() {
        // ComponentA has depth 2, ComponentB has depth 3.
        let src = r#"
            function ComponentA() {
                return <div><span></span></div>;
            }

            function ComponentB() {
                return (
                    <div>
                        <section>
                            <p></p>
                        </section>
                    </div>
                );
            }
        "#;
        assert_eq!(max_jsx_nesting(src), 3);
    }
}
