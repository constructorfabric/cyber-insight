# Reading every layer from the custom assistant

> **For agentic workers:** each task below owns a disjoint set of files. Do not
> edit files outside your task. Interfaces between tasks are fixed here — build
> to them exactly, even if a neighbouring task has not landed yet.

**Goal:** the assistant can answer from bronze, silver, gold and identity, on
any stand, without a hardcoded list of databases.

**What is already true** (verified against the local stack, 2026-09-08):

- Bronze is one ClickHouse database per source: `bronze_github`, `bronze_jira`,
  … ~22 of them. Prod will have more, so nothing may be hardcoded.
- Silver is the `silver` database (56 tables).
- Gold materialises into `insight` — `dbt_project.yml` sets
  `gold_database: insight`. That is the same database v3 already queries.
- Identity exists twice: ClickHouse `identity` (6 tables) and the MariaDB
  `identity` schema that identity-resolution owns. Use the ClickHouse one; a
  single query cannot span engines.
- The service's ClickHouse user can already read all of it. Access is not the
  blocker.
- `presentation_ro` exists: a read-only-by-construction role over silver,
  identity and insight. It does not cover bronze.

**The two blockers:**

1. `MetricQuery` compiles `JSONExtractString(raw_data, '<field>')` and
   `FROM \`<table>\``. That fits v3's own ingest shape — one JSON column in the
   default database — and nothing else. Layer tables have typed columns and
   live in other databases.
2. ~350 tables across 25+ databases cannot go in the system prompt with their
   columns. The model needs the map in the prompt and the columns on demand.

## Global Constraints

- Read-only: the assistant's query path may only SELECT. No INSERT, no DDL, no
  mutations, ever.
- Nothing may hardcode a database name. A stand with an extra `bronze_*`
  database must work with no code change.
- `cargo clippy --all-targets -- -D warnings` must pass with zero errors, and
  `cargo fmt` must leave no diff. The crate denies `expect_used` in library
  code — use `let … else` with `panic!` in tests instead.
- Every identifier that reaches SQL is validated against
  `^[A-Za-z0-9_]{1,128}$` (`is_identifier`) or bound as a parameter. No
  interpolation of caller input, ever.
- Tests first. The existing suite is 103 tests; none may regress.

---

### Task 1: a query may name a database and a typed column

**Files:**
- Modify: `src/backend/services/insight-v3-core/src/metric_query.rs`

**Interfaces:**
- Produces: `MetricQuery` accepting two new shapes, and
  `MetricQuery::database() -> Option<&str>`.

`MetricQuery` today:

```rust
struct MetricQuery {
    table: String,
    fields: Vec<Field>,
    group_by: Vec<String>,
    filters: Vec<Filter>,
    order_by: Option<OrderBy>,
    limit: Option<u32>,
}
struct Field { json: String, r#type: FieldType, agg: Option<Agg>, as_name: String }
struct Filter { json: String, r#type: FieldType, op: FilterOp, value: Value }
```

Add:

- `#[serde(default)] database: Option<String>` on `MetricQuery`. When present,
  the FROM clause is `` FROM `db`.`table` ``; when absent it stays
  `` FROM `table` `` so every stored metric keeps working.
- `#[serde(default)] column: Option<String>` on `Field` and `Filter`, and make
  `json` optional (`#[serde(default)] json: Option<String>`).
  - `column` present → the expression is `` `column` `` (a real typed column).
  - `json` present → the expression stays `JSONExtract*(raw_data, '<json>')`.
  - Exactly one of the two must be present: neither and both are refused with a
    new `MetricQueryError` variant. A field that names both is a model
    confusing the two shapes, which must not reach the database.
- `database` is validated by `is_identifier` like `table`.

**Steps:**

- [ ] Write the failing tests in the existing `mod tests`:
  - a query with `database` compiles to `` FROM `silver`.`git_commits` ``
  - a field with `column` compiles to `` `lines_changed` AS `lines` `` with no
    `JSONExtract`
  - `agg` still wraps a column: `sum(`lines_changed`) AS `total``
  - a filter with `column` compiles to `` `event` = ? `` and still binds
  - a field with both `json` and `column` is refused
  - a field with neither is refused
  - a `database` outside the identifier charset is refused
  - the existing JSON-shape tests still pass untouched
- [ ] Run them and watch them fail
- [ ] Implement
- [ ] `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt`
- [ ] Commit

---

### Task 2: discover every table on the stand

**Files:**
- Create: `src/backend/services/insight-v3-core/src/catalog.rs`
- Modify: `src/backend/services/insight-v3-core/src/main.rs` (add `mod catalog;`
  only — one line, nothing else)

**Interfaces:**
- Produces, and Task 3 consumes exactly this:

```rust
pub(crate) enum Layer { Bronze, Silver, Gold, Identity, Ingest, Other }

pub(crate) struct TableSchema {
    pub(crate) database: String,
    pub(crate) table: String,
    pub(crate) layer: Layer,
    /// (column, ClickHouse type), in the table's own order.
    pub(crate) columns: Vec<(String, String)>,
}

pub(crate) struct Catalog { /* private */ }

impl Catalog {
    pub(crate) fn new(client: insight_clickhouse::Client, gold_database: String) -> Self;
    /// Every table the connected user can see, cached for CACHE_TTL.
    pub(crate) async fn tables(&self) -> Result<Vec<TableSchema>, CatalogError>;
    /// The named tables only, for a schema lookup. `database.table` or a bare
    /// table, which matches in any database.
    pub(crate) async fn describe(&self, names: &[String]) -> Result<Vec<TableSchema>, CatalogError>;
}
```

**How:** one query against `system.columns`, ordered so a table's columns stay
in position order:

```sql
SELECT database, table, name, type
FROM system.columns
WHERE database NOT IN ('system', 'information_schema', 'INFORMATION_SCHEMA', 'default')
ORDER BY database, table, position
```

Classify by name, never by a hardcoded list:
`bronze_*` → Bronze · `silver` → Silver · the configured gold database → Gold ·
`identity` → Identity · a table whose columns are exactly the v3 ingest schema
(`id`, `table_name`, `raw_data`, `received_at`) → Ingest · anything else →
Other. A stand with a new `bronze_x` database is Bronze with no code change.

Cache the whole listing behind a `tokio::sync::RwLock` with a 5-minute TTL:
schemas change when a migration runs, not per request, and this is read on
every chat turn.

**Steps:**

- [ ] Write the failing tests: classification per database name (including an
      unknown `bronze_whatever`), the ingest-schema case, that the SQL excludes
      the system databases, that `describe` accepts both `db.table` and a bare
      `table`, and that a second `tables()` call inside the TTL issues no
      second query (use `clickhouse::test::Mock` with one handler — the house
      pattern is in `src/tables.rs`)
- [ ] Run them and watch them fail
- [ ] Implement
- [ ] `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt`
- [ ] Commit

---

### Task 3: a read-only principal for the query path

**Files:**
- Modify: `src/ingestion/scripts/bootstrap-db/presentation-role.sql`
- Modify: `deploy/compose/clickhouse-init.sql`
- Modify: `src/backend/services/insight-v3-core/src/config.rs`
- Modify: `src/backend/services/insight-v3-core/src/gear.rs`
- Modify: `docker-compose.yml`

**What:** the assistant reads every layer, so it gets a principal that can only
read. Do NOT widen `presentation_ro` — that role carries a documented contract
for another service.

- A new role, in `presentation-role.sql` beside the existing one:

```sql
-- insight_v3_ro: the custom assistant's query path. Reads every layer,
-- including bronze databases this file cannot enumerate (a stand adds them
-- per source), and writes nothing anywhere.
CREATE ROLE IF NOT EXISTS insight_v3_ro;
GRANT SELECT ON *.* TO insight_v3_ro;
```

  `*.*` is deliberate: it covers a `bronze_*` database added after this file
  was written, and `system.columns`, which the catalogue reads. SELECT alone —
  no INSERT, no DDL, no mutations.

- A user carrying it, in `clickhouse-init.sql` next to how `presentation` is
  created, named `insight_v3_reader`, password from
  `${CLICKHOUSE_V3_READER_PASSWORD:-insight-v3-reader-local}`.

- Config: `clickhouse_query_user` / `clickhouse_query_password`, both
  defaulting to empty. When empty the existing credentials are used, so a
  stand that has not provisioned the reader keeps working — say so in the
  field's doc comment. Add a `ValidatedConfig::clickhouse_query_client()` that
  returns a client for the reader when configured and the ordinary client
  otherwise.

- `gear.rs`: `MetricRunner` and the `Catalog` take
  `clickhouse_query_client()`. Migrations, ingest and the definition store keep
  the existing client.

- `docker-compose.yml`: pass the two new settings to both v3 containers, next
  to the existing `clickhouse_user` lines.

**Steps:**

- [ ] Write the failing config tests: the query client falls back when the
      reader is unset, and uses the reader when set
- [ ] Run them and watch them fail
- [ ] Implement
- [ ] `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt`
- [ ] Commit

---

### Task 4 (owner: the session, after 1-3): the model learns what is where

**Files:** `src/chat.rs`, `src/api/chat.rs`

The prompt carries the map — every layer, its databases and its table names,
without columns. A `look_up` tool returns the columns of named tables, so the
model asks for the few it needs and the prompt stays small. The answer/create
tools gain `database` and `column`, mirroring Task 1.
