use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Collect all TypeScript/TSX files under the given paths.
pub fn collect_ts_files(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for path in paths {
        if path.is_file() {
            if is_ts_file(path) {
                files.push(path.clone());
            }
        } else if path.is_dir() {
            for entry in WalkDir::new(path)
                .follow_links(true)
                .into_iter()
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

fn is_ts_file(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some("ts") | Some("tsx") => true,
        _ => false,
    }
}
