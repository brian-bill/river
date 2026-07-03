use std::io::Write;

use anyhow::Result;

use crate::adapters::QueryResult;
use crate::cli::render_table::value_to_string;

use super::Exporter;

pub struct CsvExporter;

impl Exporter for CsvExporter {
    fn write(&self, result: &QueryResult, w: &mut dyn Write) -> Result<()> {
        let mut writer = csv::Writer::from_writer(w);
        writer.write_record(result.columns.iter().map(|c| c.as_str()))?;
        for row in &result.rows {
            let cells: Vec<String> = row.iter().map(value_to_string).collect();
            writer.write_record(cells.iter().map(|c| c.as_str()))?;
        }
        writer.flush()?;
        Ok(())
    }
}
