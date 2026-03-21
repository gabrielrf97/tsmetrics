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

fn render_csv(result: &AnalysisResult) {
    println!("file,function,line,loc,sloc,complexity,nesting,params");
    for file in &result.files {
        for func in &file.functions {
            println!(
                "{},{},{},{},{},{},{},{}",
                csv_field(&func.file),
                csv_field(&func.name),
                func.line,
                func.loc,
                func.sloc,
                func.cyclomatic_complexity,
                func.max_nesting,
                func.param_count,
            );
        }
    }
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
