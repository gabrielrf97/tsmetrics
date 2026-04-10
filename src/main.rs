use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use tsmetrics::{
    analyze,
    config::{Config, OutputFormat, ThresholdOverrides},
};

#[derive(Parser, Debug)]
#[command(
    name = "tsmetrics",
    about = "TypeScript static analyzer — metrics for functions, classes, and files",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Analyze TypeScript/TSX files or directories
    Analyze {
        /// Paths to files or directories to analyze (optional if paths are in config file)
        paths: Vec<PathBuf>,

        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: Format,

        /// Show verbose output (skipped files will be reported to stderr)
        #[arg(short, long)]
        verbose: bool,

        /// Filter: only show functions with at least this cyclomatic complexity
        #[arg(long)]
        min_complexity: Option<usize>,

        /// Filter: only show functions with at least this many lines of code
        #[arg(long)]
        min_loc: Option<usize>,

        /// Show elapsed time and thread count after analysis
        #[arg(long)]
        timing: bool,

        /// Glob/directory patterns to exclude from scanning (can be repeated).
        /// Defaults: node_modules, .git, dist, build, coverage, .next.
        /// Use --exclude to add extra patterns on top of the defaults.
        #[arg(long = "exclude", short = 'e', num_args = 1)]
        exclude: Vec<String>,

        /// Path to a config file (tsmetrics.yaml or package.json with tsmetrics key)
        #[arg(long = "config", short = 'c')]
        config_file: Option<PathBuf>,

        /// Override cyclomatic complexity warning threshold
        #[arg(long)]
        cc_warning: Option<usize>,
        /// Override cyclomatic complexity error threshold
        #[arg(long)]
        cc_error: Option<usize>,
        /// Override lines-of-code warning threshold
        #[arg(long)]
        loc_warning: Option<usize>,
        /// Override lines-of-code error threshold
        #[arg(long)]
        loc_error: Option<usize>,
        /// Override nesting warning threshold
        #[arg(long)]
        nesting_warning: Option<usize>,
        /// Override nesting error threshold
        #[arg(long)]
        nesting_error: Option<usize>,
        /// Override params warning threshold
        #[arg(long)]
        params_warning: Option<usize>,
        /// Override params error threshold
        #[arg(long)]
        params_error: Option<usize>,
        /// Override weighted methods per class warning threshold
        #[arg(long)]
        wmc_warning: Option<usize>,
        /// Override weighted methods per class error threshold
        #[arg(long)]
        wmc_error: Option<usize>,
        /// Override number of implementations warning threshold
        #[arg(long)]
        noi_warning: Option<usize>,
        /// Override number of implementations error threshold
        #[arg(long)]
        noi_error: Option<usize>,
        /// Override maintainability index warning threshold (lower MI is worse)
        #[arg(long)]
        mi_warning: Option<f64>,
        /// Override maintainability index error threshold (lower MI is worse)
        #[arg(long)]
        mi_error: Option<f64>,
        /// Override Halstead volume warning threshold
        #[arg(long)]
        hv_warning: Option<f64>,
        /// Override Halstead volume error threshold
        #[arg(long)]
        hv_error: Option<f64>,

        /// Override maintainability index (logic-only, JSX filtered) warning threshold
        #[arg(long)]
        mi_logic_warning: Option<f64>,
        /// Override maintainability index (logic-only, JSX filtered) error threshold
        #[arg(long)]
        mi_logic_error: Option<f64>,
        /// Override Halstead volume (logic-only, JSX filtered) warning threshold
        #[arg(long)]
        hv_logic_warning: Option<f64>,
        /// Override Halstead volume (logic-only, JSX filtered) error threshold
        #[arg(long)]
        hv_logic_error: Option<f64>,

        /// Exit with code 1 if violations at this severity or above are found (for CI gating)
        #[arg(long, value_enum)]
        fail_on: Option<FailOn>,
    },
}

#[derive(ValueEnum, Clone, Debug)]
enum Format {
    Table,
    Json,
    Csv,
    Html,
}

/// Minimum violation severity that triggers a non-zero exit code.
#[derive(ValueEnum, Clone, Debug)]
enum FailOn {
    /// Exit 1 if any warning or error violation exists
    Warning,
    /// Exit 1 only if an error-severity violation exists
    Error,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Analyze {
            paths,
            format,
            verbose,
            min_complexity,
            min_loc,
            timing,
            exclude,
            config_file,
            cc_warning,
            cc_error,
            loc_warning,
            loc_error,
            nesting_warning,
            nesting_error,
            params_warning,
            params_error,
            wmc_warning,
            wmc_error,
            noi_warning,
            noi_error,
            mi_warning,
            mi_error,
            hv_warning,
            hv_error,
            mi_logic_warning,
            mi_logic_error,
            hv_logic_warning,
            hv_logic_error,
            fail_on,
        } => {
            let output_format = match format {
                Format::Table => OutputFormat::Table,
                Format::Json => OutputFormat::Json,
                Format::Csv => OutputFormat::Csv,
                Format::Html => OutputFormat::Html,
            };

            let mut config = Config::new(paths);
            config.output_format = output_format.clone();
            config.verbose = verbose;
            config.min_complexity = min_complexity;
            config.min_loc = min_loc;
            config.timing = timing;
            config.exclude = exclude;
            config.config_file = config_file;
            config.threshold_overrides = ThresholdOverrides {
                cc_warning,
                cc_error,
                loc_warning,
                loc_error,
                nesting_warning,
                nesting_error,
                params_warning,
                params_error,
                wmc_warning,
                wmc_error,
                noi_warning,
                noi_error,
                mi_warning,
                mi_error,
                hv_warning,
                hv_error,
                mi_logic_warning,
                mi_logic_error,
                hv_logic_warning,
                hv_logic_error,
            };

            if verbose {
                eprintln!("Running analysis...");
            }

            let mut result = analyze(&config)?;

            // Apply display filters.
            if min_complexity.is_some() || min_loc.is_some() {
                for file in &mut result.files {
                    file.functions.retain(|f| {
                        let ok_complexity =
                            min_complexity.map_or(true, |m| f.cyclomatic_complexity >= m);
                        let ok_loc = min_loc.map_or(true, |m| f.loc >= m);
                        ok_complexity && ok_loc
                    });
                }
                // Recompute summary counts to match what will actually be displayed.
                result.total_functions =
                    result.files.iter().map(|f| f.functions.len()).sum();
                result.total_files = result
                    .files
                    .iter()
                    .filter(|f| !f.functions.is_empty())
                    .count();
            }

            tsmetrics::output::render(&result, &output_format)?;

            if timing {
                eprintln!(
                    "Analysis completed in {:.3}s across {} file(s) using {} thread(s)",
                    result.elapsed_secs, result.total_files, result.num_threads
                );
            }

            if let Some(ref level) = fail_on {
                use tsmetrics::thresholds::Severity;
                let has_violations = result.violations.iter().any(|v| !v.suppressed && match level {
                    FailOn::Error => v.severity == Severity::Error,
                    FailOn::Warning => {
                        matches!(v.severity, Severity::Warning | Severity::Error)
                    }
                });
                if has_violations {
                    // Flush stdout to ensure all output (JSON, CSV, etc.) is written
                    // before exiting with a non-zero code.
                    use std::io::Write;
                    let _ = std::io::stdout().flush();
                    std::process::exit(1);
                }
            }
        }
    }

    Ok(())
}
