use std::fs::File;
use std::io::Write;
use std::path::Path;

use anyhow::{Result, bail};

use crate::adapters::QueryResult;

pub mod csv;
pub mod json;
pub mod txt;
pub mod xlsx;
pub mod xml;

/// A sink that serializes a [`QueryResult`] to a byte stream.
///
/// Implementors cover the text-based formats (CSV, JSON, TXT, XML); the XLSX
/// exporter writes directly to a path because the workbook is a binary archive,
/// not a streaming byte writer.
pub trait Exporter {
    fn write(&self, result: &QueryResult, w: &mut dyn Write) -> Result<()>;
}

/// Write `result` to `path` in a format selected by the path's extension.
///
/// Supported extensions: `csv`, `xlsx`, `json`, `txt`, `xml`. Any other
/// extension returns an error whose message lists the supported set. Missing
/// parent directories are created so that `--out dir/sub/out.csv` works.
pub fn export(result: &QueryResult, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    match ext {
        "csv" => write_with(result, path, csv::CsvExporter),
        "json" => write_with(result, path, json::JsonExporter),
        "txt" => write_with(result, path, txt::TxtExporter),
        "xml" => write_with(result, path, xml::XmlExporter),
        "xlsx" => xlsx::write(result, path),
        other => bail!(
            "unsupported export type: {}; supported: csv, xlsx, json, txt, xml",
            other
        ),
    }
}

fn write_with(result: &QueryResult, path: &Path, exporter: impl Exporter) -> Result<()> {
    let mut file = File::create(path)?;
    exporter.write(result, &mut file)?;
    Ok(())
}
