use std::path::Path;

use anyhow::Result;
use rust_xlsxwriter::{Format, Workbook, Worksheet};

use crate::adapters::{QueryResult, Value};

/// Write `result` to an `.xlsx` workbook at `path`.
///
/// The header row is written bold; body cells are written as typed data
/// (numbers, booleans, strings) so Excel treats them natively. `Null` cells are
/// skipped, leaving them blank.
pub fn write(result: &QueryResult, path: &Path) -> Result<()> {
    let mut workbook = Workbook::new();
    let bold = Format::new().set_bold();
    let mut worksheet = Worksheet::new();

    for (c, col) in result.columns.iter().enumerate() {
        worksheet.write_with_format(0, c as u16, col.as_str(), &bold)?;
    }

    for (r, row) in result.rows.iter().enumerate() {
        let row_num = (r + 1) as u32;
        for (c, val) in row.iter().enumerate() {
            let col_num = c as u16;
            match val {
                Value::Int(n) => {
                    worksheet.write_number(row_num, col_num, *n as f64)?;
                }
                Value::Float(f) => {
                    worksheet.write_number(row_num, col_num, *f)?;
                }
                Value::Bool(b) => {
                    worksheet.write_boolean(row_num, col_num, *b)?;
                }
                Value::String(s) => {
                    worksheet.write_string(row_num, col_num, s)?;
                }
                Value::Null => {}
            }
        }
    }

    workbook.push_worksheet(worksheet);
    workbook.save(path)?;
    Ok(())
}
