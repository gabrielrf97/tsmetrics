use crate::thresholds::Violation;
use serde::Serialize;

/// Per-function metrics.
#[derive(Debug, Clone, Serialize)]
pub struct FunctionMetrics {
    pub name: String,
    pub file: String,
    pub line: usize,
    pub loc: usize,
    pub sloc: usize,
    pub cyclomatic_complexity: usize,
    pub max_nesting: usize,
    pub param_count: usize,
    // Advanced metrics
    pub halstead_volume: f64,
    pub maintainability_index: f64,
    pub closure_depth: usize,
    // React / FP metrics
    pub hook_count: usize,
    pub effect_count: usize,
    pub effect_density: f64,
    pub render_complexity: usize,
    pub prop_drilling_depth: usize,
    pub component_responsibility: f64,
}

impl Default for FunctionMetrics {
    fn default() -> Self {
        Self {
            name: String::new(),
            file: String::new(),
            line: 0,
            loc: 0,
            sloc: 0,
            cyclomatic_complexity: 0,
            max_nesting: 0,
            param_count: 0,
            halstead_volume: 0.0,
            maintainability_index: 100.0,
            closure_depth: 0,
            hook_count: 0,
            effect_count: 0,
            effect_density: 0.0,
            render_complexity: 0,
            prop_drilling_depth: 0,
            component_responsibility: 0.0,
        }
    }
}

/// Per-class metrics.
#[derive(Debug, Clone, Serialize)]
pub struct ClassMetrics {
    pub name: String,
    pub file: String,
    pub line: usize,
    pub method_count: usize,
    /// Sum of cyclomatic complexities of all methods in the class (WMC).
    pub wmc: usize,
    /// Number of interfaces listed in the `implements` clause (NOI).
    pub noi: usize,
    // Additional OO metrics
    pub dit: usize,
    pub nom: usize,
    pub noam: usize,
    pub noom: usize,
    pub tcc: f64,
    pub cbo: usize,
    pub rfc: usize,
    pub woc: f64,
}

impl Default for ClassMetrics {
    fn default() -> Self {
        Self {
            name: String::new(),
            file: String::new(),
            line: 0,
            method_count: 0,
            wmc: 0,
            noi: 0,
            dit: 0,
            nom: 0,
            noam: 0,
            noom: 0,
            tcc: 1.0,
            cbo: 0,
            rfc: 0,
            woc: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FileMetrics {
    pub path: String,
    pub total_loc: usize,
    pub total_sloc: usize,
    pub function_count: usize,
    pub class_count: usize,
    pub import_count: usize,
    pub functions: Vec<FunctionMetrics>,
    pub classes: Vec<ClassMetrics>,
    // File-level metrics
    pub tech_debt_total: f64,
    pub tech_debt_per_100_sloc: f64,
    pub module_cohesion: f64,
    pub module_fan_out: usize,
    pub pure_fn_ratio: f64,
}

impl Default for FileMetrics {
    fn default() -> Self {
        Self {
            path: String::new(),
            total_loc: 0,
            total_sloc: 0,
            function_count: 0,
            class_count: 0,
            import_count: 0,
            functions: Vec::new(),
            classes: Vec::new(),
            tech_debt_total: 0.0,
            tech_debt_per_100_sloc: 0.0,
            module_cohesion: 1.0,
            module_fan_out: 0,
            pure_fn_ratio: 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AnalysisResult {
    pub files: Vec<FileMetrics>,
    pub total_files: usize,
    pub total_functions: usize,
    pub total_loc: usize,
    /// Wall-clock duration of the analysis in seconds (populated when timing is enabled).
    #[serde(skip)]
    pub elapsed_secs: f64,
    /// Number of Rayon threads used during analysis.
    #[serde(skip)]
    pub num_threads: usize,
    pub violations: Vec<Violation>,
}

impl AnalysisResult {
    pub fn new() -> Self {
        Self {
            files: Vec::new(),
            total_files: 0,
            total_functions: 0,
            total_loc: 0,
            elapsed_secs: 0.0,
            num_threads: 0,
            violations: Vec::new(),
        }
    }

    pub fn add_file(&mut self, file: FileMetrics) {
        self.total_loc += file.total_loc;
        self.total_functions += file.function_count;
        self.total_files += 1;
        self.files.push(file);
    }

    pub fn add_violations(&mut self, violations: Vec<Violation>) {
        self.violations.extend(violations);
    }
}

impl Default for AnalysisResult {
    fn default() -> Self {
        Self::new()
    }
}
