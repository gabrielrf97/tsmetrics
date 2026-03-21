use crate::config::OutputFormat;
use crate::structs::{AnalysisResult, FunctionMetrics};
use crate::thresholds::Severity;
use anyhow::Result;
use comfy_table::{presets::UTF8_FULL, Cell, Color, Table};

pub fn render(result: &AnalysisResult, format: &OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Table => render_table(result),
        OutputFormat::Json => render_json(result)?,
        OutputFormat::Csv => render_csv(result),
        OutputFormat::Html => render_html(result),
    }
    Ok(())
}

fn render_table(result: &AnalysisResult) {
    println!(
        "\nAnalyzed {} file(s) — {} function(s) — {} LOC total\n",
        result.total_files, result.total_functions, result.total_loc
    );

    if result.total_functions == 0 {
        println!("No functions found.");
        return;
    }

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec![
        Cell::new("File").fg(Color::Cyan),
        Cell::new("Function").fg(Color::Cyan),
        Cell::new("Line").fg(Color::Cyan),
        Cell::new("LOC").fg(Color::Cyan),
        Cell::new("SLOC").fg(Color::Cyan),
        Cell::new("Complexity").fg(Color::Cyan),
        Cell::new("Nesting").fg(Color::Cyan),
        Cell::new("Params").fg(Color::Cyan),
    ]);

    let all_functions: Vec<&FunctionMetrics> =
        result.files.iter().flat_map(|f| &f.functions).collect();

    for func in all_functions {
        let complexity_cell = if func.cyclomatic_complexity >= 10 {
            Cell::new(func.cyclomatic_complexity).fg(Color::Red)
        } else if func.cyclomatic_complexity >= 5 {
            Cell::new(func.cyclomatic_complexity).fg(Color::Yellow)
        } else {
            Cell::new(func.cyclomatic_complexity).fg(Color::Green)
        };

        table.add_row(vec![
            Cell::new(&func.file),
            Cell::new(&func.name),
            Cell::new(func.line),
            Cell::new(func.loc),
            Cell::new(func.sloc),
            complexity_cell,
            Cell::new(func.max_nesting),
            Cell::new(func.param_count),
        ]);
    }

    println!("{table}");

    // Render violations table if any
    if !result.violations.is_empty() {
        println!(
            "\nViolations ({} total):\n",
            result.violations.len()
        );
        let mut vtable = Table::new();
        vtable.load_preset(UTF8_FULL);
        vtable.set_header(vec![
            Cell::new("File").fg(Color::Cyan),
            Cell::new("Entity").fg(Color::Cyan),
            Cell::new("Line").fg(Color::Cyan),
            Cell::new("Metric").fg(Color::Cyan),
            Cell::new("Value").fg(Color::Cyan),
            Cell::new("Threshold").fg(Color::Cyan),
            Cell::new("Severity").fg(Color::Cyan),
        ]);
        for v in &result.violations {
            let severity_cell = match v.severity {
                Severity::Error => Cell::new("error").fg(Color::Red),
                Severity::Warning => Cell::new("warning").fg(Color::Yellow),
            };
            vtable.add_row(vec![
                Cell::new(&v.file),
                Cell::new(&v.entity),
                Cell::new(v.line),
                Cell::new(&v.metric),
                Cell::new(v.value),
                Cell::new(v.threshold),
                severity_cell,
            ]);
        }
        println!("{vtable}");
    }
}

fn render_json(result: &AnalysisResult) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(result)?);
    Ok(())
}

pub fn build_csv(result: &AnalysisResult) -> String {
    let mut out = String::from("file,function,line,loc,sloc,complexity,nesting,params\n");
    for file in &result.files {
        for func in &file.functions {
            out.push_str(&format!(
                "{},{},{},{},{},{},{},{}\n",
                csv_field(&func.file),
                csv_field(&func.name),
                func.line,
                func.loc,
                func.sloc,
                func.cyclomatic_complexity,
                func.max_nesting,
                func.param_count,
            ));
        }
    }
    out
}

fn render_csv(result: &AnalysisResult) {
    print!("{}", build_csv(result));
}

fn render_html(result: &AnalysisResult) {
    println!("{}", build_html(result));
}

/// Build a self-contained HTML report with inline CSS/JS.
pub fn build_html(result: &AnalysisResult) -> String {
    let css = r#"
    <style>
      *, *::before, *::after { box-sizing: border-box; }
      body { font-family: system-ui, -apple-system, sans-serif; background: #f5f5f5; color: #333; margin: 0; padding: 1.5rem; }
      h1 { font-size: 1.5rem; margin-bottom: 0.25rem; }
      .summary { color: #555; margin-bottom: 1.5rem; font-size: 0.95rem; }
      h2 { font-size: 1.1rem; margin: 1.5rem 0 0.5rem; }
      table { border-collapse: collapse; width: 100%; background: #fff; border-radius: 6px; overflow: hidden; box-shadow: 0 1px 3px rgba(0,0,0,.12); }
      th { background: #2d3748; color: #fff; padding: 0.55rem 0.75rem; text-align: left; font-size: 0.85rem; cursor: pointer; user-select: none; white-space: nowrap; }
      th:hover { background: #3a4a60; }
      th.sort-asc::after { content: " ▲"; font-size: 0.7rem; }
      th.sort-desc::after { content: " ▼"; font-size: 0.7rem; }
      td { padding: 0.45rem 0.75rem; font-size: 0.85rem; border-top: 1px solid #eee; }
      tr:hover td { background: #f0f4ff; }
      .badge { display: inline-block; padding: 0.15rem 0.5rem; border-radius: 9999px; font-weight: 600; font-size: 0.8rem; }
      .green  { background: #d1fae5; color: #065f46; }
      .yellow { background: #fef9c3; color: #854d0e; }
      .red    { background: #fee2e2; color: #991b1b; }
      .orange { background: #fed7aa; color: #9a3412; }
      .empty  { color: #aaa; font-style: italic; padding: 1rem 0.75rem; }
    </style>"#;

    let js = r#"
    <script>
      function sortTable(tableId, col) {
        var t = document.getElementById(tableId);
        var ths = t.querySelectorAll('th');
        var tbody = t.querySelector('tbody');
        var rows = Array.from(tbody.querySelectorAll('tr'));
        var asc = ths[col].classList.contains('sort-desc');
        ths.forEach(function(th) { th.classList.remove('sort-asc','sort-desc'); });
        rows.sort(function(a, b) {
          var av = a.cells[col].dataset.val || a.cells[col].textContent.trim();
          var bv = b.cells[col].dataset.val || b.cells[col].textContent.trim();
          var an = parseFloat(av), bn = parseFloat(bv);
          if (!isNaN(an) && !isNaN(bn)) return asc ? an - bn : bn - an;
          return asc ? av.localeCompare(bv) : bv.localeCompare(av);
        });
        rows.forEach(function(r) { tbody.appendChild(r); });
        ths[col].classList.add(asc ? 'sort-asc' : 'sort-desc');
      }
    </script>"#;

    let mut functions_rows = String::new();
    let all_functions: Vec<_> = result.files.iter().flat_map(|f| &f.functions).collect();
    if all_functions.is_empty() {
        functions_rows.push_str(r#"<tr><td class="empty" colspan="8">No functions found.</td></tr>"#);
    } else {
        for func in &all_functions {
            let (badge_class, _) = complexity_badge(func.cyclomatic_complexity);
            functions_rows.push_str(&format!(
                r#"<tr>
                  <td>{}</td>
                  <td>{}</td>
                  <td data-val="{}">{}</td>
                  <td data-val="{}">{}</td>
                  <td data-val="{}">{}</td>
                  <td data-val="{}"><span class="badge {}">{}</span></td>
                  <td data-val="{}">{}</td>
                  <td data-val="{}">{}</td>
                </tr>"#,
                html_escape(&func.file),
                html_escape(&func.name),
                func.line, func.line,
                func.loc, func.loc,
                func.sloc, func.sloc,
                func.cyclomatic_complexity, badge_class, func.cyclomatic_complexity,
                func.max_nesting, func.max_nesting,
                func.param_count, func.param_count,
            ));
        }
    }

    let mut violations_section = String::new();
    if !result.violations.is_empty() {
        let mut rows = String::new();
        for v in &result.violations {
            let (sev_class, sev_label) = match v.severity {
                Severity::Error => ("red", "error"),
                Severity::Warning => ("yellow", "warning"),
            };
            rows.push_str(&format!(
                r#"<tr>
                  <td>{}</td>
                  <td>{}</td>
                  <td data-val="{}">{}</td>
                  <td>{}</td>
                  <td data-val="{}">{}</td>
                  <td data-val="{}">{}</td>
                  <td><span class="badge {}">{}</span></td>
                </tr>"#,
                html_escape(&v.file),
                html_escape(&v.entity),
                v.line, v.line,
                html_escape(&v.metric),
                v.value, v.value,
                v.threshold, v.threshold,
                sev_class, sev_label,
            ));
        }
        violations_section = format!(
            r#"<h2>Violations ({} total)</h2>
            <table id="vtable">
              <thead>
                <tr>
                  <th onclick="sortTable('vtable',0)">File</th>
                  <th onclick="sortTable('vtable',1)">Entity</th>
                  <th onclick="sortTable('vtable',2)">Line</th>
                  <th onclick="sortTable('vtable',3)">Metric</th>
                  <th onclick="sortTable('vtable',4)">Value</th>
                  <th onclick="sortTable('vtable',5)">Threshold</th>
                  <th onclick="sortTable('vtable',6)">Severity</th>
                </tr>
              </thead>
              <tbody>{}</tbody>
            </table>"#,
            result.violations.len(),
            rows
        );
    }

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>TSM Analysis Report</title>
{css}
</head>
<body>
<h1>TSM Analysis Report</h1>
<p class="summary">Analyzed <strong>{total_files}</strong> file(s) &mdash; <strong>{total_functions}</strong> function(s) &mdash; <strong>{total_loc}</strong> LOC total</p>
<h2>Functions</h2>
<table id="ftable">
  <thead>
    <tr>
      <th onclick="sortTable('ftable',0)">File</th>
      <th onclick="sortTable('ftable',1)">Function</th>
      <th onclick="sortTable('ftable',2)">Line</th>
      <th onclick="sortTable('ftable',3)">LOC</th>
      <th onclick="sortTable('ftable',4)">SLOC</th>
      <th onclick="sortTable('ftable',5)">Complexity</th>
      <th onclick="sortTable('ftable',6)">Nesting</th>
      <th onclick="sortTable('ftable',7)">Params</th>
    </tr>
  </thead>
  <tbody>{functions_rows}</tbody>
</table>
{violations_section}
{js}
</body>
</html>"#,
        css = css,
        total_files = result.total_files,
        total_functions = result.total_functions,
        total_loc = result.total_loc,
        functions_rows = functions_rows,
        violations_section = violations_section,
        js = js,
    )
}

/// Returns (badge_css_class, label) for a cyclomatic complexity value.
fn complexity_badge(complexity: usize) -> (&'static str, &'static str) {
    if complexity >= 10 {
        ("red", "high")
    } else if complexity >= 5 {
        ("yellow", "medium")
    } else {
        ("green", "low")
    }
}

/// Minimal HTML escaping for user-controlled strings.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// RFC 4180 CSV field quoting: wrap in double-quotes and escape internal quotes
/// if the value contains a comma, double-quote, or newline.
fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structs::{AnalysisResult, FileMetrics, FunctionMetrics};
    use crate::thresholds::{Severity, Violation};

    fn make_func(name: &str, file: &str, line: usize, loc: usize, sloc: usize, complexity: usize, nesting: usize, params: usize) -> FunctionMetrics {
        FunctionMetrics {
            name: name.to_string(),
            file: file.to_string(),
            line,
            loc,
            sloc,
            cyclomatic_complexity: complexity,
            max_nesting: nesting,
            param_count: params,
        }
    }

    fn make_result_with_functions() -> AnalysisResult {
        let mut result = AnalysisResult::new();
        let func1 = make_func("simple", "src/a.ts", 10, 15, 12, 2, 1, 1);
        let func2 = make_func("moderate", "src/b.ts", 5, 30, 25, 6, 3, 3);
        let func3 = make_func("complex", "src/c.ts", 1, 80, 70, 15, 6, 5);
        let file1 = FileMetrics {
            path: "src/a.ts".to_string(),
            total_loc: 15,
            total_sloc: 12,
            function_count: 1,
            class_count: 0,
            import_count: 0,
            functions: vec![func1],
            classes: vec![],
        };
        let file2 = FileMetrics {
            path: "src/b.ts".to_string(),
            total_loc: 30,
            total_sloc: 25,
            function_count: 1,
            class_count: 0,
            import_count: 0,
            functions: vec![func2],
            classes: vec![],
        };
        let file3 = FileMetrics {
            path: "src/c.ts".to_string(),
            total_loc: 80,
            total_sloc: 70,
            function_count: 1,
            class_count: 0,
            import_count: 0,
            functions: vec![func3],
            classes: vec![],
        };
        result.add_file(file1);
        result.add_file(file2);
        result.add_file(file3);
        result
    }

    fn make_result_with_violations() -> AnalysisResult {
        let mut result = make_result_with_functions();
        result.add_violations(vec![
            Violation {
                file: "src/c.ts".to_string(),
                entity: "complex".to_string(),
                line: 1,
                metric: "cyclomatic_complexity".to_string(),
                value: 15,
                threshold: 10,
                severity: Severity::Error,
            },
            Violation {
                file: "src/b.ts".to_string(),
                entity: "moderate".to_string(),
                line: 5,
                metric: "cyclomatic_complexity".to_string(),
                value: 6,
                threshold: 5,
                severity: Severity::Warning,
            },
        ]);
        result
    }

    // ── CSV tests ──────────────────────────────────────────────────────────────

    #[test]
    fn csv_has_header_row() {
        let result = make_result_with_functions();
        let csv = build_csv(&result);
        assert!(csv.starts_with("file,function,line,loc,sloc,complexity,nesting,params\n"));
    }

    #[test]
    fn csv_contains_function_data() {
        let result = make_result_with_functions();
        let csv = build_csv(&result);
        assert!(csv.contains("src/a.ts,simple,10,15,12,2,1,1"));
        assert!(csv.contains("src/b.ts,moderate,5,30,25,6,3,3"));
        assert!(csv.contains("src/c.ts,complex,1,80,70,15,6,5"));
    }

    #[test]
    fn csv_row_count_matches_function_count() {
        let result = make_result_with_functions();
        let csv = build_csv(&result);
        // 1 header + 3 data rows = 4 non-empty lines
        let lines: Vec<&str> = csv.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 4);
    }

    #[test]
    fn csv_field_quotes_comma() {
        assert_eq!(csv_field("a,b"), "\"a,b\"");
    }

    #[test]
    fn csv_field_quotes_double_quote() {
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    #[test]
    fn csv_field_plain_string_unquoted() {
        assert_eq!(csv_field("hello"), "hello");
    }

    // ── HTML tests ─────────────────────────────────────────────────────────────

    #[test]
    fn html_has_doctype() {
        let result = make_result_with_functions();
        let html = build_html(&result);
        assert!(html.starts_with("<!DOCTYPE html>"));
    }

    #[test]
    fn html_contains_summary_stats() {
        let result = make_result_with_functions();
        let html = build_html(&result);
        assert!(html.contains("3</strong> file(s)"));
        assert!(html.contains("3</strong> function(s)"));
    }

    #[test]
    fn html_contains_function_names() {
        let result = make_result_with_functions();
        let html = build_html(&result);
        assert!(html.contains("simple"));
        assert!(html.contains("moderate"));
        assert!(html.contains("complex"));
    }

    #[test]
    fn html_green_badge_for_low_complexity() {
        let result = make_result_with_functions();
        let html = build_html(&result);
        assert!(html.contains(r#"class="badge green""#));
    }

    #[test]
    fn html_yellow_badge_for_medium_complexity() {
        let result = make_result_with_functions();
        let html = build_html(&result);
        assert!(html.contains(r#"class="badge yellow""#));
    }

    #[test]
    fn html_red_badge_for_high_complexity() {
        let result = make_result_with_functions();
        let html = build_html(&result);
        assert!(html.contains(r#"class="badge red""#));
    }

    #[test]
    fn html_contains_sortable_table_js() {
        let result = make_result_with_functions();
        let html = build_html(&result);
        assert!(html.contains("sortTable"));
        assert!(html.contains("onclick"));
    }

    #[test]
    fn html_no_violations_section_when_empty() {
        let result = make_result_with_functions();
        let html = build_html(&result);
        assert!(!html.contains("Violations"));
    }

    #[test]
    fn html_violations_section_present_when_nonempty() {
        let result = make_result_with_violations();
        let html = build_html(&result);
        assert!(html.contains("Violations (2 total)"));
    }

    #[test]
    fn html_violations_contain_error_badge() {
        let result = make_result_with_violations();
        let html = build_html(&result);
        assert!(html.contains(r#"class="badge red">error"#));
    }

    #[test]
    fn html_violations_contain_warning_badge() {
        let result = make_result_with_violations();
        let html = build_html(&result);
        assert!(html.contains(r#"class="badge yellow">warning"#));
    }

    #[test]
    fn html_escapes_special_chars() {
        assert_eq!(html_escape("<script>"), "&lt;script&gt;");
        assert_eq!(html_escape("a & b"), "a &amp; b");
        assert_eq!(html_escape("say \"hi\""), "say &quot;hi&quot;");
    }

    #[test]
    fn html_empty_result_shows_no_functions_message() {
        let result = AnalysisResult::new();
        let html = build_html(&result);
        assert!(html.contains("No functions found."));
    }

    #[test]
    fn html_is_self_contained_no_external_links() {
        let result = make_result_with_functions();
        let html = build_html(&result);
        // Should not contain external script or link tags
        assert!(!html.contains("src=\"http"));
        assert!(!html.contains("href=\"http"));
    }
}
