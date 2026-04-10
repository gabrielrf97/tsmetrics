use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::config::ThresholdOverrides;

const CONFIG_FILENAMES: &[&str] = &[
    "tsmetrics.yaml",
    "tsmetrics.yml",
    ".tsmetricsrc.yaml",
    ".tsmetricsrc.yml",
    ".tsmetricsrc",
];

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

/// Per-metric threshold pair for f64 metrics where higher is worse (e.g., Halstead Volume).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct FloatMetricThreshold {
    pub warning: f64,
    pub error: f64,
}

impl FloatMetricThreshold {
    pub fn new(warning: f64, error: f64) -> Self {
        Self { warning, error }
    }

    pub fn check(&self, value: f64) -> Option<Severity> {
        if value >= self.error {
            Some(Severity::Error)
        } else if value >= self.warning {
            Some(Severity::Warning)
        } else {
            None
        }
    }
}

/// Per-metric threshold pair for f64 metrics where lower is worse (e.g., MI).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct InvertedFloatMetricThreshold {
    pub warning: f64,
    pub error: f64,
}

impl InvertedFloatMetricThreshold {
    pub fn new(warning: f64, error: f64) -> Self {
        Self { warning, error }
    }

    /// Returns severity if value <= threshold (lower is worse).
    pub fn check(&self, value: f64) -> Option<Severity> {
        if value <= self.error {
            Some(Severity::Error)
        } else if value <= self.warning {
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
    pub maintainability_index: InvertedFloatMetricThreshold,
    pub halstead_volume: FloatMetricThreshold,
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
            maintainability_index: InvertedFloatMetricThreshold::new(40.0, 25.0),
            halstead_volume: FloatMetricThreshold::new(2000.0, 4000.0),
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
    pub value: f64,
    pub threshold: f64,
    pub severity: Severity,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub suppressed: bool,
}

/// Combined configuration loaded from tsmetrics.yaml (thresholds + exclude patterns).
#[derive(Debug)]
pub struct TsmetricsConfig {
    pub thresholds: ThresholdsConfig,
    /// Directory/file name patterns that should be excluded from scanning.
    pub exclude: Vec<String>,
    /// Paths to analyze (from config file; CLI paths take precedence).
    pub paths: Vec<PathBuf>,
}

// ── Internal types for partial YAML deserialization ────────────────────────────

#[derive(Debug, Deserialize, Default)]
struct TsmetricsYaml {
    #[serde(default)]
    thresholds: PartialThresholdsConfig,
    #[serde(default)]
    exclude: Vec<String>,
    #[serde(default)]
    paths: Vec<PathBuf>,
}

#[derive(Debug, Deserialize, Default)]
struct PartialThresholdsConfig {
    cyclomatic_complexity: Option<PartialMetricThreshold>,
    loc: Option<PartialMetricThreshold>,
    nesting: Option<PartialMetricThreshold>,
    params: Option<PartialMetricThreshold>,
    wmc: Option<PartialMetricThreshold>,
    noi: Option<PartialMetricThreshold>,
    maintainability_index: Option<PartialFloatMetricThreshold>,
    halstead_volume: Option<PartialFloatMetricThreshold>,
}

#[derive(Debug, Deserialize)]
struct PartialMetricThreshold {
    warning: Option<usize>,
    error: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct PartialFloatMetricThreshold {
    warning: Option<f64>,
    error: Option<f64>,
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

fn merge_float_threshold(
    partial: Option<PartialFloatMetricThreshold>,
    default: FloatMetricThreshold,
) -> anyhow::Result<FloatMetricThreshold> {
    let merged = match partial {
        None => default,
        Some(p) => FloatMetricThreshold {
            warning: p.warning.unwrap_or(default.warning),
            error: p.error.unwrap_or(default.error),
        },
    };
    if merged.warning > merged.error {
        anyhow::bail!(
            "invalid threshold: warning ({}) > error ({})",
            merged.warning, merged.error
        );
    }
    Ok(merged)
}

fn merge_inverted_float_threshold(
    partial: Option<PartialFloatMetricThreshold>,
    default: InvertedFloatMetricThreshold,
) -> anyhow::Result<InvertedFloatMetricThreshold> {
    let merged = match partial {
        None => default,
        Some(p) => InvertedFloatMetricThreshold {
            warning: p.warning.unwrap_or(default.warning),
            error: p.error.unwrap_or(default.error),
        },
    };
    if merged.warning < merged.error {
        anyhow::bail!(
            "invalid inverted threshold: warning ({}) < error ({}) — for this metric, lower values are worse",
            merged.warning, merged.error
        );
    }
    Ok(merged)
}

/// Walk upward from `start_dir` checking all CONFIG_FILENAMES plus package.json at each level.
/// Returns the first config file found, or `None` after `max_levels` parent directories.
fn find_config_file(start_dir: &Path) -> Option<PathBuf> {
    const MAX_LEVELS: usize = 64;
    let mut dir = start_dir.to_path_buf();
    for _ in 0..MAX_LEVELS {
        // Check all standalone config filenames first
        for &filename in CONFIG_FILENAMES {
            let candidate = dir.join(filename);
            if candidate.exists() {
                return Some(candidate);
            }
        }
        // Then check package.json for a "tsmetrics" key
        let pkg_json = dir.join("package.json");
        if pkg_json.exists() {
            if let Ok(content) = std::fs::read_to_string(&pkg_json) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                    if val.get("tsmetrics").is_some() {
                        return Some(pkg_json);
                    }
                }
            }
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

/// Load config from a `package.json` that contains a `"tsmetrics"` key.
fn load_config_from_package_json(path: &Path) -> anyhow::Result<TsmetricsConfig> {
    let content = std::fs::read_to_string(path)?;
    let val: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| anyhow::anyhow!("Failed to parse {}: {}", path.display(), e))?;
    let tsmetrics_val = val
        .get("tsmetrics")
        .ok_or_else(|| anyhow::anyhow!("No \"tsmetrics\" key in {}", path.display()))?;
    // Pre-parse: warn about unrecognized keys
    warn_unknown_json_keys(tsmetrics_val, path);
    let yaml: TsmetricsYaml = serde_json::from_value(tsmetrics_val.clone())
        .map_err(|e| anyhow::anyhow!("Failed to deserialize tsmetrics config from {}: {}", path.display(), e))?;
    let defaults = ThresholdsConfig::default();
    Ok(TsmetricsConfig {
        thresholds: ThresholdsConfig {
            cyclomatic_complexity: merge_threshold(
                yaml.thresholds.cyclomatic_complexity,
                defaults.cyclomatic_complexity,
            )?,
            loc: merge_threshold(yaml.thresholds.loc, defaults.loc)?,
            nesting: merge_threshold(yaml.thresholds.nesting, defaults.nesting)?,
            params: merge_threshold(yaml.thresholds.params, defaults.params)?,
            wmc: merge_threshold(yaml.thresholds.wmc, defaults.wmc)?,
            noi: merge_threshold(yaml.thresholds.noi, defaults.noi)?,
            maintainability_index: merge_inverted_float_threshold(
                yaml.thresholds.maintainability_index,
                defaults.maintainability_index,
            )?,
            halstead_volume: merge_float_threshold(
                yaml.thresholds.halstead_volume,
                defaults.halstead_volume,
            )?,
        },
        exclude: yaml.exclude,
        paths: yaml.paths,
    })
}

/// Load combined configuration (thresholds + exclude patterns) from a config file.
/// Searches given directories (and their parents) for config files.
/// Falls back to defaults if no file is found.
pub fn load_tsmetrics_config(search_dirs: &[&Path]) -> anyhow::Result<TsmetricsConfig> {
    for &dir in search_dirs {
        if let Some(found) = find_config_file(dir) {
            if found.file_name().map_or(false, |f| f == "package.json") {
                return load_config_from_package_json(&found);
            }
            return load_tsmetrics_config_from_file(&found);
        }
    }
    Ok(TsmetricsConfig {
        thresholds: ThresholdsConfig::default(),
        exclude: Vec::new(),
        paths: Vec::new(),
    })
}

/// Load config from an explicit path (for the --config flag).
pub fn load_tsmetrics_config_from_path(path: &Path) -> anyhow::Result<TsmetricsConfig> {
    if !path.exists() {
        anyhow::bail!("Config file not found: {}", path.display());
    }
    if path.file_name().map_or(false, |f| f == "package.json") {
        return load_config_from_package_json(path);
    }
    load_tsmetrics_config_from_file(path)
}

/// Apply CLI threshold overrides on top of loaded config.
/// Re-validates that warning <= error after each override.
pub fn apply_cli_overrides(
    config: &mut ThresholdsConfig,
    overrides: &ThresholdOverrides,
) -> anyhow::Result<()> {
    macro_rules! apply {
        ($field:ident, $warn:ident, $err:ident, $name:expr) => {
            if let Some(v) = overrides.$warn {
                config.$field.warning = v;
            }
            if let Some(v) = overrides.$err {
                config.$field.error = v;
            }
            if config.$field.warning > config.$field.error {
                anyhow::bail!(
                    "invalid {} threshold after CLI overrides: warning ({}) > error ({})",
                    $name,
                    config.$field.warning,
                    config.$field.error
                );
            }
        };
    }
    apply!(cyclomatic_complexity, cc_warning, cc_error, "cyclomatic_complexity");
    apply!(loc, loc_warning, loc_error, "loc");
    apply!(nesting, nesting_warning, nesting_error, "nesting");
    apply!(params, params_warning, params_error, "params");
    apply!(wmc, wmc_warning, wmc_error, "wmc");
    apply!(noi, noi_warning, noi_error, "noi");

    // Halstead Volume (normal: warning <= error)
    if let Some(v) = overrides.hv_warning {
        config.halstead_volume.warning = v;
    }
    if let Some(v) = overrides.hv_error {
        config.halstead_volume.error = v;
    }
    if config.halstead_volume.warning > config.halstead_volume.error {
        anyhow::bail!(
            "invalid halstead_volume threshold after CLI overrides: warning ({}) > error ({})",
            config.halstead_volume.warning, config.halstead_volume.error
        );
    }

    // Maintainability Index (inverted: warning >= error, lower is worse)
    if let Some(v) = overrides.mi_warning {
        config.maintainability_index.warning = v;
    }
    if let Some(v) = overrides.mi_error {
        config.maintainability_index.error = v;
    }
    if config.maintainability_index.warning < config.maintainability_index.error {
        anyhow::bail!(
            "invalid maintainability_index threshold after CLI overrides: warning ({}) < error ({}) — lower MI is worse",
            config.maintainability_index.warning, config.maintainability_index.error
        );
    }

    Ok(())
}

// ── Unknown key detection ────────────────────────────────────────────────────

const KNOWN_TOP_LEVEL_KEYS: &[&str] = &["thresholds", "exclude", "paths"];

const KNOWN_THRESHOLD_KEYS: &[&str] = &[
    "cyclomatic_complexity",
    "loc",
    "nesting",
    "params",
    "wmc",
    "noi",
    "maintainability_index",
    "halstead_volume",
];

/// Emit warnings to stderr for unrecognized keys in a parsed YAML config.
fn warn_unknown_yaml_keys(raw: &serde_yaml::Value, path: &Path) {
    let mapping = match raw.as_mapping() {
        Some(m) => m,
        None => return,
    };
    // Check top-level keys
    let top_keys: Vec<String> = mapping
        .keys()
        .filter_map(|k| k.as_str().map(String::from))
        .collect();
    for warning in check_unknown_keys(&top_keys, KNOWN_TOP_LEVEL_KEYS, "config") {
        eprintln!("warning: {} (in {})", warning, path.display());
    }
    // Check threshold keys
    if let Some(thresholds) = mapping
        .iter()
        .find(|(k, _)| k.as_str() == Some("thresholds"))
        .and_then(|(_, v)| v.as_mapping())
    {
        let threshold_keys: Vec<String> = thresholds
            .keys()
            .filter_map(|k| k.as_str().map(String::from))
            .collect();
        for warning in check_unknown_keys(&threshold_keys, KNOWN_THRESHOLD_KEYS, "threshold") {
            eprintln!("warning: {} (in {})", warning, path.display());
        }
    }
}

/// Emit warnings to stderr for unrecognized keys in a package.json tsmetrics section.
fn warn_unknown_json_keys(raw: &serde_json::Value, path: &Path) {
    let obj = match raw.as_object() {
        Some(o) => o,
        None => return,
    };
    let top_keys: Vec<String> = obj.keys().cloned().collect();
    for warning in check_unknown_keys(&top_keys, KNOWN_TOP_LEVEL_KEYS, "config") {
        eprintln!("warning: {} (in {})", warning, path.display());
    }
    if let Some(thresholds) = obj.get("thresholds").and_then(|v| v.as_object()) {
        let threshold_keys: Vec<String> = thresholds.keys().cloned().collect();
        for warning in check_unknown_keys(&threshold_keys, KNOWN_THRESHOLD_KEYS, "threshold") {
            eprintln!("warning: {} (in {})", warning, path.display());
        }
    }
}

/// Check a list of found keys against known keys, returning warning messages.
fn check_unknown_keys(found: &[String], known: &[&str], context: &str) -> Vec<String> {
    let mut warnings = Vec::new();
    for key in found {
        if !known.contains(&key.as_str()) {
            match find_best_match(key, known) {
                Some(suggestion) => {
                    warnings.push(format!(
                        "unknown {} key '{}' -- did you mean '{}'?",
                        context, key, suggestion
                    ));
                }
                None => {
                    warnings.push(format!("unknown {} key '{}'", context, key));
                }
            }
        }
    }
    warnings
}

/// Find the best fuzzy match for `input` among `candidates` using normalized
/// edit distance. Returns the best match if similarity >= 0.4.
fn find_best_match<'a>(input: &str, candidates: &[&'a str]) -> Option<&'a str> {
    let input_lower = input.to_lowercase().replace('-', "_");
    // Check prefix match first (e.g. "mi" -> "maintainability_index")
    let mut best: Option<(&str, f64)> = None;
    for &candidate in candidates {
        let score = normalized_similarity(&input_lower, candidate);
        if score >= 0.4 {
            if best.map_or(true, |(_, s)| score > s) {
                best = Some((candidate, score));
            }
        }
    }
    best.map(|(name, _)| name)
}

/// Simple normalized similarity: 1.0 - (edit_distance / max_len).
fn normalized_similarity(a: &str, b: &str) -> f64 {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let (m, n) = (a_bytes.len(), b_bytes.len());
    if m == 0 && n == 0 {
        return 1.0;
    }
    // Levenshtein distance via single-row DP
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0; n + 1];
    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a_bytes[i - 1] == b_bytes[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1)
                .min(curr[j - 1] + 1)
                .min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    let dist = prev[n];
    let max_len = m.max(n);
    1.0 - (dist as f64 / max_len as f64)
}

fn load_tsmetrics_config_from_file(path: &Path) -> anyhow::Result<TsmetricsConfig> {
    let content = std::fs::read_to_string(path)?;
    if content.trim().is_empty() {
        return Ok(TsmetricsConfig {
            thresholds: ThresholdsConfig::default(),
            exclude: Vec::new(),
            paths: Vec::new(),
        });
    }
    // Pre-parse: warn about unrecognized keys before typed deserialization
    if let Ok(raw) = serde_yaml::from_str::<serde_yaml::Value>(&content) {
        warn_unknown_yaml_keys(&raw, path);
    }
    let yaml: TsmetricsYaml = serde_yaml::from_str(&content)
        .map_err(|e| anyhow::anyhow!("Failed to parse {}: {}", path.display(), e))?;
    let defaults = ThresholdsConfig::default();
    Ok(TsmetricsConfig {
        thresholds: ThresholdsConfig {
            cyclomatic_complexity: merge_threshold(
                yaml.thresholds.cyclomatic_complexity,
                defaults.cyclomatic_complexity,
            )?,
            loc: merge_threshold(yaml.thresholds.loc, defaults.loc)?,
            nesting: merge_threshold(yaml.thresholds.nesting, defaults.nesting)?,
            params: merge_threshold(yaml.thresholds.params, defaults.params)?,
            wmc: merge_threshold(yaml.thresholds.wmc, defaults.wmc)?,
            noi: merge_threshold(yaml.thresholds.noi, defaults.noi)?,
            maintainability_index: merge_inverted_float_threshold(
                yaml.thresholds.maintainability_index,
                defaults.maintainability_index,
            )?,
            halstead_volume: merge_float_threshold(
                yaml.thresholds.halstead_volume,
                defaults.halstead_volume,
            )?,
        },
        exclude: yaml.exclude,
        paths: yaml.paths,
    })
}

/// Load thresholds from a tsmetrics.yaml found in any of the given directories.
/// Falls back to defaults if no file is found or the file has no `thresholds` section.
pub fn load_thresholds(search_dirs: &[&Path]) -> anyhow::Result<ThresholdsConfig> {
    Ok(load_tsmetrics_config(search_dirs)?.thresholds)
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
    maintainability_index: f64,
    halstead_volume: f64,
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
                value: value as f64,
                threshold: t as f64,
                severity,
                suppressed: false,
            });
        }
    }
    // Halstead Volume (higher is worse)
    if let Some(severity) = config.halstead_volume.check(halstead_volume) {
        let t = match severity {
            Severity::Error => config.halstead_volume.error,
            Severity::Warning => config.halstead_volume.warning,
        };
        violations.push(Violation {
            file: file.to_string(),
            line,
            entity: name.to_string(),
            metric: "halstead_volume".to_string(),
            value: halstead_volume,
            threshold: t,
            severity,
            suppressed: false,
        });
    }
    // Maintainability Index (lower is worse)
    if let Some(severity) = config.maintainability_index.check(maintainability_index) {
        let t = match severity {
            Severity::Error => config.maintainability_index.error,
            Severity::Warning => config.maintainability_index.warning,
        };
        violations.push(Violation {
            file: file.to_string(),
            line,
            entity: name.to_string(),
            metric: "maintainability_index".to_string(),
            value: maintainability_index,
            threshold: t,
            severity,
            suppressed: false,
        });
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
                value: value as f64,
                threshold: t as f64,
                severity,
                suppressed: false,
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
        let yaml_path = dir.path().join("tsmetrics.yaml");
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
        let yaml_path = dir.path().join("tsmetrics.yaml");
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
        let yaml_path = dir.path().join("tsmetrics.yaml");
        std::fs::write(&yaml_path, "thresholds:\n  loc:\n    warning: 200\n").unwrap();
        let result = load_thresholds(&[dir.path()]);
        assert!(result.is_err(), "expected error for warning > error, got: {:?}", result);
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("warning") && msg.contains("error"), "error message should mention warning and error: {}", msg);
    }

    #[test]
    fn test_load_thresholds_empty_thresholds_section() {
        let dir = tempfile::tempdir().unwrap();
        let yaml_path = dir.path().join("tsmetrics.yaml");
        std::fs::write(&yaml_path, "thresholds: {}\n").unwrap();
        let config = load_thresholds(&[dir.path()]).unwrap();
        let defaults = ThresholdsConfig::default();
        assert_eq!(config.cyclomatic_complexity, defaults.cyclomatic_complexity);
        assert_eq!(config.loc, defaults.loc);
    }

    #[test]
    fn test_load_thresholds_empty_file_uses_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let yaml_path = dir.path().join("tsmetrics.yaml");
        std::fs::write(&yaml_path, "").unwrap();
        let config = load_thresholds(&[dir.path()]).unwrap();
        let defaults = ThresholdsConfig::default();
        assert_eq!(config.cyclomatic_complexity, defaults.cyclomatic_complexity);
    }

    // ── check_function_violations ──────────────────────────────────────────────

    #[test]
    fn test_function_no_violations_below_thresholds() {
        let config = ThresholdsConfig::default();
        let violations = check_function_violations("fn1", "a.ts", 1, 5, 20, 2, 3, 100.0, 0.0, &config);
        assert!(violations.is_empty());
    }

    #[test]
    fn test_function_cc_warning() {
        let config = ThresholdsConfig::default(); // warning=10
        let violations = check_function_violations("fn1", "a.ts", 1, 10, 20, 2, 3, 100.0, 0.0, &config);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].metric, "cyclomatic_complexity");
        assert_eq!(violations[0].severity, Severity::Warning);
        assert_eq!(violations[0].value, 10.0);
        assert_eq!(violations[0].threshold, 10.0);
    }

    #[test]
    fn test_function_cc_error() {
        let config = ThresholdsConfig::default(); // error=25
        let violations = check_function_violations("fn1", "a.ts", 1, 25, 20, 2, 3, 100.0, 0.0, &config);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].severity, Severity::Error);
        assert_eq!(violations[0].threshold, 25.0);
    }

    #[test]
    fn test_function_multiple_violations() {
        let config = ThresholdsConfig::default();
        // loc=100 (error), nesting=5 (error), params=7 (error), cc=10 (warning)
        let violations = check_function_violations("fn1", "a.ts", 1, 10, 100, 5, 7, 100.0, 0.0, &config);
        assert_eq!(violations.len(), 4);
    }

    #[test]
    fn test_function_violation_fields_populated() {
        let config = ThresholdsConfig::default();
        let violations = check_function_violations("myFunc", "src/foo.ts", 42, 30, 20, 2, 3, 100.0, 0.0, &config);
        assert_eq!(violations.len(), 1); // only cc=30 > warning=10
        let v = &violations[0];
        assert_eq!(v.entity, "myFunc");
        assert_eq!(v.file, "src/foo.ts");
        assert_eq!(v.line, 42);
        assert_eq!(v.metric, "cyclomatic_complexity");
        assert_eq!(v.value, 30.0);
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

    // ── Config discovery ──────────────────────────────────────────────────────

    #[test]
    fn test_find_tsmetrics_yaml() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("tsmetrics.yaml"), "thresholds: {}\n").unwrap();
        let found = find_config_file(dir.path());
        assert_eq!(found.unwrap().file_name().unwrap(), "tsmetrics.yaml");
    }

    #[test]
    fn test_find_tsmetrics_yml_when_no_yaml() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("tsmetrics.yml"), "thresholds: {}\n").unwrap();
        let found = find_config_file(dir.path());
        assert_eq!(found.unwrap().file_name().unwrap(), "tsmetrics.yml");
    }

    #[test]
    fn test_find_tsmetricsrc_yaml() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".tsmetricsrc.yaml"), "thresholds: {}\n").unwrap();
        let found = find_config_file(dir.path());
        assert_eq!(found.unwrap().file_name().unwrap(), ".tsmetricsrc.yaml");
    }

    #[test]
    fn test_find_tsmetricsrc_yml() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".tsmetricsrc.yml"), "thresholds: {}\n").unwrap();
        let found = find_config_file(dir.path());
        assert_eq!(found.unwrap().file_name().unwrap(), ".tsmetricsrc.yml");
    }

    #[test]
    fn test_find_tsmetricsrc_no_extension() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".tsmetricsrc"), "thresholds: {}\n").unwrap();
        let found = find_config_file(dir.path());
        assert_eq!(found.unwrap().file_name().unwrap(), ".tsmetricsrc");
    }

    #[test]
    fn test_yaml_takes_priority_over_yml() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("tsmetrics.yaml"), "thresholds: {}\n").unwrap();
        std::fs::write(dir.path().join("tsmetrics.yml"), "thresholds: {}\n").unwrap();
        let found = find_config_file(dir.path());
        assert_eq!(found.unwrap().file_name().unwrap(), "tsmetrics.yaml");
    }

    #[test]
    fn test_standalone_config_takes_priority_over_package_json() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("tsmetrics.yaml"), "thresholds: {}\n").unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name": "test", "tsmetrics": {"thresholds": {}}}"#,
        )
        .unwrap();
        let found = find_config_file(dir.path());
        assert_eq!(found.unwrap().file_name().unwrap(), "tsmetrics.yaml");
    }

    // ── Upward walking ────────────────────────────────────────────────────────

    #[test]
    fn test_find_config_in_parent_directory() {
        let parent = tempfile::tempdir().unwrap();
        std::fs::write(parent.path().join("tsmetrics.yaml"), "thresholds: {}\n").unwrap();
        let child = parent.path().join("subdir");
        std::fs::create_dir(&child).unwrap();
        let found = find_config_file(&child);
        assert!(found.is_some());
        assert_eq!(found.unwrap().file_name().unwrap(), "tsmetrics.yaml");
    }

    #[test]
    fn test_nearest_config_wins() {
        let parent = tempfile::tempdir().unwrap();
        // Parent has a config
        std::fs::write(
            parent.path().join("tsmetrics.yaml"),
            "thresholds:\n  loc:\n    warning: 99\n    error: 100\n",
        )
        .unwrap();
        // Child also has a config
        let child = parent.path().join("child");
        std::fs::create_dir(&child).unwrap();
        std::fs::write(
            child.join("tsmetrics.yml"),
            "thresholds:\n  loc:\n    warning: 11\n    error: 22\n",
        )
        .unwrap();
        let found = find_config_file(&child);
        assert!(found.is_some());
        // The child's config should win
        assert_eq!(found.as_ref().unwrap().parent().unwrap(), child);
    }

    #[test]
    fn test_no_config_anywhere_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let deep = dir.path().join("a").join("b").join("c");
        std::fs::create_dir_all(&deep).unwrap();
        // No config files created anywhere
        let found = find_config_file(&deep);
        // We cannot guarantee None because parent directories outside tempdir may have configs,
        // but within the tempdir tree there are none. We test the main public API instead:
        let config = load_thresholds(&[deep.as_path()]).unwrap();
        // If no config found in tempdir tree, defaults should still be returned
        assert_eq!(config.cyclomatic_complexity, ThresholdsConfig::default().cyclomatic_complexity);
        // Additionally, verify find_config_file at least doesn't panic
        let _ = found;
    }

    // ── package.json ──────────────────────────────────────────────────────────

    #[test]
    fn test_load_config_from_package_json_with_tsmetrics_key() {
        let dir = tempfile::tempdir().unwrap();
        let pkg = dir.path().join("package.json");
        std::fs::write(
            &pkg,
            r#"{
                "name": "my-app",
                "tsmetrics": {
                    "thresholds": {
                        "loc": { "warning": 30, "error": 60 }
                    },
                    "exclude": ["dist"]
                }
            }"#,
        )
        .unwrap();
        let config = load_config_from_package_json(&pkg).unwrap();
        assert_eq!(config.thresholds.loc, MetricThreshold::new(30, 60));
        assert_eq!(config.exclude, vec!["dist".to_string()]);
        // Other thresholds should be defaults
        assert_eq!(
            config.thresholds.cyclomatic_complexity,
            ThresholdsConfig::default().cyclomatic_complexity
        );
    }

    #[test]
    fn test_package_json_without_tsmetrics_key_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let pkg = dir.path().join("package.json");
        std::fs::write(&pkg, r#"{"name": "my-app", "version": "1.0.0"}"#).unwrap();
        // find_config_file should not return this package.json
        let found = find_config_file(dir.path());
        assert!(found.is_none() || found.unwrap().file_name().unwrap() != "package.json");
    }

    #[test]
    fn test_find_config_file_picks_up_package_json_with_tsmetrics() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name": "test", "tsmetrics": {"thresholds": {}}}"#,
        )
        .unwrap();
        let found = find_config_file(dir.path());
        assert!(found.is_some());
        assert_eq!(found.unwrap().file_name().unwrap(), "package.json");
    }

    // ── apply_cli_overrides ───────────────────────────────────────────────────

    #[test]
    fn test_apply_cli_overrides_changes_values() {
        let mut config = ThresholdsConfig::default();
        let overrides = ThresholdOverrides {
            cc_warning: Some(5),
            cc_error: Some(20),
            loc_warning: Some(30),
            ..ThresholdOverrides::default()
        };
        apply_cli_overrides(&mut config, &overrides).unwrap();
        assert_eq!(config.cyclomatic_complexity, MetricThreshold::new(5, 20));
        assert_eq!(config.loc.warning, 30);
        // loc error should remain default
        assert_eq!(config.loc.error, ThresholdsConfig::default().loc.error);
    }

    #[test]
    fn test_apply_cli_overrides_warning_greater_than_error_fails() {
        let mut config = ThresholdsConfig::default();
        // Default cc error is 25; set warning to 30 which is > 25
        let overrides = ThresholdOverrides {
            cc_warning: Some(30),
            ..ThresholdOverrides::default()
        };
        let result = apply_cli_overrides(&mut config, &overrides);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("warning") && msg.contains("error"), "msg: {}", msg);
    }

    #[test]
    fn test_apply_cli_overrides_preserves_non_overridden_fields() {
        let mut config = ThresholdsConfig::default();
        let defaults = ThresholdsConfig::default();
        let overrides = ThresholdOverrides {
            nesting_warning: Some(2),
            nesting_error: Some(4),
            ..ThresholdOverrides::default()
        };
        apply_cli_overrides(&mut config, &overrides).unwrap();
        // Nesting changed
        assert_eq!(config.nesting, MetricThreshold::new(2, 4));
        // Everything else unchanged
        assert_eq!(config.cyclomatic_complexity, defaults.cyclomatic_complexity);
        assert_eq!(config.loc, defaults.loc);
        assert_eq!(config.params, defaults.params);
        assert_eq!(config.wmc, defaults.wmc);
        assert_eq!(config.noi, defaults.noi);
    }

    #[test]
    fn test_apply_cli_overrides_no_overrides_is_noop() {
        let mut config = ThresholdsConfig::default();
        let defaults = ThresholdsConfig::default();
        let overrides = ThresholdOverrides::default();
        apply_cli_overrides(&mut config, &overrides).unwrap();
        assert_eq!(config.cyclomatic_complexity, defaults.cyclomatic_complexity);
        assert_eq!(config.loc, defaults.loc);
        assert_eq!(config.nesting, defaults.nesting);
        assert_eq!(config.params, defaults.params);
        assert_eq!(config.wmc, defaults.wmc);
        assert_eq!(config.noi, defaults.noi);
    }

    // ── load_tsmetrics_config_from_path ───────────────────────────────────────

    #[test]
    fn test_load_config_from_explicit_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("custom-config.yaml");
        std::fs::write(
            &path,
            "thresholds:\n  params:\n    warning: 2\n    error: 5\n",
        )
        .unwrap();
        let config = load_tsmetrics_config_from_path(&path).unwrap();
        assert_eq!(config.thresholds.params, MetricThreshold::new(2, 5));
    }

    #[test]
    fn test_load_config_from_explicit_path_not_found() {
        let path = PathBuf::from("/tmp/nonexistent-tsmetrics-config.yaml");
        let result = load_tsmetrics_config_from_path(&path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_load_config_from_explicit_package_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("package.json");
        std::fs::write(
            &path,
            r#"{"name": "test", "tsmetrics": {"thresholds": {"wmc": {"warning": 15, "error": 40}}}}"#,
        )
        .unwrap();
        let config = load_tsmetrics_config_from_path(&path).unwrap();
        assert_eq!(config.thresholds.wmc, MetricThreshold::new(15, 40));
    }

    // ── Unknown key detection ─────────────────────────────────────────────

    #[test]
    fn check_unknown_keys_detects_bad_keys() {
        let found = vec!["mi-warning".to_string(), "cc-error".to_string()];
        let warnings = check_unknown_keys(&found, KNOWN_THRESHOLD_KEYS, "threshold");
        assert_eq!(warnings.len(), 2);
        // Both should have suggestions since they partially match known keys
        assert!(warnings[0].contains("mi-warning"));
        assert!(warnings[1].contains("cc-error"));
    }

    #[test]
    fn check_unknown_keys_no_warnings_for_valid_keys() {
        let found = vec![
            "cyclomatic_complexity".to_string(),
            "loc".to_string(),
            "maintainability_index".to_string(),
        ];
        let warnings = check_unknown_keys(&found, KNOWN_THRESHOLD_KEYS, "threshold");
        assert!(warnings.is_empty());
    }

    #[test]
    fn check_unknown_keys_completely_unknown_no_suggestion() {
        let found = vec!["zzzzzzzzz".to_string()];
        let warnings = check_unknown_keys(&found, KNOWN_THRESHOLD_KEYS, "threshold");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("unknown threshold key 'zzzzzzzzz'"));
        assert!(!warnings[0].contains("did you mean"));
    }

    #[test]
    fn find_best_match_suggests_nesting_for_nest() {
        let result = find_best_match("nest", KNOWN_THRESHOLD_KEYS);
        assert_eq!(result, Some("nesting"));
    }

    #[test]
    fn find_best_match_suggests_wmc_for_wmcc() {
        let result = find_best_match("wmcc", KNOWN_THRESHOLD_KEYS);
        assert_eq!(result, Some("wmc"));
    }

    #[test]
    fn find_best_match_suggests_params_for_param() {
        let result = find_best_match("param", KNOWN_THRESHOLD_KEYS);
        assert_eq!(result, Some("params"));
    }

    #[test]
    fn normalized_similarity_identical() {
        assert!((normalized_similarity("abc", "abc") - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn normalized_similarity_completely_different() {
        let score = normalized_similarity("abc", "xyz");
        assert!(score < 0.5);
    }

    #[test]
    fn warn_unknown_yaml_keys_with_flat_threshold_format() {
        // This is the exact scenario from the issue: flat keys under thresholds
        let yaml_str = r#"
paths:
  - .
thresholds:
  mi-warning: 55
  mi-error: 40
  cc-warning: 10
  cc-error: 20
"#;
        let raw: serde_yaml::Value = serde_yaml::from_str(yaml_str).unwrap();
        // We can't easily capture stderr in a unit test, but we can verify
        // check_unknown_keys returns warnings for the flat keys.
        let mapping = raw.as_mapping().unwrap();
        let thresholds = mapping
            .iter()
            .find(|(k, _)| k.as_str() == Some("thresholds"))
            .and_then(|(_, v)| v.as_mapping())
            .unwrap();
        let keys: Vec<String> = thresholds
            .keys()
            .filter_map(|k| k.as_str().map(String::from))
            .collect();
        let warnings = check_unknown_keys(&keys, KNOWN_THRESHOLD_KEYS, "threshold");
        assert_eq!(warnings.len(), 4, "should warn about all 4 flat keys");
    }
}
