use tree_sitter::Node;

/// Prop Drilling Depth result for a file.
///
/// Prop drilling occurs when a prop is passed through intermediate components
/// that don't use it, just to get it to a deeply nested child.
///
/// This metric is a static approximation:
/// - We identify JSX elements that receive props (attributes)
/// - We track the nesting depth of JSX elements that have attribute spreading
///   or explicit prop forwarding (`{...props}` or named prop passing)
/// - Max JSX nesting depth in a component = upper bound of prop drilling depth
///
/// The metric reports:
/// - `max_prop_pass_depth`: the maximum depth at which props are passed into
///   JSX elements (a proxy for how deep drilling could reach)
/// - `spread_prop_depths`: depths at which `{...props}` spread attributes appear
///   (direct evidence of forwarding)
#[derive(Debug, Clone)]
pub struct PropDrillingResult {
    /// Maximum depth of any JSX element that receives explicit props.
    pub max_prop_pass_depth: usize,
    /// Depths at which spread attributes (`{...expr}`) appear in JSX.
    pub spread_prop_depths: Vec<usize>,
    /// Whether any spread attributes were detected (evidence of forwarding).
    pub has_spread_forwarding: bool,
}

/// Compute prop drilling indicators for a subtree.
///
/// `node` should be a component function node or its body.
pub fn compute_prop_drilling(node: Node, source: &[u8]) -> PropDrillingResult {
    let _ = source;
    let mut max_prop_pass_depth = 0usize;
    let mut spread_prop_depths = Vec::new();

    collect_prop_depths(node, 0, &mut max_prop_pass_depth, &mut spread_prop_depths);

    spread_prop_depths.sort();
    let has_spread_forwarding = !spread_prop_depths.is_empty();

    PropDrillingResult {
        max_prop_pass_depth,
        spread_prop_depths,
        has_spread_forwarding,
    }
}

fn collect_prop_depths(
    node: Node,
    jsx_depth: usize,
    max_depth: &mut usize,
    spread_depths: &mut Vec<usize>,
) {
    // Compute depth for children:
    // jsx_element/jsx_fragment open a new nesting level; everything else passes through.
    let current_depth = match node.kind() {
        "jsx_element" | "jsx_fragment" => jsx_depth + 1,
        _ => jsx_depth,
    };

    // Determine prop depth for JSX tag nodes that can have attributes.
    // - jsx_opening_element: part of a jsx_element container; the container is at
    //   current_depth (= jsx_depth + 1), so opening-tag props are at jsx_depth.
    //   Wait — when we are processing jsx_opening_element, current_depth at our level
    //   is our parent's current_depth, which IS the depth of the containing jsx_element.
    //   So for an opening element that is a child of jsx_element at depth 1, jsx_depth=1.
    //   The opening element's props belong to that depth-1 container → prop depth = jsx_depth.
    //
    // - jsx_self_closing_element: the element ITSELF is a new node (not a container),
    //   so it lives one level deeper than its parent JSX context → prop depth = jsx_depth + 1.
    let (tag_kind, prop_depth) = match node.kind() {
        "jsx_opening_element" => (true, jsx_depth),
        "jsx_self_closing_element" => (true, jsx_depth + 1),
        _ => (false, 0),
    };

    if tag_kind {
        let mut cursor = node.walk();
        let mut has_attrs = false;
        for child in node.children(&mut cursor) {
            match child.kind() {
                "jsx_attribute" => {
                    has_attrs = true;
                }
                // Spread attr: `{...expr}` is a `jsx_expression` containing `spread_element`
                "jsx_expression" => {
                    let mut inner = child.walk();
                    for grandchild in child.children(&mut inner) {
                        if grandchild.kind() == "spread_element" {
                            has_attrs = true;
                            let depth = if node.kind() == "jsx_self_closing_element" {
                                jsx_depth + 1
                            } else {
                                jsx_depth
                            };
                            spread_depths.push(depth.max(1));
                        }
                    }
                }
                _ => {}
            }
        }
        if has_attrs && prop_depth > *max_depth {
            *max_depth = prop_depth.max(1);
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_prop_depths(child, current_depth, max_depth, spread_depths);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_tsx;

    fn prop_drilling(src: &str) -> PropDrillingResult {
        let tree = parse_tsx(src).expect("parse failed");
        compute_prop_drilling(tree.root_node(), src.as_bytes())
    }

    #[test]
    fn test_no_jsx_no_drilling() {
        let src = "function Comp() { return null; }";
        let result = prop_drilling(src);
        assert_eq!(result.max_prop_pass_depth, 0);
        assert!(!result.has_spread_forwarding);
    }

    #[test]
    fn test_self_closing_with_props_depth_one() {
        let src = r#"
function App() {
    return <Button onClick={handler} label="click" />;
}
"#;
        let result = prop_drilling(src);
        assert_eq!(result.max_prop_pass_depth, 1);
        assert!(!result.has_spread_forwarding);
    }

    #[test]
    fn test_nested_jsx_with_props() {
        let src = r#"
function App() {
    return (
        <Layout>
            <Section>
                <Button onClick={fn} />
            </Section>
        </Layout>
    );
}
"#;
        let result = prop_drilling(src);
        // Button is at depth 3 (Layout > Section > Button)
        assert!(result.max_prop_pass_depth >= 2);
    }

    #[test]
    fn test_spread_props_detected() {
        let src = r#"
function Wrapper(props) {
    return <Child {...props} />;
}
"#;
        let result = prop_drilling(src);
        assert!(result.has_spread_forwarding);
        assert!(!result.spread_prop_depths.is_empty());
    }

    #[test]
    fn test_spread_props_at_nested_depth() {
        let src = r#"
function App() {
    return (
        <Outer>
            <Inner {...forwardedProps} extra="val" />
        </Outer>
    );
}
"#;
        let result = prop_drilling(src);
        assert!(result.has_spread_forwarding);
        // Inner is at depth 2
        assert!(result.spread_prop_depths.iter().any(|&d| d >= 2));
    }

    #[test]
    fn test_no_props_no_depth() {
        let src = r#"
function App() {
    return (
        <div>
            <span>text</span>
        </div>
    );
}
"#;
        let result = prop_drilling(src);
        // No attributes on any element
        assert_eq!(result.max_prop_pass_depth, 0);
        assert!(!result.has_spread_forwarding);
    }

    #[test]
    fn test_spread_depths_are_sorted() {
        let src = r#"
function App() {
    return (
        <Layout>
            <Section {...sectionProps} />
            <Other {...otherProps} />
        </Layout>
    );
}
"#;
        let result = prop_drilling(src);
        let depths = result.spread_prop_depths.clone();
        let mut sorted = depths.clone();
        sorted.sort();
        assert_eq!(depths, sorted);
    }

    #[test]
    fn test_both_attrs_and_spread() {
        let src = r#"function Wrapper(props) { return <Child onClick={fn} label="hi" {...props} />; }"#;
        let result = prop_drilling(src);
        assert_eq!(result.max_prop_pass_depth, 1);
        assert!(result.has_spread_forwarding);
    }
}
