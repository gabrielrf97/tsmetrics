use std::collections::{HashMap, HashSet};
use tree_sitter::Node;

/// Module Cohesion (MC) metrics for a single TypeScript module (file).
///
/// MC = connected_export_pairs / total_possible_pairs
///
/// Two exported functions/consts are "connected" if they share at least one
/// import source (i.e., they both use identifiers that come from the same
/// `import` statement's module specifier).
///
/// Special cases:
/// - 0 or 1 export → MC = 1.0  (vacuously cohesive)
///
/// This is the functional-programming equivalent of Tight Class Cohesion (TCC):
/// instead of class methods sharing instance fields, module exports share
/// imported dependencies.
#[derive(Debug, Clone)]
pub struct ModuleCohesion {
    /// Total number of exported functions/consts considered.
    pub export_count: usize,
    /// Number of export pairs that share at least one import source.
    pub connected_pairs: usize,
    /// Total number of export pairs: export_count * (export_count - 1) / 2.
    pub total_pairs: usize,
    /// MC score in [0.0, 1.0].
    pub mc: f64,
}

/// Compute Module Cohesion for a file's root AST node.
pub fn compute_module_cohesion(root: Node, source: &[u8]) -> ModuleCohesion {
    // Step 1: collect all import declarations and build a map from imported
    // identifier name → import source (module specifier string).
    let import_map = collect_import_map(root, source);

    // Step 2: collect all exported function/const names at the top level.
    let exports = collect_exports(root, source);

    // Step 3: for each export, find which import sources it uses by scanning
    // identifiers in its body against the import map.
    let export_sources: Vec<HashSet<String>> = exports
        .iter()
        .map(|export| resolve_export_sources(export, source, &import_map))
        .collect();

    let export_count = export_sources.len();
    let total_pairs = if export_count < 2 {
        0
    } else {
        export_count * (export_count - 1) / 2
    };

    let connected_pairs = if total_pairs == 0 {
        0
    } else {
        count_connected_pairs(&export_sources)
    };

    let mc = if export_count <= 1 {
        1.0
    } else if total_pairs == 0 {
        1.0
    } else {
        connected_pairs as f64 / total_pairs as f64
    };

    ModuleCohesion {
        export_count,
        connected_pairs,
        total_pairs,
        mc,
    }
}

/// A collected export: the body node to scan plus the declared identifier names
/// that belong to this export.
struct ExportEntry<'a> {
    /// The AST node whose body we scan for identifier references.
    body: Node<'a>,
    /// All identifier names introduced by this export declaration (e.g. the
    /// function name, or all destructured binding names for `export const`).
    declared_names: HashSet<String>,
}

/// Build a map: identifier_name → set of import sources that provide it.
///
/// Handles:
/// - `import { foo, bar } from 'module'`  → foo→module, bar→module
/// - `import * as ns from 'module'`       → ns→module
/// - `import defaultExport from 'module'` → defaultExport→module
fn collect_import_map(root: Node, source: &[u8]) -> HashMap<String, HashSet<String>> {
    let mut map: HashMap<String, HashSet<String>> = HashMap::new();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "import_statement" {
            if let Some(src) = import_source(child, source) {
                for name in import_bound_names(child, source) {
                    map.entry(name).or_default().insert(src.clone());
                }
            }
        }
    }
    map
}

/// Extract the module specifier string from an `import_statement` node.
fn import_source(node: Node, source: &[u8]) -> Option<String> {
    // The source is typically the last child: a `string` node.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "string" {
            // Strip surrounding quotes.
            if let Ok(raw) = child.utf8_text(source) {
                return Some(raw.trim_matches(|c| c == '"' || c == '\'').to_string());
            }
        }
    }
    None
}

/// Extract all local identifiers bound by an `import_statement`.
fn import_bound_names(node: Node, source: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            // `import defaultName from '...'` or `import * as ns from '...'`
            "identifier" => {
                if let Ok(name) = child.utf8_text(source) {
                    names.push(name.to_string());
                }
            }
            // `import { foo, bar as baz } from '...'`
            "import_clause" => {
                collect_import_clause_names(child, source, &mut names);
            }
            // namespace import: `* as ns`
            "namespace_import" => {
                collect_identifier_children(child, source, &mut names);
            }
            _ => {}
        }
    }
    names
}

fn collect_import_clause_names(node: Node, source: &[u8], names: &mut Vec<String>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            // default import: the identifier directly under import_clause
            "identifier" => {
                if let Ok(name) = child.utf8_text(source) {
                    names.push(name.to_string());
                }
            }
            // named imports: `{ foo, bar as baz }`
            "named_imports" => {
                collect_named_imports(child, source, names);
            }
            // namespace import: `* as ns`
            "namespace_import" => {
                collect_identifier_children(child, source, names);
            }
            _ => {}
        }
    }
}

fn collect_named_imports(node: Node, source: &[u8], names: &mut Vec<String>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "import_specifier" {
            // For `bar as baz`, the local name is the `alias` field; otherwise
            // use the `name` field.
            let local = child
                .child_by_field_name("alias")
                .or_else(|| child.child_by_field_name("name"));
            if let Some(id) = local {
                if let Ok(name) = id.utf8_text(source) {
                    names.push(name.to_string());
                }
            }
        }
    }
}

fn collect_identifier_children(node: Node, source: &[u8], names: &mut Vec<String>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            if let Ok(name) = child.utf8_text(source) {
                names.push(name.to_string());
            }
        }
    }
}

/// Collect top-level exported function/const declarations.
///
/// Handles:
/// - `export function foo(...) { ... }`
/// - `export const foo = (...) => ...`
/// - `export const foo = function(...) { ... }`
///
/// Re-exports (`export { foo } from '...'`, `export * from '...'`) are
/// intentionally skipped — they have no body to scan.
fn collect_exports<'a>(root: Node<'a>, source: &[u8]) -> Vec<ExportEntry<'a>> {
    let mut exports = Vec::new();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "export_statement" {
            collect_export_entry(child, source, &mut exports);
        }
    }
    exports
}

fn collect_export_entry<'a>(
    node: Node<'a>,
    source: &[u8],
    exports: &mut Vec<ExportEntry<'a>>,
) {
    // Look for the declaration child of the export_statement.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_declaration" | "generator_function_declaration" => {
                let name = child
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("")
                    .to_string();
                let mut declared = HashSet::new();
                if !name.is_empty() {
                    declared.insert(name);
                }
                exports.push(ExportEntry {
                    body: child,
                    declared_names: declared,
                });
            }
            "lexical_declaration" | "variable_declaration" => {
                // May contain multiple declarators: `export const a = ..., b = ...`
                collect_variable_exports(child, source, exports);
            }
            _ => {}
        }
    }
}

fn collect_variable_exports<'a>(
    decl_node: Node<'a>,
    source: &[u8],
    exports: &mut Vec<ExportEntry<'a>>,
) {
    let mut cursor = decl_node.walk();
    for child in decl_node.children(&mut cursor) {
        if child.kind() == "variable_declarator" {
            let name_node = child.child_by_field_name("name");
            let value_node = child.child_by_field_name("value");

            // Only process declarators that have a value (i.e., not just a type
            // annotation).  Bare re-export patterns have no value.
            let body = match value_node {
                Some(v) => v,
                None => continue,
            };

            let mut declared = HashSet::new();
            if let Some(n) = name_node {
                collect_binding_names(n, source, &mut declared);
            }

            exports.push(ExportEntry {
                body,
                declared_names: declared,
            });
        }
    }
}

/// Recursively collect all identifier names from a binding pattern
/// (handles plain identifiers and object/array destructuring).
fn collect_binding_names(node: Node, source: &[u8], names: &mut HashSet<String>) {
    match node.kind() {
        "identifier" => {
            if let Ok(name) = node.utf8_text(source) {
                names.insert(name.to_string());
            }
        }
        "object_pattern" | "array_pattern" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_binding_names(child, source, names);
            }
        }
        _ => {}
    }
}

/// Given an export entry, return the set of import sources it depends on.
///
/// We scan all identifiers within the export's body and look each up in the
/// import map.  We also exclude identifiers that are the export's own declared
/// names (recursive references).
fn resolve_export_sources(
    export: &ExportEntry,
    source: &[u8],
    import_map: &HashMap<String, HashSet<String>>,
) -> HashSet<String> {
    let mut used_sources = HashSet::new();
    let mut identifiers = HashSet::new();
    collect_identifiers(export.body, source, &mut identifiers);

    for ident in &identifiers {
        // Skip self-references.
        if export.declared_names.contains(ident) {
            continue;
        }
        if let Some(sources) = import_map.get(ident) {
            for s in sources {
                used_sources.insert(s.clone());
            }
        }
    }
    used_sources
}

/// Recursively collect all `identifier` node texts within `node`.
fn collect_identifiers(node: Node, source: &[u8], out: &mut HashSet<String>) {
    if node.kind() == "identifier" {
        if let Ok(name) = node.utf8_text(source) {
            out.insert(name.to_string());
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_identifiers(child, source, out);
    }
}

fn count_connected_pairs(export_sources: &[HashSet<String>]) -> usize {
    let n = export_sources.len();
    let mut count = 0;
    for i in 0..n {
        for j in (i + 1)..n {
            if export_sources[i]
                .intersection(&export_sources[j])
                .next()
                .is_some()
            {
                count += 1;
            }
        }
    }
    count
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_typescript;

    fn mc_for(src: &str) -> ModuleCohesion {
        let tree = parse_typescript(src).expect("parse failed");
        compute_module_cohesion(tree.root_node(), src.as_bytes())
    }

    // ── Fully cohesive (MC = 1.0) ──────────────────────────────────────────

    /// Both exports use `logger` from the same import → MC = 1.0
    #[test]
    fn test_fully_cohesive_module() {
        let src = r#"
import { logger } from './logger';

export function greet(name: string): void {
    logger.info(`Hello ${name}`);
}

export function farewell(name: string): void {
    logger.info(`Goodbye ${name}`);
}
"#;
        let result = mc_for(src);
        assert_eq!(result.export_count, 2);
        assert_eq!(result.total_pairs, 1);
        assert_eq!(result.connected_pairs, 1);
        assert!(
            (result.mc - 1.0).abs() < 1e-9,
            "expected MC=1.0, got {}",
            result.mc
        );
    }

    // ── Non-cohesive (MC = 0.0) ────────────────────────────────────────────

    /// `processOrder` uses only `db`; `formatEmail` uses only `emailClient`.
    /// No shared import source → MC = 0.0
    #[test]
    fn test_non_cohesive_module() {
        let src = r#"
import { db } from './database';
import { emailClient } from './email';

export function processOrder(id: number): void {
    db.query(id);
}

export function formatEmail(to: string): string {
    return emailClient.format(to);
}
"#;
        let result = mc_for(src);
        assert_eq!(result.export_count, 2);
        assert_eq!(result.total_pairs, 1);
        assert_eq!(result.connected_pairs, 0);
        assert!(
            (result.mc - 0.0).abs() < 1e-9,
            "expected MC=0.0, got {}",
            result.mc
        );
    }

    // ── Partially cohesive (MC = 2/3) ─────────────────────────────────────

    /// Three exports: `foo` uses `[a]`, `bar` uses `[b]`, `baz` uses `[a, b]`.
    /// Pairs: (foo,bar) ✗  (foo,baz) ✓  (bar,baz) ✓
    /// connected=2, total=3, MC=2/3
    #[test]
    fn test_partially_cohesive_module() {
        let src = r#"
import { a } from './moduleA';
import { b } from './moduleB';

export function foo(): void {
    a();
}

export function bar(): void {
    b();
}

export function baz(): void {
    a();
    b();
}
"#;
        let result = mc_for(src);
        assert_eq!(result.export_count, 3);
        assert_eq!(result.total_pairs, 3);
        assert_eq!(result.connected_pairs, 2);
        assert!(
            (result.mc - 2.0 / 3.0).abs() < 1e-9,
            "expected MC=2/3, got {}",
            result.mc
        );
    }

    // ── Single export → MC = 1.0 (vacuous) ────────────────────────────────

    #[test]
    fn test_single_export_vacuous_cohesion() {
        let src = r#"
import { helper } from './utils';

export function doSomething(): void {
    helper();
}
"#;
        let result = mc_for(src);
        assert_eq!(result.export_count, 1);
        assert_eq!(result.total_pairs, 0);
        assert_eq!(result.connected_pairs, 0);
        assert!(
            (result.mc - 1.0).abs() < 1e-9,
            "single export → MC=1.0, got {}",
            result.mc
        );
    }

    // ── Barrel file (re-exports only) → MC = 1.0 ──────────────────────────

    /// A barrel file with only `export { ... } from '...'` has no bodies to
    /// scan, so export_count = 0 → MC = 1.0 vacuously.
    #[test]
    fn test_barrel_file_no_bodies() {
        let src = r#"
export { foo } from './foo';
export { bar } from './bar';
export { baz } from './baz';
"#;
        let result = mc_for(src);
        assert_eq!(result.export_count, 0);
        assert_eq!(result.total_pairs, 0);
        assert!(
            (result.mc - 1.0).abs() < 1e-9,
            "barrel file → MC=1.0, got {}",
            result.mc
        );
    }

    // ── Arrow function exports ─────────────────────────────────────────────

    #[test]
    fn test_arrow_function_exports() {
        let src = r#"
import { validate } from './validator';
import { transform } from './transformer';

export const process = (data: string) => {
    const valid = validate(data);
    return transform(valid);
};

export const check = (input: string) => {
    return validate(input);
};
"#;
        let result = mc_for(src);
        assert_eq!(result.export_count, 2);
        assert_eq!(result.total_pairs, 1);
        // Both use `validate` from './validator'
        assert_eq!(result.connected_pairs, 1);
        assert!(
            (result.mc - 1.0).abs() < 1e-9,
            "expected MC=1.0, got {}",
            result.mc
        );
    }

    // ── No exports → MC = 1.0 ─────────────────────────────────────────────

    #[test]
    fn test_no_exports() {
        let src = r#"
import { something } from './somewhere';

function internal(): void {
    something();
}
"#;
        let result = mc_for(src);
        assert_eq!(result.export_count, 0);
        assert_eq!(result.total_pairs, 0);
        assert!(
            (result.mc - 1.0).abs() < 1e-9,
            "no exports → MC=1.0, got {}",
            result.mc
        );
    }

    // ── No imports → MC = 0.0 for disjoint pure exports ───────────────────

    /// Two exports with no imports — neither references any import source.
    /// Both have empty source sets, so they are trivially disjoint: MC = 0.0.
    #[test]
    fn test_no_imports_two_exports() {
        let src = r#"
export function add(a: number, b: number): number {
    return a + b;
}

export function subtract(a: number, b: number): number {
    return a - b;
}
"#;
        let result = mc_for(src);
        assert_eq!(result.export_count, 2);
        assert_eq!(result.total_pairs, 1);
        assert_eq!(result.connected_pairs, 0);
        assert!(
            (result.mc - 0.0).abs() < 1e-9,
            "no shared imports → MC=0.0, got {}",
            result.mc
        );
    }
}
