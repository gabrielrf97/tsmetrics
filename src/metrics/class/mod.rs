pub mod dit;
pub mod noi;
pub mod nom;
pub mod wmc;
pub mod woc;

pub use woc::{compute_class_woc, ClassWoc};

use crate::structs::ClassMetrics;
use tree_sitter::Node;
use wmc::{compute_wmc, count_methods, extract_class_name};

/// Extract WMC metrics for every class found in the AST.
pub fn extract_classes(root: Node, source: &[u8], file_path: &str) -> Vec<ClassMetrics> {
    let mut out = Vec::new();
    collect_classes(root, source, file_path, &mut out);
    out
}

fn collect_classes(node: Node, source: &[u8], file_path: &str, out: &mut Vec<ClassMetrics>) {
    // tree-sitter-typescript: "class_declaration" for declared classes,
    // "abstract_class_declaration" for abstract classes,
    // "class" for class expressions.
    let is_class = match node.kind() {
        "class_declaration" | "abstract_class_declaration" => true,
        "class" => node.child_by_field_name("body").is_some(),
        _ => false,
    };
    if is_class {
        out.push(ClassMetrics {
            name: extract_class_name(node, source),
            file: file_path.to_string(),
            line: node.start_position().row + 1,
            method_count: count_methods(node),
            wmc: compute_wmc(node, source),
            noi: noi::count_implemented_interfaces(node),
        });
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_classes(child, source, file_path, out);
    }
}
