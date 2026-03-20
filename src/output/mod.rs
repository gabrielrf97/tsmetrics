use crate::config::OutputFormat;
use crate::structs::{AnalysisResult, FunctionMetrics};
use comfy_table::{presets::UTF8_FULL, Cell, Color, Table};

pub fn render(result: &AnalysisResult, format: &OutputFormat) {
    match format {
        OutputFormat::Table => render_table(result),
        OutputFormat::Json => render_json(result),
        OutputFormat::Csv => render_csv(result),
    }
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

    let all_functions: Vec<&FunctionMetrics> = result.files.iter().flat_map(|f| &f.functions).collect();

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
}

fn render_json(result: &AnalysisResult) {
    println!("{}", serde_json::to_string_pretty(result).unwrap_or_default());
}

fn render_csv(result: &AnalysisResult) {
    println!("file,function,line,loc,sloc,complexity,nesting,params");
    for file in &result.files {
        for func in &file.functions {
            println!(
                "{},{},{},{},{},{},{},{}",
                func.file,
                func.name,
                func.line,
                func.loc,
                func.sloc,
                func.cyclomatic_complexity,
                func.max_nesting,
                func.param_count
            );
        }
    }
}
