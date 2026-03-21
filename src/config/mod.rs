use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub paths: Vec<PathBuf>,
    pub output_format: OutputFormat,
    pub verbose: bool,
    pub min_complexity: Option<usize>,
    pub min_loc: Option<usize>,
    /// When true, record and display wall-clock time and thread count after analysis.
    pub timing: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OutputFormat {
    Table,
    Json,
    Csv,
    Html,
}

impl Config {
    pub fn new(paths: Vec<PathBuf>) -> Self {
        Self {
            paths,
            output_format: OutputFormat::Table,
            verbose: false,
            min_complexity: None,
            min_loc: None,
            timing: false,
        }
    }
}
