use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::adapters::{DatabaseAdapter, QueryResult, Value};
use crate::ai::AiClient;
use crate::cli::export;
use crate::cli::render_table;
use crate::connection::{AiConfig, DatabaseKind};
use crate::engine::executor::{execute_statement, expression_to_value, resolve_params_in_statement};
use crate::lang::ast::Statement;
use crate::lang::parse_all;

use std::collections::HashMap;

/// Exit codes used by file-processor mode. They are script-friendly so that
/// `river users.rql` can be wired into shell pipelines and CI:
///
/// | code | meaning                                       |
/// |------|-----------------------------------------------|
/// | 0    | success                                       |
/// | 1    | setup / I/O error (missing file, bad export)  |
/// | 2    | RiverQL parse error                           |
/// | 3    | execution error (database / adapter failure)  |
#[derive(Clone, Copy)]
pub enum ExitCode {
    Success = 0,
    SetupError = 1,
    ParseError = 2,
    ExecutionError = 3,
}

impl ExitCode {
    pub fn as_i32(&self) -> i32 {
        *self as i32
    }
}

/// Run file-processor mode: read a `.rql` script, parse every statement,
/// execute them through the shared engine, and dispatch the result to either
/// the table renderer (stdout) or an exporter (`--out`).
///
/// All diagnostics are written to `stderr`; query output is written to
/// `stdout`. The returned `i32` is the process exit code (see [`ExitCode`]).
/// `stdout`/`stderr` are parameters rather than hard-coded to `std::io` so the
/// behaviour is fully testable in-process.
#[allow(clippy::too_many_arguments)]
pub async fn run_file_processor(
    file: PathBuf,
    silent: bool,
    out: Option<PathBuf>,
    adapters: &HashMap<String, Box<dyn DatabaseAdapter>>,
    source_db: &[(String, DatabaseKind)],
    ai_configs: &HashMap<String, AiConfig>,
    ai_client: &AiClient,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    // 1. Validate the input file exists & is readable.
    if !file.exists() {
        let _ = writeln!(stderr, "no input file: {}", file.display());
        return ExitCode::SetupError.as_i32();
    }

    let text = match fs::read_to_string(&file) {
        Ok(t) => t,
        Err(e) => {
            let _ = writeln!(stderr, "failed to read {}: {}", file.display(), e);
            return ExitCode::SetupError.as_i32();
        }
    };

    // 2. Validate the export target up front so we fail before doing work.
    if let Some(out_path) = &out
        && !is_supported_extension(out_path)
    {
        let ext = out_path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let _ = writeln!(
            stderr,
            "unsupported export type: {}; supported: csv, xlsx, json, txt, xml",
            ext
        );
        return ExitCode::SetupError.as_i32();
    }

    // 3. Parse all statements (file-processor scripts may contain several).
    let stmts = match parse_all(&text) {
        Ok(s) => s,
        Err(e) => {
            let _ = writeln!(stderr, "{}: {}", file.display(), e);
            return ExitCode::ParseError.as_i32();
        }
    };

    let stmts: Vec<Statement> = stmts
        .into_iter()
        .filter(|s| !matches!(s, Statement::Noop))
        .collect();

    // 4. Empty file: exit cleanly, hinting when interactive.
    if stmts.is_empty() {
        if !silent && out.is_none() {
            let _ = writeln!(stderr, "no statements");
        }
        return ExitCode::Success.as_i32();
    }

    // 5. Execute each statement through the shared engine; fail-fast on error.
    let mut last_result: Option<QueryResult> = None;
    let last_index = stmts.len() - 1;
    let mut params: HashMap<String, Value> = HashMap::new();
    for (i, stmt) in stmts.iter().enumerate() {
        let is_last = i == last_index;

        if let Statement::ParamAssign { name, value } = stmt {
            if let Some(v) = expression_to_value(value) {
                params.insert(name.clone(), v);
            }
            continue;
        }

        let resolved = if params.is_empty() {
            stmt.clone()
        } else {
            resolve_params_in_statement(stmt, &params)
        };

        match execute_statement(&resolved, source_db, adapters, ai_configs, ai_client).await {
            Ok(result) => {
                if is_last && out.is_some() {
                    last_result = Some(result);
                } else if !silent {
                    let _ = stdout.write_all(render_table::render(&result).as_bytes());
                    let _ = writeln!(stderr, "{}", render_table::summary_line(&result, source_db));
                }
            }
            Err(e) => {
                let _ = writeln!(stderr, "statement {}: {}", i + 1, e);
                return ExitCode::ExecutionError.as_i32();
            }
        }
    }

    // 6. Dispatch the final result to the requested sink.
    if let Some(out_path) = out {
        let result = last_result.unwrap_or_else(empty_result);
        if let Err(e) = export::export(&result, &out_path) {
            let _ = writeln!(stderr, "cannot write {}: {}", out_path.display(), e);
            return ExitCode::SetupError.as_i32();
        }
    }

    ExitCode::Success.as_i32()
}

fn is_supported_extension(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("csv" | "xlsx" | "json" | "txt" | "xml")
    )
}

fn empty_result() -> QueryResult {
    QueryResult {
        columns: vec![],
        column_sources: vec![],
        rows: vec![],
        elapsed: std::time::Duration::ZERO,
        rows_affected: 0,
    }
}
