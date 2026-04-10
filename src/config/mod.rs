use std::path::PathBuf;

/// CLI threshold overrides — each `Some` value takes precedence over the config file.
#[derive(Debug, Clone, Default)]
pub struct ThresholdOverrides {
    pub cc_warning: Option<usize>,
    pub cc_error: Option<usize>,
    pub loc_warning: Option<usize>,
    pub loc_error: Option<usize>,
    pub nesting_warning: Option<usize>,
    pub nesting_error: Option<usize>,
    pub params_warning: Option<usize>,
    pub params_error: Option<usize>,
    pub wmc_warning: Option<usize>,
    pub wmc_error: Option<usize>,
    pub noi_warning: Option<usize>,
    pub noi_error: Option<usize>,
    pub mi_warning: Option<f64>,
    pub mi_error: Option<f64>,
    pub hv_warning: Option<f64>,
    pub hv_error: Option<f64>,
    pub mi_logic_warning: Option<f64>,
    pub mi_logic_error: Option<f64>,
    pub hv_logic_warning: Option<f64>,
    pub hv_logic_error: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub paths: Vec<PathBuf>,
    pub output_format: OutputFormat,
    pub verbose: bool,
    pub min_complexity: Option<usize>,
    pub min_loc: Option<usize>,
    /// When true, record and display wall-clock time and thread count after analysis.
    pub timing: bool,
    /// Glob/directory patterns to exclude from scanning (e.g. "node_modules", ".git").
    pub exclude: Vec<String>,
    /// CLI threshold overrides (take precedence over config file values).
    pub threshold_overrides: ThresholdOverrides,
    /// Explicit config file path (--config / -c flag).
    pub config_file: Option<PathBuf>,
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
            exclude: Vec::new(),
            threshold_overrides: ThresholdOverrides::default(),
            config_file: None,
        }
    }
}
