use std::collections::HashMap;

use rmcp::model::Tool;
use rmcp::{ErrorData as McpError, object};
use serde_json::Value;

use crate::adapters::DatabaseAdapter;
use crate::adapters::value_json::val_to_json;
use crate::ai::AiClient;
use crate::connection::{AiConfig, DatabaseKind};
use crate::engine::executor::execute_statement;
use crate::lang;

use super::docs;

pub fn tool_definitions() -> Vec<Tool> {
    vec![
        Tool::new(
            "riverql_query",
            "Execute a RiverQL query (SELECT, INSERT, UPDATE, DELETE, DDL, DML).",
            object!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The RiverQL query to execute"
                    },
                    "connection": {
                        "type": "string",
                        "description": "Default connection to use when tables don't specify @conn"
                    }
                },
                "required": ["query"]
            }),
        ),
        Tool::new(
            "riverql_describe",
            "Describe a table's schema (columns, types, nullability).",
            object!({
                "type": "object",
                "properties": {
                    "table": {
                        "type": "string",
                        "description": "Table name, optionally with @connection suffix"
                    },
                    "schema": {
                        "type": "string",
                        "description": "Schema name (defaults to connection's default schema)"
                    }
                },
                "required": ["table"]
            }),
        ),
        Tool::new(
            "riverql_list_tables",
            "List all tables/collections on a connection.",
            object!({
                "type": "object",
                "properties": {
                    "connection": {
                        "type": "string",
                        "description": "Connection to list tables from (defaults to first configured)"
                    }
                },
                "required": []
            }),
        ),
        Tool::new(
            "riverql_explain",
            "Get the query execution plan without executing — shows native SQL/MQL equivalent.",
            object!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The RiverQL query to explain"
                    }
                },
                "required": ["query"]
            }),
        ),
        Tool::new(
            "riverql_list_connections",
            "List all configured database connections.",
            object!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        ),
        Tool::new(
            "riverql_help",
            "Return RiverQL syntax reference documentation.",
            object!({
                "type": "object",
                "properties": {
                    "topic": {
                        "type": "string",
                        "description": "Specific topic: select, joins, aggregation, window, modification, ddl, meta, cross_db, operators, functions, keywords"
                    }
                },
                "required": []
            }),
        ),
    ]
}

pub async fn dispatch(
    adapters: &HashMap<String, Box<dyn DatabaseAdapter>>,
    source_db: &[(String, DatabaseKind)],
    ai_configs: &HashMap<String, AiConfig>,
    ai_client: &AiClient,
    name: &str,
    arguments: Option<serde_json::Map<String, Value>>,
) -> Result<rmcp::model::CallToolResult, McpError> {
    let args = arguments.unwrap_or_default();

    match name {
        "riverql_query" => handle_query(adapters, source_db, ai_configs, ai_client, &args).await,
        "riverql_describe" => handle_describe(adapters, source_db, ai_configs, ai_client, &args).await,
        "riverql_list_tables" => handle_list_tables(adapters, source_db, &args).await,
        "riverql_explain" => handle_explain(&args),
        "riverql_list_connections" => handle_list_connections(source_db, adapters),
        "riverql_help" => handle_help(&args),
        _ => Ok(rmcp::model::CallToolResult::error(vec![
            rmcp::model::Content::text(format!("unknown tool: {}", name)),
        ])),
    }
}

async fn run_query(
    adapters: &HashMap<String, Box<dyn DatabaseAdapter>>,
    source_db: &[(String, DatabaseKind)],
    ai_configs: &HashMap<String, AiConfig>,
    ai_client: &AiClient,
    query: &str,
) -> Result<rmcp::model::CallToolResult, McpError> {
    let stmt = lang::parse(query).map_err(|e| {
        McpError::internal_error(format!("parse error: {}", e), None)
    })?;

    let result = execute_statement(&stmt, source_db, adapters, ai_configs, ai_client)
        .await
        .map_err(|e| {
            McpError::internal_error(format!("execution error: {}", e), None)
        })?;

    let elapsed_ms = result.elapsed.as_millis() as u64;
    let max_rows = 1000usize;
    let truncated = result.rows.len() > max_rows;
    let total_rows = result.rows.len();

    let rows: Vec<Value> = result
        .rows
        .iter()
        .take(max_rows)
        .map(|row| {
            Value::Array(row.iter().map(val_to_json).collect())
        })
        .collect();

    let mut v = serde_json::json!({
        "columns": result.columns,
        "rows": rows,
        "elapsed_ms": elapsed_ms,
        "rows_affected": result.rows_affected,
        "total_rows": total_rows,
    });

    if truncated {
        v["truncated"] = serde_json::json!(true);
    }

    let text = serde_json::to_string_pretty(&v).unwrap_or_default();
    Ok(rmcp::model::CallToolResult::success(vec![
        rmcp::model::Content::text(text),
    ]))
}

async fn handle_query(
    adapters: &HashMap<String, Box<dyn DatabaseAdapter>>,
    source_db: &[(String, DatabaseKind)],
    ai_configs: &HashMap<String, AiConfig>,
    ai_client: &AiClient,
    args: &serde_json::Map<String, Value>,
) -> Result<rmcp::model::CallToolResult, McpError> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::invalid_params("missing required parameter 'query'", None))?;

    run_query(adapters, source_db, ai_configs, ai_client, query).await
}

async fn handle_describe(
    adapters: &HashMap<String, Box<dyn DatabaseAdapter>>,
    source_db: &[(String, DatabaseKind)],
    ai_configs: &HashMap<String, AiConfig>,
    ai_client: &AiClient,
    args: &serde_json::Map<String, Value>,
) -> Result<rmcp::model::CallToolResult, McpError> {
    let table_input = args
        .get("table")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::invalid_params("missing required parameter 'table'", None))?;

    let schema = args.get("schema").and_then(|v| v.as_str());

    let query = match schema {
        Some(s) => format!("describe {}.{}", s, table_input),
        None => format!("describe {}", table_input),
    };

    run_query(adapters, source_db, ai_configs, ai_client, &query).await
}

async fn handle_list_tables(
    adapters: &HashMap<String, Box<dyn DatabaseAdapter>>,
    source_db: &[(String, DatabaseKind)],
    args: &serde_json::Map<String, Value>,
) -> Result<rmcp::model::CallToolResult, McpError> {
    let default_conn = args.get("connection").and_then(|v| v.as_str());
    let conn_name = match default_conn {
        Some(c) => c.to_string(),
        None => match source_db.first() {
            Some((name, _)) => name.clone(),
            None => {
                return Ok(rmcp::model::CallToolResult::error(vec![
                    rmcp::model::Content::text(
                        "no connections configured — create a river.yaml file",
                    ),
                ]));
            }
        },
    };

    let adapter = match adapters.get(&conn_name) {
        Some(a) => a,
        None => {
            return Ok(rmcp::model::CallToolResult::error(vec![
                rmcp::model::Content::text(format!(
                    "no adapter connected for '{}'",
                    conn_name
                )),
            ]));
        }
    };

    let tables = adapter.list_tables(None).await.map_err(|e| {
        McpError::internal_error(format!("list tables error: {}", e), None)
    })?;

    let v = serde_json::json!({
        "connection": conn_name,
        "tables": tables.iter().map(|t| {
            serde_json::json!({
                "name": t.name,
                "schema": t.schema,
            })
        }).collect::<Vec<_>>()
    });

    let text = serde_json::to_string_pretty(&v).unwrap_or_default();
    Ok(rmcp::model::CallToolResult::success(vec![
        rmcp::model::Content::text(text),
    ]))
}

pub(crate) fn handle_explain(
    args: &serde_json::Map<String, Value>,
) -> Result<rmcp::model::CallToolResult, McpError> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::invalid_params("missing required parameter 'query'", None))?;

    let text = format!(
        "To see the native SQL/MQL generated by RiverQL, run:\n\n  riverql_query with query: \"explain {}\"\n\nThis will return the query execution plan.",
        query
    );

    Ok(rmcp::model::CallToolResult::success(vec![
        rmcp::model::Content::text(text),
    ]))
}

pub(crate) fn handle_list_connections(
    source_db: &[(String, DatabaseKind)],
    adapters: &HashMap<String, Box<dyn DatabaseAdapter>>,
) -> Result<rmcp::model::CallToolResult, McpError> {
    let connections: Vec<Value> = source_db
        .iter()
        .map(|(name, kind)| {
            serde_json::json!({
                "name": name,
                "kind": format!("{:?}", kind).to_lowercase(),
                "connected": adapters.contains_key(name),
            })
        })
        .collect();

    let v = serde_json::json!({ "connections": connections });
    let text = serde_json::to_string_pretty(&v).unwrap_or_default();
    Ok(rmcp::model::CallToolResult::success(vec![
        rmcp::model::Content::text(text),
    ]))
}

pub(crate) fn handle_help(
    args: &serde_json::Map<String, Value>,
) -> Result<rmcp::model::CallToolResult, McpError> {
    let topic = args.get("topic").and_then(|v| v.as_str());
    let text = match topic {
        Some(t) => docs::topic(t).unwrap_or_else(|| docs::overview()),
        None => docs::overview(),
    };

    Ok(rmcp::model::CallToolResult::success(vec![
        rmcp::model::Content::text(text),
    ]))
}
