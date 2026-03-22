pub mod closure_depth;
pub mod cyclo;
pub mod long_param_list;
pub mod halstead;
pub mod loc;
pub mod maintainability;
pub mod nesting;
pub mod params;

use crate::structs::FunctionMetrics;
use crate::metrics::react::{
    hook_complexity::compute_hook_complexity,
    effect_density::compute_effect_density,
    render_complexity::compute_render_complexity,
    prop_drilling::compute_prop_drilling,
    component_responsibility::{compute_component_responsibility, CrsWeights},
};
use tree_sitter::Node;

/// Extract all function nodes from a file's AST and compute their metrics.
pub fn extract_functions(root: Node, source: &[u8], file_path: &str) -> Vec<FunctionMetrics> {
    let mut functions = Vec::new();
    collect_functions(root, source, file_path, &mut functions);
    functions
}

fn collect_functions(node: Node, source: &[u8], file_path: &str, out: &mut Vec<FunctionMetrics>) {
    let kind = node.kind();

    let is_function = matches!(
        kind,
        "function_declaration"
            | "function_expression"
            | "arrow_function"
            | "method_definition"
    );

    if is_function {
        let name = extract_function_name(node, source);
        let line = node.start_position().row + 1;
        let loc_val = loc::count_loc(node, source);
        let sloc_val = loc::count_sloc(node, source);
        let cc = cyclo::cyclomatic_complexity(node, source);

        // Halstead volume (per-node, no re-parse needed)
        let hv = halstead::compute_for_node(node, source).volume;
        // Maintainability Index from the pre-computed inputs
        let mi = maintainability::maintainability_index(&name, hv, cc, loc_val).mi;

        // Closure depth (max nesting of closures within this function)
        let cdepth = closure_depth::max_closure_depth(node);

        // React / FP metrics
        let hook_cx = compute_hook_complexity(node, source);
        let render_cx = compute_render_complexity(node, source);
        let prop_drill = compute_prop_drilling(node, source);
        let crs = compute_component_responsibility(node, source, &CrsWeights::default());

        // Effect density requires the function body node
        let (effect_count_val, effect_density_val) = if let Some(body) = node.child_by_field_name("body") {
            let ed = compute_effect_density(body, source);
            (ed.effect_count, ed.density)
        } else {
            (0, 0.0)
        };

        let metrics = FunctionMetrics {
            name,
            file: file_path.to_string(),
            line,
            loc: loc_val,
            sloc: sloc_val,
            cyclomatic_complexity: cc,
            max_nesting: nesting::max_nesting(node),
            param_count: params::param_count(node),
            halstead_volume: hv,
            maintainability_index: mi,
            closure_depth: cdepth,
            hook_count: hook_cx.hook_count,
            effect_count: effect_count_val,
            effect_density: effect_density_val,
            render_complexity: render_cx.total,
            prop_drilling_depth: prop_drill.max_prop_pass_depth,
            component_responsibility: crs.score,
        };
        out.push(metrics);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_functions(child, source, file_path, out);
    }
}

fn extract_function_name(node: Node, source: &[u8]) -> String {
    // function_declaration / method_definition have a direct `name` field.
    if let Some(name_node) = node.child_by_field_name("name") {
        return name_node.utf8_text(source).unwrap_or("<anonymous>").to_string();
    }

    // For expressions assigned to variables, object keys, or class fields,
    // look at the parent node for the name.
    if let Some(parent) = node.parent() {
        let name_field = match parent.kind() {
            "variable_declarator" | "public_field_definition" | "assignment_expression" => {
                parent.child_by_field_name("name")
            }
            "pair" => parent.child_by_field_name("key"),
            _ => None,
        };
        if let Some(n) = name_field {
            return n.utf8_text(source).unwrap_or("<anonymous>").to_string();
        }
    }

    "<anonymous>".to_string()
}
