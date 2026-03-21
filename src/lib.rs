pub mod config;
pub mod metrics;
pub mod output;
pub mod parse;
pub mod strategies;
pub mod structs;
pub mod thresholds;
pub mod utils;

use anyhow::Result;
use rayon::prelude::*;
use std::collections::HashSet;
use std::fs;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use config::Config;
use structs::AnalysisResult;
use thresholds::{check_class_violations, check_function_violations, load_thresholds};

/// Run analysis over all TypeScript files found in the configured paths.
pub fn analyze(config: &Config) -> Result<AnalysisResult> {
    let files = utils::collect_ts_files(&config.paths);
    let verbose = config.verbose;

    let start = Instant::now();

    // Track unique OS thread IDs that actually execute work in this parallel job.
    // Using ThreadId (instead of rayon::current_thread_index) ensures we capture
    // the calling thread too, which Rayon may use when the workload is small.
    let thread_ids: Arc<Mutex<HashSet<thread::ThreadId>>> =
        Arc::new(Mutex::new(HashSet::new()));

    // Load thresholds from tsm.yaml in cwd or any of the analyzed paths
    let cwd = std::env::current_dir().unwrap_or_default();
    let mut search_dirs: Vec<&std::path::Path> = vec![cwd.as_path()];
    let path_refs: Vec<&std::path::Path> = config.paths.iter().map(|p| p.as_path()).collect();
    search_dirs.extend(path_refs.iter().copied());
    let thresholds_config = load_thresholds(&search_dirs)?;

    let file_metrics: Vec<_> = files
        .par_iter()
        .filter_map(|path| {
            thread_ids.lock().unwrap().insert(thread::current().id());

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

    let elapsed = start.elapsed();

    let mut result = AnalysisResult::new();
    for fm in file_metrics {
        // Check violations for functions
        for func in &fm.functions {
            let violations = check_function_violations(
                &func.name,
                &func.file,
                func.line,
                func.cyclomatic_complexity,
                func.loc,
                func.max_nesting,
                func.param_count,
                &thresholds_config,
            );
            result.add_violations(violations);
        }
        // Check violations for classes
        for class in &fm.classes {
            let violations = check_class_violations(
                &class.name,
                &class.file,
                class.line,
                class.wmc,
                class.noi,
                &thresholds_config,
            );
            result.add_violations(violations);
        }
        result.add_file(fm);
    }

    if config.timing {
        result.elapsed_secs = elapsed.as_secs_f64();
        result.num_threads = thread_ids.lock().unwrap().len();
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_ts_file(dir: &std::path::Path, name: &str, content: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        fs::write(&path, content).unwrap();
        path
    }

    /// `num_threads` must never exceed the number of files processed, because
    /// with fewer files than pool threads only a subset of threads participates.
    #[test]
    fn num_threads_does_not_exceed_file_count() {
        let dir = tempdir();
        // Single tiny file — at most one thread can do work.
        let path = write_ts_file(&dir, "single.ts", "const x = 1;\n");

        let mut config = Config::new(vec![path]);
        config.timing = true;

        let result = analyze(&config).unwrap();

        assert_eq!(result.total_files, 1);
        assert_eq!(
            result.num_threads, 1,
            "only 1 file → only 1 thread should be reported, got {}",
            result.num_threads
        );
    }

    /// When timing is disabled, `num_threads` stays at its zero default.
    #[test]
    fn num_threads_is_zero_when_timing_disabled() {
        let dir = tempdir();
        let path = write_ts_file(&dir, "single2.ts", "const x = 1;\n");
        let config = Config::new(vec![path]);

        let result = analyze(&config).unwrap();
        assert_eq!(result.num_threads, 0);
    }

    fn tempdir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("tsm_test_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
