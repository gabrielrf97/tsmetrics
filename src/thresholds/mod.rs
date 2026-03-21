use serde::{Deserialize, Serialize};
use std::path::Path;

/// Per-metric warning/error threshold pair.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct MetricThreshold {
    pub warning: usize,
    pub error: usize,
}

impl MetricThreshold {
    pub fn new(warning: usize, error: usize) -> Self {
        Self { warning, error }
    }

    /// Returns the violation severity if value exceeds a threshold, else None.
    /// error threshold takes precedence over warning.
    pub fn check(&self, value: usize) -> Option<Severity> {
        if value >= self.error {
            Some(Severity::Error)
        } else if value >= self.warning {
            Some(Severity::Warning)
        } else {
            None
        }
    }
}

/// All metric thresholds configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ThresholdsConfig {
    pub cyclomatic_complexity: MetricThreshold,
    pub loc: MetricThreshold,
    pub nesting: MetricThreshold,
    pub params: MetricThreshold,
    pub wmc: MetricThreshold,
    pub noi: MetricThreshold,
}

impl Default for ThresholdsConfig {
    fn default() -> Self {
        Self {
            cyclomatic_complexity: MetricThreshold::new(10, 25),
            loc: MetricThreshold::new(50, 100),
            nesting: MetricThreshold::new(3, 5),
            params: MetricThreshold::new(4, 7),
            wmc: MetricThreshold::new(20, 50),
            noi: MetricThreshold::new(3, 5),
        }
    }
}

/// Violation severity level.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Warning,
    Error,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Warning => write!(f, "warning"),
            Severity::Error => write!(f, "error"),
        }
    }
}

/// A threshold violation detected during analysis.
#[derive(Debug, Clone, Serialize)]
pub struct Violation {
    pub file: String,
    pub line: usize,
    pub entity: String,
    pub metric: String,
    pub value: usize,
    pub threshold: usize,
    pub severity: Severity,
}

// ── Internal types for partial YAML deserialization ────────────────────────────

#[derive(Debug, Deserialize, Default)]
struct TsmYaml {
    #[serde(default)]
    thresholds: PartialThresholdsConfig,
}

#[derive(Debug, Deserialize, Default)]
struct PartialThresholdsConfig {
    cyclomatic_complexity: Option<PartialMetricThreshold>,
    loc: Option<PartialMetricThreshold>,
    nesting: Option<PartialMetricThreshold>,
    params: Option<PartialMetricThreshold>,
    wmc: Option<PartialMetricThreshold>,
    noi: Option<PartialMetricThreshold>,
}

#[derive(Debug, Deserialize)]
struct PartialMetricThreshold {
    warning: Option<usize>,
    error: Option<usize>,
}

fn merge_threshold(
    partial: Option<PartialMetricThreshold>,
    default: MetricThreshold,
) -> anyhow::Result<MetricThreshold> {
    let merged = match partial {
        None => default,
        Some(p) => MetricThreshold {
            warning: p.warning.unwrap_or(default.warning),
            error: p.error.unwrap_or(default.error),
        },
    };
    if merged.warning > merged.error {
        anyhow::bail!(
            "invalid threshold: warning ({}) > error ({})",
            merged.warning,
            merged.error
        );
    }
    Ok(merged)
}

/// Load thresholds from a tsm.yaml found in any of the given directories.
/// Falls back to defaults if no file is found or the file has no `thresholds` section.
pub fn load_thresholds(search_dirs: &[&Path]) -> anyhow::Result<ThresholdsConfig> {
    for &dir in search_dirs {
        let candidate = dir.join("tsm.yaml");
        if candidate.exists() {
            return load_from_file(&candidate);
        }
    }
    Ok(ThresholdsConfig::default())
}

fn load_from_file(path: &Path) -> anyhow::Result<ThresholdsConfig> {
    let content = std::fs::read_to_string(path)?;
    // An empty file should yield defaults
    if content.trim().is_empty() {
        return Ok(ThresholdsConfig::default());
    }
    let yaml: TsmYaml = serde_yaml::from_str(&content)
        .map_err(|e| anyhow::anyhow!("Failed to parse {}: {}", path.display(), e))?;
    let defaults = ThresholdsConfig::default();
    Ok(ThresholdsConfig {
        cyclomatic_complexity: merge_threshold(
            yaml.thresholds.cyclomatic_complexity,
            defaults.cyclomatic_complexity,
        )?,
        loc: merge_threshold(yaml.thresholds.loc, defaults.loc)?,
        nesting: merge_threshold(yaml.thresholds.nesting, defaults.nesting)?,
        params: merge_threshold(yaml.thresholds.params, defaults.params)?,
        wmc: merge_threshold(yaml.thresholds.wmc, defaults.wmc)?,
        noi: merge_threshold(yaml.thresholds.noi, defaults.noi)?,
    })
}

/// Check function metrics against thresholds, returning any violations.
pub fn check_function_violations(
    name: &str,
    file: &str,
    line: usize,
    cyclomatic_complexity: usize,
    loc: usize,
    nesting: usize,
    params: usize,
    config: &ThresholdsConfig,
) -> Vec<Violation> {
    let mut violations = Vec::new();
    let checks: &[(&str, usize, &MetricThreshold)] = &[
        ("cyclomatic_complexity", cyclomatic_complexity, &config.cyclomatic_complexity),
        ("loc", loc, &config.loc),
        ("nesting", nesting, &config.nesting),
        ("params", params, &config.params),
    ];
    for &(metric, value, threshold) in checks {
        if let Some(severity) = threshold.check(value) {
            let t = match severity {
                Severity::Error => threshold.error,
                Severity::Warning => threshold.warning,
            };
            violations.push(Violation {
                file: file.to_string(),
                line,
                entity: name.to_string(),
                metric: metric.to_string(),
                value,
                threshold: t,
                severity,
            });
        }
    }
    violations
}

/// Check class metrics against thresholds, returning any violations.
pub fn check_class_violations(
    name: &str,
    file: &str,
    line: usize,
    wmc: usize,
    noi: usize,
    config: &ThresholdsConfig,
) -> Vec<Violation> {
    let mut violations = Vec::new();
    let checks: &[(&str, usize, &MetricThreshold)] = &[
        ("wmc", wmc, &config.wmc),
        ("noi", noi, &config.noi),
    ];
    for &(metric, value, threshold) in checks {
        if let Some(severity) = threshold.check(value) {
            let t = match severity {
                Severity::Error => threshold.error,
                Severity::Warning => threshold.warning,
            };
            violations.push(Violation {
                file: file.to_string(),
                line,
                entity: name.to_string(),
                metric: metric.to_string(),
                value,
                threshold: t,
                severity,
            });
        }
    }
    violations
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── MetricThreshold::check ─────────────────────────────────────────────────

    #[test]
    fn test_check_below_warning_returns_none() {
        let t = MetricThreshold::new(10, 25);
        assert_eq!(t.check(9), None);
    }

    #[test]
    fn test_check_at_warning_returns_warning() {
        let t = MetricThreshold::new(10, 25);
        assert_eq!(t.check(10), Some(Severity::Warning));
    }

    #[test]
    fn test_check_between_warning_and_error_returns_warning() {
        let t = MetricThreshold::new(10, 25);
        assert_eq!(t.check(20), Some(Severity::Warning));
    }

    #[test]
    fn test_check_at_error_returns_error() {
        let t = MetricThreshold::new(10, 25);
        assert_eq!(t.check(25), Some(Severity::Error));
    }

    #[test]
    fn test_check_above_error_returns_error() {
        let t = MetricThreshold::new(10, 25);
        assert_eq!(t.check(100), Some(Severity::Error));
    }

    // ── Default thresholds ─────────────────────────────────────────────────────

    #[test]
    fn test_default_thresholds_are_set() {
        let config = ThresholdsConfig::default();
        assert_eq!(config.cyclomatic_complexity, MetricThreshold::new(10, 25));
        assert_eq!(config.loc, MetricThreshold::new(50, 100));
        assert_eq!(config.nesting, MetricThreshold::new(3, 5));
        assert_eq!(config.params, MetricThreshold::new(4, 7));
        assert_eq!(config.wmc, MetricThreshold::new(20, 50));
        assert_eq!(config.noi, MetricThreshold::new(3, 5));
    }

    // ── load_thresholds — no file found uses defaults ──────────────────────────

    #[test]
    fn test_load_thresholds_no_file_returns_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let config = load_thresholds(&[dir.path()]).unwrap();
        assert_eq!(
            config.cyclomatic_complexity,
            ThresholdsConfig::default().cyclomatic_complexity
        );
    }

    // ── load_thresholds — file with partial overrides ─────────────────────────

    #[test]
    fn test_load_thresholds_partial_override() {
        let dir = tempfile::tempdir().unwrap();
        let yaml_path = dir.path().join("tsm.yaml");
        std::fs::write(
            &yaml_path,
            "thresholds:\n  cyclomatic_complexity:\n    warning: 5\n    error: 15\n",
        )
        .unwrap();
        let config = load_thresholds(&[dir.path()]).unwrap();
        assert_eq!(config.cyclomatic_complexity, MetricThreshold::new(5, 15));
        // Other metrics should use defaults
        assert_eq!(config.loc, ThresholdsConfig::default().loc);
    }

    #[test]
    fn test_load_thresholds_only_warning_overridden() {
        let dir = tempfile::tempdir().unwrap();
        let yaml_path = dir.path().join("tsm.yaml");
        std::fs::write(&yaml_path, "thresholds:\n  loc:\n    warning: 30\n").unwrap();
        let config = load_thresholds(&[dir.path()]).unwrap();
        assert_eq!(config.loc.warning, 30);
        assert_eq!(config.loc.error, ThresholdsConfig::default().loc.error);
    }

    #[test]
    fn test_load_thresholds_inverted_warning_error_returns_error() {
        // Setting only warning: 200 for loc (default error: 100) produces warning > error,
        // which must be rejected instead of silently misclassifying values in [100, 199].
        let dir = tempfile::tempdir().unwrap();
        let yaml_path = dir.path().join("tsm.yaml");
        std::fs::write(&yaml_path, "thresholds:\n  loc:\n    warning: 200\n").unwrap();
        let result = load_thresholds(&[dir.path()]);
        assert!(result.is_err(), "expected error for warning > error, got: {:?}", result);
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("warning") && msg.contains("error"), "error message should mention warning and error: {}", msg);
    }

    #[test]
    fn test_load_thresholds_empty_thresholds_section() {
        let dir = tempfile::tempdir().unwrap();
        let yaml_path = dir.path().join("tsm.yaml");
        std::fs::write(&yaml_path, "thresholds: {}\n").unwrap();
        let config = load_thresholds(&[dir.path()]).unwrap();
        let defaults = ThresholdsConfig::default();
        assert_eq!(config.cyclomatic_complexity, defaults.cyclomatic_complexity);
        assert_eq!(config.loc, defaults.loc);
    }

    #[test]
    fn test_load_thresholds_empty_file_uses_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let yaml_path = dir.path().join("tsm.yaml");
        std::fs::write(&yaml_path, "").unwrap();
        let config = load_thresholds(&[dir.path()]).unwrap();
        let defaults = ThresholdsConfig::default();
        assert_eq!(config.cyclomatic_complexity, defaults.cyclomatic_complexity);
    }

    // ── check_function_violations ──────────────────────────────────────────────

    #[test]
    fn test_function_no_violations_below_thresholds() {
        let config = ThresholdsConfig::default();
        let violations = check_function_violations("fn1", "a.ts", 1, 5, 20, 2, 3, &config);
        assert!(violations.is_empty());
    }

    #[test]
    fn test_function_cc_warning() {
        let config = ThresholdsConfig::default(); // warning=10
        let violations = check_function_violations("fn1", "a.ts", 1, 10, 20, 2, 3, &config);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].metric, "cyclomatic_complexity");
        assert_eq!(violations[0].severity, Severity::Warning);
        assert_eq!(violations[0].value, 10);
        assert_eq!(violations[0].threshold, 10);
    }

    #[test]
    fn test_function_cc_error() {
        let config = ThresholdsConfig::default(); // error=25
        let violations = check_function_violations("fn1", "a.ts", 1, 25, 20, 2, 3, &config);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].severity, Severity::Error);
        assert_eq!(violations[0].threshold, 25);
    }

    #[test]
    fn test_function_multiple_violations() {
        let config = ThresholdsConfig::default();
        // loc=100 (error), nesting=5 (error), params=7 (error), cc=10 (warning)
        let violations = check_function_violations("fn1", "a.ts", 1, 10, 100, 5, 7, &config);
        assert_eq!(violations.len(), 4);
    }

    #[test]
    fn test_function_violation_fields_populated() {
        let config = ThresholdsConfig::default();
        let violations = check_function_violations("myFunc", "src/foo.ts", 42, 30, 20, 2, 3, &config);
        assert_eq!(violations.len(), 1); // only cc=30 > warning=10
        let v = &violations[0];
        assert_eq!(v.entity, "myFunc");
        assert_eq!(v.file, "src/foo.ts");
        assert_eq!(v.line, 42);
        assert_eq!(v.metric, "cyclomatic_complexity");
        assert_eq!(v.value, 30);
        assert_eq!(v.severity, Severity::Error); // 30 >= 25
    }

    // ── check_class_violations ─────────────────────────────────────────────────

    #[test]
    fn test_class_no_violations() {
        let config = ThresholdsConfig::default();
        let violations = check_class_violations("MyClass", "a.ts", 1, 10, 1, &config);
        assert!(violations.is_empty());
    }

    #[test]
    fn test_class_wmc_warning() {
        let config = ThresholdsConfig::default(); // wmc warning=20
        let violations = check_class_violations("MyClass", "a.ts", 1, 20, 1, &config);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].metric, "wmc");
        assert_eq!(violations[0].severity, Severity::Warning);
    }

    #[test]
    fn test_class_noi_error() {
        let config = ThresholdsConfig::default(); // noi error=5
        let violations = check_class_violations("MyClass", "a.ts", 1, 10, 5, &config);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].metric, "noi");
        assert_eq!(violations[0].severity, Severity::Error);
    }
}
