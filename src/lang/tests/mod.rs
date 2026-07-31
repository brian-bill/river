use chumsky::prelude::*;

use super::ast::*;
use super::lexer::{self, Token};

type Spanned = (Token, std::ops::Range<usize>);
type PErr = chumsky::error::Simple<Spanned>;

fn lex_tokens(input: &str) -> Vec<Spanned> {
    let tokens = lexer::lex(input);
    tokens.into_iter().map(|s| (s.token, s.span)).collect()
}

fn parse_one(input: &str) -> Result<Vec<Statement>, Vec<PErr>> {
    let tokens = lex_tokens(input);
    super::parser::parser().parse(tokens)
}

// ── Basic queries ────────────────────────────────────────────────────────────

#[test]
fn simple_find() {
    assert!(parse_one("find users where age > 21 limit 10").is_ok());
}

#[test]
fn find_with_projection() {
    assert!(parse_one("find [name, email] from users where status = \"active\"").is_ok());
}

#[test]
fn find_star() {
    assert!(parse_one("find * from users where created_at > now()").is_ok());
}

#[test]
fn distinct() {
    assert!(parse_one("find distinct [status] from users").is_ok());
}

#[test]
fn order_by() {
    assert!(parse_one("find [name, age] from users order by age desc").is_ok());
}

#[test]
fn order_by_multiple() {
    assert!(parse_one(
        "find [name, department, salary] from employees order by department asc, salary desc"
    )
    .is_ok());
}

#[test]
fn order_by_nulls() {
    assert!(parse_one(
        "find [name, deleted_at] from users order by deleted_at asc nulls first"
    )
    .is_ok());
}

#[test]
fn limit_offset() {
    assert!(parse_one(
        "find [name] from users order by created_at desc limit 10 offset 20"
    )
    .is_ok());
}

// ── Joins ────────────────────────────────────────────────────────────────────

#[test]
fn join() {
    assert!(parse_one(
        "find [u.name, o.total] from users as u join orders as o on u.id = o.user_id"
    )
    .is_ok());
}

#[test]
fn left_join() {
    assert!(parse_one(
        "find [u.name, o.total] from users as u left join orders as o on u.id = o.user_id"
    )
    .is_ok());
}

#[test]
fn right_join() {
    assert!(parse_one(
        "find [u.name, o.total] from users as u right join orders as o on u.id = o.user_id"
    )
    .is_ok());
}

#[test]
fn full_join() {
    assert!(parse_one(
        "find [u.name, o.total] from users as u full join orders as o on u.id = o.user_id"
    )
    .is_ok());
}

#[test]
fn cross_join() {
    assert!(parse_one("find [u.name, o.total] from users as u cross join orders as o").is_ok());
}

#[test]
fn multiple_joins() {
    assert!(parse_one(
        "find [u.name, o.total, p.title] from users as u join orders as o on u.id = o.user_id join products as p on o.product_id = p.id"
    ).is_ok());
}

// ── CTEs ─────────────────────────────────────────────────────────────────────

#[test]
fn cte() {
    assert!(parse_one(
        "with active_users as ( find * from users where status = \"active\" ) find [name, email] from active_users"
    ).is_ok());
}

#[test]
fn multiple_ctes() {
    assert!(parse_one(
        "with paid_orders as ( find * from orders where status = \"paid\" ), user_totals as ( find [user_id, sum(total) as revenue] from paid_orders group by user_id ) find [u.name, ut.revenue] from users as u join user_totals as ut on u.id = ut.user_id where ut.revenue > 1000 order by ut.revenue desc"
    ).is_ok());
}

#[test]
fn multiple_ctes_multiline() {
    assert!(parse_one(
        r#"with
  paid_orders as (
    find * from orders where status = "paid"
  ),
  user_totals as (
    find [user_id, sum(total) as revenue]
    from paid_orders
    group by user_id
  )
find [u.name, ut.revenue]
from users"#
    ).is_ok());
}

#[test]
fn recursive_cte() {
    let result = parse_one(
        "with recursive org_tree as ( find * from employees where manager_id is null union all find [e.*] from employees as e join org_tree as t on e.manager_id = t.id ) find * from org_tree"
    ).unwrap();
    match &result[0] {
        Statement::With(w) => {
            assert!(w.recursive);
            assert_eq!(w.ctes.len(), 1);
            assert_eq!(w.ctes[0].name, "org_tree");
            // Verify the chain (union all) is preserved
            assert_eq!(w.ctes[0].chain.len(), 1, "Expected 1 set op in chain, got {:?}", w.ctes[0].chain.len());
            assert_eq!(w.ctes[0].chain[0].0, SetOpKind::UnionAll);
        }
        _ => panic!("Expected With statement"),
    }
}

#[test]
fn cte_with_column_aliases() {
    let result = parse_one(
        "with top_earners (id, name, pay) as ( find [id, name, salary] from users limit 10 ) find [name, pay] from top_earners"
    ).unwrap();
    match &result[0] {
        Statement::With(w) => {
            let cols = w.ctes[0].columns.as_ref().unwrap();
            assert_eq!(cols, &["id", "name", "pay"]);
        }
        _ => panic!("Expected With statement"),
    }
}

#[test]
fn cte_with_order_by_and_limit() {
    let result = parse_one(
        r#"with recent as (find [id, name] from users order by id desc limit 50) find [name] from recent"#
    ).unwrap();
    match &result[0] {
        Statement::With(w) => {
            let q = &*w.ctes[0].query;
            assert_eq!(q.limit, Some(50));
            assert_eq!(q.order_by.len(), 1);
        }
        _ => panic!("Expected With statement"),
    }
}

#[test]
fn cte_with_distinct() {
    assert!(parse_one(
        r#"with depts as (find distinct [department] from users) find [department] from depts"#
    ).is_ok());
}

#[test]
fn cte_with_having() {
    assert!(parse_one(
        r#"with big_depts as (find [department, count(*) as n] from users group by department having count(*) > 10) find [department] from big_depts"#
    ).is_ok());
}

#[test]
fn cte_with_window_function() {
    assert!(parse_one(
        r#"with ranked as (find [id, name, row_number() over (order by salary desc) as rn] from users) find [name] from ranked where rn <= 10"#
    ).is_ok());
}

#[test]
fn cte_referencing_cte() {
    let result = parse_one(
        r#"with step1 as (find [id, name] from users where status = "active"), step2 as (find [id] from step1 limit 20) find [name] from step2 join users on step2.id = users.id"#
    ).unwrap();
    match &result[0] {
        Statement::With(w) => {
            assert_eq!(w.ctes.len(), 2);
            assert_eq!(w.ctes[0].name, "step1");
            assert_eq!(w.ctes[1].name, "step2");
            // step2 should reference step1 via CteRef
            let step2_q = &*w.ctes[1].query;
            assert!(matches!(&step2_q.sources[0].kind, SourceKind::CteRef(name) if name == "step1"));
        }
        _ => panic!("Expected With statement"),
    }
}

#[test]
fn cte_union_intersect_except() {
    // UNION
    let r = parse_one(
        r#"with combined as (find [id] from users union find [id] from orders) find * from combined"#
    ).unwrap();
    match &r[0] {
        Statement::With(w) => {
            assert_eq!(w.ctes[0].chain.len(), 1);
            assert_eq!(w.ctes[0].chain[0].0, SetOpKind::Union);
        }
        _ => panic!("Expected With statement"),
    }

    // INTERSECT
    let r = parse_one(
        r#"with common as (find [id] from users intersect find [id] from orders) find * from common"#
    ).unwrap();
    match &r[0] {
        Statement::With(w) => {
            assert_eq!(w.ctes[0].chain.len(), 1);
            assert_eq!(w.ctes[0].chain[0].0, SetOpKind::Intersect);
        }
        _ => panic!("Expected With statement"),
    }

    // EXCEPT
    let r = parse_one(
        r#"with only_users as (find [id] from users except find [id] from orders) find * from only_users"#
    ).unwrap();
    match &r[0] {
        Statement::With(w) => {
            assert_eq!(w.ctes[0].chain.len(), 1);
            assert_eq!(w.ctes[0].chain[0].0, SetOpKind::Except);
        }
        _ => panic!("Expected With statement"),
    }
}

#[test]
fn cte_followed_by_semicolon() {
    assert!(parse_one(
        r#"with active as (find * from users where status = "active") find * from active;"#
    ).is_ok());
}

#[test]
fn cte_ref_resolved_to_cteref() {
    // Verify that CTE references in the body are resolved from Table to CteRef
    let result = parse_one(
        "with recent_orders as ( find [user_id, total] from orders@pg limit 1000) find [u.name, ro.total] from users@mysql as u join recent_orders as ro"
    ).unwrap();
    match &result[0] {
        Statement::With(w) => match w.body.as_ref() {
            Statement::Query(q) => {
                // Check that recent_orders in the join is now a CteRef
                let join_source = &q.joins[0].source;
                assert!(matches!(join_source.kind, SourceKind::CteRef(ref name) if name == "recent_orders"));
            }
            _ => panic!("expected Query body"),
        },
        _ => panic!("expected With statement"),
    }
}

// ── Aggregation & grouping ───────────────────────────────────────────────────

#[test]
fn group_by() {
    assert!(parse_one(
        "find [department, count(*) as total] from employees group by department"
    )
    .is_ok());
}

#[test]
fn having() {
    assert!(parse_one(
        "find [user_id, count(*) as order_count] from orders group by user_id having count(*) > 5"
    )
    .is_ok());
}

#[test]
fn aggregates() {
    assert!(parse_one(
        "find [status, count(*) as cnt, sum(total) as revenue, avg(total) as avg_order, min(created_at) as first_order, max(created_at) as last_order] from orders group by status"
    ).is_ok());
}

#[test]
fn count_distinct() {
    assert!(parse_one(
        "find [status, count_distinct(user_id) as unique_users] from orders group by status"
    ).is_ok());
}

// ── Window functions ─────────────────────────────────────────────────────────

#[test]
fn window_functions() {
    assert!(parse_one(
        "find [name, department, salary, row_number() over (partition by department order by salary desc) as rank] from employees"
    ).is_ok());
}

#[test]
fn rank_dense_rank() {
    assert!(parse_one(
        "find [name, score, rank() over (order by score desc) as position, dense_rank() over (order by score desc) as dense_position] from players"
    ).is_ok());
}

#[test]
fn named_window() {
    assert!(parse_one(
        "find [name, department, salary, avg(salary) over w as dept_avg, max(salary) over w as dept_max] from employees window w as (partition by department)"
    ).is_ok());
}

// ── CASE expressions ─────────────────────────────────────────────────────────

#[test]
fn case_simple() {
    assert!(parse_one(
        "find [name, case status when \"active\" then \"Active User\" when \"suspended\" then \"Suspended\" else \"Unknown\" end as status_label] from users"
    ).is_ok());
}

#[test]
fn case_searched() {
    assert!(parse_one(
        "find [name, salary, case when salary < 50000 then \"Low\" when salary >= 50000 and salary < 100000 then \"Medium\" when salary >= 100000 then \"High\" else \"Unknown\" end as salary_band] from employees"
    ).is_ok());
}

// ── Set operations ───────────────────────────────────────────────────────────

#[test]
fn union() {
    assert!(parse_one(
        "find [name, email] from customers union find [name, email] from suppliers"
    )
    .is_ok());
}

#[test]
fn union_all() {
    assert!(parse_one(
        "find [name, email] from customers union all find [name, email] from suppliers"
    )
    .is_ok());
}

#[test]
fn intersect() {
    assert!(parse_one(
        "find [user_id] from orders where year = 2024 intersect find [user_id] from orders where year = 2025"
    ).is_ok());
}

#[test]
fn except() {
    assert!(parse_one(
        "find [user_id] from users except find [user_id] from orders where created_at > now() - 30d"
    ).is_ok());
}

#[test]
fn union_with_distinct() {
    let result = parse_one(
        "find distinct [name, email] from customers union all find distinct [name, email] from suppliers"
    );
    assert!(result.is_ok());
}

#[test]
fn union_with_order_by() {
    let result = parse_one(
        "find [name, email] from customers union find [name, email] from suppliers order by name asc"
    );
    assert!(result.is_ok());
}

#[test]
fn union_with_limit() {
    let result = parse_one(
        "find [name, email] from customers union all find [name, email] from suppliers limit 10"
    );
    assert!(result.is_ok());
}

#[test]
fn union_with_where_clauses() {
    let result = parse_one(
        "find [name, email] from users where status = \"active\" union all find [name, email] from users where status = \"pending\""
    );
    assert!(result.is_ok());
}

#[test]
fn union_with_star_projection() {
    let result = parse_one(
        "find * from customers union all find * from suppliers"
    );
    assert!(result.is_ok());
}

#[test]
fn union_all_as_setop_statement() {
    let result = parse_one(
        "find [name] from t1 union all find [name] from t2"
    );
    assert!(result.is_ok());
    let stmts = result.unwrap();
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Statement::SetOp(op) => {
            assert_eq!(op.kind, SetOpKind::UnionAll);
        }
        _ => panic!("expected SetOp"),
    }
}

#[test]
fn union_as_setop_statement() {
    let result = parse_one(
        "find [name] from t1 union find [name] from t2"
    );
    assert!(result.is_ok());
    let stmts = result.unwrap();
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Statement::SetOp(op) => {
            assert_eq!(op.kind, SetOpKind::Union);
        }
        _ => panic!("expected SetOp"),
    }
}

#[test]
fn intersect_as_setop_statement() {
    let result = parse_one(
        "find [id] from t1 intersect find [id] from t2"
    );
    assert!(result.is_ok());
    let stmts = result.unwrap();
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Statement::SetOp(op) => {
            assert_eq!(op.kind, SetOpKind::Intersect);
        }
        _ => panic!("expected SetOp"),
    }
}

#[test]
fn except_as_setop_statement() {
    let result = parse_one(
        "find [id] from t1 except find [id] from t2"
    );
    assert!(result.is_ok());
    let stmts = result.unwrap();
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Statement::SetOp(op) => {
            assert_eq!(op.kind, SetOpKind::Except);
        }
        _ => panic!("expected SetOp"),
    }
}

// ── Subqueries ───────────────────────────────────────────────────────────────

#[test]
fn exists() {
    assert!(parse_one(
        "find [name] from users as u where exists ( find [1] from orders as o where o.user_id = u.id )"
    ).is_ok());
}

#[test]
fn not_exists() {
    assert!(parse_one(
        "find [name] from users as u where not exists ( find [1] from orders as o where o.user_id = u.id )"
    ).is_ok());
}

#[test]
fn subquery_in_where() {
    assert!(parse_one(
        "find [name, salary] from employees where salary > ( find [avg(salary)] from employees )"
    ).is_ok());
}

#[test]
fn subquery_in_select() {
    assert!(parse_one(
        "find [name, ( find [count(*)] from orders where orders.user_id = users.id ) as order_count] from users"
    ).is_ok());
}

#[test]
fn in_subquery() {
    assert!(parse_one(
        "find [name, department] from employees where department in ( find distinct [department] from departments where budget > 100000 )"
    ).is_ok());
}

// ── NULL handling ────────────────────────────────────────────────────────────

#[test]
fn is_null() {
    assert!(parse_one("find [name] from users where deleted_at is null").is_ok());
}

#[test]
fn is_not_null() {
    assert!(parse_one("find [name] from users where email_verified_at is not null").is_ok());
}

#[test]
fn coalesce() {
    assert!(parse_one(
        "find [name, coalesce(nickname, name) as display_name] from users"
    )
    .is_ok());
}

// ── Operators ────────────────────────────────────────────────────────────────

#[test]
fn like() {
    assert!(parse_one("find [name] from users where name like \"%smith%\"").is_ok());
}

#[test]
fn ilike() {
    assert!(parse_one("find [name] from users where name ilike \"%smith%\"").is_ok());
}

#[test]
fn in_list() {
    assert!(parse_one("find [name] from users where status in (\"active\", \"pending\")").is_ok());
}

#[test]
fn range_comparison() {
    assert!(parse_one(
        "find [name] from users where created_at between \"2024-01-01\" and \"2024-12-31\""
    ).is_ok());
}

#[test]
fn interval() {
    assert!(parse_one("find [name] from users where created_at > now() - 30d").is_ok());
}

// ── Casting ──────────────────────────────────────────────────────────────────

#[test]
fn cast_fn() {
    assert!(parse_one("find [name, cast(age as string) as age_str] from users").is_ok());
}

#[test]
fn cast_op() {
    assert!(parse_one("find [name, created_at::string as date_str] from users").is_ok());
}

// ── Data modification ────────────────────────────────────────────────────────

#[test]
fn insert() {
    assert!(parse_one("create user { name: \"Alice\", email: \"alice@mail.com\", age: 30 }").is_ok());
}

#[test]
fn insert_multiple() {
    assert!(parse_one(
        "create users [ { name: \"Alice\", email: \"alice@mail.com\" }, { name: \"Bob\", email: \"bob@mail.com\" } ]"
    ).is_ok());
}

#[test]
fn insert_from_query() {
    assert!(parse_one(
        "create active_users_backup ( find * from users where status = \"active\" )"
    ).is_ok());
}

#[test]
fn update() {
    assert!(parse_one(
        "update users set status = \"inactive\", updated_at = now() where last_login < now() - 90d"
    )
    .is_ok());
}

#[test]
fn delete() {
    assert!(parse_one("remove users where status = \"banned\"").is_ok());
}

// ── Meta commands ────────────────────────────────────────────────────────────

#[test]
fn describe() {
    assert!(parse_one("describe users").is_ok());
}

#[test]
fn describe_with_connection() {
    assert!(parse_one("describe users@prod_pg").is_ok());
}

#[test]
fn show_tables() {
    assert!(parse_one("show tables").is_ok());
}

#[test]
fn show_tables_with_connection() {
    assert!(parse_one("show tables @prod_pg").is_ok());
}

#[test]
fn explain() {
    assert!(parse_one("explain find [name] from users").is_ok());
}

#[test]
fn param_assign() {
    assert!(parse_one(":start_date = \"2024-01-01\"").is_ok());
}

#[test]
fn param_assign_integer() {
    assert!(parse_one(":limit = 100").is_ok());
}

#[test]
fn param_assign_boolean() {
    assert!(parse_one(":flag = true").is_ok());
}

#[test]
fn param_assign_float() {
    assert!(parse_one(":threshold = 3.14").is_ok());
}

#[test]
fn param_assign_null() {
    assert!(parse_one(":value = null").is_ok());
}

#[test]
fn query_with_named_param_in_where() {
    let result = parse_one("find [name, email] from users where status = :status");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    let stmts = result.unwrap();
    assert_eq!(stmts.len(), 1);
    if let Statement::Query(q) = &stmts[0] {
        assert!(q.filter.is_some());
        if let Some(Expression::BinaryOp { op, left, right }) = &q.filter {
            assert_eq!(*op, BinaryOp::Eq);
            assert!(matches!(left.as_ref(), Expression::Ident(_)));
            assert!(matches!(right.as_ref(), Expression::NamedParam(_)));
        } else {
            panic!("expected BinaryOp filter");
        }
    }
}

#[test]
fn query_with_named_param_formatted_query() {
    let result = parse_one(
        "find [name, email, created_at]\nfrom users@mongo\nwhere status = :status"
    );
    assert!(result.is_ok(), "parse error: {:?}", result.err());
}

#[test]
fn query_with_multiple_named_params() {
    let result = parse_one(
        "find [name, total, created_at] from orders where created_at between :start_date and :end_date"
    );
    assert!(result.is_ok(), "parse error: {:?}", result.err());
}

#[test]
fn named_param_in_update() {
    assert!(parse_one(
        "update users set status = :new_status where id = :user_id"
    ).is_ok());
}

#[test]
fn named_param_in_delete() {
    assert!(parse_one(
        "remove users where status = :status"
    ).is_ok());
}

// ── Schema-qualified tables ──────────────────────────────────────────────────

#[test]
fn schema_table() {
    let result = parse_one("find * from public.users");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    let stmts = result.unwrap();
    assert_eq!(stmts.len(), 1);
    if let Statement::Query(q) = &stmts[0] {
        assert_eq!(q.sources[0].schema, Some("public".to_string()));
        if let SourceKind::Table(t) = &q.sources[0].kind {
            assert_eq!(t, "users");
        } else {
            panic!("expected Table source kind");
        }
    }
}

#[test]
fn schema_table_at_connection() {
    let result = parse_one("find * from myschema.users@pg");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    let stmts = result.unwrap();
    assert_eq!(stmts.len(), 1);
    if let Statement::Query(q) = &stmts[0] {
        assert_eq!(q.sources[0].schema, Some("myschema".to_string()));
        assert_eq!(q.sources[0].connection, Some("pg".to_string()));
        if let SourceKind::Table(t) = &q.sources[0].kind {
            assert_eq!(t, "users");
        } else {
            panic!("expected Table source kind");
        }
    }
}

#[test]
fn schema_table_in_join() {
    let result = parse_one(
        "find [u.name, o.total] from public.users as u join sales.orders as o on u.id = o.user_id"
    );
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    let stmts = result.unwrap();
    if let Statement::Query(q) = &stmts[0] {
        assert_eq!(q.sources[0].schema, Some("public".to_string()));
        assert_eq!(q.joins[0].source.schema, Some("sales".to_string()));
    }
}

#[test]
fn describe_schema_table() {
    let result = parse_one("describe public.users");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    let stmts = result.unwrap();
    if let Statement::Describe(d) = &stmts[0] {
        assert_eq!(d.schema, Some("public".to_string()));
        assert_eq!(d.table, "users");
    } else {
        panic!("expected Describe statement");
    }
}

#[test]
fn describe_schema_table_at_connection() {
    let result = parse_one("describe inventory.products@pg");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    let stmts = result.unwrap();
    if let Statement::Describe(d) = &stmts[0] {
        assert_eq!(d.schema, Some("inventory".to_string()));
        assert_eq!(d.table, "products");
        assert_eq!(d.connection, Some("pg".to_string()));
    } else {
        panic!("expected Describe statement");
    }
}

#[test]
fn dml_with_schema() {
    let result = parse_one("create public.users { name: \"Alice\" }");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    let stmts = result.unwrap();
    if let Statement::Insert(i) = &stmts[0] {
        assert_eq!(i.schema, Some("public".to_string()));
        assert_eq!(i.table, "users");
    }
}

#[test]
fn dml_with_schema_at_connection() {
    let result = parse_one("update inventory.products@pg set price = 9.99 where id = 1");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    let stmts = result.unwrap();
    if let Statement::Update(u) = &stmts[0] {
        assert_eq!(u.schema, Some("inventory".to_string()));
        assert_eq!(u.table, "products");
        assert_eq!(u.connection, Some("pg".to_string()));
    }
}

#[test]
fn delete_with_schema() {
    let result = parse_one("remove archive.logs@mongo where created_at < now() - 30d");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    let stmts = result.unwrap();
    if let Statement::Delete(d) = &stmts[0] {
        assert_eq!(d.schema, Some("archive".to_string()));
        assert_eq!(d.table, "logs");
        assert_eq!(d.connection, Some("mongo".to_string()));
    }
}

// ── Cross-database ───────────────────────────────────────────────────────────

#[test]
fn cross_database() {
    assert!(parse_one(
        "find [name] from users@prod_pg join logs@analytics_mongo on users.id = logs.user_id where logs.action = \"login\""
    ).is_ok());
}

// ── Multiple statements ──────────────────────────────────────────────────────

#[test]
fn multiple_statements() {
    let result = parse_one(":start = \"2024\" ; find [name] from users ; :end = \"2025\"");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    let stmts = result.unwrap();
    assert_eq!(stmts.len(), 3);
}

#[test]
fn multiple_ctes_compact() {
    let result = parse_one(
        "with  paid_orders as (    find * from orders where status = \"paid\"  ),  user_totals as (    find [user_id, sum(total) as revenue]    from paid_orders    group by user_id  )find [u.name, ut.revenue]from users "
    );
    match &result {
        Ok(stmts) => {
            assert_eq!(stmts.len(), 1, "Expected 1 statement, got {}", stmts.len());
            match &stmts[0] {
                Statement::With(w) => {
                    assert_eq!(w.ctes.len(), 2, "Expected 2 CTEs, got {}", w.ctes.len());
                    assert_eq!(w.ctes[0].name, "paid_orders");
                    assert_eq!(w.ctes[1].name, "user_totals");
                }
                other => panic!("Expected With statement, got {:?}", std::mem::discriminant(other)),
            }
        }
        Err(e) => {
            for err in e {
                eprintln!("Parse error: {:?}", err);
            }
            panic!("Parse failed");
        }
    }
}

// ── CREATE TABLE tests ─────────────────────────────────────────────────────

#[test]
fn lex_double_gt_arrow() {
    let tokens = lex_tokens("find * from users >> target");
    let arrow = tokens.iter().find(|(t, _)| matches!(t, Token::Arrow));
    assert!(arrow.is_some(), "expected Arrow token for >>");
}

#[test]
fn lex_create_table_keywords() {
    let tokens = lex_tokens("create table if not exists t (col int) insert if exists on conflict ignore replace");
    let kinds: Vec<&Token> = tokens.iter().map(|(t, _)| t).collect();
    assert!(kinds.contains(&&Token::Create));
    assert!(kinds.contains(&&Token::Table));
    assert!(kinds.contains(&&Token::If));
    assert!(kinds.contains(&&Token::Exists));
    assert!(kinds.contains(&&Token::Insert));
    assert!(kinds.contains(&&Token::Conflict));
    assert!(kinds.contains(&&Token::Ignore));
    assert!(kinds.contains(&&Token::Replace));
}

#[test]
fn parse_create_table_simple() {
    let result = parse_one("create table users (name string, email string, age int)");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::CreateTable(ct) => {
            assert_eq!(ct.table, "users");
            assert_eq!(ct.columns.len(), 3);
            assert_eq!(ct.columns[0].name, "name");
            assert!(matches!(ct.columns[0].data_type, DataType::String));
            assert_eq!(ct.columns[1].name, "email");
            assert!(matches!(ct.columns[1].data_type, DataType::String));
            assert_eq!(ct.columns[2].name, "age");
            assert!(matches!(ct.columns[2].data_type, DataType::Integer));
        }
        other => panic!("Expected CreateTable, got {:?}", other),
    }
}

#[test]
fn parse_create_table_if_not_exists() {
    let result = parse_one("create table if not exists users (id int)");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::CreateTable(ct) => {
            assert!(ct.if_not_exists);
        }
        other => panic!("Expected CreateTable, got {:?}", other),
    }
}

#[test]
fn parse_create_table_not_null_default() {
    let result = parse_one("create table users (name string not null default \"anon\", age int)");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::CreateTable(ct) => {
            assert_eq!(ct.columns.len(), 2);
            assert!(!ct.columns[0].nullable);
            assert!(ct.columns[0].default.is_some());
            assert!(ct.columns[1].nullable);
            assert!(ct.columns[1].default.is_none());
        }
        other => panic!("Expected CreateTable, got {:?}", other),
    }
}

#[test]
fn parse_create_table_primary_key() {
    let result = parse_one("create table users (id int primary key, name string)");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::CreateTable(ct) => {
            assert!(ct.columns[0].primary_key);
            assert!(!ct.columns[1].primary_key);
        }
        other => panic!("Expected CreateTable, got {:?}", other),
    }
}

#[test]
fn parse_create_table_at_connection() {
    let result = parse_one("create table users@pg (name string)");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::CreateTable(ct) => {
            assert_eq!(ct.connection.as_deref(), Some("pg"));
        }
        other => panic!("Expected CreateTable, got {:?}", other),
    }
}

#[test]
fn parse_create_table_with_schema() {
    let result = parse_one("create table public.users (name string)");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::CreateTable(ct) => {
            assert_eq!(ct.schema.as_deref(), Some("public"));
            assert_eq!(ct.table, "users");
        }
        other => panic!("Expected CreateTable, got {:?}", other),
    }
}

#[test]
fn parse_query_persist() {
    let result = parse_one("find * from users >> target");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::CreateTableAs(cta) => {
            assert_eq!(cta.table, "target");
            assert!(cta.on_conflict.is_none());
        }
        other => panic!("Expected CreateTableAs, got {:?}", other),
    }
}

#[test]
fn parse_query_persist_with_schema() {
    let result = parse_one("find * from orders >> public.archive@pg");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::CreateTableAs(cta) => {
            assert_eq!(cta.schema.as_deref(), Some("public"));
            assert_eq!(cta.table, "archive");
            assert_eq!(cta.connection.as_deref(), Some("pg"));
        }
        other => panic!("Expected CreateTableAs, got {:?}", other),
    }
}

#[test]
fn parse_query_persist_insert_ignore() {
    let result = parse_one("find * from users >> target insert if exists on conflict ignore");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::CreateTableAs(cta) => {
            assert!(matches!(cta.on_conflict, Some(ConflictAction::Ignore)));
        }
        other => panic!("Expected CreateTableAs, got {:?}", other),
    }
}

#[test]
fn parse_query_persist_insert_replace() {
    let result = parse_one("find * from users >> target insert if exists on conflict replace");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::CreateTableAs(cta) => {
            assert!(matches!(cta.on_conflict, Some(ConflictAction::Replace)));
        }
        other => panic!("Expected CreateTableAs, got {:?}", other),
    }
}

#[test]
fn parse_complex_persist() {
    let result = parse_one("find [name, email] from users where status = \"active\" >> active_users@pg insert if exists on conflict replace");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::CreateTableAs(cta) => {
            assert_eq!(cta.table, "active_users");
            assert_eq!(cta.connection.as_deref(), Some("pg"));
            assert!(matches!(cta.on_conflict, Some(ConflictAction::Replace)));
            assert_eq!(cta.query.projection.len(), 2);
        }
        other => panic!("Expected CreateTableAs, got {:?}", other),
    }
}

// ── ALTER TABLE lexer tests ──────────────────────────────────────────────────

#[test]
fn lex_alter_table_keywords() {
    let tokens = lexer::lex("alter add column drop rename to type");
    let kinds: Vec<_> = tokens.iter().map(|s| &s.token).collect();
    assert!(kinds.contains(&&Token::Alter));
    assert!(kinds.contains(&&Token::Add));
    assert!(kinds.contains(&&Token::Column));
    assert!(kinds.contains(&&Token::Drop));
    assert!(kinds.contains(&&Token::Rename));
    assert!(kinds.contains(&&Token::To));
    assert!(kinds.contains(&&Token::Type));
}

#[test]
fn lex_alter_keywords_as_idents() {
    // Keywords in column position should still tokenize correctly
    let tokens = lex_tokens("alter");
    assert!(!tokens.is_empty());
}

// ── ALTER TABLE parser tests ─────────────────────────────────────────────────

#[test]
fn parse_alter_add_column() {
    let result = parse_one("alter table users add column age int");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::AlterTable(at) => {
            assert_eq!(at.table, "users");
            assert!(matches!(&at.action, AlterAction::AddColumn(c) if c.name == "age"));
        }
        other => panic!("Expected AlterTable, got {:?}", other),
    }
}

#[test]
fn parse_alter_add_column_not_null_default() {
    let result = parse_one("alter table users add column status string not null default \"active\"");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::AlterTable(at) => {
            match &at.action {
                AlterAction::AddColumn(c) => {
                    assert_eq!(c.name, "status");
                    assert!(!c.nullable);
                    assert!(c.default.is_some());
                }
                _ => panic!("Expected AddColumn, got {:?}", at.action),
            }
        }
        other => panic!("Expected AlterTable, got {:?}", other),
    }
}

#[test]
fn parse_alter_drop_column() {
    let result = parse_one("alter table users drop column age");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::AlterTable(at) => {
            assert!(matches!(&at.action, AlterAction::DropColumn { name } if name == "age"));
        }
        other => panic!("Expected AlterTable, got {:?}", other),
    }
}

#[test]
fn parse_alter_alter_column_type() {
    let result = parse_one("alter table users alter column age type float");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::AlterTable(at) => {
            match &at.action {
                AlterAction::AlterColumn { name, data_type, .. } => {
                    assert_eq!(name, "age");
                    assert_eq!(*data_type, Some(DataType::Float));
                }
                _ => panic!("Expected AlterColumn, got {:?}", at.action),
            }
        }
        other => panic!("Expected AlterTable, got {:?}", other),
    }
}

#[test]
fn parse_alter_alter_column_not_null() {
    let result = parse_one("alter table users alter column name not null");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::AlterTable(at) => {
            match &at.action {
                AlterAction::AlterColumn { name, nullable, .. } => {
                    assert_eq!(name, "name");
                    assert_eq!(*nullable, Some(false));
                }
                _ => panic!("Expected AlterColumn, got {:?}", at.action),
            }
        }
        other => panic!("Expected AlterTable, got {:?}", other),
    }
}

#[test]
fn parse_alter_alter_column_drop_default() {
    let result = parse_one("alter table users alter column name drop default");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::AlterTable(at) => {
            match &at.action {
                AlterAction::AlterColumn { name, drop_default, .. } => {
                    assert_eq!(name, "name");
                    assert!(*drop_default);
                }
                _ => panic!("Expected AlterColumn, got {:?}", at.action),
            }
        }
        other => panic!("Expected AlterTable, got {:?}", other),
    }
}

#[test]
fn parse_alter_rename_column() {
    let result = parse_one("alter table users rename column name to full_name");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::AlterTable(at) => {
            assert!(matches!(&at.action, AlterAction::RenameColumn { from, to } if from == "name" && to == "full_name"));
        }
        other => panic!("Expected AlterTable, got {:?}", other),
    }
}

#[test]
fn parse_alter_rename_table() {
    let result = parse_one("alter table users rename to customers");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::AlterTable(at) => {
            assert!(matches!(&at.action, AlterAction::RenameTable { to } if to == "customers"));
        }
        other => panic!("Expected AlterTable, got {:?}", other),
    }
}

#[test]
fn parse_alter_with_schema() {
    let result = parse_one("alter table public.users add column bio string");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::AlterTable(at) => {
            assert_eq!(at.schema.as_deref(), Some("public"));
            assert_eq!(at.table, "users");
        }
        other => panic!("Expected AlterTable, got {:?}", other),
    }
}

#[test]
fn parse_alter_with_connection() {
    let result = parse_one("alter table users@pg drop column temp");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::AlterTable(at) => {
            assert_eq!(at.connection.as_deref(), Some("pg"));
            assert_eq!(at.table, "users");
        }
        other => panic!("Expected AlterTable, got {:?}", other),
    }
}

#[test]
fn parse_alter_with_schema_and_connection() {
    let result = parse_one("alter table public.users@pg add column notes string");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::AlterTable(at) => {
            assert_eq!(at.schema.as_deref(), Some("public"));
            assert_eq!(at.connection.as_deref(), Some("pg"));
            assert_eq!(at.table, "users");
        }
        other => panic!("Expected AlterTable, got {:?}", other),
    }
}

// ── DROP TABLE lexer tests ────────────────────────────────────────────────────

#[test]
fn lex_drop_table_keywords() {
    let tokens = lexer::lex("cascade restrict");
    let kinds: Vec<_> = tokens.iter().map(|s| &s.token).collect();
    assert!(kinds.contains(&&Token::Cascade));
    assert!(kinds.contains(&&Token::Restrict));
}

#[test]
fn lex_cascade_restrict_as_idents() {
    let tokens = lex_tokens("cascade restrict");
    assert!(!tokens.is_empty());
    let kinds: Vec<&Token> = tokens.iter().map(|(t, _)| t).collect();
    assert!(kinds.contains(&&Token::Cascade));
    assert!(kinds.contains(&&Token::Restrict));
}

// ── DROP TABLE parser tests ───────────────────────────────────────────────────

#[test]
fn parse_drop_table_simple() {
    let result = parse_one("drop table users");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::DropTable(dt) => {
            assert_eq!(dt.table, "users");
            assert!(!dt.if_exists);
            assert!(!dt.cascade);
            assert!(dt.schema.is_none());
            assert!(dt.connection.is_none());
        }
        other => panic!("Expected DropTable, got {:?}", other),
    }
}

#[test]
fn parse_drop_table_if_exists() {
    let result = parse_one("drop table if exists users");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::DropTable(dt) => {
            assert_eq!(dt.table, "users");
            assert!(dt.if_exists);
            assert!(!dt.cascade);
        }
        other => panic!("Expected DropTable, got {:?}", other),
    }
}

#[test]
fn parse_drop_table_cascade() {
    let result = parse_one("drop table users cascade");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::DropTable(dt) => {
            assert_eq!(dt.table, "users");
            assert!(!dt.if_exists);
            assert!(dt.cascade);
        }
        other => panic!("Expected DropTable, got {:?}", other),
    }
}

#[test]
fn parse_drop_table_restrict() {
    let result = parse_one("drop table users restrict");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::DropTable(dt) => {
            assert_eq!(dt.table, "users");
            assert!(!dt.if_exists);
            assert!(!dt.cascade);
        }
        other => panic!("Expected DropTable, got {:?}", other),
    }
}

#[test]
fn parse_drop_table_if_exists_cascade() {
    let result = parse_one("drop table if exists users cascade");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::DropTable(dt) => {
            assert_eq!(dt.table, "users");
            assert!(dt.if_exists);
            assert!(dt.cascade);
        }
        other => panic!("Expected DropTable, got {:?}", other),
    }
}

#[test]
fn parse_drop_table_with_schema() {
    let result = parse_one("drop table public.users");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::DropTable(dt) => {
            assert_eq!(dt.schema.as_deref(), Some("public"));
            assert_eq!(dt.table, "users");
        }
        other => panic!("Expected DropTable, got {:?}", other),
    }
}

#[test]
fn parse_drop_table_with_connection() {
    let result = parse_one("drop table users@pg");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::DropTable(dt) => {
            assert_eq!(dt.connection.as_deref(), Some("pg"));
            assert_eq!(dt.table, "users");
        }
        other => panic!("Expected DropTable, got {:?}", other),
    }
}

#[test]
fn parse_drop_table_with_schema_and_connection() {
    let result = parse_one("drop table public.users@pg");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::DropTable(dt) => {
            assert_eq!(dt.schema.as_deref(), Some("public"));
            assert_eq!(dt.connection.as_deref(), Some("pg"));
            assert_eq!(dt.table, "users");
        }
        other => panic!("Expected DropTable, got {:?}", other),
    }
}

#[test]
fn parse_drop_table_full() {
    let result = parse_one("drop table if exists public.users@pg cascade");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::DropTable(dt) => {
            assert_eq!(dt.table, "users");
            assert!(dt.if_exists);
            assert!(dt.cascade);
            assert_eq!(dt.schema.as_deref(), Some("public"));
            assert_eq!(dt.connection.as_deref(), Some("pg"));
        }
        other => panic!("Expected DropTable, got {:?}", other),
    }
}

#[test]
fn parse_create_database() {
    let result = parse_one("create database mydb");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::CreateDatabase(cd) => {
            assert_eq!(cd.name, "mydb");
            assert!(!cd.if_not_exists);
            assert!(cd.connection.is_none());
        }
        other => panic!("Expected CreateDatabase, got {:?}", other),
    }
}

#[test]
fn parse_create_database_if_not_exists() {
    let result = parse_one("create database if not exists mydb");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::CreateDatabase(cd) => {
            assert_eq!(cd.name, "mydb");
            assert!(cd.if_not_exists);
        }
        other => panic!("Expected CreateDatabase, got {:?}", other),
    }
}

#[test]
fn parse_create_database_at_connection() {
    let result = parse_one("create database mydb@pg");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::CreateDatabase(cd) => {
            assert_eq!(cd.name, "mydb");
            assert_eq!(cd.connection.as_deref(), Some("pg"));
        }
        other => panic!("Expected CreateDatabase, got {:?}", other),
    }
}

#[test]
fn parse_create_database_if_not_exists_at_connection() {
    let result = parse_one("create database if not exists mydb@pg");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::CreateDatabase(cd) => {
            assert_eq!(cd.name, "mydb");
            assert!(cd.if_not_exists);
            assert_eq!(cd.connection.as_deref(), Some("pg"));
        }
        other => panic!("Expected CreateDatabase, got {:?}", other),
    }
}

#[test]
fn parse_drop_database() {
    let result = parse_one("drop database mydb");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::DropDatabase(dd) => {
            assert_eq!(dd.name, "mydb");
            assert!(!dd.if_exists);
            assert!(dd.connection.is_none());
        }
        other => panic!("Expected DropDatabase, got {:?}", other),
    }
}

#[test]
fn parse_drop_database_if_exists() {
    let result = parse_one("drop database if exists mydb");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::DropDatabase(dd) => {
            assert_eq!(dd.name, "mydb");
            assert!(dd.if_exists);
        }
        other => panic!("Expected DropDatabase, got {:?}", other),
    }
}

#[test]
fn parse_drop_database_at_connection() {
    let result = parse_one("drop database mydb@pg");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::DropDatabase(dd) => {
            assert_eq!(dd.name, "mydb");
            assert_eq!(dd.connection.as_deref(), Some("pg"));
        }
        other => panic!("Expected DropDatabase, got {:?}", other),
    }
}

#[test]
fn parse_drop_database_if_exists_at_connection() {
    let result = parse_one("drop database if exists mydb@pg");
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    match &result.unwrap()[0] {
        Statement::DropDatabase(dd) => {
            assert_eq!(dd.name, "mydb");
            assert!(dd.if_exists);
            assert_eq!(dd.connection.as_deref(), Some("pg"));
        }
        other => panic!("Expected DropDatabase, got {:?}", other),
    }
}

#[test]
fn parse_ai_query_two_args() {
    let result = parse_one(r#"find [ai_query("openai", text) as summary] from reviews"#);
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    let stmts = result.unwrap();
    match &stmts[0] {
        Statement::Query(q) => {
            assert_eq!(q.projection.len(), 1);
            match &q.projection[0] {
                Projection::Expr(Expression::AiQuery { config, model, .. }, alias) => {
                    assert_eq!(config, "openai");
                    assert!(model.is_none());
                    assert_eq!(alias.as_deref(), Some("summary"));
                }
                other => panic!("Expected AiQuery projection, got {:?}", other),
            }
        }
        other => panic!("Expected Query, got {:?}", other),
    }
}

#[test]
fn parse_ai_query_three_args() {
    let result = parse_one(r#"find [ai_query("openai", "gpt-4o-mini", text) as category] from items"#);
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    let stmts = result.unwrap();
    match &stmts[0] {
        Statement::Query(q) => {
            match &q.projection[0] {
                Projection::Expr(Expression::AiQuery { config, model, .. }, alias) => {
                    assert_eq!(config, "openai");
                    assert_eq!(model.as_deref(), Some("gpt-4o-mini"));
                    assert_eq!(alias.as_deref(), Some("category"));
                }
                other => panic!("Expected AiQuery projection, got {:?}", other),
            }
        }
        other => panic!("Expected Query, got {:?}", other),
    }
}

#[test]
fn parse_ai_query_with_concat() {
    let result = parse_one(r#"find [name, ai_query("openai", concat("Summarize: ", text)) as summary] from reviews"#);
    assert!(result.is_ok(), "parse error: {:?}", result.err());
    let stmts = result.unwrap();
    match &stmts[0] {
        Statement::Query(q) => {
            assert_eq!(q.projection.len(), 2);
            match &q.projection[1] {
                Projection::Expr(Expression::AiQuery { config, prompt, .. }, alias) => {
                    assert_eq!(config, "openai");
                    assert_eq!(alias.as_deref(), Some("summary"));
                    match prompt.as_ref() {
                        Expression::FnCall { name, .. } => assert_eq!(name, "concat"),
                        other => panic!("Expected concat FnCall, got {:?}", other),
                    }
                }
                other => panic!("Expected AiQuery, got {:?}", other),
            }
        }
        other => panic!("Expected Query, got {:?}", other),
    }
}

#[test]
fn parse_ai_query_wrong_arg_count() {
    let result = parse_one(r#"find [ai_query("openai") as x] from t"#);
    assert!(result.is_err(), "Expected error for 1-arg ai_query");
}

#[test]
fn parse_ai_query_four_args_error() {
    let result = parse_one(r#"find [ai_query("openai", "gpt-4o", text, "extra") as x] from t"#);
    assert!(result.is_err(), "Expected error for 4-arg ai_query");
}
