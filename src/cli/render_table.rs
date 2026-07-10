use crate::adapters::{QueryResult, Value};
use crate::connection::DatabaseKind;

/// Serialize a River [`Value`] to a plain string for table, CSV, TXT, and XML
/// output. `Null` becomes the empty string (so it vanishes in delimited
/// formats), booleans become `"true"`/`"false"`, and numbers use their Rust
/// `Display` form. JSON output uses [`crate::adapters::value_json::val_to_json`]
/// instead, where `Null` maps to JSON `null`.
pub fn value_to_string(val: &Value) -> String {
    match val {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Int(n) => n.to_string(),
        Value::Float(f) => format_float(*f),
        Value::Bool(b) => b.to_string(),
    }
}

fn format_float(f: f64) -> String {
    if f == f.trunc() && f.is_finite() {
        format!("{:.1}", f)
    } else {
        format!("{}", f)
    }
}

/// Render a [`QueryResult`] as a fixed-width ASCII table.
///
/// The table uses `|` column separators and a `-` divider row, with each
/// column sized to its widest member (header or cell). Output is plain bytes —
/// no `ratatui`/`crossterm` — so it survives pipes and file redirection. An
/// empty result renders as `(no rows)`.
pub fn render(result: &QueryResult) -> String {
    if result.columns.is_empty() {
        return String::from("(no rows)\n");
    }

    let widths: Vec<usize> = result
        .columns
        .iter()
        .enumerate()
        .map(|(i, col)| {
            let cell_width = result
                .rows
                .iter()
                .map(|row| value_to_string(&row[i]).chars().count())
                .max()
                .unwrap_or(0);
            col.chars().count().max(cell_width)
        })
        .collect();

    let mut out = String::new();

    // Header
    out.push('|');
    for (i, col) in result.columns.iter().enumerate() {
        out.push(' ');
        out.push_str(&pad(col, widths[i]));
        out.push_str(" |");
    }
    out.push('\n');

    // Separator
    out.push('|');
    for &w in &widths {
        out.push(' ');
        out.push_str(&"-".repeat(w));
        out.push_str(" |");
    }
    out.push('\n');

    // Rows
    for row in &result.rows {
        out.push('|');
        for (i, cell) in row.iter().enumerate() {
            let text = value_to_string(cell);
            out.push(' ');
            out.push_str(&pad(&text, widths[i]));
            out.push_str(" |");
        }
        out.push('\n');
    }

    out
}

fn pad(s: &str, width: usize) -> String {
    let len = s.chars().count();
    if len >= width {
        s.to_string()
    } else {
        let mut out = String::with_capacity(width);
        out.push_str(s);
        out.push_str(&" ".repeat(width - len));
        out
    }
}

/// Format the one-line summary mirroring the TUI footer: `N rows in Xms`
/// (or `N rows affected in Xms` for DML), optionally annotated with the
/// connection when exactly one source is configured.
pub fn summary_line(result: &QueryResult, source_db: &[(String, DatabaseKind)]) -> String {
    let elapsed_ms = result.elapsed.as_millis();
    let (count, noun) = if result.rows.is_empty() && result.rows_affected > 0 {
        (result.rows_affected, "rows affected")
    } else {
        (result.rows.len() as u64, "rows")
    };

    let conn = single_connection_label(source_db);
    match conn {
        Some(label) => format!("{} {} in {}ms ({})", count, noun, elapsed_ms, label),
        None => format!("{} {} in {}ms", count, noun, elapsed_ms),
    }
}

fn single_connection_label(source_db: &[(String, DatabaseKind)]) -> Option<String> {
    if source_db.len() == 1 {
        let (name, kind) = &source_db[0];
        Some(format!("{:?}@{}", kind, name))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{QueryResult, Value};
    use std::time::Duration;

    fn sample_result() -> QueryResult {
        QueryResult {
            columns: vec!["id".into(), "name".into()],
            column_sources: vec![None, None],
            rows: vec![
                vec![Value::Int(1), Value::String("Alice".into())],
                vec![Value::Int(2), Value::String("Bob".into())],
            ],
            elapsed: Duration::from_millis(12),
            rows_affected: 0,
        }
    }

    #[test]
    fn render_aligns_columns_and_includes_header_and_rows() {
        let table = render(&sample_result());
        assert!(table.contains("| id | name  |"));
        assert!(table.contains("| -- | ----- |"));
        assert!(table.contains("| 1  | Alice |"));
        assert!(table.contains("| 2  | Bob   |"));
    }

    #[test]
    fn render_empty_result_shows_no_rows() {
        let empty = QueryResult {
            columns: vec![],
            column_sources: vec![],
            rows: vec![],
            elapsed: Duration::ZERO,
            rows_affected: 0,
        };
        assert_eq!(render(&empty), "(no rows)\n");
    }

    #[test]
    fn value_to_string_handles_all_variants() {
        assert_eq!(value_to_string(&Value::Null), "");
        assert_eq!(value_to_string(&Value::String("x".into())), "x");
        assert_eq!(value_to_string(&Value::Int(42)), "42");
        assert_eq!(value_to_string(&Value::Bool(true)), "true");
        assert_eq!(value_to_string(&Value::Float(3.0)), "3.0");
        assert_eq!(value_to_string(&Value::Float(std::f64::consts::PI)), "3.141592653589793");
    }

    #[test]
    fn summary_line_includes_connection_when_single_source() {
        let source_db = vec![("test".into(), DatabaseKind::SQLite)];
        let line = summary_line(&sample_result(), &source_db);
        assert_eq!(line, "2 rows in 12ms (SQLite@test)");
    }

    #[test]
    fn summary_line_omits_connection_when_multiple_sources() {
        let source_db = vec![
            ("a".into(), DatabaseKind::SQLite),
            ("b".into(), DatabaseKind::Postgres),
        ];
        let line = summary_line(&sample_result(), &source_db);
        assert_eq!(line, "2 rows in 12ms");
    }
}
