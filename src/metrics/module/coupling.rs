use serde::Serialize;
use std::collections::HashSet;
use tree_sitter::Node;

/// Module-level coupling metrics for a single file.
///
/// Fan-Out = number of distinct import sources in a file.
/// This is the FP / module-level equivalent of CBO (Coupling Between Objects).
///
/// Import sources are deduplicated: `import { a } from 'x'` and
/// `import { b } from 'x'` count as a single source `'x'`.
///
/// Import sources are classified into three categories:
///   - **Relative**: starts with `./` or `../` (intra-project modules).
///   - **Package**: all other sources (npm packages, Node built-ins, path aliases).
///   - **Type-only**: sources imported exclusively via `import type { … }`;
///     these are erased at runtime and indicate weaker structural coupling.
///
/// A source appearing in both a regular import and a type-only import is
/// classified as a package/relative import (not type-only), since it carries
/// a runtime dependency.
///
/// # Future work — Fan-In and Instability
///
/// Fan-In (the number of *other* files that import this file) requires a
/// cross-file, two-pass analysis and is not implemented here.  Once Fan-In is
/// available the Instability metric can be derived:
///
///   Instability = Fan-Out / (Fan-In + Fan-Out)
///
/// A value near 1.0 means the module is unstable (depends on many others,
/// nothing depends on it).  A value near 0.0 means it is stable / depended
/// upon by many.  (Robert C. Martin, "Stable Dependencies Principle".)
#[derive(Debug, Clone, Serialize)]
pub struct ModuleCoupling {
    /// File path being analyzed.
    pub file: String,
    /// Total fan-out: number of distinct import sources.
    pub fan_out: usize,
    /// Relative fan-out: distinct sources beginning with `./` or `../`.
    pub relative_fan_out: usize,
    /// Package fan-out: distinct non-relative sources.
    pub package_fan_out: usize,
    /// Type-only fan-out: distinct sources that are *only* imported via
    /// `import type { … }` (never as a value import).
    pub type_only_fan_out: usize,
    /// Sorted list of all distinct import sources.
    pub import_sources: Vec<String>,
}

/// Compute module coupling metrics for the file represented by `root`.
pub fn compute_module_coupling(root: Node, source: &[u8], file: &str) -> ModuleCoupling {
    // Sets of distinct sources in each category.
    let mut all_sources: HashSet<String> = HashSet::new();
    let mut value_sources: HashSet<String> = HashSet::new(); // at least one non-type import

    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "import_statement" {
            if let Some(src) = extract_import_source(child, source) {
                let is_type_only = is_type_only_import(child, source);
                all_sources.insert(src.clone());
                if !is_type_only {
                    value_sources.insert(src);
                }
            }
        }
    }

    let relative_fan_out = all_sources
        .iter()
        .filter(|s| s.starts_with("./") || s.starts_with("../"))
        .count();

    let package_fan_out = all_sources
        .iter()
        .filter(|s| !s.starts_with("./") && !s.starts_with("../"))
        .count();

    // type-only = sources that never appear as a value import
    let type_only_fan_out = all_sources
        .iter()
        .filter(|s| !value_sources.contains(*s))
        .count();

    let mut import_sources: Vec<String> = all_sources.into_iter().collect();
    import_sources.sort();

    ModuleCoupling {
        file: file.to_string(),
        fan_out: import_sources.len(),
        relative_fan_out,
        package_fan_out,
        type_only_fan_out,
        import_sources,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract the module specifier string from an `import_statement` node.
///
/// The source is the `string` node that appears after `from`, i.e. the last
/// `string`-kinded child of the statement.  Quotes are stripped.
fn extract_import_source(import_stmt: Node, source: &[u8]) -> Option<String> {
    // Walk children and grab the last `string` node (the module specifier).
    let mut last_string: Option<Node> = None;
    let mut cursor = import_stmt.walk();
    for child in import_stmt.children(&mut cursor) {
        if child.kind() == "string" {
            last_string = Some(child);
        }
    }

    let string_node = last_string?;
    let raw = string_node.utf8_text(source).ok()?;
    // Strip surrounding quotes (' or ").
    Some(raw.trim_matches(|c| c == '\'' || c == '"').to_string())
}

/// Return `true` if the import statement is a type-only import
/// (`import type { … } from '…'`).
///
/// In tree-sitter-typescript, `import type` places a plain `"type"` keyword
/// node as a direct child of `import_statement`, before the import clause.
fn is_type_only_import(import_stmt: Node, _source: &[u8]) -> bool {
    let mut cursor = import_stmt.walk();
    for child in import_stmt.children(&mut cursor) {
        if child.kind() == "type" {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Tests (written first — TDD)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_typescript;

    fn coupling_for(src: &str) -> ModuleCoupling {
        let tree = parse_typescript(src).expect("parse failed");
        compute_module_coupling(tree.root_node(), src.as_bytes(), "test.ts")
    }

    // --- zero imports ---

    #[test]
    fn test_no_imports_has_zero_fan_out() {
        let src = "const x = 1;";
        let m = coupling_for(src);
        assert_eq!(m.fan_out, 0);
        assert_eq!(m.relative_fan_out, 0);
        assert_eq!(m.package_fan_out, 0);
        assert_eq!(m.type_only_fan_out, 0);
        assert!(m.import_sources.is_empty());
    }

    // --- single imports ---

    #[test]
    fn test_single_package_import() {
        let src = "import { useState } from 'react';";
        let m = coupling_for(src);
        assert_eq!(m.fan_out, 1);
        assert_eq!(m.package_fan_out, 1);
        assert_eq!(m.relative_fan_out, 0);
        assert_eq!(m.import_sources, vec!["react"]);
    }

    #[test]
    fn test_single_relative_import() {
        let src = "import { foo } from './utils';";
        let m = coupling_for(src);
        assert_eq!(m.fan_out, 1);
        assert_eq!(m.relative_fan_out, 1);
        assert_eq!(m.package_fan_out, 0);
        assert_eq!(m.import_sources, vec!["./utils"]);
    }

    #[test]
    fn test_parent_relative_import() {
        let src = "import { bar } from '../helpers';";
        let m = coupling_for(src);
        assert_eq!(m.fan_out, 1);
        assert_eq!(m.relative_fan_out, 1);
        assert_eq!(m.package_fan_out, 0);
    }

    // --- multiple distinct sources ---

    #[test]
    fn test_multiple_distinct_sources() {
        let src = r#"
import { useState } from 'react';
import { Router } from 'express';
import { helper } from './utils';
"#;
        let m = coupling_for(src);
        assert_eq!(m.fan_out, 3);
        assert_eq!(m.package_fan_out, 2);
        assert_eq!(m.relative_fan_out, 1);
    }

    // --- deduplication ---

    #[test]
    fn test_duplicate_sources_counted_once() {
        let src = r#"
import { a } from 'lodash';
import { b } from 'lodash';
import { c } from 'lodash';
"#;
        let m = coupling_for(src);
        assert_eq!(m.fan_out, 1, "three imports from same source → fan_out=1");
        assert_eq!(m.import_sources, vec!["lodash"]);
    }

    #[test]
    fn test_dedup_mixed_relative_and_package() {
        let src = r#"
import { a } from './shared';
import { b } from './shared';
import { c } from 'react';
"#;
        let m = coupling_for(src);
        assert_eq!(m.fan_out, 2);
        assert_eq!(m.relative_fan_out, 1);
        assert_eq!(m.package_fan_out, 1);
    }

    // --- type-only imports ---

    #[test]
    fn test_type_only_import_counted_in_fan_out() {
        let src = "import type { Foo } from './types';";
        let m = coupling_for(src);
        assert_eq!(m.fan_out, 1);
        assert_eq!(m.type_only_fan_out, 1);
        assert_eq!(m.relative_fan_out, 1);
    }

    #[test]
    fn test_type_only_package_import() {
        let src = "import type { Config } from 'webpack';";
        let m = coupling_for(src);
        assert_eq!(m.fan_out, 1);
        assert_eq!(m.type_only_fan_out, 1);
        assert_eq!(m.package_fan_out, 1);
    }

    #[test]
    fn test_mixed_type_and_value_import_same_source_not_type_only() {
        // Source 'react' appears once as value and once as type-only.
        // It should NOT be counted as type_only since it has a runtime dependency.
        let src = r#"
import { useState } from 'react';
import type { FC } from 'react';
"#;
        let m = coupling_for(src);
        assert_eq!(m.fan_out, 1, "same source → fan_out=1");
        assert_eq!(
            m.type_only_fan_out, 0,
            "source also imported as value → not type-only"
        );
    }

    #[test]
    fn test_multiple_type_only_from_different_sources() {
        let src = r#"
import type { A } from './a';
import type { B } from './b';
import { C } from './c';
"#;
        let m = coupling_for(src);
        assert_eq!(m.fan_out, 3);
        assert_eq!(m.type_only_fan_out, 2); // ./a and ./b
    }

    // --- side-effect imports ---

    #[test]
    fn test_side_effect_import() {
        // `import 'polyfill'` — no import clause, just the module
        let src = "import 'reflect-metadata';";
        let m = coupling_for(src);
        assert_eq!(m.fan_out, 1);
        assert_eq!(m.package_fan_out, 1);
    }

    // --- default imports ---

    #[test]
    fn test_default_import() {
        let src = "import React from 'react';";
        let m = coupling_for(src);
        assert_eq!(m.fan_out, 1);
        assert_eq!(m.package_fan_out, 1);
        assert_eq!(m.import_sources, vec!["react"]);
    }

    // --- namespace imports ---

    #[test]
    fn test_namespace_import() {
        let src = "import * as path from 'path';";
        let m = coupling_for(src);
        assert_eq!(m.fan_out, 1);
        assert_eq!(m.package_fan_out, 1);
        assert_eq!(m.import_sources, vec!["path"]);
    }

    // --- import sources are sorted ---

    #[test]
    fn test_import_sources_are_sorted() {
        let src = r#"
import { z } from 'zod';
import { a } from 'axios';
import { r } from 'react';
"#;
        let m = coupling_for(src);
        assert_eq!(m.import_sources, vec!["axios", "react", "zod"]);
    }

    // --- file path stored correctly ---

    #[test]
    fn test_file_path_is_stored() {
        let src = "const x = 1;";
        let tree = parse_typescript(src).expect("parse failed");
        let m = compute_module_coupling(tree.root_node(), src.as_bytes(), "src/app/index.ts");
        assert_eq!(m.file, "src/app/index.ts");
    }

    // --- heavy fan-out ---

    #[test]
    fn test_heavy_fan_out_file() {
        let src = r#"
import React, { useState, useEffect } from 'react';
import { BrowserRouter, Route } from 'react-router-dom';
import axios from 'axios';
import type { Config } from './config';
import { Logger } from '../utils/logger';
import { Database } from '../db/connection';
import { UserService } from './services/user';
import type { User } from './models/user';
"#;
        let m = coupling_for(src);
        assert_eq!(m.fan_out, 8);
        assert_eq!(m.package_fan_out, 3); // react, react-router-dom, axios
        assert_eq!(m.relative_fan_out, 5); // ./config, ../utils/logger, ../db/connection, ./services/user, ./models/user
        assert_eq!(m.type_only_fan_out, 2); // ./config, ./models/user
    }

    // --- dynamic imports are NOT counted (static analysis only) ---
    // Note: `import('x')` is a call_expression, not an import_statement.
    // Dynamic imports require runtime information and are excluded by design.
    #[test]
    fn test_dynamic_import_not_counted() {
        let src = r#"
async function load() {
    const mod = await import('./lazy');
}
"#;
        let m = coupling_for(src);
        assert_eq!(m.fan_out, 0, "dynamic imports are not counted");
    }

    // --- re-exports are NOT counted as imports of this module ---
    // `export { X } from 'y'` is an export_statement, not import_statement.
    #[test]
    fn test_re_export_not_counted() {
        let src = r#"
export { Foo } from './foo';
export { Bar } from './bar';
"#;
        let m = coupling_for(src);
        assert_eq!(
            m.fan_out, 0,
            "re-exports are export_statements, not import_statements"
        );
    }
}
