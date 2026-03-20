use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use ts_static_analyzer::{
    analyze,
    config::{Config, OutputFormat},
};

#[derive(Parser, Debug)]
#[command(
    name = "ts-static-analyzer",
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
        /// Paths to files or directories to analyze
        #[arg(required = true)]
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
    },
}

#[derive(ValueEnum, Clone, Debug)]
enum Format {
    Table,
    Json,
    Csv,
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
        } => {
            let output_format = match format {
                Format::Table => OutputFormat::Table,
                Format::Json => OutputFormat::Json,
                Format::Csv => OutputFormat::Csv,
            };

            let mut config = Config::new(paths);
            config.output_format = output_format.clone();
            config.verbose = verbose;
            config.min_complexity = min_complexity;
            config.min_loc = min_loc;

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

            ts_static_analyzer::output::render(&result, &output_format)?;
        }
    }

    Ok(())
}
