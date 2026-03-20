pub mod cyclo;
pub mod loc;
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
    // method_definition has a name field
    if let Some(name_node) = node.child_by_field_name("name") {
        return name_node.utf8_text(source).unwrap_or("<anonymous>").to_string();
    }

    // function_declaration: look at parent for variable declarator
    if let Some(parent) = node.parent() {
        if parent.kind() == "variable_declarator" {
            if let Some(name_node) = parent.child_by_field_name("name") {
                return name_node.utf8_text(source).unwrap_or("<anonymous>").to_string();
            }
        }
    }

    "<anonymous>".to_string()
}
