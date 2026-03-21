pub mod cyclo;
pub mod halstead;
pub mod loc;
pub mod maintainability;
pub mod nesting;
pub mod params;

use crate::structs::FunctionMetrics;
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

        let metrics = FunctionMetrics {
            name,
            file: file_path.to_string(),
            line,
            loc: loc::count_loc(node, source),
            sloc: loc::count_sloc(node, source),
            cyclomatic_complexity: cyclo::cyclomatic_complexity(node, source),
            max_nesting: nesting::max_nesting(node),
            param_count: params::param_count(node),
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
