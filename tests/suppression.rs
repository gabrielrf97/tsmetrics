use std::io::Write;
use std::process::Command;
use tempfile::NamedTempFile;

fn tsmetrics_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_tsmetrics"))
}

fn write_ts(content: &str) -> NamedTempFile {
    let mut f = tempfile::Builder::new()
        .suffix(".ts")
        .tempfile()
        .expect("create temp file");
    f.write_all(content.as_bytes()).expect("write temp file");
    f
}

// ── Function suppression ────────────────────────────────────────────────────

const SUPPRESSED_FUNCTION: &str = r#"
// tsmetrics-disable-next-function
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

const UNSUPPRESSED_FUNCTION: &str = r#"
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

#[test]
fn suppressed_function_does_not_trigger_fail_on() {
    let file = write_ts(SUPPRESSED_FUNCTION);
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
    assert!(
        status.success(),
        "suppressed function should not trigger --fail-on"
    );
}

#[test]
fn unsuppressed_function_triggers_fail_on() {
    let file = write_ts(UNSUPPRESSED_FUNCTION);
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
    assert!(
        !status.success(),
        "unsuppressed function should trigger --fail-on"
    );
}

#[test]
fn suppressed_violations_appear_in_json_with_suppressed_field() {
    let file = write_ts(SUPPRESSED_FUNCTION);
    let output = tsmetrics_bin()
        .args([
            "analyze",
            "--cc-warning", "1",
            "--cc-error", "1",
            "--format", "json",
        ])
        .arg(file.path())
        .output()
        .expect("run tsmetrics");

    let stdout = String::from_utf8(output.stdout).expect("valid utf-8");
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("valid JSON");

    let violations = parsed["violations"].as_array().expect("violations array");
    assert!(!violations.is_empty(), "should have violations");
    for v in violations {
        assert_eq!(
            v["suppressed"], true,
            "all violations should be suppressed"
        );
    }
}

#[test]
fn unsuppressed_violations_omit_suppressed_field_in_json() {
    let file = write_ts(UNSUPPRESSED_FUNCTION);
    let output = tsmetrics_bin()
        .args([
            "analyze",
            "--cc-warning", "1",
            "--cc-error", "1",
            "--format", "json",
        ])
        .arg(file.path())
        .output()
        .expect("run tsmetrics");

    let stdout = String::from_utf8(output.stdout).expect("valid utf-8");
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("valid JSON");

    let violations = parsed["violations"].as_array().expect("violations array");
    assert!(!violations.is_empty(), "should have violations");
    for v in violations {
        assert!(
            v.get("suppressed").is_none(),
            "unsuppressed violations should not have suppressed field"
        );
    }
}

// ── Class suppression ───────────────────────────────────────────────────────

const SUPPRESSED_CLASS: &str = r#"
// tsmetrics-disable-next-class
class BigClass {
  method1() { if (true) { if (true) {} } }
  method2() { if (true) { if (true) {} } }
  method3() { if (true) { if (true) {} } }
  method4() { if (true) { if (true) {} } }
  method5() { if (true) { if (true) {} } }
  method6() { if (true) { if (true) {} } }
  method7() { if (true) { if (true) {} } }
  method8() { if (true) { if (true) {} } }
  method9() { if (true) { if (true) {} } }
  method10() { if (true) { if (true) {} } }
}
"#;

#[test]
fn suppressed_class_does_not_trigger_fail_on() {
    let file = write_ts(SUPPRESSED_CLASS);
    let status = tsmetrics_bin()
        .args([
            "analyze",
            "--wmc-warning", "1",
            "--wmc-error", "1",
            "--fail-on", "error",
        ])
        .arg(file.path())
        .output()
        .expect("run tsmetrics")
        .status;
    assert!(
        status.success(),
        "suppressed class should not trigger --fail-on"
    );
}

// ── Module suppression ──────────────────────────────────────────────────────

const SUPPRESSED_MODULE: &str = r#"
// tsmetrics-disable-module
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

class BigClass {
  method1() { if (true) { if (true) {} } }
  method2() { if (true) { if (true) {} } }
  method3() { if (true) { if (true) {} } }
  method4() { if (true) { if (true) {} } }
  method5() { if (true) { if (true) {} } }
}
"#;

#[test]
fn module_suppression_suppresses_all_violations() {
    let file = write_ts(SUPPRESSED_MODULE);
    let status = tsmetrics_bin()
        .args([
            "analyze",
            "--cc-warning", "1",
            "--cc-error", "1",
            "--wmc-warning", "1",
            "--wmc-error", "1",
            "--fail-on", "error",
        ])
        .arg(file.path())
        .output()
        .expect("run tsmetrics")
        .status;
    assert!(
        status.success(),
        "module suppression should suppress all violations"
    );
}

#[test]
fn module_suppression_marks_all_violations_suppressed_in_json() {
    let file = write_ts(SUPPRESSED_MODULE);
    let output = tsmetrics_bin()
        .args([
            "analyze",
            "--cc-warning", "1",
            "--cc-error", "1",
            "--wmc-warning", "1",
            "--wmc-error", "1",
            "--format", "json",
        ])
        .arg(file.path())
        .output()
        .expect("run tsmetrics");

    let stdout = String::from_utf8(output.stdout).expect("valid utf-8");
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("valid JSON");

    let violations = parsed["violations"].as_array().expect("violations array");
    assert!(!violations.is_empty(), "should have violations");
    for v in violations {
        assert_eq!(
            v["suppressed"], true,
            "all violations should be suppressed under module suppression"
        );
    }
}

// ── Export variants ─────────────────────────────────────────────────────────

#[test]
fn suppression_works_on_exported_const_arrow() {
    let file = write_ts(r#"
// tsmetrics-disable-next-function
export const handler = (a: number, b: number, c: number, d: number, e: number) => {
  if (a > 0) { if (b > 0) { if (c > 0) { if (d > 0) { if (e > 0) { return a; } } } } }
  return 0;
}
"#);
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
    assert!(
        status.success(),
        "suppression should work on exported const arrow functions"
    );
}
