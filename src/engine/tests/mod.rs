use std::collections::HashMap;

use super::planner::{plan_statement, translate_insert_mongo, PlanNode};
use crate::connection::AiConfig;
use super::translator::{
    MSSQLDialect, MySQLDialect, PostgresDialect, SQLiteDialect, SqlDialect,
    translate_alter_table, translate_data_type, translate_drop_table, translate_expr,
    translate_query, translate_query_mongo, translate_statement_sql,
};
use crate::connection::DatabaseKind;
use crate::lang::ast::*;

fn make_query(projection: Vec<Projection>, sources: Vec<Source>, filter: Option<Expression>) -> Query {
    Query {
        projection,
        sources,
        filter,
        ..Query::default()
    }
}

fn table_source(name: &str) -> Source {
    Source {
        name: name.to_string(),
        alias: None,
        connection: None,
        schema: None,
        kind: SourceKind::Table(name.to_string()),
    }
}

// ── SQL: find users ───────────────────────────────────────────────────────

#[test]
fn translate_find_users_postgres() {
    let q = make_query(vec![], vec![table_source("users")], None);
    let dialect = PostgresDialect;
    let sql = translate_query(&q, &dialect);
    assert_eq!(sql, r#"SELECT * FROM "users""#);
}

#[test]
fn translate_find_users_mysql() {
    let q = make_query(vec![], vec![table_source("users")], None);
    let dialect = MySQLDialect;
    let sql = translate_query(&q, &dialect);
    assert_eq!(sql, "SELECT * FROM `users`");
}

#[test]
fn translate_find_users_mssql() {
    let q = make_query(vec![], vec![table_source("users")], None);
    let dialect = MSSQLDialect;
    let sql = translate_query(&q, &dialect);
    assert_eq!(sql, "SELECT * FROM [users]");
}

// ── SQL: find users where age > 21 ────────────────────────────────────────

#[test]
fn translate_filter_postgres() {
    let filter = Expression::BinaryOp {
        op: BinaryOp::Gt,
        left: Box::new(Expression::Ident("age".into())),
        right: Box::new(Expression::Integer(21)),
    };
    let q = make_query(vec![], vec![table_source("users")], Some(filter));
    let dialect = PostgresDialect;
    let sql = translate_query(&q, &dialect);
    assert_eq!(sql, r#"SELECT * FROM "users" WHERE ("age" > 21)"#);
}

#[test]
fn translate_filter_mysql() {
    let filter = Expression::BinaryOp {
        op: BinaryOp::Gt,
        left: Box::new(Expression::Ident("age".into())),
        right: Box::new(Expression::Integer(21)),
    };
    let q = make_query(vec![], vec![table_source("users")], Some(filter));
    let dialect = MySQLDialect;
    let sql = translate_query(&q, &dialect);
    assert_eq!(sql, "SELECT * FROM `users` WHERE (`age` > 21)");
}

#[test]
fn translate_filter_mssql() {
    let filter = Expression::BinaryOp {
        op: BinaryOp::Gt,
        left: Box::new(Expression::Ident("age".into())),
        right: Box::new(Expression::Integer(21)),
    };
    let q = make_query(vec![], vec![table_source("users")], Some(filter));
    let dialect = MSSQLDialect;
    let sql = translate_query(&q, &dialect);
    assert_eq!(sql, "SELECT * FROM [users] WHERE ([age] > 21)");
}

// ── SQL: projection ───────────────────────────────────────────────────────

#[test]
fn translate_projection_postgres() {
    let proj = vec![
        Projection::Expr(Expression::Ident("name".into()), None),
        Projection::Expr(Expression::Ident("email".into()), None),
    ];
    let q = make_query(proj, vec![table_source("users")], None);
    let dialect = PostgresDialect;
    let sql = translate_query(&q, &dialect);
    assert_eq!(sql, r#"SELECT "name", "email" FROM "users""#);
}

#[test]
fn translate_wildcard_projection() {
    let proj = vec![Projection::Wildcard];
    let q = make_query(proj, vec![table_source("users")], None);
    let dialect = PostgresDialect;
    let sql = translate_query(&q, &dialect);
    assert_eq!(sql, r#"SELECT * FROM "users""#);
}

// ── SQL: aggregation ──────────────────────────────────────────────────────

#[test]
fn translate_group_by_postgres() {
    let proj = vec![
        Projection::Expr(Expression::Ident("department".into()), None),
        Projection::Expr(
            Expression::Aggregate {
                name: "count".into(),
                distinct: false,
                args: vec![],
            },
            Some("total".into()),
        ),
    ];
    let mut q = make_query(proj, vec![table_source("employees")], None);
    q.group_by = vec![Expression::Ident("department".into())];

    let dialect = PostgresDialect;
    let sql = translate_query(&q, &dialect);
    assert_eq!(
        sql,
        r#"SELECT "department", COUNT(*) AS "total" FROM "employees" GROUP BY "department""#
    );
}

#[test]
fn translate_sum_postgres() {
    let proj = vec![
        Projection::Expr(Expression::Ident("department".into()), None),
        Projection::Expr(
            Expression::Aggregate {
                name: "sum".into(),
                distinct: false,
                args: vec![Expression::Ident("salary".into())],
            },
            Some("total_salary".into()),
        ),
    ];
    let mut q = make_query(proj, vec![table_source("employees")], None);
    q.group_by = vec![Expression::Ident("department".into())];

    let dialect = PostgresDialect;
    let sql = translate_query(&q, &dialect);
    assert_eq!(
        sql,
        r#"SELECT "department", SUM("salary") AS "total_salary" FROM "employees" GROUP BY "department""#
    );
}

// ── SQL: limit/offset ─────────────────────────────────────────────────────

#[test]
fn translate_limit_postgres() {
    let mut q = make_query(vec![], vec![table_source("users")], None);
    q.limit = Some(10);
    let dialect = PostgresDialect;
    let sql = translate_query(&q, &dialect);
    assert_eq!(sql, r#"SELECT * FROM "users" LIMIT 10"#);
}

#[test]
fn translate_limit_offset_postgres() {
    let mut q = make_query(vec![], vec![table_source("users")], None);
    q.limit = Some(10);
    q.offset = Some(5);
    let dialect = PostgresDialect;
    let sql = translate_query(&q, &dialect);
    assert_eq!(sql, r#"SELECT * FROM "users" LIMIT 10 OFFSET 5"#);
}

#[test]
fn translate_limit_offset_mysql() {
    let mut q = make_query(vec![], vec![table_source("users")], None);
    q.limit = Some(10);
    q.offset = Some(5);
    let dialect = MySQLDialect;
    let sql = translate_query(&q, &dialect);
    assert_eq!(sql, "SELECT * FROM `users` LIMIT 5, 10");
}

#[test]
fn translate_limit_offset_mssql() {
    let mut q = make_query(vec![], vec![table_source("users")], None);
    q.limit = Some(10);
    q.offset = Some(5);
    let dialect = MSSQLDialect;
    let sql = translate_query(&q, &dialect);
    assert_eq!(
        sql,
        "SELECT * FROM [users] OFFSET 5 ROWS FETCH NEXT 10 ROWS ONLY"
    );
}

// ── SQL: literals ─────────────────────────────────────────────────────────

#[test]
fn translate_literals_postgres() {
    let filter = Expression::BinaryOp {
        op: BinaryOp::And,
        left: Box::new(Expression::BinaryOp {
            op: BinaryOp::Eq,
            left: Box::new(Expression::Ident("name".into())),
            right: Box::new(Expression::String("Alice".into())),
        }),
        right: Box::new(Expression::BinaryOp {
            op: BinaryOp::Gte,
            left: Box::new(Expression::Ident("age".into())),
            right: Box::new(Expression::Integer(30)),
        }),
    };
    let q = make_query(vec![], vec![table_source("users")], Some(filter));
    let dialect = PostgresDialect;
    let sql = translate_query(&q, &dialect);
    assert_eq!(
        sql,
        r#"SELECT * FROM "users" WHERE (("name" = 'Alice') AND ("age" >= 30))"#
    );
}

#[test]
fn translate_boolean_null_literals() {
    let filter = Expression::BinaryOp {
        op: BinaryOp::And,
        left: Box::new(Expression::BinaryOp {
            op: BinaryOp::Eq,
            left: Box::new(Expression::Ident("active".into())),
            right: Box::new(Expression::Boolean(true)),
        }),
        right: Box::new(Expression::BinaryOp {
            op: BinaryOp::Neq,
            left: Box::new(Expression::Ident("deleted_at".into())),
            right: Box::new(Expression::Null),
        }),
    };
    let q = make_query(vec![], vec![table_source("users")], Some(filter));
    let dialect = PostgresDialect;
    let sql = translate_query(&q, &dialect);
    assert_eq!(
        sql,
        r#"SELECT * FROM "users" WHERE (("active" = TRUE) AND "deleted_at" IS NOT NULL)"#
    );
}

#[test]
fn translate_is_null_postgres() {
    let filter = Expression::BinaryOp {
        op: BinaryOp::Eq,
        left: Box::new(Expression::Ident("deleted_at".into())),
        right: Box::new(Expression::Null),
    };
    let q = make_query(vec![], vec![table_source("users")], Some(filter));
    let dialect = PostgresDialect;
    let sql = translate_query(&q, &dialect);
    assert_eq!(
        sql,
        r#"SELECT * FROM "users" WHERE "deleted_at" IS NULL"#
    );
}

#[test]
fn translate_is_not_null_mysql() {
    let filter = Expression::BinaryOp {
        op: BinaryOp::Neq,
        left: Box::new(Expression::Ident("email_verified".into())),
        right: Box::new(Expression::Null),
    };
    let q = make_query(vec![], vec![table_source("users")], Some(filter));
    let dialect = MySQLDialect;
    let sql = translate_query(&q, &dialect);
    assert_eq!(
        sql,
        "SELECT * FROM `users` WHERE `email_verified` IS NOT NULL"
    );
}

#[test]
fn translate_float_literal() {
    let filter = Expression::BinaryOp {
        op: BinaryOp::Gt,
        left: Box::new(Expression::Ident("score".into())),
        right: Box::new(Expression::Number(3.5)),
    };
    let q = make_query(vec![], vec![table_source("games")], Some(filter));
    let dialect = PostgresDialect;
    let sql = translate_query(&q, &dialect);
    assert_eq!(sql, r#"SELECT * FROM "games" WHERE ("score" > 3.5)"#);
}

// ── SQL: LIKE / IN ────────────────────────────────────────────────────────

#[test]
fn translate_like_postgres() {
    let filter = Expression::BinaryOp {
        op: BinaryOp::Like,
        left: Box::new(Expression::Ident("name".into())),
        right: Box::new(Expression::String("A%".into())),
    };
    let q = make_query(vec![], vec![table_source("users")], Some(filter));
    let dialect = PostgresDialect;
    let sql = translate_query(&q, &dialect);
    assert_eq!(
        sql,
        r#"SELECT * FROM "users" WHERE "name" LIKE 'A%'"#
    );
}

#[test]
fn translate_ilike_postgres() {
    let filter = Expression::BinaryOp {
        op: BinaryOp::ILike,
        left: Box::new(Expression::Ident("name".into())),
        right: Box::new(Expression::String("a%".into())),
    };
    let q = make_query(vec![], vec![table_source("users")], Some(filter));
    let dialect = PostgresDialect;
    let sql = translate_query(&q, &dialect);
    assert_eq!(
        sql,
        r#"SELECT * FROM "users" WHERE "name" ILIKE 'a%'"#
    );
}

#[test]
fn translate_ilike_mysql_fallback() {
    let filter = Expression::BinaryOp {
        op: BinaryOp::ILike,
        left: Box::new(Expression::Ident("name".into())),
        right: Box::new(Expression::String("a%".into())),
    };
    let q = make_query(vec![], vec![table_source("users")], Some(filter));
    let dialect = MySQLDialect;
    let sql = translate_query(&q, &dialect);
    assert_eq!(
        sql,
        "SELECT * FROM `users` WHERE `name` LIKE 'a%'"
    );
}

// ── SQL: order by ─────────────────────────────────────────────────────────

#[test]
fn translate_order_by_postgres() {
    let mut q = make_query(vec![], vec![table_source("users")], None);
    q.order_by = vec![OrderBy {
        expr: Expression::Ident("age".into()),
        direction: OrderDir::Desc,
        nulls: NullsOrder::Default,
    }];
    let dialect = PostgresDialect;
    let sql = translate_query(&q, &dialect);
    assert_eq!(
        sql,
        r#"SELECT * FROM "users" ORDER BY "age" DESC"#
    );
}

#[test]
fn translate_order_by_with_nulls_postgres() {
    let mut q = make_query(vec![], vec![table_source("users")], None);
    q.order_by = vec![OrderBy {
        expr: Expression::Ident("deleted_at".into()),
        direction: OrderDir::Asc,
        nulls: NullsOrder::First,
    }];
    let dialect = PostgresDialect;
    let sql = translate_query(&q, &dialect);
    assert_eq!(
        sql,
        r#"SELECT * FROM "users" ORDER BY "deleted_at" ASC NULLS FIRST"#
    );
}

// ── SQL: distinct ─────────────────────────────────────────────────────────

#[test]
fn translate_distinct_mysql() {
    let mut q = make_query(
        vec![Projection::Expr(
            Expression::Ident("status".into()),
            None,
        )],
        vec![table_source("users")],
        None,
    );
    q.distinct = true;
    let dialect = MySQLDialect;
    let sql = translate_query(&q, &dialect);
    assert_eq!(sql, "SELECT DISTINCT `status` FROM `users`");
}

// ── SQL: joins ────────────────────────────────────────────────────────────

#[test]
fn translate_inner_join_postgres() {
    let mut q = make_query(vec![], vec![table_source("users")], None);
    q.joins = vec![Join {
        kind: JoinKind::Inner,
        source: table_source("orders"),
        alias: None,
        condition: Some(Expression::BinaryOp {
            op: BinaryOp::Eq,
            left: Box::new(Expression::QualifiedIdent {
                table: "users".into(),
                field: "id".into(),
            }),
            right: Box::new(Expression::QualifiedIdent {
                table: "orders".into(),
                field: "user_id".into(),
            }),
        }),
    }];
    let dialect = PostgresDialect;
    let sql = translate_query(&q, &dialect);
    assert_eq!(
        sql,
        r#"SELECT * FROM "users" INNER JOIN "orders" ON ("users"."id" = "orders"."user_id")"#
    );
}

#[test]
fn translate_left_join_mysql() {
    let mut q = make_query(vec![], vec![table_source("customers")], None);
    q.joins = vec![Join {
        kind: JoinKind::Left,
        source: table_source("orders"),
        alias: None,
        condition: Some(Expression::BinaryOp {
            op: BinaryOp::Eq,
            left: Box::new(Expression::QualifiedIdent {
                table: "customers".into(),
                field: "id".into(),
            }),
            right: Box::new(Expression::QualifiedIdent {
                table: "orders".into(),
                field: "cust_id".into(),
            }),
        }),
    }];
    let dialect = MySQLDialect;
    let sql = translate_query(&q, &dialect);
    assert_eq!(
        sql,
        "SELECT * FROM `customers` LEFT JOIN `orders` ON (`customers`.`id` = `orders`.`cust_id`)"
    );
}

// ── SQL: expression literals ──────────────────────────────────────────────

#[test]
fn translate_string_with_quote() {
    let filter = Expression::BinaryOp {
        op: BinaryOp::Eq,
        left: Box::new(Expression::Ident("name".into())),
        right: Box::new(Expression::String("O'Brien".into())),
    };
    let q = make_query(vec![], vec![table_source("users")], Some(filter));
    let dialect = PostgresDialect;
    let sql = translate_query(&q, &dialect);
    assert_eq!(
        sql,
        r#"SELECT * FROM "users" WHERE ("name" = 'O''Brien')"#
    );
}

// ── MongoDB translator ────────────────────────────────────────────────────

#[test]
fn translate_find_users_mongo() {
    let q = make_query(vec![], vec![table_source("users")], None);
    let json = translate_query_mongo(&q, "mydb");
    assert_eq!(json["database"], "mydb");
    assert_eq!(json["collection"], "users");
    let pipeline = json["pipeline"].as_array().unwrap();
    assert!(pipeline.is_empty());
}

#[test]
fn translate_filter_gt_mongo() {
    let filter = Expression::BinaryOp {
        op: BinaryOp::Gt,
        left: Box::new(Expression::Ident("age".into())),
        right: Box::new(Expression::Integer(21)),
    };
    let q = make_query(vec![], vec![table_source("users")], Some(filter));
    let json = translate_query_mongo(&q, "mydb");
    let pipeline = json["pipeline"].as_array().unwrap();
    assert_eq!(pipeline.len(), 1);
    assert_eq!(
        pipeline[0]["$match"]["age"]["$gt"],
        serde_json::Value::Number(serde_json::Number::from(21))
    );
}

#[test]
fn translate_aggregate_mongo() {
    let proj = vec![
        Projection::Expr(Expression::Ident("department".into()), None),
        Projection::Expr(
            Expression::Aggregate {
                name: "count".into(),
                distinct: false,
                args: vec![],
            },
            Some("total".into()),
        ),
    ];
    let mut q = make_query(proj, vec![table_source("employees")], None);
    q.group_by = vec![Expression::Ident("department".into())];
    let json = translate_query_mongo(&q, "mydb");
    let pipeline = json["pipeline"].as_array().unwrap();
    assert_eq!(pipeline.len(), 1);
    assert!(pipeline[0]["$group"].is_object());
    assert_eq!(
        pipeline[0]["$group"]["_id"],
        serde_json::Value::String("$department".into())
    );
    assert_eq!(
        pipeline[0]["$group"]["total"]["$count"],
        serde_json::Value::String("count".into())
    );
}

#[test]
fn translate_limit_skip_mongo() {
    let mut q = make_query(vec![], vec![table_source("users")], None);
    q.limit = Some(10);
    q.offset = Some(5);
    let json = translate_query_mongo(&q, "mydb");
    let pipeline = json["pipeline"].as_array().unwrap();

    let has_limit = pipeline.iter().any(|s| s["$limit"].is_number());
    let has_skip = pipeline.iter().any(|s| s["$skip"].is_number());
    assert!(has_limit, "pipeline should contain $limit");
    assert!(has_skip, "pipeline should contain $skip");
}

#[test]
fn translate_and_filter_mongo() {
    let filter = Expression::BinaryOp {
        op: BinaryOp::And,
        left: Box::new(Expression::BinaryOp {
            op: BinaryOp::Gt,
            left: Box::new(Expression::Ident("age".into())),
            right: Box::new(Expression::Integer(21)),
        }),
        right: Box::new(Expression::BinaryOp {
            op: BinaryOp::Eq,
            left: Box::new(Expression::Ident("status".into())),
            right: Box::new(Expression::String("active".into())),
        }),
    };
    let q = make_query(vec![], vec![table_source("users")], Some(filter));
    let json = translate_query_mongo(&q, "mydb");
    let pipeline = json["pipeline"].as_array().unwrap();
    assert_eq!(pipeline.len(), 1);
    let match_stage = &pipeline[0]["$match"];
    assert!(match_stage["$and"].is_array());
}

// ── Dialect quoting ───────────────────────────────────────────────────────

#[test]
fn quote_ident_postgres() {
    assert_eq!(PostgresDialect.quote_ident("my_table"), r#""my_table""#);
}

#[test]
fn quote_ident_mysql() {
    assert_eq!(MySQLDialect.quote_ident("my_table"), "`my_table`");
}

#[test]
fn quote_ident_mssql() {
    assert_eq!(MSSQLDialect.quote_ident("my_table"), "[my_table]");
}

#[test]
fn quote_ident_with_special_chars() {
    assert_eq!(
        PostgresDialect.quote_ident(r#"weird"name"#),
        r#""weird""name""#
    );
}

// ── BETWEEN ───────────────────────────────────────────────────────────────

#[test]
fn translate_between_postgres() {
    let filter = Expression::Between {
        expr: Box::new(Expression::Ident("age".into())),
        low: Box::new(Expression::Integer(18)),
        high: Box::new(Expression::Integer(65)),
    };
    let q = make_query(vec![], vec![table_source("users")], Some(filter));
    let dialect = PostgresDialect;
    let sql = translate_query(&q, &dialect);
    assert_eq!(
        sql,
        r#"SELECT * FROM "users" WHERE "age" BETWEEN 18 AND 65"#
    );
}

// ── INSERT / UPDATE / DELETE ──────────────────────────────────────────────

#[test]
fn translate_insert_postgres() {
    let insert = Insert {
        schema: None,
        table: "users".into(),
        connection: None,
        columns: Some(vec!["name".into(), "age".into()]),
        rows: vec![vec![
            ("name".into(), Expression::String("Alice".into())),
            ("age".into(), Expression::Integer(30)),
        ]],
        query: None,
    };
    let sql = translate_statement_sql(&Statement::Insert(insert), &PostgresDialect);
    assert_eq!(
        sql,
        r#"INSERT INTO "users" ("name", "age") VALUES ('Alice', 30)"#
    );
}

#[test]
fn translate_update_mysql() {
    let update = Update {
        schema: None,
        table: "users".into(),
        connection: None,
        assignments: vec![
            ("status".into(), Expression::String("inactive".into())),
        ],
        filter: Some(Expression::BinaryOp {
            op: BinaryOp::Eq,
            left: Box::new(Expression::Ident("id".into())),
            right: Box::new(Expression::Integer(42)),
        }),
    };
    let sql = translate_statement_sql(&Statement::Update(update), &MySQLDialect);
    assert_eq!(
        sql,
        "UPDATE `users` SET `status` = 'inactive' WHERE (`id` = 42)"
    );
}

#[test]
fn translate_delete_mssql() {
    let delete = Delete {
        schema: None,
        table: "users".into(),
        connection: None,
        filter: Some(Expression::BinaryOp {
            op: BinaryOp::Lt,
            left: Box::new(Expression::Ident("age".into())),
            right: Box::new(Expression::Integer(18)),
        }),
    };
    let sql = translate_statement_sql(&Statement::Delete(delete), &MSSQLDialect);
    assert_eq!(
        sql,
        "DELETE FROM [users] WHERE ([age] < 18)"
    );
}

// ── CAST ──────────────────────────────────────────────────────────────────

#[test]
fn translate_cast_postgres() {
    let expr = Expression::Cast {
        expr: Box::new(Expression::String("42".into())),
        target: DataType::Integer,
    };
    let dialect = PostgresDialect;
    let sql = translate_expr(&expr, &dialect);
    assert_eq!(sql, "CAST('42' AS INTEGER)");
}

// ── CASE ──────────────────────────────────────────────────────────────────

#[test]
fn translate_case_postgres() {
    let expr = Expression::Case {
        expr: None,
        whens: vec![
            (
                Expression::BinaryOp {
                    op: BinaryOp::Gt,
                    left: Box::new(Expression::Ident("score".into())),
                    right: Box::new(Expression::Integer(90)),
                },
                Expression::String("A".into()),
            ),
        ],
        else_expr: Some(Box::new(Expression::String("B".into()))),
    };
    let dialect = PostgresDialect;
    let sql = translate_expr(&expr, &dialect);
    assert!(sql.starts_with("CASE WHEN"), "got: {sql}");
    assert!(sql.contains("THEN 'A'"), "got: {sql}");
    assert!(sql.contains("ELSE 'B'"), "got: {sql}");
    assert!(sql.ends_with("END"), "got: {sql}");
}

// ── Schema-qualified tables ────────────────────────────────────────────────

#[test]
fn translate_schema_table_postgres() {
    let q = Query {
        projection: vec![Projection::Wildcard],
        sources: vec![Source {
            name: "users".into(),
            alias: None,
            connection: None,
            schema: Some("public".into()),
            kind: SourceKind::Table("users".into()),
        }],
        ..Default::default()
    };
    let sql = translate_query(&q, &PostgresDialect);
    assert_eq!(sql, r#"SELECT * FROM "public"."users""#);
}

#[test]
fn translate_schema_table_mysql() {
    let q = Query {
        projection: vec![Projection::Wildcard],
        sources: vec![Source {
            name: "users".into(),
            alias: None,
            connection: None,
            schema: Some("inventory".into()),
            kind: SourceKind::Table("users".into()),
        }],
        ..Default::default()
    };
    let sql = translate_query(&q, &MySQLDialect);
    assert_eq!(sql, "SELECT * FROM `inventory`.`users`");
}

#[test]
fn translate_schema_table_aliased() {
    let q = Query {
        projection: vec![Projection::Wildcard],
        sources: vec![Source {
        name: "u".into(),
        alias: Some("u".into()),
        connection: None,
        schema: Some("public".into()),
        kind: SourceKind::Table("users".into()),
        }],
        ..Default::default()
    };
    let sql = translate_query(&q, &PostgresDialect);
    assert_eq!(sql, r#"SELECT * FROM "public"."users" AS "u""#);
}

#[test]
fn translate_insert_with_schema() {
    let insert = Insert {
        schema: Some("public".into()),
        table: "users".into(),
        connection: None,
        columns: Some(vec!["name".into()]),
        rows: vec![vec![("name".into(), Expression::String("Alice".into()))]],
        query: None,
    };
    let sql = translate_statement_sql(&Statement::Insert(insert), &PostgresDialect);
    assert_eq!(sql, r#"INSERT INTO "public"."users" ("name") VALUES ('Alice')"#);
}

#[test]
fn translate_update_with_schema() {
    let update = Update {
        schema: Some("sales".into()),
        table: "orders".into(),
        connection: None,
        assignments: vec![("status".into(), Expression::String("shipped".into()))],
        filter: Some(Expression::BinaryOp {
            op: BinaryOp::Eq,
            left: Box::new(Expression::Ident("id".into())),
            right: Box::new(Expression::Integer(1)),
        }),
    };
    let sql = translate_statement_sql(&Statement::Update(update), &MySQLDialect);
    assert_eq!(sql, "UPDATE `sales`.`orders` SET `status` = 'shipped' WHERE (`id` = 1)");
}

#[test]
fn translate_delete_with_schema() {
    let delete = Delete {
        schema: Some("archive".into()),
        table: "logs".into(),
        connection: None,
        filter: Some(Expression::BinaryOp {
            op: BinaryOp::Eq,
            left: Box::new(Expression::Ident("id".into())),
            right: Box::new(Expression::Integer(42)),
        }),
    };
    let sql = translate_statement_sql(&Statement::Delete(delete), &MSSQLDialect);
    assert_eq!(sql, "DELETE FROM [archive].[logs] WHERE ([id] = 42)");
}

// ── CREATE TABLE translator tests ──────────────────────────────────────────

#[test]
fn translate_create_table_postgres() {
    let ct = CreateTable {
        table: "users".into(),
        connection: None,
        schema: None,
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Integer,
                nullable: false,
                default: None,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::String,
                nullable: false,
                default: None,
                primary_key: false,
            },
            ColumnDef {
                name: "age".into(),
                data_type: DataType::Integer,
                nullable: true,
                default: None,
                primary_key: false,
            },
        ],
        if_not_exists: false,
    };
    let sql = translate_statement_sql(&Statement::CreateTable(ct), &PostgresDialect);
    assert_eq!(
        sql,
        r#"CREATE TABLE "users" ("id" INTEGER NOT NULL, "name" TEXT NOT NULL, "age" INTEGER, PRIMARY KEY ("id"))"#
    );
}

#[test]
fn translate_create_table_mysql() {
    let ct = CreateTable {
        table: "users".into(),
        connection: None,
        schema: None,
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Integer,
                nullable: false,
                default: None,
                primary_key: true,
            },
        ],
        if_not_exists: false,
    };
    let sql = translate_statement_sql(&Statement::CreateTable(ct), &MySQLDialect);
    assert_eq!(
        sql,
        "CREATE TABLE `users` (`id` INTEGER NOT NULL, PRIMARY KEY (`id`))"
    );
}

#[test]
fn translate_create_table_if_not_exists() {
    let ct = CreateTable {
        table: "cache".into(),
        connection: None,
        schema: None,
        columns: vec![ColumnDef {
            name: "key".into(),
            data_type: DataType::String,
            nullable: false,
            default: None,
            primary_key: true,
        }],
        if_not_exists: true,
    };
    let sql = translate_statement_sql(&Statement::CreateTable(ct), &PostgresDialect);
    assert!(sql.starts_with("CREATE TABLE IF NOT EXISTS"));
}

#[test]
fn translate_create_table_primary_key() {
    let ct = CreateTable {
        table: "t".into(),
        connection: None,
        schema: None,
        columns: vec![
            ColumnDef {
                name: "a".into(),
                data_type: DataType::Integer,
                nullable: false,
                default: None,
                primary_key: true,
            },
            ColumnDef {
                name: "b".into(),
                data_type: DataType::Integer,
                nullable: false,
                default: None,
                primary_key: true,
            },
            ColumnDef {
                name: "c".into(),
                data_type: DataType::String,
                nullable: true,
                default: None,
                primary_key: false,
            },
        ],
        if_not_exists: false,
    };
    let sql = translate_statement_sql(&Statement::CreateTable(ct), &PostgresDialect);
    assert!(sql.contains(r#"PRIMARY KEY ("a", "b")"#));
}

#[test]
fn translate_data_types() {
    assert_eq!(
        translate_data_type(&DataType::Integer, &PostgresDialect),
        "INTEGER"
    );
    assert_eq!(
        translate_data_type(&DataType::String, &PostgresDialect),
        "TEXT"
    );
    assert_eq!(
        translate_data_type(&DataType::Float, &PostgresDialect),
        "DOUBLE PRECISION"
    );
    assert_eq!(
        translate_data_type(&DataType::Boolean, &PostgresDialect),
        "BOOLEAN"
    );
    assert_eq!(
        translate_data_type(&DataType::DateTime, &PostgresDialect),
        "TIMESTAMP"
    );
    assert_eq!(
        translate_data_type(&DataType::Json, &PostgresDialect),
        "JSONB"
    );
    assert_eq!(
        translate_data_type(&DataType::Json, &MySQLDialect),
        "JSON"
    );
}

#[test]
fn translate_insert_with_conflict_ignore() {
    let sql = build_insert_into_str("target", &["id".into(), "val".into()], Some(&ConflictAction::Ignore), &PostgresDialect);
    assert!(sql.contains("ON CONFLICT DO NOTHING"));
}

#[test]
fn translate_insert_with_conflict_replace() {
    let sql = build_insert_into_str("target", &["id".into(), "val".into()], Some(&ConflictAction::Replace), &PostgresDialect);
    assert!(sql.contains("ON CONFLICT"));
    assert!(sql.contains("DO UPDATE SET"));
}

// Helper to test insert builder without real data
fn build_insert_into_str(
    table: &str, columns: &[String],
    on_conflict: Option<&ConflictAction>, dialect: &dyn SqlDialect,
) -> String {
    let cols = columns.iter().map(|c| dialect.quote_ident(c)).collect::<Vec<_>>().join(", ");
    let base = format!("INSERT INTO {} ({}) VALUES (1, 'x')", dialect.quote_ident(table), cols);
    match on_conflict {
        Some(ConflictAction::Ignore) => format!("{} ON CONFLICT DO NOTHING", base),
        Some(ConflictAction::Replace) => format!(
            "{} ON CONFLICT ({}) DO UPDATE SET {}",
            base,
            columns.iter().map(|c| dialect.quote_ident(c)).collect::<Vec<_>>().join(", "),
            columns.iter().map(|c| format!("{} = EXCLUDED.{}", dialect.quote_ident(c), dialect.quote_ident(c))).collect::<Vec<_>>().join(", ")
        ),
        None => base,
    }
}

#[test]
fn plan_create_table_mongo_generates_json() {
    let ct = CreateTable {
        table: "logs".into(),
        connection: Some("mongo".into()),
        schema: None,
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Integer,
                nullable: true,
                default: None,
                primary_key: false,
            },
        ],
        if_not_exists: false,
    };
    let stmt = Statement::CreateTable(ct);
    let source_db = vec![("mongo".to_string(), DatabaseKind::MongoDB)];
    let ai_configs = HashMap::<String, AiConfig>::new();
    let plan = plan_statement(&stmt, &source_db, &ai_configs);
    match &plan.root {
        PlanNode::CreateTable { sql, .. } => {
            assert!(
                sql.contains("\"create\""),
                "Expected MongoDB create JSON, got: {}",
                sql
            );
            assert!(
                sql.contains("\"collection\""),
                "Expected MongoDB JSON, got: {}",
                sql
            );
            assert!(
                sql.contains("\"create\""),
                "Expected MongoDB JSON, got: {}",
                sql
            );
        }
        other => panic!("Expected CreateTable plan node, got: {:?}", other),
    }
}

#[test]
fn plan_create_table_postgres_generates_sql() {
    let ct = CreateTable {
        table: "logs".into(),
        connection: Some("pg".into()),
        schema: None,
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Integer,
                nullable: true,
                default: None,
                primary_key: false,
            },
        ],
        if_not_exists: false,
    };
    let stmt = Statement::CreateTable(ct);
    let source_db = vec![("pg".to_string(), DatabaseKind::Postgres)];
    let ai_configs = HashMap::<String, AiConfig>::new();
    let plan = plan_statement(&stmt, &source_db, &ai_configs);
    match &plan.root {
        PlanNode::CreateTable { sql, .. } => {
            assert!(
                sql.starts_with("CREATE TABLE"),
                "Expected SQL, got: {}",
                sql
            );
        }
        other => panic!("Expected CreateTable plan node, got: {:?}", other),
    }
}

#[test]
fn plan_insert_mongo_generates_json() {
    let insert = Insert {
        table: "test_coll".into(),
        connection: Some("mongo".into()),
        schema: None,
        columns: None,
        rows: vec![vec![
            ("name".into(), Expression::String("Alice".into())),
            ("age".into(), Expression::Integer(30)),
        ]],
        query: None,
    };
    let json = translate_insert_mongo(&insert);
    assert!(json.contains("\"database\""), "Missing database field: {}", json);
    assert!(json.contains("\"collection\""), "Missing collection field: {}", json);
    assert!(json.contains("\"documents\""), "Missing documents field: {}", json);
    assert!(json.contains("\"Alice\""), "Missing Alice value: {}", json);
    assert!(json.contains("30"), "Missing age value: {}", json);
    eprintln!("MongoDB insert JSON: {}", json);
}

#[test]
fn plan_insert_mongo_from_statement() {
    let insert = Insert {
        table: "test_coll".into(),
        connection: Some("mongo".into()),
        schema: None,
        columns: None,
        rows: vec![vec![
            ("x".into(), Expression::Integer(42)),
        ]],
        query: None,
    };
    let stmt = Statement::Insert(insert);
    let source_db = vec![("mongo".to_string(), DatabaseKind::MongoDB)];
    let ai_configs = HashMap::<String, AiConfig>::new();
    let plan = plan_statement(&stmt, &source_db, &ai_configs);
    match &plan.root {
        PlanNode::Dml { sql, database } => {
            assert_eq!(database.1, DatabaseKind::MongoDB);
            eprintln!("MongoDB DML SQL: {}", sql);
            assert!(!sql.starts_with("INSERT"), "Expected JSON not SQL: {}", sql);
        }
        other => panic!("Expected Dml plan node, got: {:?}", other),
    }
}

// ── ALTER TABLE translator tests ─────────────────────────────────────────────

#[test]
fn translate_alter_add_column_postgres() {
    let at = AlterTable {
        table: "users".into(),
        connection: None,
        schema: Some("public".into()),
        action: AlterAction::AddColumn(ColumnDef {
            name: "age".into(),
            data_type: DataType::Integer,
            nullable: true,
            default: None,
            primary_key: false,
        }),
    };
    let sql = translate_alter_table(&at, &PostgresDialect);
    assert!(sql.contains("ALTER TABLE"), "Missing ALTER TABLE: {}", sql);
    assert!(sql.contains("ADD COLUMN"), "Missing ADD COLUMN: {}", sql);
    assert!(sql.contains("\"public\".\"users\""), "Missing table name: {}", sql);
    assert!(sql.contains("\"age\""), "Missing column name: {}", sql);
    assert!(sql.contains("INTEGER"), "Missing type: {}", sql);
}

#[test]
fn translate_alter_add_column_mysql() {
    let at = AlterTable {
        table: "users".into(),
        connection: None,
        schema: Some("public".into()),
        action: AlterAction::AddColumn(ColumnDef {
            name: "age".into(),
            data_type: DataType::Integer,
            nullable: true,
            default: None,
            primary_key: false,
        }),
    };
    let sql = translate_alter_table(&at, &MySQLDialect);
    assert!(sql.contains("`users`"), "Missing backtick table: {}", sql);
    assert!(sql.contains("`age`"), "Missing backtick column: {}", sql);
}

#[test]
fn translate_alter_drop_column_postgres() {
    let at = AlterTable {
        table: "users".into(),
        connection: None,
        schema: None,
        action: AlterAction::DropColumn { name: "temp".into() },
    };
    let sql = translate_alter_table(&at, &PostgresDialect);
    assert!(sql.contains("DROP COLUMN"), "Missing DROP COLUMN: {}", sql);
    assert!(sql.contains("\"temp\""), "Missing column name: {}", sql);
}

#[test]
fn translate_alter_rename_column_postgres() {
    let at = AlterTable {
        table: "users".into(),
        connection: None,
        schema: None,
        action: AlterAction::RenameColumn { from: "name".into(), to: "full_name".into() },
    };
    let sql = translate_alter_table(&at, &PostgresDialect);
    assert!(sql.contains("RENAME COLUMN"), "Missing RENAME COLUMN: {}", sql);
    assert!(sql.contains("\"name\""), "Missing from column: {}", sql);
    assert!(sql.contains("\"full_name\""), "Missing to column: {}", sql);
}

#[test]
fn translate_alter_rename_table() {
    let at = AlterTable {
        table: "users".into(),
        connection: None,
        schema: None,
        action: AlterAction::RenameTable { to: "customers".into() },
    };
    let sql = translate_alter_table(&at, &PostgresDialect);
    assert!(sql.contains("RENAME TO"), "Missing RENAME TO: {}", sql);
    assert!(sql.contains("\"customers\""), "Missing new name: {}", sql);
}

#[test]
fn translate_alter_not_null_default_postgres() {
    let at = AlterTable {
        table: "users".into(),
        connection: None,
        schema: None,
        action: AlterAction::AlterColumn {
            name: "status".into(),
            data_type: Some(DataType::String),
            nullable: Some(false),
            default: Some(Expression::String("active".into())),
            drop_default: false,
        },
    };
    let sql = translate_alter_table(&at, &PostgresDialect);
    assert!(sql.contains("SET NOT NULL"), "Missing SET NOT NULL: {}", sql);
    assert!(sql.contains("SET DEFAULT"), "Missing SET DEFAULT: {}", sql);
}

#[test]
fn translate_alter_drop_default() {
    let at = AlterTable {
        table: "users".into(),
        connection: None,
        schema: None,
        action: AlterAction::AlterColumn {
            name: "status".into(),
            data_type: None,
            nullable: None,
            default: None,
            drop_default: true,
        },
    };
    let sql = translate_alter_table(&at, &PostgresDialect);
    assert!(sql.contains("DROP DEFAULT"), "Missing DROP DEFAULT: {}", sql);
}

#[test]
fn plan_alter_table() {
    let at = AlterTable {
        table: "users".into(),
        connection: Some("pg".into()),
        schema: None,
        action: AlterAction::AddColumn(ColumnDef {
            name: "bio".into(),
            data_type: DataType::String,
            nullable: true,
            default: None,
            primary_key: false,
        }),
    };
    let stmt = Statement::AlterTable(at);
    let source_db = vec![("pg".to_string(), DatabaseKind::Postgres)];
    let ai_configs = HashMap::<String, AiConfig>::new();
    let plan = plan_statement(&stmt, &source_db, &ai_configs);
    match &plan.root {
        PlanNode::AlterTable { database, sql } => {
            assert_eq!(database.0, "pg");
            assert_eq!(database.1, DatabaseKind::Postgres);
            assert!(sql.contains("ADD COLUMN"), "Missing ADD COLUMN: {}", sql);
        }
        other => panic!("Expected AlterTable plan node, got: {:?}", other),
    }
}

#[test]
fn plan_alter_table_mongodb_empty() {
    let at = AlterTable {
        table: "users".into(),
        connection: Some("mongo".into()),
        schema: None,
        action: AlterAction::AddColumn(ColumnDef {
            name: "bio".into(),
            data_type: DataType::String,
            nullable: true,
            default: None,
            primary_key: false,
        }),
    };
    let stmt = Statement::AlterTable(at);
    let source_db = vec![("mongo".to_string(), DatabaseKind::MongoDB)];
    let ai_configs = HashMap::<String, AiConfig>::new();
    let plan = plan_statement(&stmt, &source_db, &ai_configs);
    match &plan.root {
        PlanNode::AlterTable { database, sql } => {
            assert_eq!(database.1, DatabaseKind::MongoDB);
            assert!(sql.is_empty(), "SQL should be empty for MongoDB: {}", sql);
        }
        other => panic!("Expected AlterTable plan node, got: {:?}", other),
    }
}

// ── DROP TABLE translator tests ───────────────────────────────────────────────

#[test]
fn translate_drop_table_postgres() {
    let dt = DropTable {
        table: "users".into(),
        connection: None,
        schema: None,
        if_exists: false,
        cascade: false,
    };
    let sql = translate_drop_table(&dt, &PostgresDialect);
    assert_eq!(sql, r#"DROP TABLE "users""#);
}

#[test]
fn translate_drop_table_if_exists_postgres() {
    let dt = DropTable {
        table: "users".into(),
        connection: None,
        schema: None,
        if_exists: true,
        cascade: false,
    };
    let sql = translate_drop_table(&dt, &PostgresDialect);
    assert_eq!(sql, r#"DROP TABLE IF EXISTS "users""#);
}

#[test]
fn translate_drop_table_cascade_postgres() {
    let dt = DropTable {
        table: "users".into(),
        connection: None,
        schema: None,
        if_exists: false,
        cascade: true,
    };
    let sql = translate_drop_table(&dt, &PostgresDialect);
    assert_eq!(sql, r#"DROP TABLE "users" CASCADE"#);
}

#[test]
fn translate_drop_table_cascade_mysql() {
    let dt = DropTable {
        table: "users".into(),
        connection: None,
        schema: None,
        if_exists: false,
        cascade: true,
    };
    let sql = translate_drop_table(&dt, &MySQLDialect);
    assert_eq!(sql, "DROP TABLE `users` CASCADE");
}

#[test]
fn translate_drop_table_mssql_no_cascade() {
    let dt = DropTable {
        table: "users".into(),
        connection: None,
        schema: None,
        if_exists: false,
        cascade: true,
    };
    let sql = translate_drop_table(&dt, &MSSQLDialect);
    assert_eq!(sql, "DROP TABLE [users]");
}

#[test]
fn translate_drop_table_sqlite_no_cascade() {
    let dt = DropTable {
        table: "users".into(),
        connection: None,
        schema: None,
        if_exists: false,
        cascade: true,
    };
    let sql = translate_drop_table(&dt, &SQLiteDialect);
    assert_eq!(sql, r#"DROP TABLE "users""#);
}

#[test]
fn translate_drop_table_with_schema() {
    let dt = DropTable {
        table: "users".into(),
        connection: None,
        schema: Some("public".into()),
        if_exists: false,
        cascade: false,
    };
    let sql = translate_drop_table(&dt, &PostgresDialect);
    assert_eq!(sql, r#"DROP TABLE "public"."users""#);
}

#[test]
fn translate_drop_table_if_exists_cascade() {
    let dt = DropTable {
        table: "logs".into(),
        connection: None,
        schema: None,
        if_exists: true,
        cascade: true,
    };
    let sql = translate_drop_table(&dt, &PostgresDialect);
    assert_eq!(sql, r#"DROP TABLE IF EXISTS "logs" CASCADE"#);
}

#[test]
fn translate_create_database_postgres() {
    let cd = CreateDatabase {
        name: "mydb".into(),
        connection: None,
        if_not_exists: false,
    };
    let sql = super::translator::translate_create_database(&cd, &PostgresDialect);
    assert_eq!(sql, r#"CREATE DATABASE "mydb""#);
}

#[test]
fn translate_create_database_if_not_exists_postgres() {
    let cd = CreateDatabase {
        name: "mydb".into(),
        connection: None,
        if_not_exists: true,
    };
    let sql = super::translator::translate_create_database(&cd, &PostgresDialect);
    assert_eq!(sql, r#"CREATE DATABASE "mydb""#);
}

#[test]
fn translate_create_database_mysql() {
    let cd = CreateDatabase {
        name: "mydb".into(),
        connection: None,
        if_not_exists: true,
    };
    let sql = super::translator::translate_create_database(&cd, &MySQLDialect);
    assert_eq!(sql, "CREATE DATABASE IF NOT EXISTS `mydb`");
}

#[test]
fn translate_create_database_mssql() {
    let cd = CreateDatabase {
        name: "mydb".into(),
        connection: None,
        if_not_exists: true,
    };
    let sql = super::translator::translate_create_database(&cd, &MSSQLDialect);
    assert_eq!(sql, "CREATE DATABASE IF NOT EXISTS [mydb]");
}

#[test]
fn translate_drop_database_postgres() {
    let dd = DropDatabase {
        name: "mydb".into(),
        connection: None,
        if_exists: false,
    };
    let sql = super::translator::translate_drop_database(&dd, &PostgresDialect);
    assert_eq!(sql, r#"DROP DATABASE "mydb""#);
}

#[test]
fn translate_drop_database_if_exists_mysql() {
    let dd = DropDatabase {
        name: "mydb".into(),
        connection: None,
        if_exists: true,
    };
    let sql = super::translator::translate_drop_database(&dd, &MySQLDialect);
    assert_eq!(sql, "DROP DATABASE IF EXISTS `mydb`");
}

#[test]
fn translate_drop_database_mssql() {
    let dd = DropDatabase {
        name: "mydb".into(),
        connection: None,
        if_exists: false,
    };
    let sql = super::translator::translate_drop_database(&dd, &MSSQLDialect);
    assert_eq!(sql, "DROP DATABASE [mydb]");
}
