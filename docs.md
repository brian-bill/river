# RiverQL Reference

RiverQL is a universal query language for interacting with PostgreSQL, MySQL, SQLite, MongoDB, and SQL Server through a single, consistent syntax. Write once, query anywhere.

---

## MCP Server

River ships with an MCP (Model Context Protocol) server that exposes the query engine to AI agents. Start it with the `--server` flag:

```bash
river --server
river --server --config /path/to/river.yaml
```

### Installing in AI Tools

Add River to any MCP-compatible host's configuration file.

**Claude Desktop** (`~/Library/Application Support/Claude/claude_desktop_config.json` on macOS, `%APPDATA%/Claude/claude_desktop_config.json` on Windows):

```json
{
  "mcpServers": {
    "river": {
      "command": "river",
      "args": ["--server", "--config", "/path/to/river.yaml"]
    }
  }
}
```

**Cursor** (`.cursor/mcp.json` in project root):

```json
{
  "mcpServers": {
    "river": {
      "command": "river",
      "args": ["--server", "--config", "/path/to/river.yaml"]
    }
  }
}
```

**opencode** (`~/.config/opencode/opencode.jsonc`):

```json
{
  "mcpServers": {
    "river": {
      "type": "local",
      "command": [
        "river",
        "--server",
        "--config",
        "/path/to/river.yaml"
      ],
      "enabled": true
    }
  }
}
```

**VS Code** (via any MCP extension, add to `settings.json`):

```json
{
  "mcp.servers": {
    "river": {
      "command": "river",
      "args": ["--server", "--config", "/path/to/river.yaml"]
    }
  }
}
```

If `river` is not on your `$PATH`, use the absolute path (e.g. `./target/release/river`). Build first with `cargo build --release`.

### Available Tools

| Tool | Description |
|------|-------------|
| `riverql_query` | Execute any RiverQL query (SELECT, INSERT, UPDATE, DELETE, DDL) |
| `riverql_describe` | Describe a table's schema (columns, types, nullability) |
| `riverql_list_tables` | List all tables/collections on a connection |
| `riverql_explain` | Get the native SQL/MQL equivalent of a RiverQL query |
| `riverql_list_connections` | List all configured database connections |
| `riverql_help` | Return RiverQL syntax reference by topic |

### Available Resources

| URI | Content |
|-----|---------|
| `river://docs` | Full RiverQL language reference |
| `river://docs/quickref` | Quick reference: keywords, operators, functions |
| `river://docs/keywords` | Keyword-to-purpose mapping table |

---

## File Processor

Beyond the interactive TUI and the MCP server, River can run RiverQL scripts
stored in `.rql` files directly from the command line. This turns River into a
query runner that fits into shell scripts, cron jobs, and CI pipelines — it
reuses the exact same engine pipeline (`parser → planner → translator →
executor`) as the TUI and MCP server, so behavior never diverges.

Pass a `.rql` file as the first positional argument. Mode precedence is
`--server` > file > TUI: a file argument never starts when `--server` is set,
and `river` with no file still launches the TUI.

```bash
# Run a script and print the result as an aligned table to stdout
river users.rql

# Run silently — exit cleanly, print nothing (for scripts/cron)
river users.rql -s

# Export the final result to a file; the extension picks the format
river users.rql --out users.csv
river users.rql --out users.xlsx
river users.rql --out users.json
river users.rql --out users.txt
river users.rql --out users.xml
```

A script may contain several statements separated by `;`. Intermediate
results print to stdout (unless `--silent`); only the **final** statement's
result is exported when `--out` is set.

```riverql
# users.rql
create table if not exists users@pg (id int primary key, name string not null, active bool);
create users@pg { id: 1, name: "Alice", active: true };
find [id, name, active] from users@pg order by id
```

### Flags

| Flag | Description |
|------|-------------|
| `FILE` (positional) | RiverQL script file to run (file-processor mode) |
| `-s`, `--silent` | Execute and print nothing to stdout |
| `--out FILE` | Export the final result; extension selects the format (`csv`, `xlsx`, `json`, `txt`, `xml`) |
| `--config FILE` | Path to the connection config (defaults to `river.yaml`) |

### Export Formats

| Extension | Format |
|-----------|--------|
| `.csv` | Header row + one row per record; `Null` becomes an empty field |
| `.xlsx` | Excel workbook with a bold header row and typed body cells |
| `.json` | `{ "columns": [...], "rows": [{ col: val, ... }], "rows_affected": N, "elapsed_ms": M }` |
| `.txt` | The same aligned ASCII table that stdout prints |
| `.xml` | `<results columns="..."><row><col name="...">value</col></row></results>` |

### Exit Codes

File-processor mode uses script-friendly exit codes so it composes in pipelines:

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | Setup / I/O error (missing file, unsupported export extension, write failure) |
| `2` | RiverQL parse error |
| `3` | Execution error (database / adapter failure) |

---

## Query Basics

### SELECT — `find`

Every query begins with `find`. It retrieves rows from a table.

```sql
find users
```

This is equivalent to `SELECT * FROM users`.

### Column Selection

Use square brackets to specify columns:

```sql
find [name, email] from users
```

Combine column selection with filters:

```sql
find [name, email, department] from users
where status = "active"
```

### Filtering — `where`

Filter rows with conditions:

```sql
find users where status = "active"
```

Chain conditions with `and` / `or`:

```sql
find users where status = "active" and age > 21
```

### Limiting Results — `limit`

Control the number of rows returned:

```sql
find users limit 10
```

Combined with a filter:

```sql
find [name, email] from users
where department = "Engineering"
limit 25
```

### Sorting — `order by`

Sort results:

```sql
find [name, salary] from users
order by salary desc
```

Multiple sort keys:

```sql
find [name, department, salary] from employees
order by department asc, salary desc
```

### Pagination — `limit` and `offset`

Page through results:

```sql
find [name, email] from users
order by created_at desc
limit 20 offset 40
```

This skips the first 40 rows and returns the next 20.

### Combined Query

Clause order: `find` -> `from` -> `where` -> `order by` -> `limit` -> `offset`

```sql
find [name, email, department, salary] from users
where status = "active" and salary > 50000
order by salary desc
limit 10
```

---

## Expressions and Operators

### Comparison Operators

```sql
find users where age > 21
find users where salary >= 75000
find users where status != "banned"
find products where price < 100
find orders where total <= 50.00
```

The `<>` operator is equivalent to `!=`:

```sql
find users where status <> "inactive"
```

### Logical Operators

Chain conditions with `and`, `or`, and `not`:

```sql
find users where status = "active" and department = "Engineering"
find users where department = "Sales" or department = "Marketing"
find users where not status = "banned"
```

Parentheses control precedence:

```sql
find users
where (department = "Sales" or department = "Marketing")
  and salary > 60000
```

### NULL Handling

Test for NULL values with `is null` and `is not null`:

```sql
find users where deleted_at is null
find users where email_verified_at is not null
```

`coalesce` — returns the first non-null value:

```sql
find [name, coalesce(nickname, name) as display_name] from users
```

`nullif` — returns NULL if two values are equal (useful for avoiding division by zero):

```sql
find [name, nullif(discount, 0) as effective_discount] from products
```

`ifnull` — alias for a two-argument coalesce:

```sql
find [name, ifnull(phone, "N/A") as contact] from users
```

### BETWEEN

Test whether a value falls within an inclusive range:

```sql
find [name, created_at] from users
where created_at between "2024-01-01" and "2024-12-31"
```

```sql
find products where price between 10 and 50
```

### IN / NOT IN

Test membership in a list:

```sql
find users where status in ("active", "pending")
find users where department not in ("HR", "Legal")
```

`IN` also supports subqueries (see the Subqueries section).

### Pattern Matching — LIKE / ILIKE

- `%` matches any number of characters
- `_` matches exactly one character

```sql
find users where name like "%smith%"
find users where email like "%@gmail.com"
find users where name like "J_n"
```

`ilike` is the case-insensitive variant:

```sql
find users where name ilike "%smith%"
```

### Arithmetic

Operators: `+`, `-`, `*`, `/`, `%` (modulo)

```sql
find [name, price * 1.1 as price_with_tax] from products
find [name, salary] from employees where salary * 12 > 100000
```

### String Concatenation

Use `||` to concatenate strings:

```sql
find [first || " " || last as full_name] from users
```

### Type Casting

Two equivalent syntaxes:

```sql
-- Function style
find [name, cast(age as string) as age_str] from users

-- Operator style (:: shorthand)
find [name, created_at::string as date_str] from users
```

Supported target types: `string`, `integer`, `float`, `boolean`, `datetime`, `json`

### Interval Literals

Interval literals for relative time calculations:

```sql
find users where created_at > now() - 30d
find users where created_at > now() - 1h
find users where last_login < now() - 90d
```

| Suffix | Meaning |
|--------|---------|
| `y`    | Years   |
| `mon`  | Months  |
| `w`    | Weeks   |
| `d`    | Days    |
| `h`    | Hours   |
| `m`    | Minutes |
| `s`    | Seconds |

Date functions:

```sql
find [date_trunc("month", created_at) as month, count(*)] from users
group by month

find [extract(year from created_at) as yr] from users
```

### Built-in Functions

String functions:

```sql
find [upper(name), lower(email)] from users
find [concat(first, " ", last) as full_name] from users
find [length(name) as name_len] from users
```

Math functions:

```sql
find [abs(amount), round(price, 2), ceil(score), floor(score)] from products
find [power(population, 2) as pop_sq] from cities
find [sqrt(variance) as stddev] from stats
```

### AI Functions

Call AI/LLM models inline within queries using `ai_query`. This evaluates per-row after the base database query, calling the configured AI gateway for each result.

**Syntax:**

```sql
ai_query(config_name, prompt)
ai_query(config_name, model, prompt)
```

- `config_name` — AI connection name from `river.yaml`
- `model` (optional) — Override the default model, e.g. `"gpt-4o-mini"`
- `prompt` — Expression evaluated per row (can use `concat`, column refs, etc.)

**Examples:**

```sql
find [review_text, ai_query("openai", concat("Summarize: ", review_text)) as summary] from reviews

find [text, ai_query("openai", "gpt-4o-mini", concat("Classify: ", text)) as category] from items
```

**Supported Providers:**

| Provider | Description |
|----------|-------------|
| `openai` (default) | OpenAI-compatible protocol — covers OpenAI, Deepseek, Kimi (Moonshot), Ollama, vLLM, Groq, Together, Mistral, Perplexity, xAI, and any service with a `/chat/completions` endpoint |
| `anthropic` | Anthropic Claude — native Messages API |
| `gemini` | Google Gemini — native generateContent API (AI Studio and Vertex AI) |

**Configuration:**

AI connections are defined in `river.yaml` alongside database connections:

```yaml
# OpenAI (and any OpenAI-compatible: Deepseek, Kimi, Ollama, etc.)
- name: openai
  kind: ai
  provider: openai          # default
  uri: "https://api.openai.com/v1"
  api_key: "${OPENAI_API_KEY}"
  model: "gpt-4o"

# Anthropic Claude
- name: claude
  kind: ai
  provider: anthropic
  uri: "https://api.anthropic.com"
  api_key: "${ANTHROPIC_API_KEY}"
  model: "claude-sonnet-4-20250514"

# Google Gemini (AI Studio)
- name: gemini
  kind: ai
  provider: gemini
  uri: "https://generativelanguage.googleapis.com/v1beta"
  api_key: "${GEMINI_API_KEY}"
  model: "gemini-2.0-flash"

# Deepseek (OpenAI-compatible)
- name: deepseek
  kind: ai
  uri: "https://api.deepseek.com/v1"
  api_key: "${DEEPSEEK_API_KEY}"
  model: "deepseek-chat"

# Ollama (local, OpenAI-compatible)
- name: ollama
  kind: ai
  uri: "http://localhost:11434/v1"
  api_key: "ollama"
  model: "llama3"
```

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `name` | yes | — | Connection name used in `ai_query("name", ...)` |
| `kind` | yes | — | Must be `ai` |
| `provider` | no | `openai` | Provider: `openai`, `anthropic`, or `gemini` |
| `uri` | yes | — | API base URL |
| `api_key` | yes | — | API key for authentication |
| `model` | yes | — | Default model name |
| `headers` | no | `{}` | Additional HTTP headers |
| `concurrency` | no | `10` | Max parallel AI calls |
| `timeout_secs` | no | `60` | HTTP request timeout |
| `max_tokens` | no | `1024` | Max response tokens |
| `temperature` | no | `0.0` | Model temperature |

**Error Handling:**

Failed AI calls don't abort the query. The cell gets an error marker string:
- `[AI Error: connection refused]` — gateway unreachable
- `[AI Error: timeout]` — request timed out
- `[AI Error: HTTP 401]` — authentication failed
- `[AI Error: empty response]` — no choices in response
- `[AI Error: invalid response format]` — unparseable JSON

If the prompt evaluates to NULL, the AI call is skipped and the cell is NULL.

### Named Parameters

Define reusable parameters for templated queries:

```sql
:start_date = "2024-01-01"
:end_date = "2024-12-31"

find [name, total, created_at] from orders
where created_at between :start_date and :end_date
```

---

## Joins

Joins combine rows from two or more tables based on a related column.

### Inner Join

An inner join returns only rows that have matching values in both tables. `join` is shorthand for `inner join`.

```sql
find [u.name, o.total]
from users as u
join orders as o on u.id = o.user_id
```

### Left Join

Returns all rows from the left table, with matched rows from the right (or NULL if no match):

```sql
find [u.name, o.total]
from users as u
left join orders as o on u.id = o.user_id
```

### Right Join

Returns all rows from the right table, with matched rows from the left:

```sql
find [u.name, o.total]
from users as u
right join orders as o on u.id = o.user_id
```

### Full Join

Returns all rows from both tables, with NULLs where there is no match on either side:

```sql
find [u.name, o.total]
from users as u
full join orders as o on u.id = o.user_id
```

### Cross Join

Returns the cartesian product of both tables. No `ON` clause is needed. Use with caution on large tables.

```sql
find [u.name, p.name as product]
from users as u
cross join products as p
limit 100
```

### Multiple Joins

Chain joins to combine three or more tables:

```sql
find [u.name, o.total, p.name as product]
from users as u
join orders as o on u.id = o.user_id
join order_items as oi on o.id = oi.order_id
join products as p on oi.product_id = p.id
```

### Self Join

Join a table to itself using different aliases:

```sql
find [e.name, m.name as manager]
from employees as e
left join employees as m on e.manager_id = m.id
```

### Table Aliases

The `as` keyword assigns a short alias to a table. Required when the same table appears multiple times, and useful for readability:

```sql
find [u.name, u.email]
from users as u
where u.status = "active"
```

### Joins with Filters

Combine joins with `where`:

```sql
find [u.name, o.total, o.status]
from users as u
join orders as o on u.id = o.user_id
where o.status = "paid" and o.total > 100
order by o.total desc
limit 20
```

### Qualified Column References

When columns have the same name in multiple tables, use `table.column` syntax:

```sql
find [u.name, o.status, p.name]
from users as u
join orders as o on u.id = o.user_id
join products as p on o.product_id = p.id
where u.status = "active"
```

---

## Aggregation

Aggregate functions compute a single value from a set of rows.

| Function              | Description                  |
|-----------------------|------------------------------|
| `count(*)`            | Count all rows               |
| `count(expr)`         | Count non-NULL values        |
| `count_distinct(expr)`| Count distinct non-NULL values |
| `sum(expr)`           | Sum of values                |
| `avg(expr)`           | Average (mean)               |
| `min(expr)`           | Minimum value                |
| `max(expr)`           | Maximum value                |

```sql
find [count(*)] from users
find [sum(total)] from orders
find [avg(salary)] from employees
find [min(price), max(price)] from products
```

### Multiple Aggregates

```sql
find [
  count(*) as total_orders,
  sum(total) as revenue,
  avg(total) as avg_order,
  min(total) as smallest,
  max(total) as largest
]
from orders
where status = "paid"
```

### GROUP BY

Group rows by one or more columns, then apply aggregates to each group:

```sql
find [department, count(*) as headcount]
from employees
group by department
```

Multiple grouping columns:

```sql
find [department, status, count(*) as cnt]
from employees
group by department, status
```

### GROUP BY with Full Projection

```sql
find [
  category,
  count(*) as product_count,
  avg(price) as avg_price,
  min(price) as cheapest,
  max(price) as most_expensive
]
from products
group by category
order by product_count desc
```

### HAVING

Filter groups after aggregation (unlike `where`, which filters rows before aggregation):

```sql
find [user_id, count(*) as order_count]
from orders
group by user_id
having count(*) > 5
```

### WHERE vs. HAVING

- `where` — filters individual rows before grouping
- `having` — filters groups after aggregation

```sql
find [department, avg(salary) as avg_sal]
from employees
where status = "active"
group by department
having avg(salary) > 75000
```

### Multiple HAVING Conditions

```sql
find [department, count(*) as cnt, avg(salary) as avg_sal]
from employees
group by department
having cnt > 3 and avg_sal > 50000
```

### COUNT DISTINCT

Count unique values in a column:

```sql
find [
  status,
  count(*) as total_orders,
  count_distinct(user_id) as unique_customers
]
from orders
group by status
```

### Aggregation with Joins

```sql
find [u.name, count(*) as order_count, sum(o.total) as total_spent]
 from users as u
 join orders as o on u.id = o.user_id
 where o.status = "paid"
 group by u.name
 having total_spent > 1000
 order by total_spent desc
 limit 10
```

---

## Window Functions

Window functions perform calculations across a set of rows related to the current row, without collapsing rows.

### Window Syntax

```sql
function() over (partition by col order by col)
```

- `partition by` — divide rows into groups (like GROUP BY, but rows are preserved)
- `order by` — define the order within each partition

### ROW_NUMBER

Assigns a sequential number to each row within a partition:

```sql
find [
  name,
  department,
  salary,
  row_number() over (partition by department order by salary desc) as rank
]
from employees
```

### RANK and DENSE_RANK

`rank()` — rows with equal values get the same rank, with gaps:

```sql
find [
  name,
  score,
  rank() over (order by score desc) as position
]
from players
```

If two players tie for 1st, the next player is 3rd (gap at 2nd).

`dense_rank()` — same, but no gaps:

```sql
find [
  name,
  score,
  dense_rank() over (order by score desc) as position
]
from players
```

If two players tie for 1st, the next player is 2nd (no gap).

### LAG and LEAD

Access values from previous or next rows:

```sql
find [
  date,
  revenue,
  lag(revenue, 1) over (order by date) as prev_day,
  lead(revenue, 1) over (order by date) as next_day
]
from daily_stats
```

- `lag(expr, N)` — value from N rows before
- `lead(expr, N)` — value from N rows after

Day-over-day change:

```sql
find [
  date,
  revenue,
  revenue - lag(revenue, 1) over (order by date) as daily_change
]
from daily_stats
```

### Running Totals

Use `sum()` as a window function:

```sql
find [
  date,
  amount,
  sum(amount) over (order by date) as running_total
]
from transactions
```

### Aggregates Over Windows

Any aggregate function can be used as a window function:

```sql
find [
  name,
  department,
  salary,
  avg(salary) over (partition by department) as dept_avg,
  salary - avg(salary) over (partition by department) as diff_from_avg
]
from employees
```

### Named Windows

Define a window specification once with `window`:

```sql
find [
  name,
  department,
  salary,
  avg(salary) over w as dept_avg,
  max(salary) over w as dept_max,
  min(salary) over w as dept_min
]
from employees
window w as (partition by department)
```

### Window Function Examples

**Top N Per Group** — top 3 highest-paid employees per department:

```sql
find * from (
  find [
    name,
    department,
    salary,
    row_number() over (partition by department order by salary desc) as rn
  ]
  from employees
) as ranked
where rn <= 3
```

**Percentage of Total:**

```sql
find [
  category,
  sum(total) as cat_revenue,
  sum(total) * 100.0 / sum(sum(total)) over () as pct_of_total
]
from orders
join products on orders.product_id = products.id
group by category
```

**Moving Average:**

```sql
find [
  date,
  revenue,
  avg(revenue) over (order by date) as cumulative_avg
]
from daily_stats
```

---

## Advanced Queries

### CTEs (Common Table Expressions)

Define temporary named result sets for use later in the query.

```sql
with active_users as (
  find * from users where status = "active"
)
find [name, email] from active_users
```

#### Multiple CTEs

Chain CTEs with commas:

```sql
with
  paid_orders as (
    find * from orders where status = "paid"
  ),
  user_totals as (
    find [user_id, sum(total) as revenue]
    from paid_orders
    group by user_id
  )
 find [u.name, ut.revenue]
 from users as u
 join user_totals as ut on u.id = ut.user_id
 where ut.revenue > 10
 order by ut.revenue desc
```

#### Recursive CTEs

For hierarchical data (org charts, category trees, etc.):

```sql
with recursive org_tree as (
  find * from employees where manager_id is null
  union all
  find [e.*]
  from employees as e
  join org_tree as t on e.manager_id = t.id
)
find * from org_tree
```

The first query is the base case; the `union all` query references the CTE itself to recurse.

### Subqueries

#### Scalar Subquery in WHERE

```sql
find [name, salary]
 from users
 where salary > (
  find [avg(salary)] from users
)
```

#### IN Subquery

```sql
find [name, department]
 from employees
where department in (
  find distinct [department] from departments
  where budget > 100000
)
```

#### NOT IN Subquery

```sql
find [name] from users
 where id not in (
  find distinct [user_id] from orders
  where created_at > now() - 365d
)
```

#### EXISTS / NOT EXISTS

```sql
find [name] from users as u
where exists (
  find [1] from orders as o where o.user_id = u.id
)
```

Find users with no orders:

```sql
find [name] from users as u
 where not exists (
  find [1] from orders as o where o.user_id = u.id
)
```

#### Scalar Subquery in SELECT

```sql
find [
  name,
  (find [count(*)] from orders where orders.user_id = users.id) as order_count
]

from users
```

#### Subquery in FROM (Derived Table)

```sql
find * from (
  find [user_id, sum(total) as revenue]
  from orders
  group by user_id
) as user_revenue
where revenue > 500
```

### Set Operations

#### UNION

```sql
find [name, email] from customers
union
find [name, email] from suppliers
```

#### UNION ALL

```sql
find [name, email] from customers
union all
find [name, email] from suppliers
```

#### INTERSECT

```sql
find [user_id] from orders where year = 2024
intersect
find [user_id] from orders where year = 2025
```

#### EXCEPT

```sql
find [user_id] from users
except
find [user_id] from orders where created_at > now() - 30d
```

### CASE Expressions

#### Searched CASE

```sql
find [
  name,
  salary,
  case
    when salary < 50000 then "Low"
    when salary >= 50000 and salary < 100000 then "Medium"
    when salary >= 100000 then "High"
    else "Unknown"
  end as salary_band
]

from users
```

#### Simple CASE

```sql
find [
  name,
  case status
    when "active" then "Active User"
    when "suspended" then "Suspended"
    else "Unknown"
  end as status_label
]

from users
```

#### CASE in ORDER BY

```sql
find [name, priority] from tasks
order by case priority
  when "urgent" then 1
  when "high" then 2
  when "normal" then 3
  else 4
end
```

#### CASE in WHERE

```sql
find [name, total] from orders
where case
  when user_id in (find [id] from vip_users) then total > 50
  else total > 100
end
```

### DISTINCT

Remove duplicate rows:

```sql
find distinct [department] from employees
find distinct [status] from orders
```

### EXPLAIN

View the query plan without executing:

```sql
explain find [name] from users
join orders on users.id = orders.user_id
```

---

## Schema-Qualified Tables

Prefix a table name with `schema.` to query tables in a specific database schema:

```sql
find * from public.users
```

When omitted, the connection's `schema` (configured in `river.yaml`) or the database-native default (e.g., `public` for Postgres) is used.

Combine with a connection reference:

```sql
find [name, email] from myschema.users@pg
```

Schema qualification works across all statement types:

```sql
describe public.users
describe inventory.products@pg
show tables @pg
create public.users { name: "Alice" }
update sales.orders@pg set status = "shipped" where id = 1
remove archive.logs@mongo where created_at < now() - 30d
```

For Postgres, MySQL, and MSSQL, `describe` filters by both table name and schema, avoiding ambiguity when the same table name exists in multiple schemas. SQLite ignores the schema prefix (SQLite has no schemas).

---

## Cross-Database Queries

River can query across different database systems in a single statement. Use `@connection-name` to specify which database a table lives in.

### Connection References

Append `@connection` to any table name:

```sql
find [name, email] from users@pg
```

### Cross-Database Joins

```sql
find [u.name, o.total]
 from users@pg as u
 join orders@mysql as o on u.id = o.user_id
 where o.status = "paid"
```

River fetches data from each source independently, performs the join, then applies filters and projections.

### Cross-Database CTEs

```sql
with
  pg_users as (
    find [id, name] from users@pg
    where status = "active"
  ),
  mongo_logs as (
    find [user_id, action, timestamp]
    from logs@mongo
    where timestamp > now() - 7d
  )
 find [pg_users.name, count(*) as login_count]
 from pg_users
 join mongo_logs on pg_users.id = mongo_logs.user_id
 where mongo_logs.action = "login"
 group by pg_users.name
 order by login_count desc
 limit 10
```

### Connection Configuration

Connections are defined in `river.yaml`:

NOTE: Avoid using `-` in connection names

```yaml
- name: pg
  kind: postgres
  uri: "postgres://river:river@localhost:5432/river"
  schema: public

- name: mysql
  kind: mysql
  uri: "mysql://river:river@localhost:3306/river"
  schema: river

- name: mongo
  kind: mongodb
  uri: "mongodb://localhost:27017"

- name: sqlite
  kind: sqlite
  uri: "sqlite:river.db?mode=rwc"

- name: openai
  kind: ai
  base_url: "https://api.openai.com/v1"
  api_key: "${OPENAI_API_KEY}"
  model: "gpt-4o"
```

### Meta Commands with Connections

Describe a remote table:

```sql
describe users@pg
```

List tables on a connection:

```sql
show tables @pg
show tables @mysql
```

### Performance Guidance

Cross-database joins fetch data from each source, then join locally. Recommendations:

- Push filters down with `where` before the join to reduce transferred rows
- Use CTEs or subqueries with `limit` to cap what is fetched
- Ensure join columns are indexed on both sides

Efficient pattern:

```sql
with
  recent_orders as (
    find [user_id, total] from orders@pg
    where created_at > now() - 7d
    limit 1000
  )
find [u.name, ro.total]
from users@mysql as u
join recent_orders as ro on u.id = ro.user_id
```

Inefficient pattern (avoid):

```sql
-- Fetches ALL orders and ALL users before joining
find [u.name, o.total]
from users@mysql as u
join orders@pg as o on u.id = o.user_id
```

---

## Data Modification

### INSERT — `create`

Single row (object syntax):

```sql
create users { name: "Alice", email: "alice@example.com", age: 30 }
```

Multiple rows (array syntax):

```sql
create users [
  { name: "Alice", email: "alice@example.com" },
  { name: "Bob", email: "bob@example.com" },
  { name: "Carol", email: "carol@example.com" }
]
```

Insert from query:

```sql
create active_users_backup (
  find * from users where status = "active"
)
```

Insert to a specific connection:

```sql
create users@pg { name: "Dave", email: "dave@example.com" }
```

### UPDATE

Modify existing rows with `update ... set ... where`:

```sql
update users
set status = "inactive", updated_at = now()
where last_login < now() - 90d
```

Update with expressions:

```sql
update products
set price = price * 1.1
where category = "premium"
```

Update on a specific connection:

```sql
update users@pg
set status = "verified"
where email_verified_at is not null
```

Without a `where` clause, the update applies to all rows. Always use `where` unless you intend a full-table update.

### DELETE — `remove`

```sql
remove users where status = "banned"
```

Delete with subquery:

```sql
remove users
where id not in (
  find distinct [user_id] from orders
  where created_at > now() - 365d
)
```

Delete on a specific connection:

```sql
remove logs@mongo
where timestamp < now() - 30d
```

Without `where`, all rows are deleted. Always use `where` unless you intend a full-table deletion.

### CREATE TABLE

Define and create a new table with column definitions:

```sql
create table products (
  name string,
  price float,
  category string default "general",
  created_at datetime
)
```

With primary key and constraints:

```sql
create table users (
  id int primary key,
  name string not null,
  email string not null,
  status string default "active"
)
```

On a specific connection:

```sql
create table archive.orders@pg (
  id int,
  total float,
  created_at datetime
)
```

With IF NOT EXISTS (idempotent):

```sql
create table if not exists cache (
  key string primary key,
  value json,
  expires_at datetime
)
```

### ALTER TABLE

Modify an existing table's structure:

```sql
-- Add a new column
alter table users add column bio string

-- Add a non-null column with default
alter table users add column tier string not null default "free"

-- Drop a column
alter table users drop column temp_data

-- Change a column's type
alter table users alter column age type float

-- Set not null + default
alter table users alter column status type string not null default "active"

-- Drop a default constraint
alter table users alter column status drop default

-- Rename a column
alter table users rename column name to full_name

-- Rename a table
alter table users rename to customers

-- On a specific connection
alter table users@pg add column notes string
```

### DROP TABLE

Remove a table from the database:

```sql
drop table users
```

With IF EXISTS (safe for scripts):

```sql
drop table if exists temp_logs
```

With CASCADE (drop dependent objects):

```sql
drop table users cascade
```

With RESTRICT (default — refuse if dependencies exist):

```sql
drop table users restrict
```

On a specific connection or schema:

```sql
drop table if exists archive.logs@pg cascade
```

### CREATE DATABASE

Create a new database on a connected server:

```sql
create database analytics
```

With IF NOT EXISTS (idempotent):

```sql
create database if not exists analytics
```

On a specific connection:

```sql
create database analytics@pg
create database reports@mysql
```

> **SQLite note:** SQLite does not support server-level databases. Use separate `.db` files with different URIs instead.
> **MongoDB note:** MongoDB databases are auto-created on first document insertion. `create database` is accepted but is effectively a no-op.

### DROP DATABASE

Remove a database from a connected server:

```sql
drop database analytics
```

With IF EXISTS (safe for scripts):

```sql
drop database if exists temp_logs
```

On a specific connection:

```sql
drop database analytics@pg
drop database reports@mysql
```

> **Warning:** `DROP DATABASE` is irreversible. All data in the database is permanently deleted.
> **SQLite note:** Use filesystem operations to delete the `.db` file. `drop database` returns an error.
> **MongoDB note:** `drop database` maps to MongoDB's `db.dropDatabase()` command.

### Persisting Query Results — `>>`

Save the results of any query to a table using `>>`:

```sql
find * from users >> user_backup
```

With conflict handling:

```sql
find [user_id, sum(total) as revenue]
from orders
group by user_id
>> user_revenue@pg
insert if exists on conflict replace
```

Ignore duplicates:

```sql
find distinct [email] from new_signups
>> verified_emails
insert if exists on conflict ignore
```

### Operations Reference

| Operation               | RiverQL                                        | SQL Equivalent                    |
|-------------------------|------------------------------------------------|-----------------------------------|
| Create table            | `create table t (col type, ...)`               | `CREATE TABLE t (...)`            |
| Insert one              | `create table { ... }`                         | `INSERT INTO table VALUES (...)`  |
| Insert many             | `create table [ {...}, {...} ]`                | `INSERT INTO table VALUES (...)`  |
| Insert from query       | `create table (find ...)`                      | `INSERT INTO table SELECT ...`    |
| Persist query results   | `find ... >> target`                           | `CREATE TABLE AS ... + INSERT`    |
| Persist with upsert     | `find ... >> target insert if exists on conflict replace` | `ON CONFLICT DO UPDATE` |
| Add column              | `alter table t add column col type ...`        | `ALTER TABLE t ADD COLUMN ...`    |
| Drop column             | `alter table t drop column col`                | `ALTER TABLE t DROP COLUMN col`   |
| Alter column type       | `alter table t alter column col type ...`      | `ALTER TABLE t ALTER COLUMN ...`  |
| Rename column           | `alter table t rename column col to new`       | `ALTER TABLE t RENAME COLUMN ...` |
| Rename table            | `alter table t rename to new`                  | `ALTER TABLE t RENAME TO ...`     |
| Drop table              | `drop table t`                                 | `DROP TABLE t`                    |
| Drop if exists          | `drop table if exists t`                       | `DROP TABLE IF EXISTS t`          |
| Drop cascade            | `drop table t cascade`                         | `DROP TABLE t CASCADE`            |
| Create database         | `create database name`                         | `CREATE DATABASE name`            |
| Create database if not  | `create database if not exists name`           | `CREATE DATABASE IF NOT EXISTS name`|
| Drop database           | `drop database name`                           | `DROP DATABASE name`              |
| Drop database if exists | `drop database if exists name`                 | `DROP DATABASE IF EXISTS name`    |
| Update                  | `update table set ... where ...`               | `UPDATE table SET ... WHERE ...`  |
| Delete                  | `remove table where ...`                       | `DELETE FROM table WHERE ...`     |

---

## Meta Commands

Meta commands inspect database structure and query plans.

### DESCRIBE

View the schema of a table:

```sql
describe users
```

Target a specific connection:

```sql
describe users@pg
describe orders@mysql
```

### SHOW TABLES

List all tables (or collections):

```sql
show tables
```

For a specific connection:

```sql
show tables @pg
show tables @mongo
```

### EXPLAIN

View the execution plan without running the query:

```sql
explain find [name] from users
where department = "Engineering"
order by salary desc
```

For cross-database queries:

```sql
explain find [u.name, o.total]
from users@pg as u
join orders@mysql as o on u.id = o.user_id
```

### Named Parameters

Define reusable values that persist for the session:

```sql
:start_date = "2024-01-01"
:end_date = "2024-12-31"
:min_amount = 100
```

Use in queries:

```sql
find [name, total, created_at] from orders
where created_at between :start_date and :end_date
  and total > :min_amount
```

### Comments

Line comments:

```sql
-- This is a comment
find users  -- inline comment
```

Block comments:

```sql
/* This is a
   multi-line comment */
find users where status = "active"
```

### Multiple Statements

Separate statements with semicolons:

```sql
:threshold = 1000;
find [name, total] from orders where total > :threshold;
describe orders
```

---

## Quick Reference

### Query Structure

```sql
find [columns] from table[@connection]
where condition
group by columns
having condition
window name as (spec)
order by column [asc|desc] [nulls first|last]
limit N offset M
```

### Keywords

| Keyword                                   | Purpose                  |
|-------------------------------------------|--------------------------|
| `find`                                    | Start a query (SELECT)   |
| `from`                                    | Specify source table(s)  |
| `where`                                   | Filter rows              |
| `join` / `left` / `right` / `full` / `cross` | Join tables           |
| `on`                                      | Join condition           |
| `as`                                      | Alias                    |
| `group by`                                | Group rows               |
| `having`                                  | Filter groups            |
| `order by`                                | Sort results             |
| `asc` / `desc`                            | Sort direction           |
| `nulls first` / `nulls last`              | NULL sort position       |
| `limit` / `offset`                        | Pagination               |
| `distinct`                                | Remove duplicates        |
| `with` / `recursive`                      | CTEs                     |
| `union` / `union all` / `intersect` / `except` | Set operations      |
| `case` / `when` / `then` / `else` / `end` | Conditional logic        |
| `exists` / `not exists`                   | Subquery predicate       |
| `in` / `not in` / `between`               | Set/range membership     |
| `like` / `ilike`                          | Pattern matching         |
| `is null` / `is not null`                 | NULL tests               |
| `and` / `or` / `not`                      | Logical operators        |
| `over` / `partition by` / `window`        | Window functions         |
| `create`                                  | INSERT / CREATE TABLE / CREATE DATABASE |
| `table`                                   | CREATE TABLE syntax      |
| `if` / `exists`                           | IF NOT EXISTS            |
| `insert`                                  | Persist query (`>>`)     |
| `conflict` / `ignore` / `replace`         | ON CONFLICT handling     |
| `update` / `set`                          | UPDATE                   |
| `remove`                                  | DELETE                   |
| `explain`                                 | Show query plan          |
| `alter`                                   | ALTER TABLE              |
| `add`                                     | ALTER TABLE ADD COLUMN   |
| `column`                                  | ALTER TABLE column ref   |
| `drop`                                    | DROP TABLE / DROP COLUMN / DROP DATABASE |
| `database`                                | CREATE DATABASE / DROP DATABASE          |
| `rename`                                  | ALTER TABLE RENAME       |
| `to`                                      | ALTER TABLE RENAME TO    |
| `type`                                    | ALTER COLUMN TYPE        |
| `cascade`                                 | DROP TABLE CASCADE       |
| `restrict`                                | DROP TABLE RESTRICT      |
| `describe`                                | Show table schema        |
| `show tables`                             | List tables              |

### Operators

| Operator           | Meaning               |
|--------------------|-----------------------|
| `=`                | Equal                 |
| `!=`, `<>`         | Not equal             |
| `<`, `>`, `<=`, `>=` | Comparison          |
| `+`, `-`, `*`, `/`, `%` | Arithmetic        |
| `\|\|`             | String concatenation  |
| `::`               | Type cast             |
| `>>`               | Persist query results  |
| `@`                | Connection reference  |
| `.`                | Schema/field separator |

### Operator Precedence (low to high)

1. `or`
2. `and`
3. `not`
4. `=`, `!=`, `<`, `>`, `<=`, `>=`, `like`, `ilike`, `in`, `between`, `is null`, `exists`
5. `+`, `-` (binary)
6. `*`, `/`, `%`
7. `-` (unary), `not`
8. `.` (field), `::` (cast), `()` (call)

### Literals

| Type      | Example             |
|-----------|---------------------|
| String    | `"hello"`           |
| Integer   | `42`                |
| Float     | `3.14`              |
| Boolean   | `true`, `false`     |
| NULL      | `null`              |
| Array     | `[1, 2, 3]`         |
| Object    | `{ key: "value" }`  |
| Interval  | `30d`, `1h`, `7w`   |
| Parameter | `:name`             |

### Interval Suffixes

| Suffix | Unit   |
|--------|--------|
| `y`    | Year   |
| `mon`  | Month  |
| `w`    | Week   |
| `d`    | Day    |
| `h`    | Hour   |
| `m`    | Minute |
| `s`    | Second |

### Aggregate Functions

```
count(*), count(expr), count_distinct(expr)
sum(expr), avg(expr), min(expr), max(expr)
```

### Window Functions

```
row_number() over (...)
rank() over (...)
dense_rank() over (...)
lag(expr, N) over (...)
lead(expr, N) over (...)
first_value(expr) over (...)
last_value(expr) over (...)
nth_value(expr, N) over (...)
```

Any aggregate can also be used as a window function:

```
sum(expr) over (...)
avg(expr) over (...)
```

### Built-in Functions

| Category    | Functions                                      |
|-------------|------------------------------------------------|
| String      | `upper()`, `lower()`, `concat()`, `length()`   |
| Math        | `abs()`, `round()`, `ceil()`, `floor()`, `power()`, `sqrt()` |
| Date/Time   | `now()`, `date_trunc()`, `extract()`           |
| Null Handling | `coalesce()`, `nullif()`, `ifnull()`         |
| Type        | `cast(expr as type)`                           |

### Data Types (for CAST)

`string`, `integer`, `float`, `boolean`, `datetime`, `json`

### Data Modification

```sql
-- Create table with columns
create table t (col1 type1, col2 type2 not null, ...)

-- Insert one
create table { col: value, ... }

-- Insert many
create table [{ ... }, { ... }]

-- Insert from query
create table (find ...)

-- Persist query results
find ... >> target [insert if exists on conflict (ignore | replace)]

-- Update
update table set col = value where ...

-- Delete
remove table where ...
```

### Cross-Database

```sql
-- Query a specific connection
find * from users@prod-pg

-- Join across databases
find [u.name, o.total]
from users@prod-pg as u
join orders@analytics-mysql as o on u.id = o.user_id
```

### Meta Commands

```sql
describe table[@connection]
show tables [@connection]
explain find ...
:param = value
```

---

## Cardinal Rules

1. Every query starts with `find`
2. Clause order: `find` -> `from` -> `where` -> `group by` -> `having` -> `order by` -> `limit` -> `offset`
3. Always use `where` with `update` and `remove`
4. The `@connection` suffix enables cross-database queries
5. Use `explain` before executing expensive queries
