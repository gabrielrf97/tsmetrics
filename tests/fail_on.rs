use std::io::Write;
use std::process::Command;
use tempfile::NamedTempFile;

fn tsmetrics_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_tsmetrics"))
}

/// A TypeScript file with enough complexity to trigger violations at low thresholds.
const COMPLEX_TS: &str = r#"
function complexFunction(a: number, b: number, c: number, d: number, e: number): number {
  if (a > 0) {
    if (b > 0) {
      if (c > 0) {
        if (d > 0) {
          if (e > 0) {
            return a + b + c + d + e;
          }
        }
      }
    }
  }
  return 0;
}
"#;

/// A trivial TypeScript file that should not trigger any violations.
const SIMPLE_TS: &str = r#"
function add(a: number, b: number): number {
  return a + b;
}
"#;

fn write_ts(content: &str) -> NamedTempFile {
    let mut f = tempfile::Builder::new()
        .suffix(".ts")
        .tempfile()
        .expect("create temp file");
    f.write_all(content.as_bytes())
        .expect("write temp file");
    f
}

#[test]
fn no_fail_on_exits_zero_even_with_violations() {
    let file = write_ts(COMPLEX_TS);
    let status = tsmetrics_bin()
        .args(["analyze", "--cc-warning", "1", "--cc-error", "1"])
        .arg(file.path())
        .output()
        .expect("run tsmetrics")
        .status;
    assert!(status.success(), "without --fail-on, exit code should be 0");
}

#[test]
fn fail_on_error_exits_one_when_errors_present() {
    let file = write_ts(COMPLEX_TS);
    let status = tsmetrics_bin()
        .args([
            "analyze",
            "--cc-warning", "1",
            "--cc-error", "1",
            "--fail-on", "error",
        ])
        .arg(file.path())
        .output()
        .expect("run tsmetrics")
        .status;
    assert!(!status.success(), "--fail-on error should exit 1 when error violations exist");
}

#[test]
fn fail_on_error_exits_zero_when_only_warnings() {
    let file = write_ts(SIMPLE_TS);
    let status = tsmetrics_bin()
        .args(["analyze", "--fail-on", "error"])
        .arg(file.path())
        .output()
        .expect("run tsmetrics")
        .status;
    assert!(status.success(), "--fail-on error should exit 0 when no error violations exist");
}

#[test]
fn fail_on_warning_exits_one_when_warnings_present() {
    let file = write_ts(COMPLEX_TS);
    let status = tsmetrics_bin()
        .args([
            "analyze",
            "--cc-warning", "1",
            "--cc-error", "10",
            "--fail-on", "warning",
        ])
        .arg(file.path())
        .output()
        .expect("run tsmetrics")
        .status;
    assert!(!status.success(), "--fail-on warning should exit 1 when warning violations exist");
}

#[test]
fn fail_on_exits_zero_when_no_violations() {
    let file = write_ts(SIMPLE_TS);
    let status = tsmetrics_bin()
        .args(["analyze", "--fail-on", "warning"])
        .arg(file.path())
        .output()
        .expect("run tsmetrics")
        .status;
    assert!(status.success(), "--fail-on should exit 0 when no violations exist");
}

#[test]
fn fail_on_with_json_format_outputs_valid_json() {
    let file = write_ts(COMPLEX_TS);
    let output = tsmetrics_bin()
        .args([
            "analyze",
            "--cc-warning", "1",
            "--cc-error", "1",
            "--fail-on", "error",
            "--format", "json",
        ])
        .arg(file.path())
        .output()
        .expect("run tsmetrics");

    assert!(!output.status.success(), "should exit 1");

    let stdout = String::from_utf8(output.stdout).expect("valid utf-8");
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON even when exit code is 1");
    assert!(parsed.get("violations").is_some(), "JSON should contain violations key");
}
