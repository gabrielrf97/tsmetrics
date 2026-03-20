use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionMetrics {
    pub name: String,
    pub file: String,
    pub line: usize,
    pub loc: usize,
    pub sloc: usize,
    pub cyclomatic_complexity: usize,
    pub max_nesting: usize,
    pub param_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetrics {
    pub path: String,
    pub total_loc: usize,
    pub total_sloc: usize,
    pub function_count: usize,
    pub class_count: usize,
    pub import_count: usize,
    pub functions: Vec<FunctionMetrics>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub files: Vec<FileMetrics>,
    pub total_files: usize,
    pub total_functions: usize,
    pub total_loc: usize,
}

impl AnalysisResult {
    pub fn new() -> Self {
        Self {
            files: Vec::new(),
            total_files: 0,
            total_functions: 0,
            total_loc: 0,
        }
    }

    pub fn add_file(&mut self, file: FileMetrics) {
        self.total_loc += file.total_loc;
        self.total_functions += file.function_count;
        self.total_files += 1;
        self.files.push(file);
    }
}

impl Default for AnalysisResult {
    fn default() -> Self {
        Self::new()
    }
}
