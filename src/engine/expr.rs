use crate::adapters::Value;
use crate::lang::ast::*;
use tracing::warn;

pub(crate) fn cmp_values(a: &Value, b: &Value) -> std::cmp::Ordering {
    match (a, b) {
        (Value::Null, Value::Null) => std::cmp::Ordering::Equal,
        (Value::Null, _) => std::cmp::Ordering::Less,
        (_, Value::Null) => std::cmp::Ordering::Greater,
        (Value::String(a), Value::String(b)) => a.cmp(b),
        (Value::Int(a), Value::Int(b)) => a.cmp(b),
        (Value::Float(a), Value::Float(b)) => a
            .partial_cmp(b)
            .unwrap_or_else(|| {
                warn!("NaN comparison: treating {:?} <=> {:?} as equal", a, b);
                std::cmp::Ordering::Equal
            }),
        (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
        (Value::Int(a), Value::Float(b)) => (*a as f64)
            .partial_cmp(b)
            .unwrap_or_else(|| {
                warn!("NaN comparison: treating {} <=> {:?} as equal", a, b);
                std::cmp::Ordering::Equal
            }),
        (Value::Float(a), Value::Int(b)) => a
            .partial_cmp(&(*b as f64))
            .unwrap_or_else(|| {
                warn!("NaN comparison: treating {:?} <=> {} as equal", a, b);
                std::cmp::Ordering::Equal
            }),
        _ => std::cmp::Ordering::Equal,
    }
}

pub(crate) fn value_to_string(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
    }
}

pub(crate) fn is_truthy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Null => false,
        Value::Int(i) => *i != 0,
        Value::Float(f) => *f != 0.0,
        Value::String(s) => !s.is_empty(),
    }
}

pub(crate) fn eval_binary(op: &BinaryOp, left: &Value, right: &Value) -> Value {
    match op {
        BinaryOp::And => Value::Bool(is_truthy(left) && is_truthy(right)),
        BinaryOp::Or => Value::Bool(is_truthy(left) || is_truthy(right)),
        BinaryOp::Eq => Value::Bool(left == right),
        BinaryOp::Neq => Value::Bool(left != right),
        BinaryOp::Gt => Value::Bool(cmp_values(left, right) == std::cmp::Ordering::Greater),
        BinaryOp::Gte => {
            Value::Bool(cmp_values(left, right) != std::cmp::Ordering::Less)
        }
        BinaryOp::Lt => Value::Bool(cmp_values(left, right) == std::cmp::Ordering::Less),
        BinaryOp::Lte => {
            Value::Bool(cmp_values(left, right) != std::cmp::Ordering::Greater)
        }
        BinaryOp::Like | BinaryOp::ILike => {
            if let (Value::String(s), Value::String(pat)) = (left, right) {
                let pattern = pat.replace(['%', '_'], "");
                Value::Bool(s.contains(&pattern))
            } else {
                Value::Bool(false)
            }
        }
        BinaryOp::Add => arith_op(left, right, |a, b| a.checked_add(b), |a, b| a + b),
        BinaryOp::Sub => arith_op(left, right, |a, b| a.checked_sub(b), |a, b| a - b),
        BinaryOp::Mul => arith_op(left, right, |a, b| a.checked_mul(b), |a, b| a * b),
        BinaryOp::Div => arith_op(left, right, |a, b| a.checked_div(b), |a, b| a / b),
        BinaryOp::Mod => arith_op(left, right, |a, b| a.checked_rem(b), |a, b| a % b),
        _ => Value::Null,
    }
}

pub(crate) fn arith_op<F1, F2>(left: &Value, right: &Value, int_op: F1, float_op: F2) -> Value
where
    F1: Fn(i64, i64) -> Option<i64>,
    F2: Fn(f64, f64) -> f64,
{
    match (left, right) {
        (Value::Int(a), Value::Int(b)) => int_op(*a, *b).map(Value::Int).unwrap_or(Value::Null),
        (Value::Float(a), Value::Float(b)) => Value::Float(float_op(*a, *b)),
        (Value::Int(a), Value::Float(b)) => Value::Float(float_op(*a as f64, *b)),
        (Value::Float(a), Value::Int(b)) => Value::Float(float_op(*a, *b as f64)),
        _ => Value::Null,
    }
}

pub(crate) fn eval_unary(op: &UnaryOp, val: &Value) -> Value {
    match op {
        UnaryOp::Not => Value::Bool(!is_truthy(val)),
        UnaryOp::Neg => match val {
            Value::Int(i) => Value::Int(-i),
            Value::Float(f) => Value::Float(-f),
            _ => Value::Null,
        },
    }
}

pub(crate) fn cast_value(v: &Value, target: &DataType) -> Value {
    match target {
        DataType::Integer => match v {
            Value::Int(_) => v.clone(),
            Value::Float(f) => Value::Int(*f as i64),
            Value::String(s) => s.parse().map(Value::Int).unwrap_or_else(|e| {
                warn!("failed to cast string to int: {} ({})", s, e);
                Value::Null
            }),
            Value::Bool(b) => Value::Int(if *b { 1 } else { 0 }),
            _ => Value::Null,
        },
        DataType::Float => match v {
            Value::Float(_) => v.clone(),
            Value::Int(i) => Value::Float(*i as f64),
            Value::String(s) => s.parse().map(Value::Float).unwrap_or_else(|e| {
                warn!("failed to cast string to float: {} ({})", s, e);
                Value::Null
            }),
            _ => Value::Null,
        },
        DataType::String => match v {
            Value::String(_) => v.clone(),
            Value::Null => Value::Null,
            Value::Int(i) => Value::String(i.to_string()),
            Value::Float(f) => Value::String(f.to_string()),
            Value::Bool(b) => Value::String(b.to_string()),
        },
        DataType::DateTime | DataType::Json => match v {
            Value::String(s) => Value::String(s.clone()),
            Value::Int(i) => Value::String(i.to_string()),
            Value::Float(f) => Value::String(f.to_string()),
            Value::Bool(b) => Value::String(b.to_string()),
            Value::Null => Value::Null,
        },
        DataType::Boolean => match v {
            Value::Bool(_) => v.clone(),
            _ => Value::Bool(is_truthy(v)),
        },
    }
}

pub(crate) fn eval_expr_bool(expr: &Expression, columns: &[String], column_sources: &[Option<String>], row: &[Value]) -> bool {
    matches!(eval_expr(expr, columns, column_sources, row), Value::Bool(true))
}

pub fn eval_expr(expr: &Expression, columns: &[String], column_sources: &[Option<String>], row: &[Value]) -> Value {
    match expr {
        Expression::String(s) => Value::String(s.clone()),
        Expression::Number(n) => Value::Float(*n),
        Expression::Integer(i) => Value::Int(*i),
        Expression::Boolean(b) => Value::Bool(*b),
        Expression::Null => Value::Null,
        Expression::Ident(name) => {
            columns
                .iter()
                .position(|c| c == name)
                .and_then(|i| row.get(i))
                .cloned()
                .unwrap_or(Value::Null)
        }
        Expression::QualifiedIdent { table, field } => {
            let idx = columns
                .iter()
                .enumerate()
                .position(|(i, c)| {
                    c == field && column_sources.get(i).and_then(|s| s.as_deref()) == Some(table.as_str())
                })
                .or_else(|| columns.iter().position(|c| c == field));
            idx.and_then(|i| row.get(i)).cloned().unwrap_or(Value::Null)
        }
        Expression::BinaryOp { op, left, right } => {
            let l = eval_expr(left, columns, column_sources, row);
            let r = eval_expr(right, columns, column_sources, row);
            eval_binary(op, &l, &r)
        }
        Expression::UnaryOp { op, expr } => {
            let v = eval_expr(expr, columns, column_sources, row);
            eval_unary(op, &v)
        }
        Expression::Between {
            expr,
            low,
            high,
        } => {
            let v = eval_expr(expr, columns, column_sources, row);
            let lo = eval_expr(low, columns, column_sources, row);
            let hi = eval_expr(high, columns, column_sources, row);
            Value::Bool(cmp_values(&v, &lo) != std::cmp::Ordering::Less
                && cmp_values(&v, &hi) != std::cmp::Ordering::Greater)
        }
        Expression::Case {
            expr: case_val,
            whens,
            else_expr,
        } => {
            for (when, then) in whens {
                let match_val = if let Some(cv) = case_val {
                    let cv_val = eval_expr(cv, columns, column_sources, row);
                    let when_val = eval_expr(when, columns, column_sources, row);
                    cv_val == when_val
                } else {
                    eval_expr_bool(when, columns, column_sources, row)
                };
                if match_val {
                    return eval_expr(then, columns, column_sources, row);
                }
            }
            else_expr
                .as_ref()
                .map(|e| eval_expr(e, columns, column_sources, row))
                .unwrap_or(Value::Null)
        }
        Expression::Cast { expr, target } => {
            let v = eval_expr(expr, columns, column_sources, row);
            cast_value(&v, target)
        }
        Expression::Array(_) => Value::String("[...]".into()),
        Expression::Object(_) => Value::String("{...}".into()),
        Expression::FnCall { name, args } => {
            let evaluated: Vec<Value> = args
                .iter()
                .map(|a| eval_expr(a, columns, column_sources, row))
                .collect();
            match name.to_lowercase().as_str() {
                "concat" => {
                    let s: String = evaluated
                        .iter()
                        .map(value_to_string)
                        .collect();
                    Value::String(s)
                }
                "now" | "current_timestamp" => {
                    let now = time::OffsetDateTime::now_utc();
                    Value::String(
                        now.format(&time::macros::format_description!(
                            "[year]-[month]-[day] [hour]:[minute]:[second]"
                        ))
                        .unwrap_or_else(|_| now.to_string()),
                    )
                }
                "coalesce" => {
                    for v in &evaluated {
                        if !matches!(v, Value::Null) {
                            return v.clone();
                        }
                    }
                    Value::Null
                }
                "replace" => {
                    if evaluated.len() >= 3 {
                        let s = value_to_string(&evaluated[0]);
                        let from = value_to_string(&evaluated[1]);
                        let to = value_to_string(&evaluated[2]);
                        Value::String(s.replace(&from, &to))
                    } else {
                        Value::Null
                    }
                }
                _ => {
                    let s: String = evaluated
                        .iter()
                        .map(value_to_string)
                        .collect::<Vec<_>>()
                        .join(", ");
                    Value::String(s)
                }
            }
        }
        _ => Value::Null,
    }
}
