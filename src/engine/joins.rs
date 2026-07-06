use std::collections::HashMap;

use crate::adapters::{QueryResult, Value};
use crate::engine::expr::eval_expr_bool;
use crate::lang::ast::JoinKind;
use crate::engine::planner::JoinStrategy;
use crate::lang::ast::*;
use crate::error::RiverError;

pub(crate) fn merge_columns(
    left_cols: &[String],
    left_sources: &[Option<String>],
    right_cols: &[String],
    right_sources: &[Option<String>],
) -> (Vec<String>, Vec<Option<String>>) {
    let mut cols = left_cols.to_vec();
    let mut sources = left_sources.to_vec();
    cols.extend(right_cols.iter().cloned());
    sources.extend(right_sources.iter().cloned());
    (cols, sources)
}

pub(crate) fn merge_vals(left: &[Value], right: &[Value]) -> Vec<Value> {
    let mut vals = left.to_vec();
    vals.extend(right.iter().cloned());
    vals
}

pub(crate) fn join_results(
    left: QueryResult,
    right: QueryResult,
    condition: &Expression,
    strategy: &JoinStrategy,
    join_kind: JoinKind,
    limit: Option<u64>,
) -> Result<QueryResult, RiverError> {
    let can_hash = matches!(strategy, JoinStrategy::Hash | JoinStrategy::Auto)
        && resolve_equi_columns(
            condition,
            &left.columns,
            &left.column_sources,
            &right.columns,
            &right.column_sources,
        )
        .is_some();

    if can_hash {
        hash_join(left, right, condition, join_kind)
    } else {
        nested_loop_join(left, right, condition, join_kind, limit)
    }
}

pub(crate) fn resolve_equi_columns(
    condition: &Expression,
    left_cols: &[String],
    left_sources: &[Option<String>],
    right_cols: &[String],
    right_sources: &[Option<String>],
) -> Option<(usize, usize)> {
    match condition {
        Expression::BinaryOp {
            op: BinaryOp::Eq,
            left,
            right,
        } => {
            let li = find_col_idx(left, left_cols, left_sources, right_cols, right_sources)?;
            let ri = find_col_idx(right, left_cols, left_sources, right_cols, right_sources)?;
            if li.1 != ri.1 {
                if li.1 {
                    Some((ri.0, li.0))
                } else {
                    Some((li.0, ri.0))
                }
            } else {
                let l_name = extract_field_name(left)?;
                let r_name = extract_field_name(right)?;
                let l_idx = left_cols.iter().position(|c| c == l_name)?;
                let r_idx = right_cols.iter().position(|c| c == r_name)?;
                Some((l_idx, r_idx))
            }
        }
        _ => None,
    }
}

pub(crate) fn extract_field_name(expr: &Expression) -> Option<&str> {
    match expr {
        Expression::Ident(n) => Some(n.as_str()),
        Expression::QualifiedIdent { field, .. } => Some(field.as_str()),
        _ => None,
    }
}

pub(crate) fn find_col_idx(
    expr: &Expression,
    left_cols: &[String],
    left_sources: &[Option<String>],
    right_cols: &[String],
    right_sources: &[Option<String>],
) -> Option<(usize, bool)> {
    let resolve_exact = |cols: &[String], sources: &[Option<String>], table: &str, field: &str| -> Option<usize> {
        cols.iter()
            .enumerate()
            .position(|(i, c)| {
                c == field && sources.get(i).and_then(|s| s.as_deref()) == Some(table)
            })
    };
    let resolve_name = |cols: &[String], name: &str| -> Option<usize> {
        cols.iter().position(|c| c == name)
    };

    match expr {
        Expression::Ident(name) => {
            if let Some(i) = resolve_name(left_cols, name) {
                Some((i, false))
            } else {
                resolve_name(right_cols, name).map(|i| (i, true))
            }
        }
        Expression::QualifiedIdent { table, field } => {
            if let Some(i) = resolve_exact(left_cols, left_sources, table, field) {
                Some((i, false))
            } else if let Some(i) = resolve_exact(right_cols, right_sources, table, field) {
                Some((i, true))
            } else if let Some(i) = resolve_name(left_cols, field) {
                Some((i, false))
            } else {
                resolve_name(right_cols, field).map(|i| (i, true))
            }
        }
        _ => None,
    }
}

pub(crate) fn resolve_col_idx(
    expr: &Expression,
    columns: &[String],
    sources: &[Option<String>],
) -> Option<usize> {
    match expr {
        Expression::Ident(name) => columns.iter().position(|c| c == name),
        Expression::QualifiedIdent { table, field } => {
            columns
                .iter()
                .enumerate()
                .position(|(i, c)| {
                    c == field && sources.get(i).and_then(|s| s.as_deref()) == Some(table.as_str())
                })
                .or_else(|| columns.iter().position(|c| c == field))
        }
        _ => None,
    }
}

pub(crate) fn hash_join(
    left: QueryResult,
    right: QueryResult,
    condition: &Expression,
    join_kind: JoinKind,
) -> Result<QueryResult, RiverError> {
    let (left_key_idx, right_key_idx) =
        resolve_equi_columns(
            condition,
            &left.columns,
            &left.column_sources,
            &right.columns,
            &right.column_sources,
        )
            .ok_or_else(|| {
                RiverError::Unsupported("hash join requires equi-join condition".into())
            })?;

    let orig_left_cols = left.columns.len();
    let orig_right_cols = right.columns.len();

    let (build, probe, build_key, probe_key, swapped) = if left.rows.len() <= right.rows.len() {
        (left, right, left_key_idx, right_key_idx, false)
    } else {
        (right, left, right_key_idx, left_key_idx, true)
    };
    let left_col_count = if swapped { orig_right_cols } else { orig_left_cols };
    let right_col_count = if swapped { orig_left_cols } else { orig_right_cols };

    let mut hash_map: HashMap<Value, Vec<usize>> = HashMap::new();
    for (i, row) in build.rows.iter().enumerate() {
        let key = row.get(build_key).cloned().unwrap_or(Value::Null);
        hash_map.entry(key).or_default().push(i);
    }

    let (columns, column_sources) = if swapped {
        merge_columns(&probe.columns, &probe.column_sources, &build.columns, &build.column_sources)
    } else {
        merge_columns(&build.columns, &build.column_sources, &probe.columns, &probe.column_sources)
    };

    let mut rows: Vec<Vec<Value>> = Vec::new();
    let mut probe_matched: Vec<bool> = vec![false; probe.rows.len()];
    let mut build_matched: Vec<bool> = vec![false; build.rows.len()];

    for (pi, probe_row) in probe.rows.iter().enumerate() {
        let key = probe_row.get(probe_key).cloned().unwrap_or(Value::Null);
        if let Some(build_idxs) = hash_map.get(&key) {
            for &bi in build_idxs {
                probe_matched[pi] = true;
                build_matched[bi] = true;
                let merged = if swapped {
                    merge_vals(probe_row, &build.rows[bi])
                } else {
                    merge_vals(&build.rows[bi], probe_row)
                };
                rows.push(merged);
            }
        }
    }

    let include_left = matches!(join_kind, JoinKind::Left | JoinKind::Full);
    let include_right = matches!(join_kind, JoinKind::Right | JoinKind::Full);

    if include_left {
        let nulls = vec![Value::Null; right_col_count];
        if swapped {
            for (pi, &matched) in probe_matched.iter().enumerate() {
                if !matched {
                    rows.push(merge_vals(&probe.rows[pi], &nulls));
                }
            }
        } else {
            for (bi, &matched) in build_matched.iter().enumerate() {
                if !matched {
                    rows.push(merge_vals(&build.rows[bi], &nulls));
                }
            }
        }
    }

    if include_right {
        let nulls = vec![Value::Null; left_col_count];
        if swapped {
            for (bi, &matched) in build_matched.iter().enumerate() {
                if !matched {
                    rows.push(merge_vals(&nulls, &build.rows[bi]));
                }
            }
        } else {
            for (pi, &matched) in probe_matched.iter().enumerate() {
                if !matched {
                    rows.push(merge_vals(&nulls, &probe.rows[pi]));
                }
            }
        }
    }

    Ok(QueryResult {
        columns,
        column_sources,
        rows,
        elapsed: std::time::Duration::default(),
        rows_affected: 0,
    })
}

pub(crate) fn nested_loop_join(
    left: QueryResult,
    right: QueryResult,
    condition: &Expression,
    join_kind: JoinKind,
    limit: Option<u64>,
) -> Result<QueryResult, RiverError> {
    let (columns, column_sources) = merge_columns(
        &left.columns, &left.column_sources,
        &right.columns, &right.column_sources,
    );
    let right_col_count = right.columns.len();
    let left_col_count = left.columns.len();
    let mut rows: Vec<Vec<Value>> = Vec::new();

    if join_kind == JoinKind::Cross {
        'outer: for l_row in &left.rows {
            for r_row in &right.rows {
                rows.push(merge_vals(l_row, r_row));
                if let Some(lim) = limit
                    && rows.len() >= lim as usize {
                        break 'outer;
                    }
            }
        }
        return Ok(QueryResult {
            columns,
            column_sources,
            rows,
            elapsed: std::time::Duration::default(),
            rows_affected: 0,
        });
    }

    let mut left_matched: Vec<bool> = vec![false; left.rows.len()];
    let mut right_matched: Vec<bool> = vec![false; right.rows.len()];

    for (li, l_row) in left.rows.iter().enumerate() {
        for (ri, r_row) in right.rows.iter().enumerate() {
            let merged = merge_vals(l_row, r_row);
            if eval_expr_bool(condition, &columns, &column_sources, &merged) {
                rows.push(merged.clone());
                left_matched[li] = true;
                right_matched[ri] = true;
            }
            if let Some(lim) = limit
                && rows.len() >= lim as usize {
                    return Ok(QueryResult {
                        columns,
                        column_sources,
                        rows,
                        elapsed: std::time::Duration::default(),
                        rows_affected: 0,
                    });
                }
        }
    }

    let include_left = matches!(join_kind, JoinKind::Left | JoinKind::Full);
    let include_right = matches!(join_kind, JoinKind::Right | JoinKind::Full);

    if include_left {
        for (li, &matched) in left_matched.iter().enumerate() {
            if !matched {
                let nulls = vec![Value::Null; right_col_count];
                rows.push(merge_vals(&left.rows[li], &nulls));
            }
        }
    }

    if include_right {
        for (ri, &matched) in right_matched.iter().enumerate() {
            if !matched {
                let nulls = vec![Value::Null; left_col_count];
                rows.push(merge_vals(&nulls, &right.rows[ri]));
            }
        }
    }

    Ok(QueryResult {
        columns,
        column_sources,
        rows,
        elapsed: std::time::Duration::default(),
        rows_affected: 0,
    })
}
