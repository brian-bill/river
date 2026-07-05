use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use crate::adapters::{QueryResult, Value, create_adapter};
use crate::ai::AiClient;
use crate::connection::{ConnectionConfig, DatabaseKind};
use crate::lang::parse_all;

use super::export;
use super::render_table;
use super::run_file_processor;

// ── Test fixtures ───────────────────────────────────────────────────────────

fn sample_result() -> QueryResult {
    QueryResult {
        columns: vec!["id".into(), "name".into(), "active".into()],
        column_sources: vec![None, None, None],
        rows: vec![
            vec![
                Value::Int(1),
                Value::String("Alice".into()),
                Value::Bool(true),
            ],
            vec![Value::Int(2), Value::String("Bob".into()), Value::Null],
        ],
        elapsed: Duration::from_millis(5),
        rows_affected: 0,
    }
}

/// Spin up an in-process SQLite adapter backed by a file at `db_path` so that
/// the pool's connections share state (in-memory `:memory:` would give each
/// pool connection its own database). Returns the adapter map and source-db
/// metadata expected by the file processor.
async fn sqlite_ctx(
    db_path: &Path,
) -> (
    HashMap<String, Box<dyn crate::adapters::DatabaseAdapter>>,
    Vec<(String, DatabaseKind)>,
) {
    let cfg = ConnectionConfig {
        name: "test".into(),
        kind: DatabaseKind::SQLite,
        uri: format!("sqlite:{}?mode=rwc", db_path.display()),
        schema: None,
    };
    let adapter = create_adapter(&cfg).await.unwrap();
    let mut adapters: HashMap<String, Box<dyn crate::adapters::DatabaseAdapter>> = HashMap::new();
    adapters.insert("test".into(), adapter);
    let source_db = vec![("test".into(), DatabaseKind::SQLite)];
    (adapters, source_db)
}

fn empty_ai_configs() -> HashMap<String, crate::connection::AiConfig> {
    HashMap::new()
}

const SCRIPT: &str = "\
create table if not exists users@test (id int primary key, name string not null);
create users@test { id: 1, name: \"Alice\" };
find [id, name] from users@test";

// ── Exporter unit tests (no database required) ──────────────────────────────

#[test]
fn csv_export_round_trips_columns_and_values() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.csv");
    export::export(&sample_result(), &path).unwrap();

    let mut reader = csv::Reader::from_path(&path).unwrap();
    let headers: Vec<&str> = reader.headers().unwrap().iter().collect();
    assert_eq!(headers, vec!["id", "name", "active"]);

    let records: Vec<csv::StringRecord> = reader.records().map(|r| r.unwrap()).collect();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].get(0), Some("1"));
    assert_eq!(records[0].get(1), Some("Alice"));
    assert_eq!(records[0].get(2), Some("true"));
    // Null serializes as an empty CSV field.
    assert_eq!(records[1].get(2), Some(""));
}

#[test]
fn json_export_serializes_columns_rows_and_null() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.json");
    export::export(&sample_result(), &path).unwrap();

    let bytes = std::fs::read(&path).unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(v["columns"], serde_json::json!(["id", "name", "active"]));
    assert_eq!(v["rows"][0]["name"], serde_json::json!("Alice"));
    assert_eq!(v["rows"][0]["active"], serde_json::json!(true));
    assert_eq!(v["rows"][1]["active"], serde_json::Value::Null);
    assert_eq!(v["rows_affected"], serde_json::json!(0));
}

#[test]
fn xml_export_wraps_rows_and_columns() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.xml");
    export::export(&sample_result(), &path).unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.contains("<results"), "missing results root");
    assert!(
        content.contains("columns=\"id,name,active\""),
        "missing columns attribute: {content}"
    );
    assert!(content.contains("<row>"), "missing row element");
    assert!(
        content.contains("<col name=\"name\">Alice</col>"),
        "missing named cell: {content}"
    );
    assert!(
        content.contains("<col name=\"name\">Bob</col>"),
        "missing second row cell: {content}"
    );
}

#[test]
fn txt_export_reuses_table_renderer() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.txt");
    export::export(&sample_result(), &path).unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    assert_eq!(content, render_table::render(&sample_result()));
    assert!(content.contains("Alice"));
    assert!(content.contains("Bob"));
}

#[test]
fn xlsx_export_writes_non_empty_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.xlsx");
    export::export(&sample_result(), &path).unwrap();

    let meta = std::fs::metadata(&path).unwrap();
    assert!(meta.len() > 0, "xlsx file is empty");
}

#[test]
fn export_rejects_unsupported_extension() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.xyz");
    let err = export::export(&sample_result(), &path).unwrap_err();
    assert!(format!("{err}").contains("unsupported export type"));
    assert!(format!("{err}").contains("csv, xlsx, json, txt, xml"));
}

#[test]
fn export_creates_missing_parent_directories() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested/deep/out.csv");
    export::export(&sample_result(), &path).unwrap();
    assert!(path.exists());
}

// ── parse_all: multi-statement scripts ─────────────────────────────────────

#[test]
fn parse_all_returns_every_statement() {
    let stmts = parse_all("find a; find b; find c").unwrap();
    assert_eq!(stmts.len(), 3);
}

#[test]
fn parse_all_empty_input_yields_empty_vec() {
    let stmts = parse_all("").unwrap();
    assert!(stmts.is_empty());
}

#[test]
fn parse_all_surfaces_parse_errors() {
    assert!(parse_all("@@@").is_err());
}

// ── File processor end-to-end against in-process SQLite ─────────────────────

async fn run(
    rql_path: &Path,
    silent: bool,
    out: Option<&Path>,
    adapters: &HashMap<String, Box<dyn crate::adapters::DatabaseAdapter>>,
    source_db: &[(String, DatabaseKind)],
) -> (i32, Vec<u8>, Vec<u8>) {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = run_file_processor(
        rql_path.to_path_buf(),
        silent,
        out.map(|p| p.to_path_buf()),
        adapters,
        source_db,
        &empty_ai_configs(),
        &AiClient::new(),
        &mut stdout,
        &mut stderr,
    )
    .await;
    (code, stdout, stderr)
}

#[tokio::test]
async fn file_processor_exports_final_result_to_txt() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let (adapters, source_db) = sqlite_ctx(&db_path).await;
    let rql = dir.path().join("script.rql");
    std::fs::write(&rql, SCRIPT).unwrap();
    let out = dir.path().join("out.txt");

    let (code, stdout, _stderr) = run(&rql, false, Some(&out), &adapters, &source_db).await;
    assert_eq!(code, 0, "expected success");

    // Intermediate statements (DDL/DML) print their tables when not silent.
    assert!(String::from_utf8(stdout).unwrap().contains("(no rows)"));

    // The final statement is exported, not printed.
    let content = std::fs::read_to_string(&out).unwrap();
    assert!(content.contains("Alice"), "export missing data: {content}");
    assert!(content.contains("id"), "export missing header: {content}");
}

#[tokio::test]
async fn file_processor_silent_prints_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let (adapters, source_db) = sqlite_ctx(&db_path).await;
    let rql = dir.path().join("script.rql");
    std::fs::write(&rql, SCRIPT).unwrap();

    let (code, stdout, _stderr) = run(&rql, true, None, &adapters, &source_db).await;
    assert_eq!(code, 0);
    assert!(stdout.is_empty(), "silent mode printed to stdout");
}

#[tokio::test]
async fn file_processor_prints_table_to_stdout_when_no_out() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let (adapters, source_db) = sqlite_ctx(&db_path).await;
    let rql = dir.path().join("script.rql");
    std::fs::write(&rql, SCRIPT).unwrap();

    let (code, stdout, stderr) = run(&rql, false, None, &adapters, &source_db).await;
    assert_eq!(code, 0);
    let out = String::from_utf8(stdout).unwrap();
    // The final find result is printed as a table.
    assert!(out.contains("Alice"), "stdout missing final table: {out}");
    // Summary line goes to stderr.
    assert!(
        String::from_utf8(stderr).unwrap().contains("rows in"),
        "stderr missing summary"
    );
}

#[tokio::test]
async fn file_processor_missing_file_exits_one() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nope.rql");
    let (adapters, source_db) = sqlite_ctx(&dir.path().join("test.db")).await;

    let (code, _stdout, stderr) = run(&missing, false, None, &adapters, &source_db).await;
    assert_eq!(code, 1);
    assert!(
        String::from_utf8(stderr).unwrap().contains("no input file"),
        "expected 'no input file' message"
    );
}

#[tokio::test]
async fn file_processor_parse_error_exits_two() {
    let dir = tempfile::tempdir().unwrap();
    let (adapters, source_db) = sqlite_ctx(&dir.path().join("test.db")).await;
    let rql = dir.path().join("bad.rql");
    std::fs::write(&rql, "@@@").unwrap();

    let (code, _stdout, stderr) = run(&rql, false, None, &adapters, &source_db).await;
    assert_eq!(code, 2);
    let err = String::from_utf8(stderr).unwrap();
    assert!(
        err.contains("bad.rql"),
        "error should mention file path: {err}"
    );
}

#[tokio::test]
async fn file_processor_unsupported_extension_exits_one() {
    let dir = tempfile::tempdir().unwrap();
    let (adapters, source_db) = sqlite_ctx(&dir.path().join("test.db")).await;
    let rql = dir.path().join("script.rql");
    std::fs::write(&rql, SCRIPT).unwrap();
    let out = dir.path().join("out.xyz");

    let (code, _stdout, stderr) = run(&rql, false, Some(&out), &adapters, &source_db).await;
    assert_eq!(code, 1);
    let err = String::from_utf8(stderr).unwrap();
    assert!(err.contains("unsupported export type"), "unexpected: {err}");
}

#[tokio::test]
async fn file_processor_empty_script_exits_zero() {
    let dir = tempfile::tempdir().unwrap();
    let (adapters, source_db) = sqlite_ctx(&dir.path().join("test.db")).await;
    let rql = dir.path().join("empty.rql");
    std::fs::write(&rql, "").unwrap();

    let (code, _stdout, stderr) = run(&rql, false, None, &adapters, &source_db).await;
    assert_eq!(code, 0);
    assert!(
        String::from_utf8(stderr).unwrap().contains("no statements"),
        "expected 'no statements' hint"
    );
}
