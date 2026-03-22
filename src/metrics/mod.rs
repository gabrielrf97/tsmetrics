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
    let source_str = std::str::from_utf8(source).unwrap_or("");

    // ── Function metrics ────────────────────────────────────────────────────
    let functions = function::extract_functions(root, source, path);

    // ── Class metrics ───────────────────────────────────────────────────────
    let mut classes = class::extract_classes(root, source, path);

    // Enrich classes with additional OO metrics (computed at file level)
    let dit_results    = class::dit::compute_dit(root, source);
    let nom_results    = class::nom::compute_class_nom(root, source);
    let tcc_results    = class::tcc::compute_class_tcc(root, source);
    let cbo_results    = class::cbo::compute_class_cbo(root, source);
    let rfc_results    = class::rfc::compute_class_rfc(root, source);
    let woc_results    = class::woc::compute_class_woc(root, source);

    for cm in &mut classes {
        // Match by class name (best-effort; unique names assumed within a file)
        if let Some(d) = dit_results.iter().find(|d| d.name == cm.name) {
            cm.dit = d.dit;
        }
        if let Some(n) = nom_results.iter().find(|n| n.class_name == cm.name) {
            cm.nom  = n.nom;
            cm.noam = n.noam;
            cm.noom = n.noom;
        }
        if let Some(t) = tcc_results.iter().find(|t| t.class_name == cm.name) {
            cm.tcc = t.tcc;
        }
        if let Some(c) = cbo_results.iter().find(|c| c.class_name == cm.name) {
            cm.cbo = c.cbo;
        }
        if let Some(r) = rfc_results.iter().find(|r| r.class_name == cm.name) {
            cm.rfc = r.rfc;
        }
        if let Some(w) = woc_results.iter().find(|w| w.class_name == cm.name) {
            cm.woc = w.woc;
        }
    }

    // ── File-level metrics ─────────────────────────────────────────────────
    let debt         = file::technical_debt::compute(source_str);
    let cohesion     = module::cohesion::compute_module_cohesion(root, source);
    let coupling     = module::coupling::compute_module_coupling(root, source, path);
    let purity       = module::purity::compute_module_purity(root, source, path);

    let total_loc  = root.end_position().row + 1;
    let total_sloc = count_sloc_str(source_str);

    FileMetrics {
        path: path.to_string(),
        total_loc,
        total_sloc,
        function_count: functions.len(),
        class_count: count_classes(root),
        import_count: count_imports(root, source),
        functions,
        classes,
        tech_debt_total: debt.total,
        tech_debt_per_100_sloc: debt.per_100_sloc,
        module_cohesion: cohesion.mc,
        module_fan_out: coupling.fan_out,
        pure_fn_ratio: purity.ratio,
    }
}
