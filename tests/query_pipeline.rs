//! Integration tests for the full query pipeline (parse → plan → execute).
//!
//! Tests cover projections, filters, ordering, limits, distinct, and aggregation
//! across PostgreSQL, MySQL, and MongoDB adapters.

mod common;

use common::{
    assert_all_match, assert_columns, assert_ordered_asc, assert_ordered_desc, assert_row_count,
    assert_row_count_between, assert_row_count_gte, execute_river, TestContext,
};
use river::adapters::Value;

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Extract a numeric value from a Value, regardless of whether it was returned
/// as Int, Float, or String containing a number.
fn value_as_f64(val: &Value) -> Option<f64> {
    match val {
        Value::Int(i) => Some(*i as f64),
        Value::Float(f) => Some(*f),
        Value::String(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

/// Check whether a Value equals the given string, regardless of representation.
fn value_matches_str(val: &Value, expected: &str) -> bool {
    match val {
        Value::String(s) => s == expected,
        Value::Int(i) => i.to_string() == expected,
        Value::Float(f) => f.to_string() == expected,
        Value::Bool(b) => b.to_string() == expected,
        Value::Null => false,
    }
}

// ── Projection Tests ────────────────────────────────────────────────────────

#[tokio::test]
async fn pipeline_projection_two_columns() {
    let ctx = TestContext::new().await;
    let result = execute_river(&ctx, r#"find [name, salary] from users@pg limit 5"#)
        .await
        .expect("query failed");

    assert_columns(&result, &["name", "salary"]);
    assert_row_count(&result, 5);
}

#[tokio::test]
async fn pipeline_projection_single_column() {
    let ctx = TestContext::new().await;
    let result = execute_river(&ctx, r#"find [email] from users@pg limit 3"#)
        .await
        .expect("query failed");

    assert_columns(&result, &["email"]);
    assert_row_count(&result, 3);
}

#[tokio::test]
async fn pipeline_wildcard() {
    let ctx = TestContext::new().await;
    let result = execute_river(&ctx, r#"find * from users@pg limit 1"#)
        .await
        .expect("query failed");

    assert_row_count(&result, 1);
    assert!(
        result.columns.len() >= 5,
        "Expected at least 5 columns for wildcard query, got {}",
        result.columns.len()
    );
}

// ── Filter Tests ────────────────────────────────────────────────────────────

#[tokio::test]
async fn pipeline_filter_equals() {
    let ctx = TestContext::new().await;
    let result = execute_river(
        &ctx,
        r#"find [name, department] from users@pg where department = "Engineering""#,
    )
    .await
    .expect("query failed");

    assert_columns(&result, &["name", "department"]);
    assert_row_count_gte(&result, 1);
    assert_all_match(&result, "department", |v| {
        value_matches_str(v, "Engineering")
    });
}

#[tokio::test]
async fn pipeline_filter_greater_than() {
    let ctx = TestContext::new().await;
    let result = execute_river(
        &ctx,
        r#"find [name, salary] from users@pg where salary > 100000"#,
    )
    .await
    .expect("query failed");

    assert_columns(&result, &["name", "salary"]);
    assert_row_count_gte(&result, 1);
    assert_all_match(&result, "salary", |v| {
        let salary = value_as_f64(v).expect("salary should be numeric");
        salary > 100000.0
    });
}

#[tokio::test]
async fn pipeline_filter_and() {
    let ctx = TestContext::new().await;
    let result = execute_river(
        &ctx,
        r#"find [name] from users@pg where department = "Engineering" and status = "active""#,
    )
    .await
    .expect("query failed");

    assert_row_count_gte(&result, 1);
}

// ── Ordering Tests ──────────────────────────────────────────────────────────

#[tokio::test]
async fn pipeline_order_by_asc() {
    let ctx = TestContext::new().await;
    let result = execute_river(
        &ctx,
        r#"find [name, salary] from users@pg order by salary asc limit 20"#,
    )
    .await
    .expect("query failed");

    assert_columns(&result, &["name", "salary"]);
    assert_row_count(&result, 20);
    assert_ordered_asc(&result, "salary");
}

#[tokio::test]
async fn pipeline_order_by_desc() {
    let ctx = TestContext::new().await;
    let result = execute_river(
        &ctx,
        r#"find [name, salary] from users@pg order by salary desc limit 20"#,
    )
    .await
    .expect("query failed");

    assert_columns(&result, &["name", "salary"]);
    assert_row_count(&result, 20);
    assert_ordered_desc(&result, "salary");
}

// ── Limit / Offset Tests ────────────────────────────────────────────────────

#[tokio::test]
async fn pipeline_limit() {
    let ctx = TestContext::new().await;
    let result = execute_river(&ctx, r#"find [name] from users@pg limit 7"#)
        .await
        .expect("query failed");

    assert_columns(&result, &["name"]);
    assert_row_count(&result, 7);
}

#[tokio::test]
async fn pipeline_limit_offset() {
    let ctx = TestContext::new().await;

    let first_page = execute_river(
        &ctx,
        r#"find [id, name] from users@pg order by id asc limit 5"#,
    )
    .await
    .expect("first page query failed");

    let second_page = execute_river(
        &ctx,
        r#"find [id, name] from users@pg order by id asc limit 5 offset 5"#,
    )
    .await
    .expect("second page query failed");

    assert_row_count(&first_page, 5);
    assert_row_count(&second_page, 5);

    // The two pages should contain different rows
    let first_names: Vec<&Value> = first_page.rows.iter().map(|r| &r[1]).collect();
    let second_names: Vec<&Value> = second_page.rows.iter().map(|r| &r[1]).collect();
    assert_ne!(
        first_names, second_names,
        "First and second page returned identical rows — offset may not be working"
    );
}

// ── Distinct Test ───────────────────────────────────────────────────────────

#[tokio::test]
async fn pipeline_distinct() {
    let ctx = TestContext::new().await;
    let result = execute_river(&ctx, r#"find distinct [status] from users@pg"#)
        .await
        .expect("query failed");

    // status has values: active, inactive, suspended, pending — expect between 1 and 10
    assert_row_count_between(&result, 1, 10);

    // All values should be unique
    let values: Vec<&Value> = result.rows.iter().map(|r| &r[0]).collect();
    let unique_count = {
        let mut seen = std::collections::HashSet::new();
        values.iter().filter(|v| seen.insert(format!("{:?}", v))).count()
    };
    assert_eq!(
        values.len(),
        unique_count,
        "DISTINCT query returned duplicate values"
    );
}

// ── Between / Like Tests ────────────────────────────────────────────────────

#[tokio::test]
async fn pipeline_between() {
    let ctx = TestContext::new().await;
    let result = execute_river(
        &ctx,
        r#"find [name, salary] from users@pg where salary between 50000 and 60000"#,
    )
    .await
    .expect("query failed");

    assert_columns(&result, &["name", "salary"]);
    assert_row_count_gte(&result, 1);
    assert_all_match(&result, "salary", |v| {
        let salary = value_as_f64(v).expect("salary should be numeric");
        (50_000.00..=60_000.00).contains(&salary)
    });
}

#[tokio::test]
async fn pipeline_like() {
    let ctx = TestContext::new().await;
    let result = execute_river(
        &ctx,
        r#"find [name] from users@pg where name like "Alice%""#,
    )
    .await
    .expect("query failed");

    assert_row_count_gte(&result, 1);
    assert_all_match(&result, "name", |v| match v {
        Value::String(s) => s.starts_with("Alice"),
        _ => false,
    });
}

// ── Aggregation Test ────────────────────────────────────────────────────────

#[tokio::test]
async fn pipeline_count() {
    let ctx = TestContext::new().await;
    let result = execute_river(
        &ctx,
        r#"find [department, count(*) as cnt] from users@pg group by department"#,
    )
    .await
    .expect("query failed");

    // 10 departments seeded, but allow some flexibility
    assert_row_count_between(&result, 5, 15);
}

// ── Cross-Adapter Pipeline Tests ────────────────────────────────────────────

#[tokio::test]
async fn pipeline_mysql_filter_and_limit() {
    let ctx = TestContext::new().await;
    let result = execute_river(
        &ctx,
        r#"find [name, category] from products@mysql where category = "Electronics" limit 10"#,
    )
    .await
    .expect("query failed");

    assert_columns(&result, &["name", "category"]);
    assert_row_count(&result, 10);
    assert_all_match(&result, "category", |v| {
        value_matches_str(v, "Electronics")
    });
}

#[tokio::test]
async fn pipeline_mongo_filter_and_limit() {
    let ctx = TestContext::new().await;
    let result = execute_river(
        &ctx,
        r#"find [name, status] from users@mongo where status = "active" limit 10"#,
    )
    .await
    .expect("query failed");

    assert_columns(&result, &["name", "status"]);
    assert_row_count(&result, 10);
    assert_all_match(&result, "status", |v| value_matches_str(v, "active"));
}
