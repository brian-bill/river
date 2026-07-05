pub mod mssql;
pub mod mysql;
pub mod postgres;
pub mod sqlite;

pub mod mongodb;

pub mod value_json;

use std::hash::{Hash, Hasher};
use std::time::Duration;

use async_trait::async_trait;

use crate::connection::{ConnectionConfig, DatabaseKind};
use crate::error::RiverError;

#[derive(Debug, Clone)]
pub enum Value {
    Null,
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Null, Value::Null) => true,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a.to_bits() == b.to_bits(),
            (Value::Bool(a), Value::Bool(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for Value {}

impl Hash for Value {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Value::Null => {}
            Value::String(s) => s.hash(state),
            Value::Int(i) => i.hash(state),
            Value::Float(f) => f.to_bits().hash(state),
            Value::Bool(b) => b.hash(state),
        }
    }
}

#[derive(Debug, Clone)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub column_sources: Vec<Option<String>>,
    pub rows: Vec<Vec<Value>>,
    pub elapsed: Duration,
    pub rows_affected: u64,
}

#[derive(Debug, Clone)]
pub struct TableInfo {
    pub name: String,
    pub schema: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub is_primary_key: bool,
}

#[derive(Debug, Clone)]
pub struct TableSchema {
    #[allow(dead_code)]
    pub name: String,
    pub columns: Vec<ColumnInfo>,
}

/// Every database adapter must implement this trait.
///
/// Implementors handle connection lifecycle and query execution
/// for a specific database backend.
#[async_trait]
pub trait DatabaseAdapter: Send + Sync {
    /// Open a connection using the given configuration.
    async fn connect(config: &ConnectionConfig) -> Result<Self, RiverError>
    where
        Self: Sized;

    /// Execute a raw SQL query and return the result set.
    async fn execute(&self, query: &str) -> Result<QueryResult, RiverError>;

    /// List all tables, optionally filtered by schema.
    async fn list_tables(&self, schema: Option<&str>) -> Result<Vec<TableInfo>, RiverError>;

    /// Describe the columns of a given table, optionally within a specific schema.
    async fn describe_table(&self, table: &str, schema: Option<&str>) -> Result<TableSchema, RiverError>;

    /// Return which database dialect this adapter speaks.
    #[allow(dead_code)]
    fn dialect(&self) -> DatabaseKind;

    /// Execute a server-level DDL command (CREATE DATABASE, DROP DATABASE).
    /// This connects to the server's maintenance database, not the user database.
    async fn exec_maintenance(&self, sql: &str) -> Result<QueryResult, RiverError>;
}

pub async fn create_adapter(
    config: &ConnectionConfig,
) -> Result<Box<dyn DatabaseAdapter>, RiverError> {
    match config.kind {
        DatabaseKind::Postgres => Ok(Box::new(postgres::PostgresAdapter::connect(config).await?)),
        DatabaseKind::MySQL => Ok(Box::new(mysql::MySQLAdapter::connect(config).await?)),
        DatabaseKind::MSSQL => Ok(Box::new(mssql::MssqlAdapter::connect(config).await?)),
        DatabaseKind::SQLite => Ok(Box::new(sqlite::SQLiteAdapter::connect(config).await?)),
        DatabaseKind::MongoDB => Ok(Box::new(mongodb::MongoAdapter::connect(config).await?)),
        DatabaseKind::AI => Err(RiverError::Unsupported(
            "AI connections are not database adapters".into(),
        )),
    }
}

pub(crate) fn format_primitive_dt(dt: time::PrimitiveDateTime) -> String {
    let format = time::macros::format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");
    dt.format(format).unwrap_or_else(|_| dt.to_string())
}

pub(crate) fn format_offset_dt(dt: time::OffsetDateTime) -> String {
    let format = time::macros::format_description!("[year]-[month]-[day]T[hour]:[minute]:[second][offset_hour sign:mandatory]:[offset_minute]");
    dt.format(format).unwrap_or_else(|_| dt.to_string())
}

pub(crate) fn format_date(d: time::Date) -> String {
    let format = time::macros::format_description!("[year]-[month]-[day]");
    d.format(format).unwrap_or_else(|_| d.to_string())
}

pub(crate) fn format_time(t: time::Time) -> String {
    let format = time::macros::format_description!("[hour]:[minute]:[second]");
    t.format(format).unwrap_or_else(|_| t.to_string())
}

/// Replace the database name in a connection URI with the given maintenance database.
pub(crate) fn swap_database_in_uri(uri: &str, new_db: &str) -> Result<String, RiverError> {
    let scheme_end = uri
        .find("://")
        .map(|p| p + 3)
        .ok_or_else(|| RiverError::Unsupported("invalid URI: missing scheme".into()))?;
    let after_scheme = &uri[scheme_end..];
    let slash_pos = after_scheme.find('/').map(|p| p + scheme_end);

    match slash_pos {
        Some(slash) => {
            let prefix = &uri[..=slash];
            let rest = &uri[slash + 1..];
            let query_pos = rest.find('?').unwrap_or(rest.len());
            Ok(format!("{}{}{}", prefix, new_db, &rest[query_pos..]))
        }
        None => Ok(format!("{}/{}", uri, new_db)),
    }
}
