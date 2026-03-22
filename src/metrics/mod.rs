pub mod class;
pub mod file;
pub mod function;
pub mod module;
pub mod react;

use crate::structs::FileMetrics;
use function::loc::count_sloc_str;
use tree_sitter::Node;

/// Count top-level import statements in a file.
pub fn count_imports(root: Node, _source: &[u8]) -> usize {
    let mut count = 0;
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "import_statement" {
            count += 1;
        }
    }
    count
}

/// Count class declarations in a file.
pub fn count_classes(root: Node) -> usize {
    let mut count = 0;
    count_nodes_of_kind(root, "class_declaration", &mut count);
    count
}

fn count_nodes_of_kind(node: Node, kind: &str, count: &mut usize) {
    if node.kind() == kind {
        *count += 1;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        count_nodes_of_kind(child, kind, count);
    }
}

/// Compute all metrics for a parsed file.
pub fn compute_file_metrics(root: Node, source: &[u8], path: &str) -> FileMetrics {
    let functions = function::extract_functions(root, source, path);
    let classes = class::extract_classes(root, source, path);
    let total_loc = root.end_position().row + 1;
    let total_sloc = count_sloc_str(std::str::from_utf8(source).unwrap_or(""));

    FileMetrics {
        path: path.to_string(),
        total_loc,
        total_sloc,
        function_count: functions.len(),
        class_count: count_classes(root),
        import_count: count_imports(root, source),
        functions,
        classes,
    }
}
