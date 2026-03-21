pub mod config;
pub mod metrics;
pub mod output;
pub mod parse;
pub mod strategies;
pub mod structs;
pub mod utils;

use anyhow::Result;
use rayon::prelude::*;
use std::fs;

use config::Config;
use structs::AnalysisResult;

/// Run analysis over all TypeScript files found in the configured paths.
pub fn analyze(config: &Config) -> Result<AnalysisResult> {
    let files = utils::collect_ts_files(&config.paths);
    let verbose = config.verbose;

    let file_metrics: Vec<_> = files
        .par_iter()
        .filter_map(|path| {
            let path_str = path.to_string_lossy().to_string();
            let source = match fs::read_to_string(path) {
                Ok(s) => s,
                Err(e) => {
                    if verbose {
                        eprintln!("warning: skipping {path_str}: {e}");
                    }
                    return None;
                }
            };
            let tree = match parse::parse_file(&source, &path_str) {
                Ok(t) => t,
                Err(e) => {
                    if verbose {
                        eprintln!("warning: failed to parse {path_str}: {e}");
                    }
                    return None;
                }
            };
            let fm = metrics::compute_file_metrics(tree.root_node(), source.as_bytes(), &path_str);
            Some(fm)
        })
        .collect();

    let mut result = AnalysisResult::new();
    for fm in file_metrics {
        result.add_file(fm);
    }
    Ok(result)
}
