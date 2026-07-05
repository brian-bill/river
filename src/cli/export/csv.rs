use std::io::Write;

use anyhow::Result;

use crate::adapters::{QueryResult, Value};
use crate::cli::render_table::value_to_string;

use super::Exporter;

fn sanitize_csv_cell(s: &str) -> String {
    match s.chars().next() {
        Some('=' | '+' | '-' | '@') => format!("'{}", s),
        _ => s.to_string(),
    }
}

fn value_to_csv_cell(val: &Value) -> String {
    match val {
        Value::String(s) => sanitize_csv_cell(s),
        _ => value_to_string(val),
    }
}

pub struct CsvExporter;

impl Exporter for CsvExporter {
    fn write(&self, result: &QueryResult, w: &mut dyn Write) -> Result<()> {
        let mut writer = csv::Writer::from_writer(w);
        writer.write_record(result.columns.iter().map(|c| c.as_str()))?;
        for row in &result.rows {
            let cells: Vec<String> = row.iter().map(value_to_csv_cell).collect();
            writer.write_record(cells.iter().map(|c| c.as_str()))?;
        }
        writer.flush()?;
        Ok(())
    }
}
