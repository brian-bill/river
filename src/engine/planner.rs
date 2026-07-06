use std::collections::HashMap;
use std::sync::Arc;

use crate::connection::{AiConfig, DatabaseKind};
use crate::lang::ast::*;

pub const CROSS_DB_BATCH_SIZE: usize = 1000;

#[derive(Debug, Clone, PartialEq)]
pub struct AiColumn {
    pub expr: Expression,
    pub alias: Option<String>,
}

impl PartialEq for PlanNode {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (PlanNode::Scan { source: s1, database: d1, filter: f1 }, PlanNode::Scan { source: s2, database: d2, filter: f2 }) => {
                s1 == s2 && d1 == d2 && f1 == f2
            }
            (PlanNode::Join { left: l1, right: r1, condition: c1, strategy: s1, join_kind: j1, limit: li1 },
             PlanNode::Join { left: l2, right: r2, condition: c2, strategy: s2, join_kind: j2, limit: li2 }) => {
                l1 == l2 && r1 == r2 && c1 == c2 && s1 == s2 && j1 == j2 && li1 == li2
            }
            (PlanNode::Project { input: i1, fields: f1 }, PlanNode::Project { input: i2, fields: f2 }) => {
                i1 == i2 && f1 == f2
            }
            (PlanNode::Aggregate { input: i1, group_by: g1, aggs: a1, having: h1 }, PlanNode::Aggregate { input: i2, group_by: g2, aggs: a2, having: h2 }) => {
                i1 == i2 && g1 == g2 && a1 == a2 && h1 == h2
            }
            (PlanNode::Limit { input: i1, limit: l1, offset: o1 }, PlanNode::Limit { input: i2, limit: l2, offset: o2 }) => {
                i1 == i2 && l1 == l2 && o1 == o2
            }
            (PlanNode::Order { input: i1, order_by: o1 }, PlanNode::Order { input: i2, order_by: o2 }) => {
                i1 == i2 && o1 == o2
            }
            (PlanNode::Distinct { input: i1 }, PlanNode::Distinct { input: i2 }) => i1 == i2,
            (PlanNode::Filter { input: i1, condition: c1 }, PlanNode::Filter { input: i2, condition: c2 }) => {
                i1 == i2 && c1 == c2
            }
            (PlanNode::Union { left: l1, right: r1, all: a1 }, PlanNode::Union { left: l2, right: r2, all: a2 }) => {
                l1 == l2 && r1 == r2 && a1 == a2
            }
            (PlanNode::Intersect { left: l1, right: r1 }, PlanNode::Intersect { left: l2, right: r2 }) => {
                l1 == l2 && r1 == r2
            }
            (PlanNode::Except { left: l1, right: r1 }, PlanNode::Except { left: l2, right: r2 }) => {
                l1 == l2 && r1 == r2
            }
            (PlanNode::SemiJoinFetch { build: b1, probe_source: p1, probe_database: d1, build_key: bk1, probe_key: pk1, join_kind: j1, condition: c1 },
             PlanNode::SemiJoinFetch { build: b2, probe_source: p2, probe_database: d2, build_key: bk2, probe_key: pk2, join_kind: j2, condition: c2 }) => {
                b1 == b2 && p1 == p2 && d1 == d2 && bk1 == bk2 && pk1 == pk2 && j1 == j2 && c1 == c2
            }
            (PlanNode::InlineData { columns: c1, rows: r1 }, PlanNode::InlineData { columns: c2, rows: r2 }) => {
                c1 == c2 && r1 == r2
            }
            (PlanNode::ListTables { database: d1 }, PlanNode::ListTables { database: d2 }) => d1 == d2,
            (PlanNode::DescribeTable { database: d1, table: t1, schema: s1 }, PlanNode::DescribeTable { database: d2, table: t2, schema: s2 }) => {
                d1 == d2 && t1 == t2 && s1 == s2
            }
            (PlanNode::Dml { database: d1, sql: s1 }, PlanNode::Dml { database: d2, sql: s2 }) => {
                d1 == d2 && s1 == s2
            }
            (PlanNode::CreateTable { database: d1, sql: s1 }, PlanNode::CreateTable { database: d2, sql: s2 }) => {
                d1 == d2 && s1 == s2
            }
            (PlanNode::CreateTableAs { query_plan: q1, database: d1, target_table: t1, target_schema: s1, on_conflict: o1 },
             PlanNode::CreateTableAs { query_plan: q2, database: d2, target_table: t2, target_schema: s2, on_conflict: o2 }) => {
                q1 == q2 && d1 == d2 && t1 == t2 && s1 == s2 && o1 == o2
            }
            (PlanNode::AlterTable { database: d1, sql: s1 }, PlanNode::AlterTable { database: d2, sql: s2 }) => {
                d1 == d2 && s1 == s2
            }
            (PlanNode::DropTable { database: d1, sql: s1 }, PlanNode::DropTable { database: d2, sql: s2 }) => {
                d1 == d2 && s1 == s2
            }
            (PlanNode::CreateDatabase { database: d1, sql: s1, is_mongo: im1, if_not_exists: ine1, db_name: dn1 },
             PlanNode::CreateDatabase { database: d2, sql: s2, is_mongo: im2, if_not_exists: ine2, db_name: dn2 }) => {
                d1 == d2 && s1 == s2 && im1 == im2 && ine1 == ine2 && dn1 == dn2
            }
            (PlanNode::DropDatabase { database: d1, sql: s1, is_mongo: im1, if_exists: ie1, db_name: dn1 },
             PlanNode::DropDatabase { database: d2, sql: s2, is_mongo: im2, if_exists: ie2, db_name: dn2 }) => {
                d1 == d2 && s1 == s2 && im1 == im2 && ie1 == ie2 && dn1 == dn2
            }
            (PlanNode::ConnectionError { message: m1 }, PlanNode::ConnectionError { message: m2 }) => m1 == m2,
            (PlanNode::AiProject { input: i1, ai_columns: a1, .. }, PlanNode::AiProject { input: i2, ai_columns: a2, .. }) => {
                i1 == i2 && a1 == a2
            }
            (PlanNode::Empty, PlanNode::Empty) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum JoinStrategy {
    NestedLoop,
    Hash,
    Auto,
}

#[derive(Debug, Clone)]
pub enum PlanNode {
    Scan {
        source: Source,
        database: Option<(String, DatabaseKind)>,
        filter: Option<Expression>,
    },
    Join {
        left: Box<PlanNode>,
        right: Box<PlanNode>,
        condition: Expression,
        strategy: JoinStrategy,
        join_kind: JoinKind,
        limit: Option<u64>,
    },
    Project {
        input: Box<PlanNode>,
        fields: Vec<Projection>,
    },
    Aggregate {
        input: Box<PlanNode>,
        group_by: Vec<Expression>,
        aggs: Vec<(Expression, Option<String>)>,
        having: Option<Expression>,
    },
    Limit {
        input: Box<PlanNode>,
        limit: u64,
        offset: u64,
    },
    Order {
        input: Box<PlanNode>,
        order_by: Vec<OrderBy>,
    },
    Distinct {
        input: Box<PlanNode>,
    },
    Filter {
        input: Box<PlanNode>,
        condition: Expression,
    },
    #[allow(dead_code)]
    Union {
        left: Box<PlanNode>,
        right: Box<PlanNode>,
        all: bool,
    },
    #[allow(dead_code)]
    Intersect {
        left: Box<PlanNode>,
        right: Box<PlanNode>,
    },
    #[allow(dead_code)]
    Except {
        left: Box<PlanNode>,
        right: Box<PlanNode>,
    },
    SemiJoinFetch {
        build: Box<PlanNode>,
        probe_source: Box<Source>,
        probe_database: (String, DatabaseKind),
        build_key: Expression,
        probe_key: Expression,
        join_kind: JoinKind,
        condition: Expression,
    },
    InlineData {
        columns: Vec<String>,
        rows: Vec<Vec<crate::adapters::Value>>,
    },
    ListTables {
        database: (String, DatabaseKind),
    },
    DescribeTable {
        database: (String, DatabaseKind),
        table: String,
        schema: Option<String>,
    },
    Dml {
        database: (String, DatabaseKind),
        sql: String,
    },
    CreateTable {
        database: (String, DatabaseKind),
        sql: String,
    },
    CreateTableAs {
        query_plan: Box<PlanNode>,
        database: (String, DatabaseKind),
        target_table: String,
        target_schema: Option<String>,
        on_conflict: Option<ConflictAction>,
    },
    AlterTable {
        database: (String, DatabaseKind),
        sql: String,
    },
    DropTable {
        database: (String, DatabaseKind),
        sql: String,
    },
    CreateDatabase {
        database: (String, DatabaseKind),
        sql: String,
        is_mongo: bool,
        if_not_exists: bool,
        db_name: String,
    },
    DropDatabase {
        database: (String, DatabaseKind),
        sql: String,
        is_mongo: bool,
        if_exists: bool,
        db_name: String,
    },
    ConnectionError {
        message: String,
    },
    AiProject {
        input: Box<PlanNode>,
        ai_columns: Vec<AiColumn>,
        ai_configs: Arc<HashMap<String, AiConfig>>,
    },
    Empty,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QueryPlan {
    pub root: PlanNode,
}

fn resolve_database(
    source: &Source,
    source_db: &[(String, DatabaseKind)],
) -> Option<(String, DatabaseKind)> {
    if let Some(conn_name) = &source.connection {
        source_db
            .iter()
            .find(|(name, _)| name == conn_name)
            .cloned()
    } else {
        source_db.first().cloned()
    }
}

fn plan_source(source: &Source, source_db: &[(String, DatabaseKind)], ai_configs: &HashMap<String, AiConfig>) -> PlanNode {
    if let SourceKind::Subquery(subquery) = &source.kind {
        let inner_plan = plan_query(subquery, source_db, ai_configs);
        return PlanNode::Project {
            input: Box::new(inner_plan.root),
            fields: vec![Projection::Wildcard],
        };
    }
    let database = if matches!(&source.kind, SourceKind::CteRef(_)) {
        None
    } else {
        resolve_database(source, source_db)
    };
    PlanNode::Scan {
        source: source.clone(),
        database,
        filter: None,
    }
}

fn extract_equi_keys(condition: &Expression) -> Option<(Expression, Expression)> {
    match condition {
        Expression::BinaryOp {
            op: BinaryOp::Eq,
            left,
            right,
        } => Some((*left.clone(), *right.clone())),
        _ => None,
    }
}

fn key_belongs_to_source(key: &Expression, source: &Source) -> bool {
    match key {
        Expression::QualifiedIdent { table, .. } => {
            source.alias.as_deref() == Some(table) || source.name == *table
        }
        Expression::Ident(_) => true,
        _ => false,
    }
}

fn find_single_db(node: &PlanNode) -> Option<(String, DatabaseKind)> {
    match node {
        PlanNode::Scan { database, .. } => database.clone(),
        PlanNode::Filter { input, .. }
        | PlanNode::Project { input, .. }
        | PlanNode::Order { input, .. }
        | PlanNode::Limit { input, .. }
        | PlanNode::Aggregate { input, .. }
        | PlanNode::Distinct { input, .. } => find_single_db(input),
        PlanNode::Join { left, right, .. } => {
            let l = find_single_db(left)?;
            let r = find_single_db(right)?;
            if l.0 == r.0 { Some(l) } else { None }
        }
        PlanNode::Union { left, right, .. }
        | PlanNode::Intersect { left, right, .. }
        | PlanNode::Except { left, right, .. } => {
            let l = find_single_db(left)?;
            let r = find_single_db(right)?;
            if l.0 == r.0 { Some(l) } else { None }
        }
        PlanNode::SemiJoinFetch { .. } => None,
        PlanNode::ListTables { database } => Some(database.clone()),
        PlanNode::DescribeTable { database, .. } => Some(database.clone()),
        PlanNode::Dml { database, .. } => Some(database.clone()),
        PlanNode::CreateTable { database, .. } => Some(database.clone()),
        PlanNode::CreateTableAs { database, .. } => Some(database.clone()),
        PlanNode::AlterTable { database, .. } => Some(database.clone()),
        PlanNode::DropTable { database, .. } => Some(database.clone()),
        PlanNode::CreateDatabase { database, .. } => Some(database.clone()),
        PlanNode::DropDatabase { database, .. } => Some(database.clone()),
        PlanNode::AiProject { input, .. } => find_single_db(input),
        PlanNode::InlineData { .. } | PlanNode::Empty | PlanNode::ConnectionError { .. } => None,
    }
}

fn any_ai_expr(expr: &Expression) -> bool {
    match expr {
        Expression::AiQuery { .. } => true,
        Expression::BinaryOp { left, right, .. } => any_ai_expr(left) || any_ai_expr(right),
        Expression::UnaryOp { expr, .. } => any_ai_expr(expr),
        Expression::FnCall { args, .. } => args.iter().any(any_ai_expr),
        Expression::Case { expr, whens, else_expr } => {
            expr.as_ref().is_some_and(|e| any_ai_expr(e))
                || whens.iter().any(|(w, t)| any_ai_expr(w) || any_ai_expr(t))
                || else_expr.as_ref().is_some_and(|e| any_ai_expr(e))
        }
        Expression::Cast { expr, .. } => any_ai_expr(expr),
        Expression::Between { expr, low, high } => {
            any_ai_expr(expr) || any_ai_expr(low) || any_ai_expr(high)
        }
        _ => false,
    }
}

fn split_ai_projections(
    projections: &[Projection],
    ai_configs: &HashMap<String, AiConfig>,
) -> Result<(Vec<Projection>, Vec<AiColumn>), String> {
    let mut regular = Vec::new();
    let mut ai_cols = Vec::new();

    for proj in projections {
        match proj {
            Projection::Expr(Expression::AiQuery { config, model, prompt }, alias) => {
                if !ai_configs.contains_key(config.as_str()) {
                    return Err(format!("AI config '{}' not found in river.yaml", config));
                }
                ai_cols.push(AiColumn {
                    expr: Expression::AiQuery {
                        config: config.clone(),
                        model: model.clone(),
                        prompt: prompt.clone(),
                    },
                    alias: alias.clone(),
                });
            }
            Projection::Expr(expr, alias) if any_ai_expr(expr) => {
                return Err("ai_query() is only supported in SELECT projections, not in WHERE or other clauses".to_string());
            }
            _ => {
                regular.push(proj.clone());
            }
        }
    }

    Ok((regular, ai_cols))
}

pub fn plan_query(
    query: &Query,
    source_db: &[(String, DatabaseKind)],
    ai_configs: &HashMap<String, AiConfig>,
) -> QueryPlan {
    let mut needs_default_cross_join_limit = false;

    let (regular_projections, ai_columns) = if !query.projection.is_empty() {
        match split_ai_projections(&query.projection, ai_configs) {
            Ok(result) => result,
            Err(msg) => {
                return QueryPlan {
                    root: PlanNode::ConnectionError { message: msg },
                };
            }
        }
    } else {
        (vec![], vec![])
    };

    let mut effective_query = query.clone();
    effective_query.projection = regular_projections;

    let mut current: PlanNode = if effective_query.sources.is_empty() {
        PlanNode::Empty
    } else {
        let mut root = plan_source(&effective_query.sources[0], source_db, ai_configs);

        for source in &effective_query.sources[1..] {
            if query.limit.is_none() {
                needs_default_cross_join_limit = true;
            }
            root = PlanNode::Join {
                left: Box::new(root),
                right: Box::new(plan_source(source, source_db, ai_configs)),
                condition: Expression::Boolean(true),
                strategy: JoinStrategy::Auto,
                join_kind: JoinKind::Cross,
                limit: query.limit,
            };
        }

        for join in &query.joins {
            let right_node = plan_source(&join.source, source_db, ai_configs);
            let left_db = find_single_db(&root);
            let right_db = match &right_node {
                PlanNode::Scan { database, .. } => database.clone(),
                _ => None,
            };

            let is_cross_db_join = match (&left_db, &right_db) {
                (Some((ln, _)), Some((rn, _))) => ln != rn,
                _ => false,
            };

            if is_cross_db_join {
                let condition = join.condition.clone().unwrap_or(Expression::Boolean(true));
                let is_cross_join = join.kind == JoinKind::Cross
                    || matches!(&condition, Expression::Boolean(true));
                let equi_keys = extract_equi_keys(&condition);

                if let Some((left_key, right_key)) = equi_keys {
                    let (probe_source, probe_database) = match &right_node {
                        PlanNode::Scan { source, database, .. } => {
                            (source.clone(), database.clone().unwrap())
                        }
                        _ => {
                            root = PlanNode::Join {
                                left: Box::new(root),
                                right: Box::new(right_node),
                                condition,
                                strategy: JoinStrategy::Hash,
                                join_kind: join.kind,
                                limit: None,
                            };
                            continue;
                        }
                    };

                    let (build_key, probe_key) =
                        if key_belongs_to_source(&right_key, &probe_source) {
                            (left_key, right_key)
                        } else {
                            (right_key, left_key)
                        };

                    root = PlanNode::SemiJoinFetch {
                        build: Box::new(root),
                        probe_source: Box::new(probe_source),
                        probe_database,
                        build_key,
                        probe_key,
                        join_kind: join.kind,
                        condition,
                    };
                } else if is_cross_join {
                    // Cross-DB cross join — executor will reject if unbounded
                    root = PlanNode::Join {
                        left: Box::new(root),
                        right: Box::new(right_node),
                        condition,
                        strategy: JoinStrategy::NestedLoop,
                        join_kind: join.kind,
                        limit: query.limit,
                    };
                } else {
                    root = PlanNode::Join {
                        left: Box::new(root),
                        right: Box::new(right_node),
                        condition,
                        strategy: JoinStrategy::NestedLoop,
                        join_kind: join.kind,
                        limit: None,
                    };
                }
            } else {
                if join.kind == JoinKind::Cross && query.limit.is_none() {
                    needs_default_cross_join_limit = true;
                }
                root = PlanNode::Join {
                    left: Box::new(root),
                    right: Box::new(right_node),
                    condition: join
                        .condition
                        .clone()
                        .unwrap_or(Expression::Boolean(true)),
                    strategy: match join.kind {
                        JoinKind::Cross => JoinStrategy::NestedLoop,
                        _ => JoinStrategy::Hash,
                    },
                    join_kind: join.kind,
                    limit: if join.kind == JoinKind::Cross { query.limit } else { None },
                };
            }
        }

        root
    };

    if let Some(filter) = &query.filter {
        current = PlanNode::Filter {
            input: Box::new(current),
            condition: filter.clone(),
        };
    }

    if !query.group_by.is_empty()
        || query
            .projection
            .iter()
            .any(|p| matches!(p, Projection::Expr(Expression::Aggregate { .. }, _)))
    {
        let aggs: Vec<(Expression, Option<String>)> = query
            .projection
            .iter()
            .filter_map(|p| match p {
                Projection::Expr(e, alias) if matches!(e, Expression::Aggregate { .. }) => {
                    Some((e.clone(), alias.clone()))
                }
                _ => None,
            })
            .collect();

        current = PlanNode::Aggregate {
            input: Box::new(current),
            group_by: query.group_by.clone(),
            aggs,
            having: query.having.clone(),
        };
    }

    if query.distinct {
        current = PlanNode::Distinct {
            input: Box::new(current),
        };
    }

    if !effective_query.projection.is_empty() {
        let fields = effective_query.projection.clone();

        if !fields.is_empty() {
            current = PlanNode::Project {
                input: Box::new(current),
                fields,
            };
        }
    }

    if !query.order_by.is_empty() {
        current = PlanNode::Order {
            input: Box::new(current),
            order_by: query.order_by.clone(),
        };
    }

    let effective_limit = if needs_default_cross_join_limit && query.limit.is_none() {
        Some(CROSS_DB_BATCH_SIZE as u64)
    } else {
        query.limit
    };

    if effective_limit.is_some() || query.offset.is_some() {
        current = PlanNode::Limit {
            input: Box::new(current),
            limit: effective_limit.unwrap_or(u64::MAX),
            offset: query.offset.unwrap_or(0),
        };
    }

    if !ai_columns.is_empty() {
        current = PlanNode::AiProject {
            input: Box::new(current),
            ai_columns,
            ai_configs: Arc::new(ai_configs.clone()),
        };
    }

    QueryPlan { root: current }
}

pub fn plan_statement(
    stmt: &Statement,
    source_db: &[(String, DatabaseKind)],
    ai_configs: &HashMap<String, AiConfig>,
) -> QueryPlan {
    match stmt {
        Statement::Query(q) => plan_query(q, source_db, ai_configs),
        Statement::ShowTables(conn) => {
            let db = resolve_connection(conn, source_db);
            match db {
                Some(database) => QueryPlan {
                    root: PlanNode::ListTables { database },
                },
                None => QueryPlan {
                    root: PlanNode::ConnectionError {
                        message: conn_name_error(conn),
                    },
                },
            }
        }
        Statement::Describe(desc) => {
            let db = resolve_connection(&desc.connection, source_db);
            match db {
                Some(database) => QueryPlan {
                    root: PlanNode::DescribeTable {
                        database,
                        table: desc.table.clone(),
                        schema: desc.schema.clone(),
                    },
                },
                None => QueryPlan {
                    root: PlanNode::ConnectionError {
                        message: conn_name_error(&desc.connection),
                    },
                },
            }
        }
        Statement::Insert(insert) => {
            let db = resolve_connection(&insert.connection, source_db);
            match db {
                Some((db_name, db_kind)) => {
                    let sql = if db_kind == DatabaseKind::MongoDB {
                        translate_insert_mongo(insert)
                    } else {
                        let dialect = dialect_for_kind(&db_kind);
                        crate::engine::translator::translate_statement_sql(stmt, &*dialect)
                    };
                    QueryPlan {
                        root: PlanNode::Dml {
                            database: (db_name, db_kind),
                            sql,
                        },
                    }
                }
                None => QueryPlan {
                    root: PlanNode::ConnectionError {
                        message: conn_name_error(&insert.connection),
                    },
                },
            }
        }
        Statement::Update(update) => {
            let db = resolve_connection(&update.connection, source_db);
            match db {
                Some((db_name, db_kind)) => {
                    let sql = if db_kind == DatabaseKind::MongoDB {
                        translate_update_mongo(update)
                    } else {
                        let dialect = dialect_for_kind(&db_kind);
                        crate::engine::translator::translate_statement_sql(stmt, &*dialect)
                    };
                    QueryPlan {
                        root: PlanNode::Dml {
                            database: (db_name, db_kind),
                            sql,
                        },
                    }
                }
                None => QueryPlan {
                    root: PlanNode::ConnectionError {
                        message: conn_name_error(&update.connection),
                    },
                },
            }
        }
        Statement::Delete(delete) => {
            let db = resolve_connection(&delete.connection, source_db);
            match db {
                Some((db_name, db_kind)) => {
                    let sql = if db_kind == DatabaseKind::MongoDB {
                        translate_delete_mongo(delete)
                    } else {
                        let dialect = dialect_for_kind(&db_kind);
                        crate::engine::translator::translate_statement_sql(stmt, &*dialect)
                    };
                    QueryPlan {
                        root: PlanNode::Dml {
                            database: (db_name, db_kind),
                            sql,
                        },
                    }
                }
                None => QueryPlan {
                    root: PlanNode::ConnectionError {
                        message: conn_name_error(&delete.connection),
                    },
                },
            }
        }
        Statement::CreateTable(ct) => {
            let db = resolve_connection(&ct.connection, source_db);
            match db {
                Some((db_name, db_kind)) => {
                    let sql = if db_kind == DatabaseKind::MongoDB {
                        format!(
                            r#"{{"database":"","collection":"{}","create":true}}"#,
                            ct.table
                        )
                    } else {
                        let dialect = dialect_for_kind(&db_kind);
                        crate::engine::translator::translate_create_table(ct, &*dialect)
                    };
                    QueryPlan {
                        root: PlanNode::CreateTable {
                            database: (db_name, db_kind),
                            sql,
                        },
                    }
                }
                None => QueryPlan {
                    root: PlanNode::ConnectionError {
                        message: conn_name_error(&ct.connection),
                    },
                },
            }
        }
        Statement::CreateTableAs(cta) => {
            let db = resolve_connection(&cta.connection, source_db);
            match db {
                Some((db_name, db_kind)) => {
                    let query_plan = plan_query(&cta.query, source_db, ai_configs);
                    QueryPlan {
                        root: PlanNode::CreateTableAs {
                            query_plan: Box::new(query_plan.root),
                            database: (db_name, db_kind),
                            target_table: cta.table.clone(),
                            target_schema: cta.schema.clone(),
                            on_conflict: cta.on_conflict.clone(),
                        },
                    }
                }
                None => QueryPlan {
                    root: PlanNode::ConnectionError {
                        message: conn_name_error(&cta.connection),
                    },
                },
            }
        }
        Statement::AlterTable(at) => {
            let db = resolve_connection(&at.connection, source_db);
            match db {
                Some((db_name, db_kind)) => {
                    if db_kind == DatabaseKind::MongoDB {
                        QueryPlan {
                            root: PlanNode::AlterTable {
                                database: (db_name, db_kind),
                                sql: String::new(),
                            },
                        }
                    } else {
                        let dialect = dialect_for_kind(&db_kind);
                        let sql = crate::engine::translator::translate_alter_table(at, &*dialect);
                        QueryPlan {
                            root: PlanNode::AlterTable {
                                database: (db_name, db_kind),
                                sql,
                            },
                        }
                    }
                }
                None => QueryPlan {
                    root: PlanNode::ConnectionError {
                        message: conn_name_error(&at.connection),
                    },
                },
            }
        }
        Statement::DropTable(dt) => {
            let db = resolve_connection(&dt.connection, source_db);
            match db {
                Some((db_name, db_kind)) => {
                    let sql = if db_kind == DatabaseKind::MongoDB {
                        format!(
                            r#"{{"database":"","collection":"{}","drop":true}}"#,
                            dt.table
                        )
                    } else {
                        let dialect = dialect_for_kind(&db_kind);
                        crate::engine::translator::translate_drop_table(dt, &*dialect)
                    };
                    QueryPlan {
                        root: PlanNode::DropTable {
                            database: (db_name, db_kind),
                            sql,
                        },
                    }
                }
                None => QueryPlan {
                    root: PlanNode::ConnectionError {
                        message: conn_name_error(&dt.connection),
                    },
                },
            }
        }
        Statement::CreateDatabase(cd) => {
            let db = resolve_connection(&cd.connection, source_db);
            match db {
                Some((db_name, db_kind)) => {
                    let (sql, is_mongo) = if db_kind == DatabaseKind::MongoDB {
                        (String::new(), true)
                    } else if db_kind == DatabaseKind::SQLite {
                        (String::new(), false)
                    } else {
                        let dialect = dialect_for_kind(&db_kind);
                        let sql = crate::engine::translator::translate_create_database(cd, &*dialect);
                        (sql, false)
                    };
                    QueryPlan {
                        root: PlanNode::CreateDatabase {
                            database: (db_name, db_kind),
                            sql,
                            is_mongo,
                            if_not_exists: cd.if_not_exists,
                            db_name: cd.name.clone(),
                        },
                    }
                }
                None => QueryPlan {
                    root: PlanNode::ConnectionError {
                        message: conn_name_error(&cd.connection),
                    },
                },
            }
        }
        Statement::DropDatabase(dd) => {
            let db = resolve_connection(&dd.connection, source_db);
            match db {
                Some((db_name, db_kind)) => {
                    let (sql, is_mongo) = if db_kind == DatabaseKind::MongoDB {
                        (format!(r#"{{"database":"{}","dropDatabase":1}}"#, dd.name), true)
                    } else if db_kind == DatabaseKind::SQLite {
                        (String::new(), false)
                    } else {
                        let dialect = dialect_for_kind(&db_kind);
                        let sql = crate::engine::translator::translate_drop_database(dd, &*dialect);
                        (sql, false)
                    };
                    QueryPlan {
                        root: PlanNode::DropDatabase {
                            database: (db_name, db_kind),
                            sql,
                            is_mongo,
                            if_exists: dd.if_exists,
                            db_name: dd.name.clone(),
                        },
                    }
                }
                None => QueryPlan {
                    root: PlanNode::ConnectionError {
                        message: conn_name_error(&dd.connection),
                    },
                },
            }
        }
        Statement::SetOp(setop) => {
            let left_plan = plan_query(&setop.left, source_db, ai_configs);
            let right_plan = plan_query(&setop.right, source_db, ai_configs);
            match setop.kind {
                SetOpKind::Union | SetOpKind::UnionAll => QueryPlan {
                    root: PlanNode::Union {
                        left: Box::new(left_plan.root),
                        right: Box::new(right_plan.root),
                        all: matches!(setop.kind, SetOpKind::UnionAll),
                    },
                },
                SetOpKind::Intersect => QueryPlan {
                    root: PlanNode::Intersect {
                        left: Box::new(left_plan.root),
                        right: Box::new(right_plan.root),
                    },
                },
                SetOpKind::Except => QueryPlan {
                    root: PlanNode::Except {
                        left: Box::new(left_plan.root),
                        right: Box::new(right_plan.root),
                    },
                },
            }
        }
        Statement::With(_)
        | Statement::Explain(_)
        | Statement::ParamAssign { .. }
        | Statement::Noop => QueryPlan {
            root: PlanNode::Empty,
        },
    }
}

fn resolve_connection(
    conn: &Option<String>,
    source_db: &[(String, DatabaseKind)],
) -> Option<(String, DatabaseKind)> {
    if let Some(name) = conn {
        source_db.iter().find(|(n, _)| n == name).cloned()
    } else {
        source_db.first().cloned()
    }
}

fn conn_name_error(conn: &Option<String>) -> String {
    match conn {
        Some(name) => format!("no connection configured with name '{}'", name),
        None => "no database connection configured — provide a connection name with `@connection` or add a connection to river.yaml".into(),
    }
}

fn expr_to_json_literal(expr: &Expression) -> String {
    match expr {
        Expression::String(s) => format!("\"{}\"", s.replace('"', "\\\"").replace('\n', "\\n")),
        Expression::Number(n) => n.to_string(),
        Expression::Integer(i) => i.to_string(),
        Expression::Boolean(true) => "true".to_string(),
        Expression::Boolean(false) => "false".to_string(),
        Expression::Null => "null".to_string(),
        Expression::Ident(name) => format!("\"{}\"", name),
        _ => "null".to_string(),
    }
}

pub fn translate_insert_mongo(insert: &Insert) -> String {
    let docs: Vec<String> = insert
        .rows
        .iter()
        .map(|row| {
            let fields: Vec<String> = row
                .iter()
                .map(|(col, expr)| format!("\"{}\": {}", col, expr_to_json_literal(expr)))
                .collect();
            format!("{{{}}}", fields.join(", "))
        })
        .collect();

    format!(
        r#"{{"database":"","collection":"{}","documents":[{}]}}"#,
        insert.table,
        docs.join(", ")
    )
}

fn translate_delete_mongo(delete: &Delete) -> String {
    let filter = match &delete.filter {
        Some(f) => expr_to_mongo_filter(f),
        None => "{}".to_string(),
    };
    format!(
        r#"{{"database":"","collection":"{}","delete":{}}}"#,
        delete.table,
        filter
    )
}

fn translate_update_mongo(update: &Update) -> String {
    let set_fields: Vec<String> = update
        .assignments
        .iter()
        .map(|(col, expr)| format!("\"{}\": {}", col, expr_to_json_literal(expr)))
        .collect();
    let set_obj = format!("{{{}}}", set_fields.join(", "));
    let filter = match &update.filter {
        Some(f) => expr_to_mongo_filter(f),
        None => "{}".to_string(),
    };
    format!(
        r#"{{"database":"","collection":"{}","filter":{},"update":{}}}"#,
        update.table,
        filter,
        set_obj
    )
}

fn expr_to_mongo_filter(expr: &Expression) -> String {
    match expr {
        Expression::BinaryOp { op: BinaryOp::Eq, left, right } => {
            let col = match left.as_ref() {
                Expression::Ident(name) => name.clone(),
                Expression::QualifiedIdent { field, .. } => field.clone(),
                _ => return "{}".to_string(),
            };
            // If right side is also an Ident, treat as match-all (e.g., "where col = col")
            match right.as_ref() {
                Expression::Ident(_) | Expression::QualifiedIdent { .. } => {
                    return "{}".to_string();
                }
                _ => {}
            }
            let val = expr_to_json_literal(right);
            format!(r#"{{"{}": {}}}"#, col, val)
        }
        Expression::BinaryOp { op: BinaryOp::Gt, left, right } => {
            let col = match left.as_ref() {
                Expression::Ident(name) => name.clone(),
                Expression::QualifiedIdent { field, .. } => field.clone(),
                _ => return "{}".to_string(),
            };
            let val = match right.as_ref() {
                Expression::Integer(n) => n.to_string(),
                _ => return "{}".to_string(),
            };
            format!(r#"{{"{}": {{"$gt": {}}}}}"#, col, val)
        }
        Expression::Boolean(true) => "{}".to_string(),
        _ => "{}".to_string(),
    }
}

fn dialect_for_kind(kind: &DatabaseKind) -> Box<dyn crate::engine::translator::SqlDialect> {
    crate::engine::translator::dialect_for(kind)
}

pub fn format_plan(node: &PlanNode) -> Vec<String> {
    let mut lines = Vec::new();
    format_node(node, &mut lines, String::new(), true);
    lines
}

fn format_node(node: &PlanNode, lines: &mut Vec<String>, prefix: String, is_last: bool) {
    let connector = if prefix.is_empty() {
        String::new()
    } else if is_last {
        format!("{}└─ ", prefix)
    } else {
        format!("{}├─ ", prefix)
    };
    let child_prefix = if prefix.is_empty() {
        String::from("  ")
    } else if is_last {
        format!("{}   ", prefix)
    } else {
        format!("{}│  ", prefix)
    };

    match node {
        PlanNode::Scan { source, database, filter } => {
            let db_str = match database {
                Some((name, kind)) => format!("{name} → {kind:?}"),
                None => "no database".to_string(),
            };
            let mut desc = match &source.kind {
                crate::lang::ast::SourceKind::Table(t) => {
                    if source.name != *t {
                        format!("Scan: {} ({db_str})", source.name)
                    } else {
                        format!("Scan: {t} ({db_str})")
                    }
                }
                crate::lang::ast::SourceKind::Subquery(_) => {
                    format!("Scan: <subquery> as {} ({db_str})", source.name)
                }
                crate::lang::ast::SourceKind::CteRef(n) => {
                    format!("Scan: CTE \"{n}\"")
                }
            };
            if let Some(alias) = &source.alias {
                let table_name = match &source.kind {
                    crate::lang::ast::SourceKind::Table(t) => t.as_str(),
                    _ => "",
                };
                if alias != table_name && alias != &source.name {
                    desc.push_str(&format!(" AS {alias}"));
                }
            }
            if let Some(f) = filter {
                desc.push_str(&format!(" [filter: {}]", expr_str(f)));
            }
            lines.push(format!("{connector}{desc}"));
        }
        PlanNode::Filter { input, condition } => {
            lines.push(format!("{connector}Filter: {}", expr_str(condition)));
            format_node(input, lines, child_prefix, true);
        }
        PlanNode::Project { input, fields } => {
            let proj_str: Vec<String> = fields
                .iter()
                .map(|p| match p {
                    crate::lang::ast::Projection::Wildcard => "*".to_string(),
                    crate::lang::ast::Projection::QualifiedWildcard(t) => format!("{t}.*"),
                    crate::lang::ast::Projection::Expr(e, alias) => {
                        let e_str = expr_str(e);
                        match alias {
                            Some(a) => format!("{e_str} AS {a}"),
                            None => e_str,
                        }
                    }
                })
                .collect();
            lines.push(format!("{connector}Project [{}]", proj_str.join(", ")));
            format_node(input, lines, child_prefix, true);
        }
        PlanNode::Order { input, order_by } => {
            let orders: Vec<String> = order_by
                .iter()
                .map(|o| {
                    let e = expr_str(&o.expr);
                    let dir = match o.direction {
                        crate::lang::ast::OrderDir::Asc => "ASC",
                        crate::lang::ast::OrderDir::Desc => "DESC",
                    };
                    format!("{e} {dir}")
                })
                .collect();
            lines.push(format!("{connector}Order: {}", orders.join(", ")));
            format_node(input, lines, child_prefix, true);
        }
        PlanNode::Limit { input, limit, offset } => {
            let mut desc = format!("Limit: {limit}");
            if *offset > 0 {
                desc.push_str(&format!(" offset {offset}"));
            }
            lines.push(format!("{connector}{desc}"));
            format_node(input, lines, child_prefix, true);
        }
        PlanNode::Aggregate { input, group_by, aggs, having } => {
            let groups: Vec<String> = group_by.iter().map(expr_str).collect();
            let agg_strs: Vec<String> = aggs
                .iter()
                .map(|(e, alias)| {
                    let s = expr_str(e);
                    match alias {
                        Some(a) => format!("{s} AS {a}"),
                        None => s,
                    }
                })
                .collect();
            let mut desc = format!("Aggregate [{}]", agg_strs.join(", "));
            if !groups.is_empty() {
                desc.push_str(&format!(" GROUP BY [{}]", groups.join(", ")));
            }
            if let Some(h) = having {
                desc.push_str(&format!(" HAVING {}", expr_str(h)));
            }
            lines.push(format!("{connector}{desc}"));
            format_node(input, lines, child_prefix, true);
        }
        PlanNode::Distinct { input } => {
            lines.push(format!("{connector}Distinct"));
            format_node(input, lines, child_prefix, true);
        }
        PlanNode::Join { left, right, condition, strategy, join_kind, limit } => {
            let kind_str = match join_kind {
                crate::lang::ast::JoinKind::Inner => "INNER JOIN",
                crate::lang::ast::JoinKind::Left => "LEFT JOIN",
                crate::lang::ast::JoinKind::Right => "RIGHT JOIN",
                crate::lang::ast::JoinKind::Full => "FULL JOIN",
                crate::lang::ast::JoinKind::Cross => "CROSS JOIN",
            };
            let mut desc = format!("{kind_str} (strategy: {strategy:?})");
            if !matches!(condition, crate::lang::ast::Expression::Boolean(true)) {
                desc.push_str(&format!(" ON {}", expr_str(condition)));
            }
            if let Some(lim) = limit {
                desc.push_str(&format!(" [limit: {lim}]"));
            }
            lines.push(format!("{connector}{desc}"));
            format_node(left, lines, child_prefix.clone(), false);
            format_node(right, lines, child_prefix, true);
        }
        PlanNode::Union { left, right, all } => {
            let kind = if *all { "UNION ALL" } else { "UNION" };
            lines.push(format!("{connector}{kind}"));
            format_node(left, lines, child_prefix.clone(), false);
            format_node(right, lines, child_prefix, true);
        }
        PlanNode::Intersect { left, right } => {
            lines.push(format!("{connector}INTERSECT"));
            format_node(left, lines, child_prefix.clone(), false);
            format_node(right, lines, child_prefix, true);
        }
        PlanNode::Except { left, right } => {
            lines.push(format!("{connector}EXCEPT"));
            format_node(left, lines, child_prefix.clone(), false);
            format_node(right, lines, child_prefix, true);
        }
        PlanNode::SemiJoinFetch {
            build,
            probe_source,
            probe_database,
            build_key,
            probe_key,
            join_kind,
            condition,
        } => {
            let kind_str = match join_kind {
                crate::lang::ast::JoinKind::Inner => "INNER JOIN",
                crate::lang::ast::JoinKind::Left => "LEFT JOIN",
                crate::lang::ast::JoinKind::Right => "RIGHT JOIN",
                crate::lang::ast::JoinKind::Full => "FULL JOIN",
                crate::lang::ast::JoinKind::Cross => "CROSS JOIN",
            };
            lines.push(format!(
                "{connector}SemiJoinFetch ({kind_str}) — probe {}.{} @ {}:{:?} ON {}",
                probe_source.name,
                expr_str(probe_key),
                probe_database.0,
                probe_database.1,
                expr_str(condition),
            ));
            lines.push(format!(
                "{child_prefix}build key: {}",
                expr_str(build_key)
            ));
            format_node(build, lines, child_prefix, true);
        }
        PlanNode::InlineData { columns, rows } => {
            lines.push(format!(
                "{connector}InlineData: {} cols × {} rows",
                columns.len(),
                rows.len()
            ));
        }
        PlanNode::ListTables { database } => {
            lines.push(format!(
                "{connector}ListTables @ {}:{:?}",
                database.0, database.1
            ));
        }
        PlanNode::DescribeTable { database, table, schema } => {
            let schema_prefix = schema
                .as_ref()
                .map(|s| format!("{}.", s))
                .unwrap_or_default();
            lines.push(format!(
                "{connector}DescribeTable \"{schema_prefix}{table}\" @ {}:{:?}",
                database.0, database.1
            ));
        }
        PlanNode::Dml { database, sql } => {
            lines.push(format!(
                "{connector}DML @ {}:{:?} — {sql}",
                database.0, database.1
            ));
        }
        PlanNode::CreateTable { database, sql } => {
            lines.push(format!(
                "{connector}CreateTable @ {}:{:?} — {sql}",
                database.0, database.1
            ));
        }
        PlanNode::CreateTableAs {
            query_plan,
            database,
            target_table,
            target_schema,
            on_conflict,
        } => {
            let schema_prefix = target_schema
                .as_ref()
                .map(|s| format!("{}.", s))
                .unwrap_or_default();
            let conflict_str = on_conflict
                .as_ref()
                .map(|c| format!(" ON CONFLICT {:?}", c))
                .unwrap_or_default();
            lines.push(format!(
                "{connector}CreateTableAs INTO {schema_prefix}{target_table} @ {}:{:?}{conflict_str}",
                database.0, database.1
            ));
            format_node(query_plan, lines, child_prefix, true);
        }
        PlanNode::AlterTable { database, sql } => {
            lines.push(format!(
                "{connector}AlterTable @ {}:{:?} — {sql}",
                database.0, database.1
            ));
        }
        PlanNode::DropTable { database, sql } => {
            lines.push(format!(
                "{connector}DropTable @ {}:{:?} — {sql}",
                database.0, database.1
            ));
        }
        PlanNode::CreateDatabase { database, sql, .. } => {
            lines.push(format!(
                "{connector}CreateDatabase @ {}:{:?} — {sql}",
                database.0, database.1
            ));
        }
        PlanNode::DropDatabase { database, sql, .. } => {
            lines.push(format!(
                "{connector}DropDatabase @ {}:{:?} — {sql}",
                database.0, database.1
            ));
        }
        PlanNode::Empty => {
            lines.push(format!("{connector}(empty plan)"));
        }
        PlanNode::AiProject { input, ai_columns, .. } => {
            let col_strs: Vec<String> = ai_columns
                .iter()
                .map(|c| {
                    let e = expr_str(&c.expr);
                    match &c.alias {
                        Some(a) => format!("{e} AS {a}"),
                        None => e,
                    }
                })
                .collect();
            lines.push(format!("{connector}AiProject [{}]", col_strs.join(", ")));
            format_node(input, lines, child_prefix, true);
        }
        PlanNode::ConnectionError { message } => {
            lines.push(format!("{connector}ERROR: {message}"));
        }
    }
}

fn expr_str(expr: &crate::lang::ast::Expression) -> String {
    match expr {
        crate::lang::ast::Expression::Ident(n) => n.clone(),
        crate::lang::ast::Expression::QualifiedIdent { table, field } => format!("{table}.{field}"),
        crate::lang::ast::Expression::String(s) => format!("\"{s}\""),
        crate::lang::ast::Expression::Number(n) => n.to_string(),
        crate::lang::ast::Expression::Integer(i) => i.to_string(),
        crate::lang::ast::Expression::Boolean(b) => b.to_string(),
        crate::lang::ast::Expression::Null => "NULL".to_string(),
        crate::lang::ast::Expression::BinaryOp { op, left, right } => {
            let op_str = match op {
                crate::lang::ast::BinaryOp::Eq => "=",
                crate::lang::ast::BinaryOp::Neq => "!=",
                crate::lang::ast::BinaryOp::Gt => ">",
                crate::lang::ast::BinaryOp::Gte => ">=",
                crate::lang::ast::BinaryOp::Lt => "<",
                crate::lang::ast::BinaryOp::Lte => "<=",
                crate::lang::ast::BinaryOp::And => "AND",
                crate::lang::ast::BinaryOp::Or => "OR",
                crate::lang::ast::BinaryOp::Add => "+",
                crate::lang::ast::BinaryOp::Sub => "-",
                crate::lang::ast::BinaryOp::Mul => "*",
                crate::lang::ast::BinaryOp::Div => "/",
                crate::lang::ast::BinaryOp::Mod => "%",
                crate::lang::ast::BinaryOp::Concat => "||",
                crate::lang::ast::BinaryOp::Like => "LIKE",
                crate::lang::ast::BinaryOp::ILike => "ILIKE",
                _ => "?",
            };
            format!("{} {} {}", expr_str(left), op_str, expr_str(right))
        }
        crate::lang::ast::Expression::UnaryOp { op, expr: e } => {
            let op_str = match op {
                crate::lang::ast::UnaryOp::Not => "NOT",
                crate::lang::ast::UnaryOp::Neg => "-",
            };
            format!("{op_str}({})", expr_str(e))
        }
        crate::lang::ast::Expression::FnCall { name, args } => {
            let args_str: Vec<String> = args.iter().map(expr_str).collect();
            format!("{name}({})", args_str.join(", "))
        }
        crate::lang::ast::Expression::Aggregate { name, distinct, args } => {
            let distinct_str = if *distinct { "DISTINCT " } else { "" };
            if args.is_empty() {
                format!("{name}({distinct_str}*)")
            } else {
                let args_str: Vec<String> = args.iter().map(expr_str).collect();
                format!("{name}({distinct_str}{})", args_str.join(", "))
            }
        }
        crate::lang::ast::Expression::Between { expr: e, low, high } => {
            format!(
                "{} BETWEEN {} AND {}",
                expr_str(e),
                expr_str(low),
                expr_str(high)
            )
        }
        crate::lang::ast::Expression::Cast { expr: e, target } => {
            format!("CAST({} AS {target:?})", expr_str(e))
        }
        crate::lang::ast::Expression::Exists(_q, is_exists) => {
            let prefix = if *is_exists { "EXISTS" } else { "NOT EXISTS" };
            format!("{prefix}(<subquery>)")
        }
        crate::lang::ast::Expression::Subquery(_) => "<subquery>".to_string(),
        crate::lang::ast::Expression::AiQuery { config, model, prompt } => {
            let model_str = model.as_ref().map(|m| format!(r#", "{}""#, m)).unwrap_or_default();
            format!(r#"ai_query("{config}"{model_str}, {})"#, expr_str(prompt))
        }
        crate::lang::ast::Expression::NamedParam(name) => format!("${name}"),
        _ => format!("{expr:?}"),
    }
}

/// Returns true if the plan involves databases of different kinds
pub fn is_cross_db(node: &PlanNode) -> bool {
    let dbs = find_all_databases(node);
    let kinds: std::collections::HashSet<&DatabaseKind> = dbs.iter().map(|(_, k)| k).collect();
    kinds.len() > 1
}

/// Collects all distinct databases referenced in a plan tree
pub fn find_all_databases(node: &PlanNode) -> Vec<(String, DatabaseKind)> {
    let mut dbs = Vec::new();
    collect_databases(node, &mut dbs);
    dbs.sort_by(|a, b| a.0.cmp(&b.0));
    dbs.dedup_by(|a, b| a.0 == b.0);
    dbs
}

fn collect_databases(node: &PlanNode, out: &mut Vec<(String, DatabaseKind)>) {
    match node {
        PlanNode::Scan { database, .. } => {
            if let Some(db) = database {
                out.push(db.clone());
            }
        }
        PlanNode::Filter { input, .. }
        | PlanNode::Project { input, .. }
        | PlanNode::Order { input, .. }
        | PlanNode::Limit { input, .. }
        | PlanNode::Aggregate { input, .. }
        | PlanNode::Distinct { input, .. } => {
            collect_databases(input, out);
        }
        PlanNode::Join { left, right, .. }
        | PlanNode::Union { left, right, .. }
        | PlanNode::Intersect { left, right, .. }
        | PlanNode::Except { left, right, .. } => {
            collect_databases(left, out);
            collect_databases(right, out);
        }
        PlanNode::SemiJoinFetch { build, probe_database, .. } => {
            collect_databases(build, out);
            out.push(probe_database.clone());
        }
        PlanNode::ListTables { database } => {
            out.push(database.clone());
        }
        PlanNode::DescribeTable { database, .. } => {
            out.push(database.clone());
        }
        PlanNode::Dml { database, .. } => {
            out.push(database.clone());
        }
        PlanNode::CreateTable { database, .. } => {
            out.push(database.clone());
        }
        PlanNode::CreateTableAs {
            query_plan, database, ..
        } => {
            collect_databases(query_plan, out);
            out.push(database.clone());
        }
        PlanNode::AlterTable { database, .. } => {
            out.push(database.clone());
        }
        PlanNode::DropTable { database, .. } => {
            out.push(database.clone());
        }
        PlanNode::CreateDatabase { database, .. } => {
            out.push(database.clone());
        }
        PlanNode::DropDatabase { database, .. } => {
            out.push(database.clone());
        }
        PlanNode::AiProject { input, .. } => {
            collect_databases(input, out);
        }
        PlanNode::InlineData { .. } | PlanNode::Empty | PlanNode::ConnectionError { .. } => {}
    }
}
