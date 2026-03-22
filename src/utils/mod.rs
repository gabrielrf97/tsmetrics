use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Directory/file name patterns excluded by default.
pub const DEFAULT_EXCLUDES: &[&str] = &[
    "node_modules",
    ".git",
    "dist",
    "build",
    "coverage",
    ".next",
    "__pycache__",
];

/// Collect all TypeScript/TSX files under the given paths, skipping excluded directories.
///
/// `extra_exclude` are additional patterns on top of `DEFAULT_EXCLUDES`.
pub fn collect_ts_files(paths: &[PathBuf], extra_exclude: &[String]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for path in paths {
        if path.is_file() {
            if is_ts_file(path) && !is_excluded_path(path, extra_exclude) {
                files.push(path.clone());
            }
        } else if path.is_dir() {
            for entry in WalkDir::new(path)
                .follow_links(true)
                .into_iter()
                .filter_entry(|e| !is_excluded_entry(e.path(), extra_exclude))
                .filter_map(|e| e.ok())
            {
                let p = entry.path().to_path_buf();
                if p.is_file() && is_ts_file(&p) {
                    files.push(p);
                }
            }
        }
    }
    files
}

/// Returns true if any component of the path matches a default or extra exclude pattern.
fn is_excluded_path(path: &Path, extra: &[String]) -> bool {
    path.components().any(|c| {
        let s = c.as_os_str().to_string_lossy();
        DEFAULT_EXCLUDES.contains(&s.as_ref()) || extra.iter().any(|e| e == s.as_ref())
    })
}

/// Used by WalkDir's `filter_entry` to prune entire subtrees.
fn is_excluded_entry(path: &Path, extra: &[String]) -> bool {
    if let Some(name) = path.file_name() {
        let s = name.to_string_lossy();
        return DEFAULT_EXCLUDES.contains(&s.as_ref()) || extra.iter().any(|e| e == s.as_ref());
    }
    false
}

fn is_ts_file(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some("ts") | Some("tsx") => true,
        _ => false,
    }
}
