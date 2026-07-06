use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use futures::TryStreamExt;
use tiberius::{Client, Config, ToSql};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

use super::{ColumnInfo, DatabaseAdapter, QueryResult, TableInfo, TableSchema, Value};
use crate::connection::{ConnectionConfig, DatabaseKind};
use crate::error::RiverError;

use tracing::warn;

pub struct MssqlAdapter {
    config_uri: String,
    config: ConnectionConfig,
    client: Arc<Mutex<Client<Compat<TcpStream>>>>,
}

async fn connect_client(uri: &str) -> Result<Client<Compat<TcpStream>>, RiverError> {
    let config = Config::from_ado_string(uri)?;

    let tcp = TcpStream::connect(config.get_addr()).await?;
    tcp.set_nodelay(true)?;

    let client = Client::connect(config, tcp.compat_write()).await?;
    Ok(client)
}

fn row_to_values(row: &tiberius::Row, col_count: usize) -> Vec<Value> {
    let mut values = Vec::with_capacity(col_count);
    for i in 0..col_count {
        let val = row
            .try_get::<&str, _>(i)
            .ok()
            .flatten()
            .map(|s: &str| Value::String(s.to_string()))
            .or_else(|| {
                row.try_get::<i32, _>(i)
                    .ok()
                    .flatten()
                    .map(|v| Value::Int(v as i64))
            })
            .or_else(|| {
                row.try_get::<i64, _>(i)
                    .ok()
                    .flatten()
                    .map(Value::Int)
            })
            .or_else(|| {
                row.try_get::<f64, _>(i)
                    .ok()
                    .flatten()
                    .map(Value::Float)
            })
            .or_else(|| {
                row.try_get::<bool, _>(i)
                    .ok()
                    .flatten()
                    .map(Value::Bool)
            })
            .or_else(|| {
                row.try_get::<time::OffsetDateTime, _>(i)
                    .ok()
                    .flatten()
                    .map(|dt| Value::String(super::format_offset_dt(dt)))
            })
            .or_else(|| {
                row.try_get::<time::PrimitiveDateTime, _>(i)
                    .ok()
                    .flatten()
                    .map(|dt| Value::String(super::format_primitive_dt(dt)))
            })
            .or_else(|| {
                row.try_get::<time::Date, _>(i)
                    .ok()
                    .flatten()
                    .map(|d| Value::String(super::format_date(d)))
            })
            .or_else(|| {
                row.try_get::<time::Time, _>(i)
                    .ok()
                    .flatten()
                    .map(|t| Value::String(super::format_time(t)))
            })
            .unwrap_or(Value::Null);
        values.push(val);
    }
    values
}

async fn execute_stream(
    client: &mut Client<Compat<TcpStream>>,
    query: &str,
    params: &[&dyn ToSql],
) -> Result<(Vec<String>, Vec<Vec<Value>>, u64), RiverError> {
    let mut stream = client.query(query, params).await?;

    let cols = stream.columns().await?;
    let columns: Vec<String> = cols
        .map(|c| c.iter().map(|col| col.name().to_string()).collect())
        .unwrap_or_default();
    let col_count = columns.len();

    let row_stream = stream.into_row_stream();
    futures::pin_mut!(row_stream);

    let mut rows = Vec::new();
    while let Some(row) = row_stream.try_next().await? {
        rows.push(row_to_values(&row, col_count));
    }

    Ok((columns, rows, col_count as u64))
}

#[async_trait]
impl DatabaseAdapter for MssqlAdapter {
    async fn connect(config: &ConnectionConfig) -> Result<Self, RiverError> {
        let client = connect_client(&config.uri).await?;
        Ok(Self {
            config_uri: config.uri.clone(),
            config: config.clone(),
            client: Arc::new(Mutex::new(client)),
        })
    }

    fn dialect(&self) -> DatabaseKind {
        DatabaseKind::MSSQL
    }

    async fn execute(&self, query: &str) -> Result<QueryResult, RiverError> {
        let start = Instant::now();

        let mut guard = self.client.lock().await;

        // Attempt query with reconnection on transport failure
        let (columns, rows, rows_affected) =
            match execute_stream(&mut guard, query, &[]).await {
                Ok(result) => result,
                Err(first_err) => {
                    // Transport error — drop lock, reconnect, retry
                    drop(guard);
                    warn!(
                        "MSSQL query failed, attempting reconnection: {}",
                        first_err
                    );
                    let new_client = connect_client(&self.config_uri).await?;
                    let mut guard = self.client.lock().await;
                    *guard = new_client;
                    execute_stream(&mut guard, query, &[]).await?
                }
            };

        let elapsed = start.elapsed();
        let num_cols = columns.len();

        Ok(QueryResult {
            columns,
            column_sources: vec![None; num_cols],
            rows_affected,
            rows,
            elapsed,
        })
    }

    async fn list_tables(&self, schema: Option<&str>) -> Result<Vec<TableInfo>, RiverError> {
        let mut guard = self.client.lock().await;
        let (_, rows, _) = if let Some(s) = schema {
            execute_stream(&mut guard,
                "SELECT TABLE_SCHEMA, TABLE_NAME FROM INFORMATION_SCHEMA.TABLES \
                 WHERE TABLE_TYPE = 'BASE TABLE' AND TABLE_SCHEMA = @P1 \
                 ORDER BY TABLE_SCHEMA, TABLE_NAME",
                &[&s as &dyn ToSql],
            ).await?
        } else {
            execute_stream(&mut guard,
                "SELECT TABLE_SCHEMA, TABLE_NAME FROM INFORMATION_SCHEMA.TABLES \
                 WHERE TABLE_TYPE = 'BASE TABLE' \
                 ORDER BY TABLE_SCHEMA, TABLE_NAME",
                &[],
            ).await?
        };

        let mut tables = Vec::new();
        for row in &rows {
            let schema = match row.first() {
                Some(Value::String(s)) => Some(s.clone()),
                _ => None,
            };
            let name = match row.get(1) {
                Some(Value::String(n)) => n.clone(),
                _ => continue,
            };
            tables.push(TableInfo { name, schema });
        }

        Ok(tables)
    }

    async fn describe_table(
        &self,
        table: &str,
        schema: Option<&str>,
    ) -> Result<TableSchema, RiverError> {
        let mut guard = self.client.lock().await;
        let (_, rows, _) = if let Some(s) = schema {
            execute_stream(&mut guard,
                "SELECT COLUMN_NAME, DATA_TYPE, IS_NULLABLE \
                 FROM INFORMATION_SCHEMA.COLUMNS \
                 WHERE TABLE_NAME = @P1 AND TABLE_SCHEMA = @P2 ORDER BY ORDINAL_POSITION",
                &[&table as &dyn ToSql, &s as &dyn ToSql],
            ).await?
        } else {
            execute_stream(&mut guard,
                "SELECT COLUMN_NAME, DATA_TYPE, IS_NULLABLE \
                 FROM INFORMATION_SCHEMA.COLUMNS \
                 WHERE TABLE_NAME = @P1 ORDER BY ORDINAL_POSITION",
                &[&table as &dyn ToSql],
            ).await?
        };

        let mut columns = Vec::new();
        for row in &rows {
            let name = match row.first() {
                Some(Value::String(n)) => n.clone(),
                _ => continue,
            };
            let data_type = match row.get(1) {
                Some(Value::String(t)) => t.clone(),
                _ => String::new(),
            };
            let nullable = match row.get(2) {
                Some(Value::String(n)) => n == "YES",
                _ => true,
            };
            columns.push(ColumnInfo {
                name,
                data_type,
                nullable,
                is_primary_key: false,
            });
        }

        Ok(TableSchema {
            name: table.to_string(),
            columns,
        })
    }

    async fn exec_maintenance(&self, sql: &str) -> Result<QueryResult, RiverError> {
        use super::swap_database_in_uri;
        let maint_uri = swap_database_in_uri(&self.config.uri, "master")?;
        let mut client = connect_client(&maint_uri).await?;
        let (columns, rows, rows_affected) = execute_stream(&mut client, sql, &[]).await?;
        let num_cols = columns.len();
        Ok(QueryResult {
            columns,
            column_sources: vec![None; num_cols],
            rows,
            elapsed: std::time::Duration::default(),
            rows_affected,
        })
    }
}
