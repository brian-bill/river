use std::io::Write;

use anyhow::Result;

use crate::adapters::QueryResult;
use crate::cli::render_table::render;

use super::Exporter;

pub struct TxtExporter;

impl Exporter for TxtExporter {
    fn write(&self, result: &QueryResult, w: &mut dyn Write) -> Result<()> {
        w.write_all(render(result).as_bytes())?;
        Ok(())
    }
}
