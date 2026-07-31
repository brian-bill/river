use std::collections::HashMap;
use std::time::Instant;

use crate::adapters::{DatabaseAdapter, QueryResult, Value};
use crate::ai::AiClient;
use crate::connection::{AiConfig, DatabaseKind};
use crate::engine::planner::{PlanNode, QueryPlan};
use crate::engine::translator::*;
use crate::lang::ast::*;
use crate::error::RiverError;

use crate::engine::expr::{eval_expr, eval_expr_bool, cmp_values};
use crate::engine::joins::{hash_join, join_results, resolve_col_idx};

pub async fn execute_plan(
    plan: &QueryPlan,
    adapters: &HashMap<String, Box<dyn DatabaseAdapter>>,
    ai_client: &AiClient,
) -> Result<QueryResult, RiverError> {
    let start = Instant::now();
    let has_limit = plan_has_limit(&plan.root);
    let mut result = execute_node(&plan.root, adapters, ai_client, has_limit).await?;
    result.elapsed = start.elapsed();
    Ok(result)
}

/// Returns true if a Limit node exists anywhere in the plan tree
fn plan_has_limit(node: &PlanNode) -> bool {
    match node {
        PlanNode::Limit { .. } => true,
        PlanNode::Filter { input, .. }
        | PlanNode::Project { input, .. }
        | PlanNode::Order { input, .. }
        | PlanNode::Aggregate { input, .. }
        | PlanNode::Distinct { input, .. } => plan_has_limit(input),
        PlanNode::Join { left, right, .. }
        | PlanNode::Union { left, right, .. }
        | PlanNode::Intersect { left, right, .. }
        | PlanNode::Except { left, right, .. } => plan_has_limit(left) || plan_has_limit(right),
        PlanNode::SemiJoinFetch { build, .. } => plan_has_limit(build),
        PlanNode::CreateTableAs { query_plan, .. } => plan_has_limit(query_plan),
        PlanNode::AiProject { input, .. } => plan_has_limit(input),
        PlanNode::Scan { .. } | PlanNode::InlineData { .. } | PlanNode::Empty | PlanNode::ListTables { .. } | PlanNode::DescribeTable { .. } | PlanNode::Dml { .. } | PlanNode::CreateTable { .. } | PlanNode::AlterTable { .. } | PlanNode::DropTable { .. } | PlanNode::CreateDatabase { .. } | PlanNode::DropDatabase { .. } | PlanNode::ConnectionError { .. } => false,
    }
}

pub fn resolve_params(expr: &Expression, params: &HashMap<String, Value>) -> Expression {
    match expr {
        Expression::NamedParam(name) => match params.get(name) {
            Some(v) => value_to_expr(v),
            None => expr.clone(),
        },
        Expression::String(_)
        | Expression::Number(_)
        | Expression::Integer(_)
        | Expression::Boolean(_)
        | Expression::Null
        | Expression::Ident(_)
        | Expression::QualifiedWildcard(_)
        | Expression::Interval { .. } => expr.clone(),
        Expression::Array(items) => Expression::Array(
            items.iter().map(|e| resolve_params(e, params)).collect(),
        ),
        Expression::Object(pairs) => Expression::Object(
            pairs
                .iter()
                .map(|(k, v)| (k.clone(), resolve_params(v, params)))
                .collect(),
        ),
        Expression::QualifiedIdent { table, field } => Expression::QualifiedIdent {
            table: table.clone(),
            field: field.clone(),
        },
        Expression::BinaryOp { op, left, right } => Expression::BinaryOp {
            op: op.clone(),
            left: Box::new(resolve_params(left, params)),
            right: Box::new(resolve_params(right, params)),
        },
        Expression::UnaryOp { op, expr: inner } => Expression::UnaryOp {
            op: op.clone(),
            expr: Box::new(resolve_params(inner, params)),
        },
        Expression::FnCall { name, args } => Expression::FnCall {
            name: name.clone(),
            args: args.iter().map(|a| resolve_params(a, params)).collect(),
        },
        Expression::Aggregate {
            name,
            distinct,
            args,
        } => Expression::Aggregate {
            name: name.clone(),
            distinct: *distinct,
            args: args.iter().map(|a| resolve_params(a, params)).collect(),
        },
        Expression::WindowFn {
            func,
            over,
            window_name,
        } => Expression::WindowFn {
            func: resolve_window_function(func, params),
            over: WindowSpec {
                partition_by: over
                    .partition_by
                    .iter()
                    .map(|e| resolve_params(e, params))
                    .collect(),
                order_by: over
                    .order_by
                    .iter()
                    .map(|o| OrderBy {
                        expr: resolve_params(&o.expr, params),
                        direction: o.direction.clone(),
                        nulls: o.nulls.clone(),
                    })
                    .collect(),
            },
            window_name: window_name.clone(),
        },
        Expression::Case {
            expr: case_expr,
            whens,
            else_expr,
        } => Expression::Case {
            expr: case_expr
                .as_ref()
                .map(|e| Box::new(resolve_params(e, params))),
            whens: whens
                .iter()
                .map(|(w, t)| (resolve_params(w, params), resolve_params(t, params)))
                .collect(),
            else_expr: else_expr
                .as_ref()
                .map(|e| Box::new(resolve_params(e, params))),
        },
        Expression::Between { expr: inner, low, high } => Expression::Between {
            expr: Box::new(resolve_params(inner, params)),
            low: Box::new(resolve_params(low, params)),
            high: Box::new(resolve_params(high, params)),
        },
        Expression::Subquery(q) => {
            Expression::Subquery(Box::new(resolve_params_in_query(q, params)))
        }
        Expression::Exists(q, is_exists) => {
            Expression::Exists(Box::new(resolve_params_in_query(q, params)), *is_exists)
        }
        Expression::QuantifiedCmp {
            op,
            left,
            quant,
            subquery,
        } => Expression::QuantifiedCmp {
            op: op.clone(),
            left: Box::new(resolve_params(left, params)),
            quant: quant.clone(),
            subquery: Box::new(resolve_params_in_query(subquery, params)),
        },
        Expression::Cast { expr: inner, target } => Expression::Cast {
            expr: Box::new(resolve_params(inner, params)),
            target: target.clone(),
        },
        Expression::AiQuery {
            config,
            model,
            prompt,
        } => Expression::AiQuery {
            config: config.clone(),
            model: model.clone(),
            prompt: Box::new(resolve_params(prompt, params)),
        },
    }
}

fn value_to_expr(v: &Value) -> Expression {
    match v {
        Value::Null => Expression::Null,
        Value::String(s) => Expression::String(s.clone()),
        Value::Int(i) => Expression::Integer(*i),
        Value::Float(f) => Expression::Number(*f),
        Value::Bool(b) => Expression::Boolean(*b),
    }
}

fn resolve_window_function(func: &WindowFunction, params: &HashMap<String, Value>) -> WindowFunction {
    match func {
        WindowFunction::RowNumber => WindowFunction::RowNumber,
        WindowFunction::Rank => WindowFunction::Rank,
        WindowFunction::DenseRank => WindowFunction::DenseRank,
        WindowFunction::Lag(e, def) => WindowFunction::Lag(
            Box::new(resolve_params(e, params)),
            *def,
        ),
        WindowFunction::Lead(e, def) => WindowFunction::Lead(
            Box::new(resolve_params(e, params)),
            *def,
        ),
        WindowFunction::FirstValue(e) => WindowFunction::FirstValue(Box::new(resolve_params(e, params))),
        WindowFunction::LastValue(e) => WindowFunction::LastValue(Box::new(resolve_params(e, params))),
        WindowFunction::NthValue(e, n) => WindowFunction::NthValue(Box::new(resolve_params(e, params)), *n),
        WindowFunction::Expr(e) => WindowFunction::Expr(Box::new(resolve_params(e, params))),
    }
}

fn resolve_params_in_query(query: &Query, params: &HashMap<String, Value>) -> Query {
    let mut q = query.clone();
    q.filter = q.filter.as_ref().map(|f| resolve_params(f, params));
    q.projection = q
        .projection
        .iter()
        .map(|p| match p {
            Projection::Wildcard => Projection::Wildcard,
            Projection::QualifiedWildcard(t) => Projection::QualifiedWildcard(t.clone()),
            Projection::Expr(e, alias) => {
                Projection::Expr(resolve_params(e, params), alias.clone())
            }
        })
        .collect();
    q.group_by = q
        .group_by
        .iter()
        .map(|e| resolve_params(e, params))
        .collect();
    q.having = q.having.as_ref().map(|h| resolve_params(h, params));
    q.order_by = q
        .order_by
        .iter()
        .map(|o| OrderBy {
            expr: resolve_params(&o.expr, params),
            direction: o.direction.clone(),
            nulls: o.nulls.clone(),
        })
        .collect();

    q.window_defs = q
        .window_defs
        .iter()
        .map(|d| WindowDef {
            name: d.name.clone(),
            spec: WindowSpec {
                partition_by: d
                    .spec
                    .partition_by
                    .iter()
                    .map(|e| resolve_params(e, params))
                    .collect(),
                order_by: d
                    .spec
                    .order_by
                    .iter()
                    .map(|o| OrderBy {
                        expr: resolve_params(&o.expr, params),
                        direction: o.direction.clone(),
                        nulls: o.nulls.clone(),
                    })
                    .collect(),
            },
        })
        .collect();

    q.sources = q
        .sources
        .iter()
        .map(|s| {
            let mut s2 = s.clone();
            s2.kind = match &s.kind {
                SourceKind::Subquery(subq) => {
                    SourceKind::Subquery(Box::new(resolve_params_in_query(subq, params)))
                }
                other => other.clone(),
            };
            s2
        })
        .collect();

    q.joins = q
        .joins
        .iter()
        .map(|j| {
            let mut j2 = j.clone();
            j2.condition = j2.condition.as_ref().map(|c| resolve_params(c, params));
            j2.source.kind = match &j.source.kind {
                SourceKind::Subquery(subq) => {
                    SourceKind::Subquery(Box::new(resolve_params_in_query(subq, params)))
                }
                other => other.clone(),
            };
            j2
        })
        .collect();

    q
}

pub fn resolve_params_in_statement(
    stmt: &Statement,
    params: &HashMap<String, Value>,
) -> Statement {
    match stmt {
        Statement::Query(q) => Statement::Query(resolve_params_in_query(q, params)),
        Statement::Insert(insert) => {
            let mut i = insert.clone();
            i.rows = i
                .rows
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|(col, val)| (col.clone(), resolve_params(val, params)))
                        .collect()
                })
                .collect();
            i.query = i.query.as_ref().map(|q| Box::new(resolve_params_in_query(q, params)));
            Statement::Insert(i)
        }
        Statement::Update(update) => {
            let mut u = update.clone();
            u.assignments = u
                .assignments
                .iter()
                .map(|(col, val)| (col.clone(), resolve_params(val, params)))
                .collect();
            u.filter = u.filter.as_ref().map(|f| resolve_params(f, params));
            Statement::Update(u)
        }
        Statement::Delete(delete) => {
            let mut d = delete.clone();
            d.filter = d.filter.as_ref().map(|f| resolve_params(f, params));
            Statement::Delete(d)
        }
        Statement::With(w) => {
            let mut new_ctes = Vec::new();
            for cte in &w.ctes {
                let mut new_cte = cte.clone();
                new_cte.query = Box::new(resolve_params_in_query(&cte.query, params));
                new_cte.chain = cte
                    .chain
                    .iter()
                    .map(|(kind, q)| (kind.clone(), resolve_params_in_query(q, params)))
                    .collect();
                new_ctes.push(new_cte);
            }
            Statement::With(With {
                recursive: w.recursive,
                ctes: new_ctes,
                body: Box::new(resolve_params_in_statement(w.body.as_ref(), params)),
            })
        }
        Statement::SetOp(s) => Statement::SetOp(SetOp {
            kind: s.kind.clone(),
            left: Box::new(resolve_params_in_query(s.left.as_ref(), params)),
            right: Box::new(resolve_params_in_query(s.right.as_ref(), params)),
        }),
        Statement::Explain(inner) => Statement::Explain(Box::new(resolve_params_in_statement(
            inner.as_ref(),
            params,
        ))),
        Statement::CreateTableAs(ctas) => {
            let mut c = ctas.clone();
            c.query = Box::new(resolve_params_in_query(ctas.query.as_ref(), params));
            Statement::CreateTableAs(c)
        }
        other => other.clone(),
    }
}

pub fn expression_to_value(expr: &Expression) -> Option<Value> {
    match expr {
        Expression::String(s) => Some(Value::String(s.clone())),
        Expression::Integer(i) => Some(Value::Int(*i)),
        Expression::Number(n) => Some(Value::Float(*n)),
        Expression::Boolean(b) => Some(Value::Bool(*b)),
        Expression::Null => Some(Value::Null),
        _ => None,
    }
}

pub async fn execute_statement(
    stmt: &Statement,
    source_db: &[(String, DatabaseKind)],
    adapters: &HashMap<String, Box<dyn DatabaseAdapter>>,
    ai_configs: &HashMap<String, AiConfig>,
    ai_client: &AiClient,
) -> Result<QueryResult, RiverError> {
    match stmt {
        Statement::With(w) => {
            let mut cte_data: HashMap<String, QueryResult> = HashMap::new();
            for cte in &w.ctes {
                let cte_stmt = Statement::Query(*cte.query.clone());
                let mut plan = crate::engine::planner::plan_statement(&cte_stmt, source_db, ai_configs);
                replace_cte_scans(&mut plan.root, &cte_data);
                let mut result = execute_plan(&plan, adapters, ai_client).await?;

                for (kind, q) in &cte.chain {
                    let chain_stmt = Statement::Query(q.clone());
                    let mut chain_plan = crate::engine::planner::plan_statement(&chain_stmt, source_db, ai_configs);
                    replace_cte_scans(&mut chain_plan.root, &cte_data);
                    let right = execute_plan(&chain_plan, adapters, ai_client).await?;
                    result = union_results(result, right, matches!(kind, SetOpKind::UnionAll))?;
                }

                cte_data.insert(cte.name.clone(), result);
            }
            let mut plan = crate::engine::planner::plan_statement(w.body.as_ref(), source_db, ai_configs);
            replace_cte_scans(&mut plan.root, &cte_data);
            execute_plan(&plan, adapters, ai_client).await
        }
        _ => {
            let plan = crate::engine::planner::plan_statement(stmt, source_db, ai_configs);
            execute_plan(&plan, adapters, ai_client).await
        }
    }
}

fn replace_cte_scans(node: &mut PlanNode, cte_data: &HashMap<String, QueryResult>) {
    match node {
        PlanNode::Scan { source, database, .. } if database.is_none() => {
            if let SourceKind::CteRef(name) = &source.kind
                && let Some(data) = cte_data.get(name) {
                    *node = PlanNode::InlineData {
                        columns: data.columns.clone(),
                        rows: data.rows.clone(),
                    };
                }
        }
        PlanNode::Filter { input, .. }
        | PlanNode::Project { input, .. }
        | PlanNode::Order { input, .. }
        | PlanNode::Limit { input, .. }
        | PlanNode::Aggregate { input, .. }
        | PlanNode::Distinct { input, .. } => {
            replace_cte_scans(input, cte_data);
        }
        PlanNode::Join { left, right, .. }
        | PlanNode::Union { left, right, .. }
        | PlanNode::Intersect { left, right, .. }
        | PlanNode::Except { left, right, .. } => {
            replace_cte_scans(left, cte_data);
            replace_cte_scans(right, cte_data);
        }
        PlanNode::SemiJoinFetch { build, .. } => {
            replace_cte_scans(build, cte_data);
        }
        PlanNode::AiProject { input, .. } => {
            replace_cte_scans(input, cte_data);
        }
        PlanNode::CreateTableAs { query_plan, .. } => {
            replace_cte_scans(query_plan, cte_data);
        }
        _ => {}
    }
}

fn build_scan_query(source: &Source) -> Query {
    let mut q = Query::default();
    q.sources.push(source.clone());
    q.projection = vec![Projection::Wildcard];
    q
}

async fn execute_on_db(
    db_name: &str,
    db_kind: &DatabaseKind,
    query: &Query,
    source_alias: Option<&str>,
    adapters: &HashMap<String, Box<dyn DatabaseAdapter>>,
) -> Result<QueryResult, RiverError> {
    let adapter = adapters.get(db_name).ok_or_else(|| {
        RiverError::Unsupported(format!("no adapter connected for '{}'", db_name))
    })?;
    let native = translate_for_kind(query, db_kind)?;
    let mut result = adapter.execute(&native).await?;
    if let Some(alias) = source_alias {
        result.column_sources = vec![Some(alias.to_string()); result.columns.len()];
    }
    Ok(result)
}

fn translate_for_kind(query: &Query, kind: &DatabaseKind) -> Result<String, RiverError> {
    match kind {
        DatabaseKind::Postgres => Ok(translate_query(query, &PostgresDialect)),
        DatabaseKind::MySQL => Ok(translate_query(query, &MySQLDialect)),
        DatabaseKind::MSSQL => Ok(translate_query(query, &MSSQLDialect)),
        DatabaseKind::SQLite => Ok(translate_query(query, &SQLiteDialect)),
        DatabaseKind::MongoDB => {
            serde_json::to_string(&translate_query_mongo(query, ""))
                .map_err(|e| RiverError::Unsupported(format!("failed to serialize MongoDB query: {}", e)))
        }
        DatabaseKind::AI => Err(RiverError::Unsupported(
            "AI connections cannot execute database queries".into(),
        )),
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

fn find_source_alias(node: &PlanNode) -> Option<String> {
    match node {
        PlanNode::Scan { source, .. } => source.alias.clone().or_else(|| Some(source.name.clone())),
        PlanNode::Filter { input, .. }
        | PlanNode::Project { input, .. }
        | PlanNode::Order { input, .. }
        | PlanNode::Limit { input, .. }
        | PlanNode::Distinct { input, .. } => find_source_alias(input),
        _ => None,
    }
}

fn collect_single_db_query(node: &PlanNode) -> Option<(String, DatabaseKind, Query)> {
    match node {
        PlanNode::Scan { source, database, .. } => {
            let (db_name, db_kind) = database.as_ref()?;
            Some((db_name.clone(), db_kind.clone(), build_scan_query(source)))
        }
        PlanNode::Filter { input, condition } => {
            let (db_name, db_kind, mut q) = collect_single_db_query(input)?;
            q.filter = Some(match q.filter.take() {
                Some(existing) => Expression::BinaryOp {
                    op: BinaryOp::And,
                    left: Box::new(existing),
                    right: Box::new(condition.clone()),
                },
                None => condition.clone(),
            });
            Some((db_name, db_kind, q))
        }
        PlanNode::Project { input, fields } => {
            let (db_name, db_kind, mut q) = collect_single_db_query(input)?;
            q.projection = fields.clone();
            Some((db_name, db_kind, q))
        }
        PlanNode::Order { input, order_by } => {
            let (db_name, db_kind, mut q) = collect_single_db_query(input)?;
            q.order_by = order_by.clone();
            Some((db_name, db_kind, q))
        }
        PlanNode::Limit { input, limit, offset } => {
            let (db_name, db_kind, mut q) = collect_single_db_query(input)?;
            q.limit = Some(*limit);
            q.offset = Some(*offset);
            Some((db_name, db_kind, q))
        }
        PlanNode::Aggregate { input, group_by, aggs, having } => {
            let (db_name, db_kind, mut q) = collect_single_db_query(input)?;
            q.group_by = group_by.clone();
            q.having = having.clone();
            for (agg, alias) in aggs {
                q.projection.push(Projection::Expr(agg.clone(), alias.clone()));
            }
            Some((db_name, db_kind, q))
        }
        PlanNode::Distinct { input } => {
            let (db_name, db_kind, mut q) = collect_single_db_query(input)?;
            q.distinct = true;
            Some((db_name, db_kind, q))
        }
        PlanNode::Join { left, right, condition, join_kind, .. } => {
            let (ldb_name, ldb_kind, mut q) = collect_single_db_query(left)?;
            let (right_source, right_db) = match right.as_ref() {
                PlanNode::Scan { source, database, .. } => {
                    (source.clone(), database.clone())
                }
                _ => return None,
            };
            let right_db = right_db?;
            if ldb_name != right_db.0 {
                return None;
            }
            q.joins.push(Join {
                kind: *join_kind,
                source: right_source,
                alias: None,
                condition: Some(condition.clone()),
            });
            Some((ldb_name, ldb_kind, q))
        }
        PlanNode::AiProject { .. } => None,
        PlanNode::Union { .. } | PlanNode::Intersect { .. } | PlanNode::Except { .. } | PlanNode::SemiJoinFetch { .. } | PlanNode::InlineData { .. } | PlanNode::Empty | PlanNode::ListTables { .. } | PlanNode::DescribeTable { .. } | PlanNode::Dml { .. } | PlanNode::CreateTable { .. } | PlanNode::CreateTableAs { .. } | PlanNode::AlterTable { .. } | PlanNode::DropTable { .. } | PlanNode::CreateDatabase { .. } | PlanNode::DropDatabase { .. } | PlanNode::ConnectionError { .. } => None,
    }
}

async fn execute_node(
    node: &PlanNode,
    adapters: &HashMap<String, Box<dyn DatabaseAdapter>>,
    ai_client: &AiClient,
    bounded: bool,
) -> Result<QueryResult, RiverError> {
    if let Some((db_name, db_kind, query)) = collect_single_db_query(node) {
        let source_alias = find_source_alias(node);
        return execute_on_db(&db_name, &db_kind, &query, source_alias.as_deref(), adapters).await;
    }

    match node {
        PlanNode::Join {
            left,
            right,
            condition,
            strategy,
            join_kind,
            limit,
        } => {
            // Guard: reject unbounded cross-DB cross joins (unless a LIMIT exists in the plan)
            if !bounded
                && (*join_kind == JoinKind::Cross
                    || matches!(condition, Expression::Boolean(true)))
            {
                let left_db = find_single_db(left);
                let right_db = find_single_db(right);
                if let (Some((ln, _)), Some((rn, _))) = (&left_db, &right_db)
                    && ln != rn {
                        return Err(RiverError::Unsupported(
                            "Cross-database cross joins require a LIMIT clause to prevent \
                             unbounded result sets. Add 'limit N' to your query."
                                .into(),
                        ));
                    }
            }

            // For cross-DB cross joins with a LIMIT, push the limit down to the
            // right-side database query to avoid materializing large tables in memory.
            let is_cross_db_join = {
                let ldb = find_single_db(left);
                let rdb = find_single_db(right);
                ldb.as_ref().zip(rdb.as_ref())
                    .is_some_and(|((ln, _), (rn, _))| ln != rn)
            };
            let right_result = if *join_kind == JoinKind::Cross
                && limit.is_some()
                && is_cross_db_join
            {
                if let Some((rdb_name, rdb_kind, mut rquery)) =
                    collect_single_db_query(right)
                {
                    rquery.limit = *limit;
                    rquery.offset = Some(0);
                    let right_alias = find_source_alias(right);
                    execute_on_db(&rdb_name, &rdb_kind, &rquery, right_alias.as_deref(), adapters).await?
                } else {
                    Box::pin(execute_node(right, adapters, ai_client, true)).await?
                }
            } else {
                Box::pin(execute_node(right, adapters, ai_client, bounded)).await?
            };

            let left_result = Box::pin(execute_node(left, adapters, ai_client, bounded)).await?;
            join_results(left_result, right_result, condition, strategy, *join_kind, *limit)
        }
        PlanNode::Filter { input, condition } => {
            let result = Box::pin(execute_node(input, adapters, ai_client, bounded)).await?;
            apply_filter(&result, condition)
        }
        PlanNode::Project { input, fields } => {
            let result = Box::pin(execute_node(input, adapters, ai_client, bounded)).await?;
            apply_projection(&result, fields)
        }
        PlanNode::Order { input, order_by } => {
            let result = Box::pin(execute_node(input, adapters, ai_client, bounded)).await?;
            apply_order(&result, order_by)
        }
        PlanNode::Limit { input, limit, offset } => {
            let result = Box::pin(execute_node(input, adapters, ai_client, true)).await?;
            apply_limit(&result, *limit, *offset)
        }
        PlanNode::Aggregate { input, group_by, aggs, having } => {
            let result = Box::pin(execute_node(input, adapters, ai_client, bounded)).await?;
            let mut agg_result = apply_aggregate(&result, group_by, aggs)?;
            if let Some(having_cond) = having {
                agg_result = apply_filter(&agg_result, having_cond)?;
            }
            Ok(agg_result)
        }
        PlanNode::Distinct { input } => {
            let result = Box::pin(execute_node(input, adapters, ai_client, bounded)).await?;
            apply_distinct(&result)
        }
        PlanNode::Union { left, right, all } => {
            let lf = Box::pin(execute_node(left, adapters, ai_client, bounded));
            let rf = Box::pin(execute_node(right, adapters, ai_client, bounded));
            let (lr, rr) = tokio::join!(lf, rf);
            union_results(lr?, rr?, *all)
        }
        PlanNode::Intersect { left, right } => {
            let lf = Box::pin(execute_node(left, adapters, ai_client, bounded));
            let rf = Box::pin(execute_node(right, adapters, ai_client, bounded));
            let (lr, rr) = tokio::join!(lf, rf);
            intersect_results(lr?, rr?)
        }
        PlanNode::Except { left, right } => {
            let lf = Box::pin(execute_node(left, adapters, ai_client, bounded));
            let rf = Box::pin(execute_node(right, adapters, ai_client, bounded));
            let (lr, rr) = tokio::join!(lf, rf);
            except_results(lr?, rr?)
        }
        PlanNode::InlineData { columns, rows } => Ok(QueryResult {
            columns: columns.clone(),
            column_sources: vec![None; columns.len()],
            rows: rows.clone(),
            elapsed: std::time::Duration::default(),
            rows_affected: rows.len() as u64,
        }),
        PlanNode::SemiJoinFetch {
            build,
            probe_source,
            probe_database,
            build_key,
            probe_key,
            join_kind,
            condition,
        } => {
            execute_semi_join_fetch(
                build, probe_source, probe_database, build_key, probe_key,
                *join_kind, condition, adapters, ai_client,
            ).await
        }
        PlanNode::ListTables { database } => {
            let adapter = adapters.get(&database.0).ok_or_else(|| {
                RiverError::Unsupported(format!("no adapter connected for '{}'", database.0))
            })?;
            let tables = adapter.list_tables(None).await?;
            let rows: Vec<Vec<Value>> = tables
                .into_iter()
                .map(|t| vec![Value::String(t.name)])
                .collect();
            Ok(QueryResult {
                columns: vec!["table_name".to_string()],
                column_sources: vec![None],
                rows,
                elapsed: std::time::Duration::default(),
                rows_affected: 0,
            })
        }
        PlanNode::DescribeTable { database, table, schema } => {
            let adapter = adapters.get(&database.0).ok_or_else(|| {
                RiverError::Unsupported(format!("no adapter connected for '{}'", database.0))
            })?;
            let table_schema = adapter.describe_table(table, schema.as_deref()).await?;
            let rows: Vec<Vec<Value>> = table_schema
                .columns
                .into_iter()
                .map(|c| {
                    vec![
                        Value::String(c.name),
                        Value::String(c.data_type),
                        Value::String(if c.nullable { "YES".to_string() } else { "NO".to_string() }),
                        Value::String(if c.is_primary_key { "PK".to_string() } else { String::new() }),
                    ]
                })
                .collect();
            Ok(QueryResult {
                columns: vec![
                    "column_name".to_string(),
                    "data_type".to_string(),
                    "nullable".to_string(),
                    "is_pk".to_string(),
                ],
                column_sources: vec![None; 4],
                rows,
                elapsed: std::time::Duration::default(),
                rows_affected: 0,
            })
        }
        PlanNode::Dml { database, sql } => {
            let adapter = adapters.get(&database.0).ok_or_else(|| {
                RiverError::Unsupported(format!("no adapter connected for '{}'", database.0))
            })?;
            adapter.execute(sql).await
        }
        PlanNode::CreateTable { database, sql } => {
            let adapter = adapters.get(&database.0).ok_or_else(|| {
                RiverError::Unsupported(format!("no adapter connected for '{}'", database.0))
            })?;
            adapter.execute(sql).await
        }
        PlanNode::CreateTableAs {
            query_plan,
            database,
            target_table,
            target_schema,
            on_conflict,
        } => {
            let query_result = Box::pin(execute_node(query_plan, adapters, ai_client, bounded)).await?;

            if query_result.rows.is_empty() {
                return Ok(QueryResult {
                    columns: vec![],
                    column_sources: vec![],
                    rows: vec![],
                    elapsed: std::time::Duration::default(),
                    rows_affected: 0,
                });
            }

            let adapter = adapters.get(&database.0).ok_or_else(|| {
                RiverError::Unsupported(format!("no adapter connected for '{}'", database.0))
            })?;

            if database.1 == DatabaseKind::MongoDB {
                let docs: Vec<serde_json::Value> = query_result
                    .rows
                    .iter()
                    .map(|row| {
                        let mut map = serde_json::Map::new();
                        for (i, col) in query_result.columns.iter().enumerate() {
                            let val = row.get(i).unwrap_or(&Value::Null);
                            let jv = match val {
                                Value::Null => serde_json::Value::Null,
                                Value::Int(n) => serde_json::json!(*n),
                                Value::Float(f) => serde_json::json!(*f),
                                Value::Bool(b) => serde_json::json!(*b),
                                Value::String(s) => serde_json::json!(s),
                            };
                            map.insert(col.clone(), jv);
                        }
                        serde_json::Value::Object(map)
                    })
                    .collect();

                let mut payload = serde_json::Map::new();
                payload.insert("database".into(), serde_json::json!(""));
                payload.insert("collection".into(), serde_json::json!(target_table.clone()));
                payload.insert("documents".into(), serde_json::json!(docs));
                if let Some(action) = on_conflict {
                    let action_str = match action {
                        ConflictAction::Ignore => "ignore",
                        ConflictAction::Replace => "replace",
                    };
                    payload.insert("on_conflict".into(), serde_json::json!(action_str));
                }
                let json = serde_json::to_string(&payload)
                    .map_err(|e| RiverError::Other(anyhow::anyhow!("failed to serialize insert payload: {}", e)))?;
                adapter.execute(&json).await
            } else {
                let column_types =
                    infer_types_from_rows(&query_result.rows, query_result.columns.len());
                let dialect: Box<dyn crate::engine::translator::SqlDialect> =
                    crate::engine::translator::dialect_for(&database.1);
                let table_name = crate::engine::translator::qualify_table(
                    target_table, target_schema.as_deref(), dialect.as_ref(),
                );

                let create_sql = build_create_table_if_not_exists(
                    &table_name, &query_result.columns, &column_types, dialect.as_ref(),
                );
                adapter.execute(&create_sql).await?;

                let insert_sql = build_insert_into(
                    &table_name, &query_result.columns, &query_result.rows,
                    on_conflict.as_ref(), dialect.as_ref(),
                );
                adapter.execute(&insert_sql).await
            }
        }
        PlanNode::Empty => Ok(empty_result()),
        PlanNode::AlterTable { database, sql } => {
            if database.1 == DatabaseKind::MongoDB {
                return Err(RiverError::Unsupported(
                    "ALTER TABLE is not supported on MongoDB".into(),
                ));
            }
            if sql.is_empty() {
                return Ok(QueryResult {
                    columns: vec![],
                    column_sources: vec![],
                    rows: vec![],
                    elapsed: std::time::Duration::default(),
                    rows_affected: 0,
                });
            }
            let adapter = adapters.get(&database.0).ok_or_else(|| {
                RiverError::Unsupported(format!("no adapter connected for '{}'", database.0))
            })?;

            let statements: Vec<&str> = sql
                .split(';')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();
            let mut total = 0u64;
            for stmt_sql in &statements {
                let result = adapter.execute(stmt_sql).await?;
                total += result.rows_affected.max(1);
            }
            Ok(QueryResult {
                columns: vec![],
                column_sources: vec![],
                rows: vec![],
                elapsed: std::time::Duration::default(),
                rows_affected: total,
            })
        }
        PlanNode::DropTable { database, sql } => {
            let adapter = adapters.get(&database.0).ok_or_else(|| {
                RiverError::Unsupported(format!("no adapter connected for '{}'", database.0))
            })?;

            if sql.is_empty() {
                return Ok(QueryResult {
                    columns: vec!["message".to_string()],
                    column_sources: vec![None],
                    rows: vec![vec![Value::String("no operation performed".to_string())]],
                    elapsed: std::time::Duration::default(),
                    rows_affected: 0,
                });
            }

            adapter.execute(sql).await
        }
        PlanNode::CreateDatabase { database, sql, is_mongo, if_not_exists, db_name } => {
            if *is_mongo {
                return Ok(QueryResult {
                    columns: vec!["message".to_string()],
                    column_sources: vec![None],
                    rows: vec![vec![Value::String(
                        "OK — MongoDB database will be created on first document insertion".to_string()
                    )]],
                    elapsed: std::time::Duration::default(),
                    rows_affected: 0,
                });
            }

            if database.1 == DatabaseKind::SQLite {
                return Err(RiverError::Unsupported(
                    "CREATE DATABASE is not supported for SQLite. Use a separate .db file instead.".into()
                ));
            }

            let adapter = adapters.get(&database.0).ok_or_else(|| {
                RiverError::Unsupported(format!("no adapter connected for '{}'", database.0))
            })?;

            if *if_not_exists && database.1 == DatabaseKind::Postgres {
                let check_sql = format!("SELECT 1 FROM pg_database WHERE datname = '{}'", db_name.replace('\'', "''"));
                let result = adapter.execute(&check_sql).await?;
                if !result.rows.is_empty() {
                    return Ok(QueryResult {
                        columns: vec![],
                        column_sources: vec![],
                        rows: vec![],
                        elapsed: std::time::Duration::default(),
                        rows_affected: 0,
                    });
                }
            }

            adapter.exec_maintenance(sql).await
        }
        PlanNode::DropDatabase { database, sql, is_mongo, if_exists: _, db_name: _ } => {
            if *is_mongo {
                let adapter = adapters.get(&database.0).ok_or_else(|| {
                    RiverError::Unsupported(format!("no adapter connected for '{}'", database.0))
                })?;
                return adapter.execute(sql).await;
            }

            if database.1 == DatabaseKind::SQLite {
                return Err(RiverError::Unsupported(
                    "DROP DATABASE is not supported for SQLite. Delete the .db file instead.".into()
                ));
            }

            let adapter = adapters.get(&database.0).ok_or_else(|| {
                RiverError::Unsupported(format!("no adapter connected for '{}'", database.0))
            })?;
            adapter.exec_maintenance(sql).await
        }
        PlanNode::AiProject { input, ai_columns, ai_configs } => {
            let result = Box::pin(execute_node(input, adapters, ai_client, bounded)).await?;

            let new_rows = crate::ai::client::execute_ai_columns(
                ai_client,
                ai_configs.clone(),
                ai_columns,
                &result.columns,
                &result.column_sources,
                &result.rows,
                eval_expr,
            ).await;

            let mut new_columns = result.columns.clone();
            for col in ai_columns {
                let name = col.alias.clone().unwrap_or_else(|| "ai_query".to_string());
                new_columns.push(name);
            }

            let new_sources = {
                let mut s = result.column_sources.clone();
                s.resize(new_columns.len(), None);
                s
            };

            Ok(QueryResult {
                columns: new_columns,
                column_sources: new_sources,
                rows: new_rows,
                elapsed: result.elapsed,
                rows_affected: result.rows_affected,
            })
        }
        PlanNode::ConnectionError { message } => Err(RiverError::Unsupported(message.clone())),
        PlanNode::Scan { source, .. } => {
            if let SourceKind::CteRef(_) = &source.kind {
                Err(RiverError::Unsupported(
                    format!("CTE \"{}\" was not resolved before execution", source.name),
                ))
            } else {
                Err(RiverError::Unsupported(
                    "no database configured — create a river.yaml file with connections".into(),
                ))
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn execute_semi_join_fetch(
    build: &PlanNode,
    probe_source: &Source,
    probe_database: &(String, DatabaseKind),
    build_key: &Expression,
    probe_key: &Expression,
    join_kind: JoinKind,
    condition: &Expression,
    adapters: &HashMap<String, Box<dyn DatabaseAdapter>>,
    ai_client: &AiClient,
) -> Result<QueryResult, RiverError> {
    use crate::engine::planner::CROSS_DB_BATCH_SIZE;

    // 1. Execute the build side
    let build_result = Box::pin(execute_node(build, adapters, ai_client, false)).await?;

    if build_result.rows.is_empty() {
        return Ok(empty_result());
    }

    // 2. Extract distinct join keys from build result
    let build_key_idx = resolve_col_idx(build_key, &build_result.columns, &build_result.column_sources)
        .ok_or_else(|| {
            RiverError::Unsupported(format!(
                "Build key column {:?} not found in build result columns: {:?}",
                build_key, build_result.columns
            ))
        })?;
    let probe_key_col = match probe_key {
        Expression::Ident(name) => name.clone(),
        Expression::QualifiedIdent { field, .. } => field.clone(),
        _ => {
            return Err(RiverError::Unsupported(
                "SemiJoinFetch requires a simple column reference as probe key".into(),
            ));
        }
    };

    let mut distinct_keys: Vec<Value> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for row in &build_result.rows {
        let key = row.get(build_key_idx).cloned().unwrap_or(Value::Null);
        if key != Value::Null && seen.insert(key.clone()) {
            distinct_keys.push(key);
        }
    }

    if distinct_keys.is_empty() {
        return Ok(empty_result());
    }

    // 3. Batch-fetch from probe side
    let (db_name, db_kind) = probe_database;
    let adapter = adapters.get(db_name).ok_or_else(|| {
        RiverError::Unsupported(format!("no adapter connected for '{}'", db_name))
    })?;

    let table_name = match &probe_source.kind {
        SourceKind::Table(t) => t.clone(),
        _ => probe_source.name.clone(),
    };
    let schema = probe_source.schema.as_deref();

    let mut probe_rows: Vec<Vec<Value>> = Vec::new();
    let mut probe_columns: Vec<String> = Vec::new();

    for chunk in distinct_keys.chunks(CROSS_DB_BATCH_SIZE) {
        let native_query = match db_kind {
            DatabaseKind::MongoDB => {
                let pipeline = crate::engine::translator::build_probe_query_mongo(
                    &table_name, &probe_key_col, chunk, "",
                );
                serde_json::to_string(&pipeline)
                    .map_err(|e| RiverError::Other(anyhow::anyhow!("failed to serialize MongoDB probe query: {}", e)))?
            }
            _ => {
                let dialect: Box<dyn crate::engine::translator::SqlDialect> = match db_kind {
                    DatabaseKind::Postgres => Box::new(crate::engine::translator::PostgresDialect),
                    DatabaseKind::MySQL => Box::new(crate::engine::translator::MySQLDialect),
                    DatabaseKind::MSSQL => Box::new(crate::engine::translator::MSSQLDialect),
                    DatabaseKind::SQLite => Box::new(crate::engine::translator::SQLiteDialect),
                    DatabaseKind::MongoDB => return Err(RiverError::Unsupported("MongoDB semi-join not supported in SQL probe path".into())),
                    DatabaseKind::AI => return Err(RiverError::Unsupported("AI connections cannot be used in semi-join".into())),
                };
                crate::engine::translator::build_probe_query_sql(
                    &table_name, schema, &probe_key_col, chunk, dialect.as_ref(),
                )
            }
        };

        let batch_result = adapter.execute(&native_query).await?;
        if probe_columns.is_empty() {
            probe_columns = batch_result.columns.clone();
        }
        probe_rows.extend(batch_result.rows);
    }

    let probe_result = QueryResult {
        columns: probe_columns.clone(),
        column_sources: vec![Some(probe_source.alias.clone().unwrap_or_else(|| probe_source.name.clone())); probe_columns.len()],
        rows: probe_rows,
        elapsed: std::time::Duration::default(),
        rows_affected: 0,
    };

    // 4. Hash join the build and probe results
    hash_join(build_result, probe_result, condition, join_kind)
}

fn empty_result() -> QueryResult {
    QueryResult {
        columns: vec![],
        column_sources: vec![],
        rows: vec![],
        elapsed: std::time::Duration::default(),
        rows_affected: 0,
    }
}

// ── In-memory operators ────────────────────────────────────────────────────

fn apply_filter(result: &QueryResult, condition: &Expression) -> Result<QueryResult, RiverError> {
    let rows: Vec<Vec<Value>> = result
        .rows
        .iter()
        .filter(|row| eval_expr_bool(condition, &result.columns, &result.column_sources, row))
        .cloned()
        .collect();
    Ok(QueryResult {
        columns: result.columns.clone(),
        column_sources: result.column_sources.clone(),
        rows,
        elapsed: std::time::Duration::default(),
        rows_affected: 0,
    })
}

fn apply_projection(
    result: &QueryResult,
    fields: &[Projection],
) -> Result<QueryResult, RiverError> {
    if fields.is_empty() {
        return Ok(result.clone());
    }
    let mut new_cols: Vec<String> = Vec::new();
    let mut new_sources: Vec<Option<String>> = Vec::new();
    let mut indices: Vec<Option<usize>> = Vec::new();
    for p in fields {
        match p {
            Projection::Wildcard => {
                new_cols.push("*".to_string());
                new_sources.push(None);
                indices.push(None);
            }
            Projection::QualifiedWildcard(q) => {
                new_cols.push("*".to_string());
                new_sources.push(Some(q.clone()));
                indices.push(None);
            }
            Projection::Expr(expr, alias) => {
                let name = alias.clone().unwrap_or_else(|| match expr {
                    Expression::Ident(n) => n.clone(),
                    Expression::QualifiedIdent { field, .. } => field.clone(),
                    Expression::Aggregate { .. } => agg_default_name(expr),
                    _ => "expr".to_string(),
                });
                let (idx, src) = match expr {
                    Expression::Ident(n) => {
                        let i = result.columns.iter().position(|c| c == n);
                        let s = i.and_then(|pos| result.column_sources.get(pos).cloned()).flatten();
                        (i, s)
                    }
                    Expression::QualifiedIdent { table, field } => {
                        let i = result.columns.iter()
                            .enumerate()
                            .position(|(pos, c)| {
                                c == field && result.column_sources.get(pos)
                                    .and_then(|s| s.as_deref()) == Some(table.as_str())
                            })
                            .or_else(|| result.columns.iter().position(|c| c == field));
                        let s = i.and_then(|pos| result.column_sources.get(pos).cloned()).flatten();
                        (i, s)
                    }
                    Expression::Aggregate { .. } => {
                        let i = result.columns.iter().position(|c| c == &name);
                        (i, None)
                    }
                    _ => (None, None),
                };
                new_cols.push(name);
                new_sources.push(src);
                indices.push(idx);
            }
        }
    }

    let has_wildcard = fields.iter().any(|p| matches!(p, Projection::Wildcard));
    if has_wildcard {
        return Ok(result.clone());
    }

    let rows: Vec<Vec<Value>> = result
        .rows
        .iter()
        .map(|row| {
            indices
                .iter()
                .map(|opt_idx| match opt_idx {
                    Some(idx) => row.get(*idx).cloned().unwrap_or(Value::Null),
                    None => Value::Null,
                })
                .collect()
        })
        .collect();

    Ok(QueryResult {
        columns: new_cols,
        column_sources: new_sources,
        rows,
        elapsed: std::time::Duration::default(),
        rows_affected: 0,
    })
}

fn apply_order(
    result: &QueryResult,
    order_by: &[OrderBy],
) -> Result<QueryResult, RiverError> {
    let mut rows = result.rows.clone();
    rows.sort_by(|a, b| {
        for order in order_by {
            let va = eval_expr(&order.expr, &result.columns, &result.column_sources, a);
            let vb = eval_expr(&order.expr, &result.columns, &result.column_sources, b);
            let cmp = cmp_values(&va, &vb);
            if cmp != std::cmp::Ordering::Equal {
                return if order.direction == OrderDir::Desc {
                    cmp.reverse()
                } else {
                    cmp
                };
            }
        }
        std::cmp::Ordering::Equal
    });
    Ok(QueryResult {
        columns: result.columns.clone(),
        column_sources: result.column_sources.clone(),
        rows,
        elapsed: std::time::Duration::default(),
        rows_affected: 0,
    })
}

fn apply_limit(
    result: &QueryResult,
    limit: u64,
    offset: u64,
) -> Result<QueryResult, RiverError> {
    let rows: Vec<Vec<Value>> = result
        .rows
        .iter()
        .skip(offset as usize)
        .take(limit as usize)
        .cloned()
        .collect();
    Ok(QueryResult {
        columns: result.columns.clone(),
        column_sources: result.column_sources.clone(),
        rows,
        elapsed: std::time::Duration::default(),
        rows_affected: 0,
    })
}

fn apply_aggregate(
    result: &QueryResult,
    group_by: &[Expression],
    aggs: &[(Expression, Option<String>)],
) -> Result<QueryResult, RiverError> {
    if aggs.is_empty() && group_by.is_empty() {
        return Ok(result.clone());
    }

    let mut out_columns: Vec<String> = Vec::new();
    let mut out_sources: Vec<Option<String>> = Vec::new();
    for gb in group_by {
        let name = match gb {
            Expression::Ident(n) => n.clone(),
            Expression::QualifiedIdent { field, .. } => field.clone(),
            _ => "expr".to_string(),
        };
        let src = resolve_col_idx(gb, &result.columns, &result.column_sources)
            .and_then(|i| result.column_sources.get(i).cloned())
            .flatten();
        out_columns.push(name);
        out_sources.push(src);
    }
    for (agg_expr, alias) in aggs {
        let name = alias.clone().unwrap_or_else(|| agg_default_name(agg_expr));
        out_columns.push(name);
        out_sources.push(None);
    }

    let mut groups: HashMap<Vec<Value>, Vec<Vec<Value>>> = HashMap::new();
    let mut group_order: Vec<Vec<Value>> = Vec::new();

    for row in &result.rows {
        let key: Vec<Value> = group_by
            .iter()
            .map(|gb| eval_expr(gb, &result.columns, &result.column_sources, row))
            .collect();
        if !groups.contains_key(&key) {
            group_order.push(key.clone());
        }
        groups.entry(key).or_default().push(row.clone());
    }

    let mut out_rows: Vec<Vec<Value>> = Vec::new();
    for key in &group_order {
        let group_rows = groups.get(key).unwrap();
        let mut out_row: Vec<Value> = key.clone();
        for (agg_expr, _) in aggs {
            out_row.push(compute_aggregate(agg_expr, &result.columns, &result.column_sources, group_rows));
        }
        out_rows.push(out_row);
    }

    if group_by.is_empty() && !aggs.is_empty() && out_rows.is_empty() {
        let mut out_row: Vec<Value> = Vec::new();
        for (agg_expr, _) in aggs {
            out_row.push(compute_aggregate(agg_expr, &result.columns, &result.column_sources, &[]));
        }
        out_rows.push(out_row);
    }

    Ok(QueryResult {
        columns: out_columns,
        column_sources: out_sources,
        rows: out_rows,
        elapsed: std::time::Duration::default(),
        rows_affected: 0,
    })
}

fn agg_default_name(expr: &Expression) -> String {
    match expr {
        Expression::Aggregate { name, args, .. } => {
            if args.is_empty() {
                format!("{}(*)", name)
            } else if let Expression::Ident(col) = &args[0] {
                format!("{}({})", name, col)
            } else {
                format!("{}(...)", name)
            }
        }
        _ => "expr".to_string(),
    }
}

fn compute_aggregate(expr: &Expression, columns: &[String], column_sources: &[Option<String>], rows: &[Vec<Value>]) -> Value {
    match expr {
        Expression::Aggregate {
            name,
            distinct,
            args,
        } => {
            let arg_values: Vec<Value> = if args.is_empty() {
                rows.iter().map(|_| Value::Int(1)).collect()
            } else {
                rows.iter()
                    .map(|r| eval_expr(&args[0], columns, column_sources, r))
                    .collect()
            };

            match name.as_str() {
                "count" => {
                    if *distinct {
                        let mut seen = std::collections::HashSet::new();
                        for v in &arg_values {
                            seen.insert(v.clone());
                        }
                        Value::Int(seen.len() as i64)
                    } else {
                        Value::Int(arg_values.len() as i64)
                    }
                }
                "count_distinct" => {
                    let mut seen = std::collections::HashSet::new();
                    for v in &arg_values {
                        seen.insert(v.clone());
                    }
                    Value::Int(seen.len() as i64)
                }
                "sum" => {
                    let vals: Vec<f64> = arg_values.iter().filter_map(to_f64).collect();
                    if vals.is_empty() {
                        Value::Null
                    } else if arg_values.iter().all(|v| matches!(v, Value::Int(_))) {
                        Value::Int(vals.iter().sum::<f64>() as i64)
                    } else {
                        Value::Float(vals.iter().sum())
                    }
                }
                "avg" => {
                    let vals: Vec<f64> = arg_values.iter().filter_map(to_f64).collect();
                    if vals.is_empty() {
                        Value::Null
                    } else {
                        Value::Float(vals.iter().sum::<f64>() / vals.len() as f64)
                    }
                }
                "min" => arg_values
                    .iter()
                    .min_by(|a, b| cmp_values(a, b))
                    .cloned()
                    .unwrap_or(Value::Null),
                "max" => arg_values
                    .iter()
                    .max_by(|a, b| cmp_values(a, b))
                    .cloned()
                    .unwrap_or(Value::Null),
                _ => Value::Null,
            }
        }
        _ => Value::Null,
    }
}

fn to_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Int(i) => Some(*i as f64),
        Value::Float(f) => Some(*f),
        _ => None,
    }
}

fn apply_distinct(result: &QueryResult) -> Result<QueryResult, RiverError> {
    let mut seen = std::collections::HashSet::new();
    let rows: Vec<Vec<Value>> = result
        .rows
        .iter()
        .filter(|&row| seen.insert(row.clone()))
        .cloned()
        .collect();
    Ok(QueryResult {
        columns: result.columns.clone(),
        column_sources: result.column_sources.clone(),
        rows,
        elapsed: std::time::Duration::default(),
        rows_affected: 0,
    })
}

fn union_results(
    left: QueryResult,
    right: QueryResult,
    all: bool,
) -> Result<QueryResult, RiverError> {
    let columns = if left.columns.len() >= right.columns.len() {
        left.columns.clone()
    } else {
        right.columns.clone()
    };
    let column_sources = if left.columns.len() >= right.columns.len() {
        left.column_sources.clone()
    } else {
        right.column_sources.clone()
    };
    let mut rows: Vec<Vec<Value>> = left
        .rows
        .into_iter()
        .map(|mut row| {
            row.resize(columns.len(), Value::Null);
            row
        })
        .collect();
    for r_row in &right.rows {
        let mut aligned = r_row.clone();
        aligned.resize(columns.len(), Value::Null);
        rows.push(aligned);
    }
    if !all {
        let mut seen = std::collections::HashSet::new();
        let rows: Vec<Vec<Value>> = rows
            .into_iter()
            .filter(|row| seen.insert(row.clone()))
            .collect();
        Ok(QueryResult {
            columns,
            column_sources,
            rows,
            elapsed: std::time::Duration::default(),
            rows_affected: 0,
        })
    } else {
        Ok(QueryResult {
            columns,
            column_sources,
            rows,
            elapsed: std::time::Duration::default(),
            rows_affected: 0,
        })
    }
}

fn intersect_results(
    left: QueryResult,
    right: QueryResult,
) -> Result<QueryResult, RiverError> {
    let columns = if left.columns.len() >= right.columns.len() {
        left.columns.clone()
    } else {
        right.columns.clone()
    };
    let column_sources = if left.columns.len() >= right.columns.len() {
        left.column_sources.clone()
    } else {
        right.column_sources.clone()
    };
    let right_set: std::collections::HashSet<Vec<Value>> = right
        .rows
        .into_iter()
        .map(|mut row| {
            row.resize(columns.len(), Value::Null);
            row
        })
        .collect();
    let mut seen = std::collections::HashSet::new();
    let rows: Vec<Vec<Value>> = left
        .rows
        .into_iter()
        .filter_map(|mut row| {
            row.resize(columns.len(), Value::Null);
            if right_set.contains(row.as_slice()) && seen.insert(row.clone()) {
                Some(row)
            } else {
                None
            }
        })
        .collect();
    Ok(QueryResult {
        columns,
        column_sources,
        rows,
        elapsed: std::time::Duration::default(),
        rows_affected: 0,
    })
}

fn except_results(
    left: QueryResult,
    right: QueryResult,
) -> Result<QueryResult, RiverError> {
    let columns = if left.columns.len() >= right.columns.len() {
        left.columns.clone()
    } else {
        right.columns.clone()
    };
    let column_sources = if left.columns.len() >= right.columns.len() {
        left.column_sources.clone()
    } else {
        right.column_sources.clone()
    };
    let right_set: std::collections::HashSet<Vec<Value>> = right
        .rows
        .into_iter()
        .map(|mut row| {
            row.resize(columns.len(), Value::Null);
            row
        })
        .collect();
    let mut seen = std::collections::HashSet::new();
    let rows: Vec<Vec<Value>> = left
        .rows
        .into_iter()
        .filter_map(|mut row| {
            row.resize(columns.len(), Value::Null);
            if !right_set.contains(row.as_slice()) && seen.insert(row.clone()) {
                Some(row)
            } else {
                None
            }
        })
        .collect();
    Ok(QueryResult {
        columns,
        column_sources,
        rows,
        elapsed: std::time::Duration::default(),
        rows_affected: 0,
    })
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn infer_types_from_rows(rows: &[Vec<Value>], num_cols: usize) -> Vec<DataType> {
    let mut types: Vec<Option<DataType>> = vec![None; num_cols];
    for row in rows {
        for (i, val) in row.iter().enumerate() {
            let inferred = match val {
                Value::Null => continue,
                Value::Int(_) => DataType::Integer,
                Value::Float(_) => DataType::Float,
                Value::Bool(_) => DataType::Boolean,
                Value::String(_) => DataType::String,
            };
            types[i] = Some(match &types[i] {
                Some(current) => widen_type(current, &inferred),
                None => inferred,
            });
        }
    }
    types.into_iter().map(|t| t.unwrap_or(DataType::String)).collect()
}

fn widen_type(current: &DataType, new: &DataType) -> DataType {
    fn rank(dt: &DataType) -> i32 {
        match dt {
            DataType::Boolean => 0,
            DataType::Integer => 1,
            DataType::Float => 2,
            DataType::String => 3,
            DataType::DateTime => 4,
            DataType::Json => 5,
        }
    }
    if rank(new) > rank(current) { new.clone() } else { current.clone() }
}

fn build_create_table_if_not_exists(
    table: &str, columns: &[String], types: &[DataType], dialect: &dyn crate::engine::translator::SqlDialect,
) -> String {
    let cols: Vec<String> = columns.iter().enumerate().map(|(i, col)| {
        let dt = types.get(i).unwrap_or(&DataType::String);
        format!("{} {}", dialect.quote_ident(col), translate_data_type(dt, dialect))
    }).collect();
    format!("CREATE TABLE IF NOT EXISTS {} ({})", table, cols.join(", "))
}

fn build_insert_into(
    table: &str, columns: &[String], rows: &[Vec<Value>],
    on_conflict: Option<&ConflictAction>, dialect: &dyn crate::engine::translator::SqlDialect,
) -> String {
    let cols = columns.iter().map(|c| dialect.quote_ident(c)).collect::<Vec<_>>().join(", ");
    let values = rows.iter().map(|row| {
        let vals: Vec<String> = row.iter().map(|v| value_to_sql_literal(v, dialect)).collect();
        format!("({})", vals.join(", "))
    }).collect::<Vec<_>>().join(", ");

    let base = format!("INSERT INTO {} ({}) VALUES {}", table, cols, values);

    match on_conflict {
        Some(ConflictAction::Ignore) => {
            format!("{} ON CONFLICT DO NOTHING", base)
        }
        Some(ConflictAction::Replace) => {
            format!(
                "{} ON CONFLICT ({}) DO UPDATE SET {}",
                base,
                columns.iter().map(|c| dialect.quote_ident(c)).collect::<Vec<_>>().join(", "),
                columns.iter().map(|c| format!("{} = EXCLUDED.{}", dialect.quote_ident(c), dialect.quote_ident(c))).collect::<Vec<_>>().join(", ")
            )
        }
        None => base,
    }
}

fn value_to_sql_literal(v: &Value, _dialect: &dyn crate::engine::translator::SqlDialect) -> String {
    match v {
        Value::Null => "NULL".to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(true) => "TRUE".to_string(),
        Value::Bool(false) => "FALSE".to_string(),
        Value::String(s) => format!("'{}'", s.replace('\'', "''")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::Value;
    use crate::engine::joins::{find_col_idx, nested_loop_join, resolve_equi_columns};

    fn mk_result(columns: Vec<&str>, rows: Vec<Vec<Value>>) -> QueryResult {
        let cols: Vec<String> = columns.into_iter().map(String::from).collect();
        let num_cols = cols.len();
        QueryResult {
            columns: cols,
            column_sources: vec![None; num_cols],
            rows,
            elapsed: std::time::Duration::default(),
            rows_affected: 0,
        }
    }

    // ── resolve_equi_columns ───────────────────────────────────────────

    #[test]
    fn resolve_simple_equi() {
        let cond = Expression::BinaryOp {
            op: BinaryOp::Eq,
            left: Box::new(Expression::QualifiedIdent {
                table: "u".into(),
                field: "id".into(),
            }),
            right: Box::new(Expression::QualifiedIdent {
                table: "o".into(),
                field: "user_id".into(),
            }),
        };
        let left_cols = ["id".to_string(), "name".to_string()];
        let left_srcs: Vec<Option<String>> = vec![None; left_cols.len()];
        let right_cols = ["user_id".to_string(), "amount".to_string()];
        let right_srcs: Vec<Option<String>> = vec![None; right_cols.len()];
        let res = resolve_equi_columns(&cond, &left_cols, &left_srcs, &right_cols, &right_srcs);
        assert_eq!(res, Some((0, 0)));
    }

    #[test]
    fn resolve_equi_with_idents() {
        let cond = Expression::BinaryOp {
            op: BinaryOp::Eq,
            left: Box::new(Expression::Ident("a_id".into())),
            right: Box::new(Expression::Ident("b_id".into())),
        };
        let left_cols = ["a_id".to_string()];
        let right_cols = ["b_id".to_string()];
        let res = resolve_equi_columns(&cond, &left_cols, &[], &right_cols, &[]);
        assert_eq!(res, Some((0, 0)));
    }

    #[test]
    fn resolve_non_equi_returns_none() {
        let cond = Expression::BinaryOp {
            op: BinaryOp::Gt,
            left: Box::new(Expression::Ident("a".into())),
            right: Box::new(Expression::Ident("b".into())),
        };
        let res = resolve_equi_columns(&cond, &[], &[], &[], &[]);
        assert_eq!(res, None);
    }

    // ── hash_join ──────────────────────────────────────────────────────

    #[test]
    fn hash_join_inner() {
        let left_row1 = vec![Value::Int(1), Value::String("Alice".into())];
        let left_row2 = vec![Value::Int(10), Value::String("Bob".into())];
        let left = mk_result(
            vec!["id", "name"],
            vec![left_row1, left_row2],
        );
        let right_row1 = vec![Value::Int(1), Value::Int(100)];
        let right_row2 = vec![Value::Int(2), Value::Int(200)];
        let right_row3 = vec![Value::Int(3), Value::Int(300)];
        let right = mk_result(
            vec!["user_id", "total"],
            vec![right_row1, right_row2, right_row3],
        );
        let cond = Expression::BinaryOp {
            op: BinaryOp::Eq,
            left: Box::new(Expression::Ident("id".into())),
            right: Box::new(Expression::Ident("user_id".into())),
        };
        let result = hash_join(left, right, &cond, JoinKind::Inner).unwrap();
        // Row 1 (id=1) matches user_id=1
        // Row 2 (id=10) matches nothing
        assert_eq!(result.columns, vec!["id", "name", "user_id", "total"]);
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0][0], Value::Int(1));
        assert_eq!(result.rows[0][1], Value::String("Alice".into()));
        assert_eq!(result.rows[0][2], Value::Int(1));
        assert_eq!(result.rows[0][3], Value::Int(100));
    }

    #[test]
    fn hash_join_left() {
        let left = mk_result(
            vec!["id", "name"],
            vec![
                vec![Value::Int(1), Value::String("Alice".into())],
                vec![Value::Int(3), Value::String("Charlie".into())],
            ],
        );
        let right = mk_result(
            vec!["user_id", "total"],
            vec![vec![Value::Int(1), Value::Int(100)]],
        );
        let cond = Expression::BinaryOp {
            op: BinaryOp::Eq,
            left: Box::new(Expression::Ident("id".into())),
            right: Box::new(Expression::Ident("user_id".into())),
        };
        let result = hash_join(left, right, &cond, JoinKind::Left).unwrap();
        assert_eq!(result.rows.len(), 2);
        let matched = result.rows.iter().find(|r| r[0] == Value::Int(1)).unwrap();
        assert_eq!(matched[3], Value::Int(100));
        let unmatched = result.rows.iter().find(|r| r[0] == Value::Int(3)).unwrap();
        assert_eq!(unmatched[2], Value::Null);
        assert_eq!(unmatched[3], Value::Null);
    }

    #[test]
    fn hash_join_multiple_matches() {
        let left = mk_result(
            vec!["id"],
            vec![vec![Value::Int(1)]],
        );
        let right = mk_result(
            vec!["user_id"],
            vec![vec![Value::Int(1)], vec![Value::Int(1)]],
        );
        let cond = Expression::BinaryOp {
            op: BinaryOp::Eq,
            left: Box::new(Expression::Ident("id".into())),
            right: Box::new(Expression::Ident("user_id".into())),
        };
        let result = hash_join(left, right, &cond, JoinKind::Inner).unwrap();
        assert_eq!(result.rows.len(), 2);
    }

    // ── nested_loop_join ───────────────────────────────────────────────

    #[test]
    fn nested_loop_join_inner() {
        let left = mk_result(
            vec!["id", "name"],
            vec![
                vec![Value::Int(1), Value::String("Alice".into())],
                vec![Value::Int(2), Value::String("Bob".into())],
            ],
        );
        let right = mk_result(
            vec!["user_id", "total"],
            vec![
                vec![Value::Int(1), Value::Int(100)],
                vec![Value::Int(1), Value::Int(200)],
            ],
        );
        let cond = Expression::BinaryOp {
            op: BinaryOp::Eq,
            left: Box::new(Expression::Ident("id".into())),
            right: Box::new(Expression::Ident("user_id".into())),
        };
        let result = nested_loop_join(left, right, &cond, JoinKind::Inner, None).unwrap();
        assert_eq!(result.rows.len(), 2);
    }

    #[test]
    fn nested_loop_cross_join() {
        let left = mk_result(
            vec!["a"],
            vec![vec![Value::Int(1)], vec![Value::Int(2)]],
        );
        let right = mk_result(
            vec!["b"],
            vec![vec![Value::String("x".into())], vec![Value::String("y".into())]],
        );
        let cond = Expression::Boolean(true);
        let result = nested_loop_join(left, right, &cond, JoinKind::Cross, None).unwrap();
        assert_eq!(result.rows.len(), 4);
    }

    // ── apply_filter ───────────────────────────────────────────────────

    #[test]
    fn filter_gt() {
        let input = mk_result(
            vec!["age"],
            vec![
                vec![Value::Int(10)],
                vec![Value::Int(25)],
                vec![Value::Int(30)],
            ],
        );
        let cond = Expression::BinaryOp {
            op: BinaryOp::Gt,
            left: Box::new(Expression::Ident("age".into())),
            right: Box::new(Expression::Integer(20)),
        };
        let result = apply_filter(&input, &cond).unwrap();
        assert_eq!(result.rows.len(), 2);
    }

    #[test]
    fn filter_and() {
        let input = mk_result(
            vec!["age", "name"],
            vec![
                vec![Value::Int(25), Value::String("Alice".into())],
                vec![Value::Int(30), Value::String("Bob".into())],
                vec![Value::Int(20), Value::String("Charlie".into())],
            ],
        );
        let cond = Expression::BinaryOp {
            op: BinaryOp::And,
            left: Box::new(Expression::BinaryOp {
                op: BinaryOp::Gt,
                left: Box::new(Expression::Ident("age".into())),
                right: Box::new(Expression::Integer(20)),
            }),
            right: Box::new(Expression::BinaryOp {
                op: BinaryOp::Neq,
                left: Box::new(Expression::Ident("name".into())),
                right: Box::new(Expression::String("Bob".into())),
            }),
        };
        let result = apply_filter(&input, &cond).unwrap();
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0][1], Value::String("Alice".into()));
    }

    // ── apply_projection ───────────────────────────────────────────────

    #[test]
    fn project_single_column() {
        let input = mk_result(
            vec!["id", "name", "age"],
            vec![
                vec![
                    Value::Int(1),
                    Value::String("Alice".into()),
                    Value::Int(25),
                ],
            ],
        );
        let fields = vec![Projection::Expr(
            Expression::Ident("name".into()),
            None,
        )];
        let result = apply_projection(&input, &fields).unwrap();
        assert_eq!(result.columns, vec!["name"]);
        assert_eq!(result.rows[0], vec![Value::String("Alice".into())]);
    }

    #[test]
    fn project_with_alias() {
        let input = mk_result(
            vec!["id"],
            vec![vec![Value::Int(1)], vec![Value::Int(2)]],
        );
        let fields = vec![Projection::Expr(
            Expression::Ident("id".into()),
            Some("user_id".into()),
        )];
        let result = apply_projection(&input, &fields).unwrap();
        assert_eq!(result.columns, vec!["user_id"]);
        assert_eq!(result.rows[0], vec![Value::Int(1)]);
    }

    // ── apply_order ────────────────────────────────────────────────────

    #[test]
    fn order_asc() {
        let input = mk_result(
            vec!["val"],
            vec![vec![Value::Int(3)], vec![Value::Int(1)], vec![Value::Int(2)]],
        );
        let order = vec![OrderBy {
            expr: Expression::Ident("val".into()),
            direction: OrderDir::Asc,
            nulls: NullsOrder::Default,
        }];
        let result = apply_order(&input, &order).unwrap();
        assert_eq!(
            result.rows,
            vec![vec![Value::Int(1)], vec![Value::Int(2)], vec![Value::Int(3)]]
        );
    }

    #[test]
    fn order_desc() {
        let input = mk_result(
            vec!["val"],
            vec![vec![Value::Int(1)], vec![Value::Int(3)], vec![Value::Int(2)]],
        );
        let order = vec![OrderBy {
            expr: Expression::Ident("val".into()),
            direction: OrderDir::Desc,
            nulls: NullsOrder::Default,
        }];
        let result = apply_order(&input, &order).unwrap();
        assert_eq!(
            result.rows,
            vec![vec![Value::Int(3)], vec![Value::Int(2)], vec![Value::Int(1)]]
        );
    }

    // ── apply_limit ────────────────────────────────────────────────────

    #[test]
    fn limit_basic() {
        let input = mk_result(
            vec!["x"],
            vec![
                vec![Value::Int(1)],
                vec![Value::Int(2)],
                vec![Value::Int(3)],
                vec![Value::Int(4)],
                vec![Value::Int(5)],
            ],
        );
        let result = apply_limit(&input, 3, 0).unwrap();
        assert_eq!(
            result.rows,
            vec![vec![Value::Int(1)], vec![Value::Int(2)], vec![Value::Int(3)]]
        );
    }

    #[test]
    fn limit_offset() {
        let input = mk_result(
            vec!["x"],
            vec![
                vec![Value::Int(1)],
                vec![Value::Int(2)],
                vec![Value::Int(3)],
                vec![Value::Int(4)],
                vec![Value::Int(5)],
            ],
        );
        let result = apply_limit(&input, 2, 2).unwrap();
        assert_eq!(
            result.rows,
            vec![vec![Value::Int(3)], vec![Value::Int(4)]]
        );
    }

    // ── apply_distinct ─────────────────────────────────────────────────

    #[test]
    fn distinct_removes_dupes() {
        let input = mk_result(
            vec!["x"],
            vec![
                vec![Value::Int(1)],
                vec![Value::Int(2)],
                vec![Value::Int(2)],
                vec![Value::Int(3)],
                vec![Value::Int(1)],
                vec![Value::Int(4)],
            ],
        );
        let result = apply_distinct(&input).unwrap();
        assert_eq!(result.rows.len(), 4);
    }

    // ── eval_expr ──────────────────────────────────────────────────────

    #[test]
    fn eval_ident() {
        let cols = ["a", "b"];
        let row = [Value::Int(42), Value::String("hello".into())];
        let expr = Expression::Ident("b".into());
        let result = eval_expr(&expr, &cols.map(String::from), &[], &row);
        assert_eq!(result, Value::String("hello".into()));
    }

    #[test]
    fn eval_eq() {
        let cols = ["x"];
        let row = [Value::Int(10)];
        let expr = Expression::BinaryOp {
            op: BinaryOp::Eq,
            left: Box::new(Expression::Ident("x".into())),
            right: Box::new(Expression::Integer(10)),
        };
        assert!(eval_expr_bool(&expr, &cols.map(String::from), &[], &row));
        let row2 = [Value::Int(5)];
        assert!(!eval_expr_bool(&expr, &cols.map(String::from), &[], &row2));
    }

    #[test]
    fn eval_null() {
        let cols = ["x"];
        let row = [Value::Null];
        let expr = Expression::BinaryOp {
            op: BinaryOp::Eq,
            left: Box::new(Expression::Ident("x".into())),
            right: Box::new(Expression::Integer(10)),
        };
        assert!(!eval_expr_bool(&expr, &cols.map(String::from), &[], &row));
    }

    // ── Qualified column resolution ─────────────────────────────────────

    #[test]
    fn eval_qualified_ident() {
        let cols = ["id", "name", "id", "total"];
        let sources: Vec<Option<String>> = vec![
            Some("u".to_string()),
            Some("u".to_string()),
            Some("o".to_string()),
            Some("o".to_string()),
        ];
        let row = [Value::Int(1), Value::String("Alice".into()), Value::Int(1429), Value::Float(119.47)];
        let expr = Expression::QualifiedIdent {
            table: "o".into(),
            field: "id".into(),
        };
        let result = eval_expr(&expr, &cols.map(String::from), &sources, &row);
        assert_eq!(result, Value::Int(1429));
    }

    #[test]
    fn eval_qualified_ident_fallback() {
        let cols = ["id", "name"];
        let sources: Vec<Option<String>> = vec![None, None];
        let row = [Value::Int(1), Value::String("Alice".into())];
        let expr = Expression::QualifiedIdent {
            table: "o".into(),
            field: "id".into(),
        };
        let result = eval_expr(&expr, &cols.map(String::from), &sources, &row);
        assert_eq!(result, Value::Int(1));
    }

    #[test]
    fn find_col_idx_qualified() {
        let left_cols = ["id", "name"];
        let left_srcs: Vec<Option<String>> = vec![Some("u".into()), Some("u".into())];
        let right_cols = ["id", "total"];
        let right_srcs: Vec<Option<String>> = vec![Some("o".into()), Some("o".into())];

        let expr = Expression::QualifiedIdent {
            table: "o".into(),
            field: "id".into(),
        };
        let res = find_col_idx(&expr, &left_cols.map(String::from), &left_srcs, &right_cols.map(String::from), &right_srcs);
        assert_eq!(res, Some((0, true)));
    }

    #[test]
    fn resolve_equi_with_duplicate_names() {
        let cond = Expression::BinaryOp {
            op: BinaryOp::Eq,
            left: Box::new(Expression::QualifiedIdent {
                table: "o".into(),
                field: "id".into(),
            }),
            right: Box::new(Expression::Ident("order_id".into())),
        };
        let left_cols = ["id", "name", "id", "total"];
        let left_srcs: Vec<Option<String>> = vec![
            Some("u".into()), Some("u".into()),
            Some("o".into()), Some("o".into()),
        ];
        let right_cols = ["order_id"];
        let right_srcs: Vec<Option<String>> = vec![None];
        let res = resolve_equi_columns(
            &cond,
            &left_cols.map(String::from), &left_srcs,
            &right_cols.map(String::from), &right_srcs,
        );
        assert_eq!(res, Some((2, 0)));
    }

    // ── cmp_values ─────────────────────────────────────────────────────

    #[test]
    fn cmp_ints() {
        assert_eq!(
            cmp_values(&Value::Int(1), &Value::Int(2)),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            cmp_values(&Value::Int(5), &Value::Int(5)),
            std::cmp::Ordering::Equal
        );
    }

    #[test]
    fn cmp_nulls() {
        assert_eq!(
            cmp_values(&Value::Null, &Value::Null),
            std::cmp::Ordering::Equal
        );
        assert_eq!(
            cmp_values(&Value::Null, &Value::Int(1)),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            cmp_values(&Value::String("a".into()), &Value::Null),
            std::cmp::Ordering::Greater
        );
    }

    // ── planner: is_cross_db ────────────────────────────────────────────

    #[test]
    fn is_cross_db_true() {
        use crate::engine::planner::{PlanNode, is_cross_db};
        let plan = PlanNode::Join {
            left: Box::new(PlanNode::Scan {
                source: Source {
                    schema: None,
                    name: "u".into(),
                    alias: None,
                    connection: Some("pg".into()),
                    kind: SourceKind::Table("users".into()),
                },
                database: Some(("pg".into(), crate::connection::DatabaseKind::Postgres)),
                filter: None,
            }),
            right: Box::new(PlanNode::Scan {
                source: Source {
                    schema: None,
                    name: "o".into(),
                    alias: None,
                    connection: Some("mongo".into()),
                    kind: SourceKind::Table("orders".into()),
                },
                database: Some(("mongo".into(), crate::connection::DatabaseKind::MongoDB)),
                filter: None,
            }),
            condition: Expression::Boolean(true),
            strategy: crate::engine::planner::JoinStrategy::Hash,
            join_kind: JoinKind::Inner,
            limit: None,
        };
        assert!(is_cross_db(&plan));
    }

    #[test]
    fn is_cross_db_false_same_db() {
        use crate::engine::planner::{PlanNode, is_cross_db};
        let plan = PlanNode::Join {
            left: Box::new(PlanNode::Scan {
                source: Source {
                    schema: None,
                    name: "u".into(),
                    alias: None,
                    connection: Some("pg".into()),
                    kind: SourceKind::Table("users".into()),
                },
                database: Some(("pg".into(), crate::connection::DatabaseKind::Postgres)),
                filter: None,
            }),
            right: Box::new(PlanNode::Scan {
                source: Source {
                    schema: None,
                    name: "o".into(),
                    alias: None,
                    connection: Some("pg".into()),
                    kind: SourceKind::Table("orders".into()),
                },
                database: Some(("pg".into(), crate::connection::DatabaseKind::Postgres)),
                filter: None,
            }),
            condition: Expression::Boolean(true),
            strategy: crate::engine::planner::JoinStrategy::Hash,
            join_kind: JoinKind::Inner,
            limit: None,
        };
        assert!(!is_cross_db(&plan));
    }

    #[test]
    fn find_all_databases_returns_distinct() {
        use crate::engine::planner::{PlanNode, find_all_databases};
        let plan = PlanNode::Join {
            left: Box::new(PlanNode::Scan {
                source: Source {
                    schema: None,
                    name: "a".into(),
                    alias: None,
                    connection: Some("pg".into()),
                    kind: SourceKind::Table("t1".into()),
                },
                database: Some(("pg".into(), crate::connection::DatabaseKind::Postgres)),
                filter: None,
            }),
            right: Box::new(PlanNode::Scan {
                source: Source {
                    schema: None,
                    name: "b".into(),
                    alias: None,
                    connection: Some("mongo".into()),
                    kind: SourceKind::Table("t2".into()),
                },
                database: Some(("mongo".into(), crate::connection::DatabaseKind::MongoDB)),
                filter: None,
            }),
            condition: Expression::Boolean(true),
            strategy: crate::engine::planner::JoinStrategy::Hash,
            join_kind: JoinKind::Inner,
            limit: None,
        };
        let dbs = find_all_databases(&plan);
        assert_eq!(dbs.len(), 2);
    }

    // ── collect_single_db_query ────────────────────────────────────────

    #[test]
    fn collect_single_db_scan() {
        let source = Source {
        schema: None,
            name: "users".into(),
            alias: None,
            connection: None,
            kind: SourceKind::Table("users".into()),
        };
        let node = PlanNode::Scan {
            source,
            database: Some(("pg".into(), crate::connection::DatabaseKind::Postgres)),
            filter: None,
        };
        let result = collect_single_db_query(&node);
        assert!(result.is_some());
        let (name, kind, q) = result.unwrap();
        assert_eq!(name, "pg");
        assert_eq!(kind, crate::connection::DatabaseKind::Postgres);
        assert_eq!(q.sources.len(), 1);
    }

    #[test]
    fn collect_single_db_with_filter() {
        let source = Source {
        schema: None,
            name: "users".into(),
            alias: None,
            connection: None,
            kind: SourceKind::Table("users".into()),
        };
        let scan = PlanNode::Scan {
            source,
            database: Some(("pg".into(), crate::connection::DatabaseKind::Postgres)),
            filter: None,
        };
        let filter_node = PlanNode::Filter {
            input: Box::new(scan),
            condition: Expression::BinaryOp {
                op: BinaryOp::Gt,
                left: Box::new(Expression::Ident("age".into())),
                right: Box::new(Expression::Integer(21)),
            },
        };
        let (_, _, q) = collect_single_db_query(&filter_node).unwrap();
        assert!(q.filter.is_some());
    }

    #[test]
    fn collect_single_db_rejects_cross_db_join() {
        let left = PlanNode::Scan {
            source: Source {
                    schema: None,
                name: "a".into(),
                alias: None,
                connection: None,
                kind: SourceKind::Table("t1".into()),
            },
            database: Some(("pg".into(), crate::connection::DatabaseKind::Postgres)),
            filter: None,
        };
        let right = PlanNode::Scan {
            source: Source {
                    schema: None,
                name: "b".into(),
                alias: None,
                connection: None,
                kind: SourceKind::Table("t2".into()),
            },
            database: Some(("mongo".into(), crate::connection::DatabaseKind::MongoDB)),
            filter: None,
        };
        let join = PlanNode::Join {
            left: Box::new(left),
            right: Box::new(right),
            condition: Expression::Boolean(true),
            strategy: crate::engine::planner::JoinStrategy::Hash,
            join_kind: JoinKind::Inner,
            limit: None,
        };
        assert!(collect_single_db_query(&join).is_none());
    }

    // ── translate_for_kind ─────────────────────────────────────────────

    #[test]
    fn translate_postgres() {
        let q = Query {
            sources: vec![Source {
                schema: None,
                name: "users".into(),
                alias: None,
                connection: None,
                kind: SourceKind::Table("users".into()),
            }],
            ..Default::default()
        };
        let sql = translate_for_kind(&q, &crate::connection::DatabaseKind::Postgres).unwrap();
        assert_eq!(sql, r#"SELECT * FROM "users""#);
    }

    #[test]
    fn translate_mysql() {
        let q = Query {
            sources: vec![Source {
                schema: None,
                name: "users".into(),
                alias: None,
                connection: None,
                kind: SourceKind::Table("users".into()),
            }],
            ..Default::default()
        };
        let sql = translate_for_kind(&q, &crate::connection::DatabaseKind::MySQL).unwrap();
        assert_eq!(sql, "SELECT * FROM `users`");
    }

    // ── Chained CTE with in-memory aggregation ────────────────────────

    use crate::adapters::{DatabaseAdapter, QueryResult, TableInfo, TableSchema};
    use crate::connection::{ConnectionConfig, DatabaseKind};
    use crate::lang::parse;
    use async_trait::async_trait;

    struct MockAdapter;

    #[async_trait]
    impl DatabaseAdapter for MockAdapter {
        async fn connect(_config: &ConnectionConfig) -> Result<Self, RiverError> {
            Ok(MockAdapter)
        }

        async fn execute(&self, query: &str) -> Result<QueryResult, RiverError> {
            if query.contains("orders") {
                Ok(QueryResult {
                    columns: vec![
                        "id".into(),
                        "user_id".into(),
                        "total".into(),
                        "status".into(),
                    ],
                    column_sources: vec![None; 4],
                    rows: vec![
                        vec![Value::Int(1), Value::Int(1), Value::Int(500), Value::String("paid".into())],
                        vec![Value::Int(2), Value::Int(1), Value::Int(600), Value::String("paid".into())],
                        vec![Value::Int(3), Value::Int(2), Value::Int(200), Value::String("paid".into())],
                        vec![Value::Int(4), Value::Int(3), Value::Int(1500), Value::String("paid".into())],
                        vec![Value::Int(5), Value::Int(3), Value::Int(300), Value::String("paid".into())],
                    ],
                    elapsed: std::time::Duration::default(),
                    rows_affected: 0,
                })
            } else if query.contains("users") {
                Ok(QueryResult {
                    columns: vec!["id".into(), "name".into()],
                    column_sources: vec![None; 2],
                    rows: vec![
                        vec![Value::Int(1), Value::String("Alice".into())],
                        vec![Value::Int(2), Value::String("Bob".into())],
                        vec![Value::Int(3), Value::String("Carol".into())],
                    ],
                    elapsed: std::time::Duration::default(),
                    rows_affected: 0,
                })
            } else {
                Ok(QueryResult {
                    columns: vec![],
                    column_sources: vec![],
                    rows: vec![],
                    elapsed: std::time::Duration::default(),
                    rows_affected: 0,
                })
            }
        }

        async fn list_tables(&self, _schema: Option<&str>) -> Result<Vec<TableInfo>, RiverError> {
            Ok(vec![])
        }
        async fn describe_table(&self, _table: &str, _schema: Option<&str>) -> Result<TableSchema, RiverError> {
            Ok(TableSchema {
                name: "mock".into(),
                columns: vec![],
            })
        }
        fn dialect(&self) -> DatabaseKind {
            DatabaseKind::SQLite
        }
        async fn exec_maintenance(&self, _sql: &str) -> Result<QueryResult, RiverError> {
            Ok(QueryResult {
                columns: vec![],
                column_sources: vec![],
                rows: vec![],
                elapsed: std::time::Duration::default(),
                rows_affected: 0,
            })
        }
    }

    #[tokio::test]
    async fn chained_cte_with_in_memory_aggregation() {
        let mut adapters: HashMap<String, Box<dyn DatabaseAdapter>> = HashMap::new();
        adapters.insert("test".into(), Box::new(MockAdapter));
        let source_db = vec![("test".into(), DatabaseKind::SQLite)];

        let query = r#"with
  paid_orders as ( find * from orders where status = "paid" ),
  user_totals as ( find [user_id, sum(total) as revenue] from paid_orders group by user_id )
find [u.name, ut.revenue]
from users as u
join user_totals as ut on u.id = ut.user_id
where ut.revenue > 1000
order by ut.revenue desc"#;

        let stmt = parse(query).expect("parse failed");
        let ai_configs = std::collections::HashMap::new();
        let ai_client = crate::ai::AiClient::new();
        let result = execute_statement(&stmt, &source_db, &adapters, &ai_configs, &ai_client)
            .await
            .expect("execution failed");

        assert_eq!(result.columns, vec!["name", "revenue"]);
        assert_eq!(result.rows.len(), 2);
        assert_eq!(result.rows[0][0], Value::String("Carol".into()));
        assert_eq!(result.rows[0][1], Value::Int(1800));
        assert_eq!(result.rows[1][0], Value::String("Alice".into()));
        assert_eq!(result.rows[1][1], Value::Int(1100));
    }

    #[tokio::test]
    async fn single_cte_resolution() {
        let mut adapters: HashMap<String, Box<dyn DatabaseAdapter>> = HashMap::new();
        adapters.insert("test".into(), Box::new(MockAdapter));
        let source_db = vec![("test".into(), DatabaseKind::SQLite)];

        let query = r#"with paid_orders as ( find * from orders where status = "paid" ) find [id, total] from paid_orders"#;

        let stmt = parse(query).expect("parse failed");
        let ai_configs = std::collections::HashMap::new();
        let ai_client = crate::ai::AiClient::new();
        let result = execute_statement(&stmt, &source_db, &adapters, &ai_configs, &ai_client)
            .await
            .expect("execution failed");

        assert_eq!(result.columns, vec!["id", "total"]);
        assert_eq!(result.rows.len(), 5);
    }

    #[tokio::test]
    async fn in_memory_aggregate_sum() {
        let mut adapters: HashMap<String, Box<dyn DatabaseAdapter>> = HashMap::new();
        adapters.insert("test".into(), Box::new(MockAdapter));
        let source_db = vec![("test".into(), DatabaseKind::SQLite)];

        let query = r#"with all_orders as ( find * from orders ) find [user_id, sum(total) as revenue] from all_orders group by user_id"#;

        let stmt = parse(query).expect("parse failed");
        let ai_configs = std::collections::HashMap::new();
        let ai_client = crate::ai::AiClient::new();
        let result = execute_statement(&stmt, &source_db, &adapters, &ai_configs, &ai_client)
            .await
            .expect("execution failed");

        assert_eq!(result.columns, vec!["user_id", "revenue"]);
        assert_eq!(result.rows.len(), 3);
        let by_user: HashMap<i64, i64> = result
            .rows
            .iter()
            .map(|r| match (&r[0], &r[1]) {
                (Value::Int(u), Value::Int(v)) => (*u, *v),
                _ => panic!("expected int values, got {:?}", r),
            })
            .collect();
        assert_eq!(by_user.get(&1), Some(&1100));
        assert_eq!(by_user.get(&2), Some(&200));
        assert_eq!(by_user.get(&3), Some(&1800));
    }

    #[tokio::test]
    async fn in_memory_aggregate_count() {
        let mut adapters: HashMap<String, Box<dyn DatabaseAdapter>> = HashMap::new();
        adapters.insert("test".into(), Box::new(MockAdapter));
        let source_db = vec![("test".into(), DatabaseKind::SQLite)];

        let query = r#"with all_orders as ( find * from orders ) find [count(*) as cnt] from all_orders"#;

        let stmt = parse(query).expect("parse failed");
        let ai_configs = std::collections::HashMap::new();
        let ai_client = crate::ai::AiClient::new();
        let result = execute_statement(&stmt, &source_db, &adapters, &ai_configs, &ai_client)
            .await
            .expect("execution failed");

        assert_eq!(result.columns, vec!["cnt"]);
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0][0], Value::Int(5));
    }

    #[tokio::test]
    async fn cross_db_cross_join_without_limit_rejected() {
        let mut adapters: HashMap<String, Box<dyn DatabaseAdapter>> = HashMap::new();
        adapters.insert("pg".into(), Box::new(MockAdapter));
        adapters.insert("mysql".into(), Box::new(MockAdapter));
        let source_db = vec![
            ("pg".into(), DatabaseKind::Postgres),
            ("mysql".into(), DatabaseKind::MySQL),
        ];

        let query =
            r#"find [u.name, o.total] from users@pg as u cross join orders@mysql as o"#;
        let stmt = parse(query).expect("parse failed");
        let ai_configs = std::collections::HashMap::new();
        let ai_client = crate::ai::AiClient::new();
        let result = execute_statement(&stmt, &source_db, &adapters, &ai_configs, &ai_client).await;
        assert!(
            result.is_err(),
            "Expected error for unbounded cross-DB cross join"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("LIMIT") || err_msg.contains("limit"),
            "Expected error about LIMIT, got: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn cross_db_cross_join_with_limit_allowed() {
        let mut adapters: HashMap<String, Box<dyn DatabaseAdapter>> = HashMap::new();
        adapters.insert("pg".into(), Box::new(MockAdapter));
        adapters.insert("mysql".into(), Box::new(MockAdapter));
        let source_db = vec![
            ("pg".into(), DatabaseKind::Postgres),
            ("mysql".into(), DatabaseKind::MySQL),
        ];

        let query =
            r#"find [u.name, o.total] from users@pg as u cross join orders@mysql as o limit 5"#;
        let stmt = parse(query).expect("parse failed");
        let ai_configs = std::collections::HashMap::new();
        let ai_client = crate::ai::AiClient::new();
        let result = execute_statement(&stmt, &source_db, &adapters, &ai_configs, &ai_client).await;
        assert!(
            result.is_ok(),
            "cross-DB cross join with LIMIT should work: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn semi_join_fetch_equi_join_cross_db() {
        let mut adapters: HashMap<String, Box<dyn DatabaseAdapter>> = HashMap::new();
        adapters.insert("pg".into(), Box::new(MockAdapter));
        adapters.insert("mysql".into(), Box::new(MockAdapter));
        let source_db = vec![
            ("pg".into(), DatabaseKind::Postgres),
            ("mysql".into(), DatabaseKind::MySQL),
        ];

        let query =
            r#"find [u.name, o.total] from users@pg as u join orders@mysql as o on u.id = o.user_id"#;
        let stmt = parse(query).expect("parse failed");
        let ai_configs = std::collections::HashMap::new();
        let ai_client = crate::ai::AiClient::new();
        let result = execute_statement(&stmt, &source_db, &adapters, &ai_configs, &ai_client).await;
        assert!(
            result.is_ok(),
            "SemiJoinFetch should execute successfully: {:?}",
            result.err()
        );
        let qr = result.unwrap();
        assert!(!qr.rows.is_empty(), "Should have join results");
    }

    #[test]
    fn resolve_params_substitutes_string_param() {
        let mut params = HashMap::new();
        params.insert("status".to_string(), Value::String("active".to_string()));
        let expr = Expression::NamedParam("status".to_string());
        let resolved = super::resolve_params(&expr, &params);
        assert_eq!(resolved, Expression::String("active".to_string()));
    }

    #[test]
    fn resolve_params_substitutes_int_param() {
        let mut params = HashMap::new();
        params.insert("limit".to_string(), Value::Int(100));
        let expr = Expression::NamedParam("limit".to_string());
        let resolved = super::resolve_params(&expr, &params);
        assert_eq!(resolved, Expression::Integer(100));
    }

    #[test]
    fn resolve_params_substitutes_bool_param() {
        let mut params = HashMap::new();
        params.insert("flag".to_string(), Value::Bool(true));
        let expr = Expression::NamedParam("flag".to_string());
        let resolved = super::resolve_params(&expr, &params);
        assert_eq!(resolved, Expression::Boolean(true));
    }

    #[test]
    fn resolve_params_substitutes_float_param() {
        let mut params = HashMap::new();
        params.insert("threshold".to_string(), Value::Float(std::f64::consts::PI));
        let expr = Expression::NamedParam("threshold".to_string());
        let resolved = super::resolve_params(&expr, &params);
        assert_eq!(resolved, Expression::Number(std::f64::consts::PI));
    }

    #[test]
    fn resolve_params_substitutes_null_param() {
        let mut params = HashMap::new();
        params.insert("value".to_string(), Value::Null);
        let expr = Expression::NamedParam("value".to_string());
        let resolved = super::resolve_params(&expr, &params);
        assert_eq!(resolved, Expression::Null);
    }

    #[test]
    fn resolve_params_unknown_param_unchanged() {
        let params: HashMap<String, Value> = HashMap::new();
        let expr = Expression::NamedParam("unknown".to_string());
        let resolved = super::resolve_params(&expr, &params);
        assert_eq!(resolved, Expression::NamedParam("unknown".to_string()));
    }

    #[test]
    fn resolve_params_in_binary_op() {
        let mut params = HashMap::new();
        params.insert("status".to_string(), Value::String("active".to_string()));
        let expr = Expression::BinaryOp {
            op: BinaryOp::Eq,
            left: Box::new(Expression::Ident("status".to_string())),
            right: Box::new(Expression::NamedParam("status".to_string())),
        };
        let resolved = super::resolve_params(&expr, &params);
        let expected = Expression::BinaryOp {
            op: BinaryOp::Eq,
            left: Box::new(Expression::Ident("status".to_string())),
            right: Box::new(Expression::String("active".to_string())),
        };
        assert_eq!(resolved, expected);
    }

    #[test]
    fn resolve_params_in_between() {
        let mut params = HashMap::new();
        params.insert("start".to_string(), Value::String("2024-01-01".to_string()));
        params.insert("end".to_string(), Value::String("2024-12-31".to_string()));
        let expr = Expression::Between {
            expr: Box::new(Expression::Ident("created_at".to_string())),
            low: Box::new(Expression::NamedParam("start".to_string())),
            high: Box::new(Expression::NamedParam("end".to_string())),
        };
        let resolved = super::resolve_params(&expr, &params);
        let expected = Expression::Between {
            expr: Box::new(Expression::Ident("created_at".to_string())),
            low: Box::new(Expression::String("2024-01-01".to_string())),
            high: Box::new(Expression::String("2024-12-31".to_string())),
        };
        assert_eq!(resolved, expected);
    }

    #[test]
    fn resolve_params_expression_to_value() {
        assert_eq!(
            super::expression_to_value(&Expression::String("active".to_string())),
            Some(Value::String("active".to_string()))
        );
        assert_eq!(
            super::expression_to_value(&Expression::Integer(42)),
            Some(Value::Int(42))
        );
        assert_eq!(
            super::expression_to_value(&Expression::Boolean(true)),
            Some(Value::Bool(true))
        );
        assert_eq!(
            super::expression_to_value(&Expression::Null),
            Some(Value::Null)
        );
        assert_eq!(
            super::expression_to_value(&Expression::Ident("x".to_string())),
            None
        );
    }

    #[test]
    fn resolve_params_in_statement_query_filter() {
        let mut params = HashMap::new();
        params.insert("status".to_string(), Value::String("active".to_string()));
        let query = Query {
            filter: Some(Expression::BinaryOp {
                op: BinaryOp::Eq,
                left: Box::new(Expression::Ident("status".to_string())),
                right: Box::new(Expression::NamedParam("status".to_string())),
            }),
            ..Query::default()
        };
        let stmt = Statement::Query(query);
        let resolved = super::resolve_params_in_statement(&stmt, &params);
        match resolved {
            Statement::Query(q) => {
                let expected = Expression::BinaryOp {
                    op: BinaryOp::Eq,
                    left: Box::new(Expression::Ident("status".to_string())),
                    right: Box::new(Expression::String("active".to_string())),
                };
                assert_eq!(q.filter, Some(expected));
            }
            _ => panic!("expected Query"),
        }
    }

    #[test]
    fn resolve_params_in_statement_no_params_unchanged() {
        let params: HashMap<String, Value> = HashMap::new();
        let query = Query {
            filter: Some(Expression::BinaryOp {
                op: BinaryOp::Eq,
                left: Box::new(Expression::Ident("status".to_string())),
                right: Box::new(Expression::NamedParam("status".to_string())),
            }),
            ..Query::default()
        };
        let stmt = Statement::Query(query.clone());
        let resolved = super::resolve_params_in_statement(&stmt, &params);
        assert_eq!(resolved, stmt);
    }

    // ── union_results ──────────────────────────────────────────────────

    #[test]
    fn union_deduplicates_duplicates() {
        let left = mk_result(
            vec!["name", "email"],
            vec![
                vec![Value::String("Alice".into()), Value::String("a@x.com".into())],
                vec![Value::String("Bob".into()), Value::String("b@x.com".into())],
            ],
        );
        let right = mk_result(
            vec!["name", "email"],
            vec![
                vec![Value::String("Bob".into()), Value::String("b@x.com".into())],
                vec![Value::String("Carol".into()), Value::String("c@x.com".into())],
            ],
        );
        let result = super::union_results(left, right, false).unwrap();
        assert_eq!(result.columns, vec!["name", "email"]);
        assert_eq!(result.rows.len(), 3);
    }

    #[test]
    fn union_all_preserves_duplicates() {
        let left = mk_result(
            vec!["name", "email"],
            vec![
                vec![Value::String("Alice".into()), Value::String("a@x.com".into())],
                vec![Value::String("Bob".into()), Value::String("b@x.com".into())],
            ],
        );
        let right = mk_result(
            vec!["name", "email"],
            vec![
                vec![Value::String("Bob".into()), Value::String("b@x.com".into())],
                vec![Value::String("Carol".into()), Value::String("c@x.com".into())],
            ],
        );
        let result = super::union_results(left, right, true).unwrap();
        assert_eq!(result.columns, vec!["name", "email"]);
        assert_eq!(result.rows.len(), 4);
    }

    #[test]
    fn union_empty_left() {
        let left = mk_result(vec!["name", "email"], vec![]);
        let right = mk_result(
            vec!["name", "email"],
            vec![
                vec![Value::String("Alice".into()), Value::String("a@x.com".into())],
            ],
        );
        let result = super::union_results(left, right, false).unwrap();
        assert_eq!(result.rows.len(), 1);
    }

    #[test]
    fn union_empty_right() {
        let left = mk_result(
            vec!["name", "email"],
            vec![
                vec![Value::String("Alice".into()), Value::String("a@x.com".into())],
            ],
        );
        let right = mk_result(vec!["name", "email"], vec![]);
        let result = super::union_results(left, right, false).unwrap();
        assert_eq!(result.rows.len(), 1);
    }

    #[test]
    fn union_both_empty() {
        let left = mk_result(vec!["name", "email"], vec![]);
        let right = mk_result(vec!["name", "email"], vec![]);
        let result = super::union_results(left, right, false).unwrap();
        assert_eq!(result.rows.len(), 0);
    }

    #[test]
    fn union_mismatched_column_counts_uses_wider() {
        let left = mk_result(
            vec!["a"],
            vec![vec![Value::String("x".into())]],
        );
        let right = mk_result(
            vec!["a", "b"],
            vec![vec![Value::String("y".into()), Value::Int(1)]],
        );
        let result = super::union_results(left, right, false).unwrap();
        assert_eq!(result.columns.len(), 2);
        assert_eq!(result.columns, vec!["a", "b"]);
    }

    // ── intersect_results ──────────────────────────────────────────────

    #[test]
    fn intersect_common_rows() {
        let left = mk_result(
            vec!["name"],
            vec![
                vec![Value::String("Alice".into())],
                vec![Value::String("Bob".into())],
            ],
        );
        let right = mk_result(
            vec!["name"],
            vec![
                vec![Value::String("Bob".into())],
                vec![Value::String("Carol".into())],
            ],
        );
        let result = super::intersect_results(left, right).unwrap();
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0][0], Value::String("Bob".into()));
    }

    #[test]
    fn intersect_no_common_rows() {
        let left = mk_result(
            vec!["name"],
            vec![vec![Value::String("Alice".into())]],
        );
        let right = mk_result(
            vec!["name"],
            vec![vec![Value::String("Bob".into())]],
        );
        let result = super::intersect_results(left, right).unwrap();
        assert_eq!(result.rows.len(), 0);
    }

    #[test]
    fn intersect_empty_left() {
        let left = mk_result(vec!["name"], vec![]);
        let right = mk_result(
            vec!["name"],
            vec![vec![Value::String("Alice".into())]],
        );
        let result = super::intersect_results(left, right).unwrap();
        assert_eq!(result.rows.len(), 0);
    }

    // ── except_results ─────────────────────────────────────────────────

    #[test]
    fn except_removes_right_rows() {
        let left = mk_result(
            vec!["name"],
            vec![
                vec![Value::String("Alice".into())],
                vec![Value::String("Bob".into())],
            ],
        );
        let right = mk_result(
            vec!["name"],
            vec![vec![Value::String("Bob".into())]],
        );
        let result = super::except_results(left, right).unwrap();
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0][0], Value::String("Alice".into()));
    }

    #[test]
    fn except_no_overlap() {
        let left = mk_result(
            vec!["name"],
            vec![vec![Value::String("Alice".into())]],
        );
        let right = mk_result(
            vec!["name"],
            vec![vec![Value::String("Bob".into())]],
        );
        let result = super::except_results(left, right).unwrap();
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0][0], Value::String("Alice".into()));
    }

    #[test]
    fn except_all_removed() {
        let left = mk_result(
            vec!["name"],
            vec![vec![Value::String("Alice".into())]],
        );
        let right = mk_result(
            vec!["name"],
            vec![vec![Value::String("Alice".into())]],
        );
        let result = super::except_results(left, right).unwrap();
        assert_eq!(result.rows.len(), 0);
    }
}
