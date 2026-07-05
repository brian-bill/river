use std::io::Write;

use anyhow::Result;
use quick_xml::Writer;
use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};

use crate::adapters::QueryResult;
use crate::cli::render_table::value_to_string;

use super::Exporter;

pub struct XmlExporter;

impl Exporter for XmlExporter {
    fn write(&self, result: &QueryResult, w: &mut dyn Write) -> Result<()> {
        let mut writer = Writer::new(w);

        let mut results_el = BytesStart::new("results");
        results_el.push_attribute(("columns", result.columns.join(",").as_str()));
        writer.write_event(Event::Start(results_el))?;

        for row in &result.rows {
            writer.write_event(Event::Start(BytesStart::new("row")))?;
            for (i, val) in row.iter().enumerate() {
                let mut col_el = BytesStart::new("col");
                col_el.push_attribute(("name", result.columns[i].as_str()));
                writer.write_event(Event::Start(col_el))?;
                writer.write_event(Event::Text(BytesText::new(&value_to_string(val))))?;
                writer.write_event(Event::End(BytesEnd::new("col")))?;
            }
            writer.write_event(Event::End(BytesEnd::new("row")))?;
        }

        writer.write_event(Event::End(BytesEnd::new("results")))?;
        Ok(())
    }
}
