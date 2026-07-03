use std::io::Write;

use anyhow::Result;
use serde_json::{Map, Value, json};

use crate::adapters::QueryResult;
use crate::adapters::value_json::val_to_json;

use super::Exporter;

pub struct JsonExporter;

impl Exporter for JsonExporter {
    fn write(&self, result: &QueryResult, w: &mut dyn Write) -> Result<()> {
        let rows: Vec<Value> = result
            .rows
            .iter()
            .map(|row| {
                let mut obj = Map::new();
                for (i, col) in result.columns.iter().enumerate() {
                    obj.insert(col.clone(), val_to_json(&row[i]));
                }
                Value::Object(obj)
            })
            .collect();

        let v = json!({
            "columns": result.columns,
            "rows": rows,
            "rows_affected": result.rows_affected,
            "elapsed_ms": result.elapsed.as_millis() as u64,
        });

        serde_json::to_writer_pretty(w, &v)?;
        Ok(())
    }
}
