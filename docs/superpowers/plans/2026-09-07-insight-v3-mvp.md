# Insight v3 MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Push synthetic JSON into `insight-v3-core`, define a metric, a table widget, a graph widget and a dashboard as JSON at runtime, render that dashboard at `/portal/custom/{name}`, and do the same from a chat panel on the page.

**Architecture:** Definitions are JSON rows in three ClickHouse `ReplacingMergeTree` tables (`metrics`, `widgets`, `dashboards`), keyed by name, latest write wins. A metric's JSON is a structured query — table, fields with types, group-by, filters — that the service compiles to SQL over the `raw_data` JSON column. Widgets name a metric and how to draw it. The frontend fetches dashboard → widgets → metric results and renders from the JSON. The chat endpoint asks Claude for a definition and writes it through the same stores.

**Tech Stack:** Rust (axum via the gears toolkit, `insight_clickhouse`, `clickhouse` crate), ClickHouse, React 19 + TypeScript, TanStack Router + Query, recharts 3.10.1.

**Spec:** [docs/domain/insight-v3/specs/NOTES.md](../../domain/insight-v3/specs/NOTES.md) — "MVP check" section, plus [PRD.md](../../domain/insight-v3/specs/PRD.md) and [DESIGN.md](../../domain/insight-v3/specs/DESIGN.md).

## Global Constraints

- Everything lands in `src/backend/services/insight-v3-core` and `src/frontend`. Nothing from the analytics service is reused.
- Definitions carry no SQL. A metric names a table, fields, group-by and filters; the service compiles the SQL.
- Every identifier interpolated into SQL is validated against `^[A-Za-z0-9_]{1,128}$` before use. Values bind as parameters.
- The chat writes through the same endpoints the UI uses. It carries the portal session, never the ingest token.
- Every definition endpoint requires the portal session. The ingest endpoint keeps its own token; the two never mix.
- Definitions are global — no owner column, no per-session filtering. Per-user definitions are out of scope for this plan.
- Time is whatever a metric filters on. No dashboard period, no shared range.
- Date fields stay strings, compared lexically. `YYYY-MM-DD` sorts correctly; any other format does not, and the MVP does not guard against it.
- `/portal/custom` lists every dashboard and links to each one. `/portal/custom/{name}` renders one.
- The chat panel sits on both routes. A dashboard it creates appears in the list without a reload, and the page routes to it.
- `PUT` on a definition replaces it. The chat is the exception: it checks for the name first and refuses to overwrite.
- A widget whose metric fails to run shows an error in place of its content, naming what failed.
- New tests follow the existing patterns: `clickhouse::test::Mock` for store and SQL tests, `tests/*.sh` for end-to-end against a live stand.
- Backend commands run from `src/backend`: `cargo test -p insight-v3-core`. Frontend from `src/frontend`: `npm test`.
- The work lands on `feat/insight-v3-core-raw-data`, one commit per task, pushed to that branch — it is [PR #3255](https://github.com/constructorfabric/insight/pull/3255). No new branch, no new PR.
- Docker and `./dev-compose.sh up` work on this machine. The end-to-end script runs for real against that stack; it is not written and left unrun.
- The chat is tested against fixtures only. No live model call in any test, and no API key in the repo, a commit, or a test.
- Review media — screenshots and recordings from driving the UI — goes in `screenshots-and-etc/`, which is gitignored. Record with `agent-browser record start`, convert to GIF with `ffmpeg`. Nothing there is committed.
- The recording runs against the Vite dev server (`npm run dev`, port 3000) proxied to the compose backend, so a frontend change shows up without an image rebuild.
- The browser logs in through the real Keycloak form as `dev@company.nonpresent` with password `insight-dev` — the seeded persona convention in [deploy/compose/keycloak/README.md](../../../deploy/compose/keycloak/README.md). `AUTH_DISABLED` stays unset.

---

### Task 1: Definition storage and the three tables

**Files:**
- Create: `src/backend/services/insight-v3-core/src/definitions.rs`
- Modify: `src/backend/services/insight-v3-core/src/migration.rs`
- Modify: `src/backend/services/insight-v3-core/src/main.rs` (add `mod definitions;`)

**Interfaces:**
- Consumes: `insight_clickhouse::Client`, `crate::tables::TableName` (identifier validation pattern to copy, not call).
- Produces:
  - `DefinitionKind` — enum `Metric | Widget | Dashboard`, with `fn table(&self) -> &'static str` returning `"metrics" | "widgets" | "dashboards"`.
  - `DefinitionName` — newtype over `String`, `DefinitionName::parse(&str) -> Result<Self, DefinitionError>`, `as_str()`.
  - `DefinitionStore::new(client: insight_clickhouse::Client) -> Self`
  - `DefinitionStore::put(&self, kind: DefinitionKind, name: &DefinitionName, body: &serde_json::Value) -> Result<(), DefinitionStoreError>`
  - `DefinitionStore::get(&self, kind: DefinitionKind, name: &DefinitionName) -> Result<Option<serde_json::Value>, DefinitionStoreError>`
  - `DefinitionStore::list(&self, kind: DefinitionKind) -> Result<Vec<String>, DefinitionStoreError>`

- [ ] **Step 1: Write the failing tests**

In `src/definitions.rs`:

```rust
#[cfg(test)]
mod tests {
    use clickhouse::test::{Mock, handlers};
    use serde_json::json;

    use super::*;

    fn client(mock: &Mock) -> insight_clickhouse::Client {
        insight_clickhouse::Client::new(insight_clickhouse::Config::new(mock.url(), "insight"))
    }

    #[test]
    fn names_reject_anything_outside_the_identifier_charset() {
        assert!(DefinitionName::parse("commits_per_day").is_ok());
        assert!(DefinitionName::parse("").is_err());
        assert!(DefinitionName::parse("drop table").is_err());
        assert!(DefinitionName::parse("a`b").is_err());
        assert!(DefinitionName::parse(&"a".repeat(129)).is_err());
    }

    #[test]
    fn each_kind_has_its_own_table() {
        assert_eq!(DefinitionKind::Metric.table(), "metrics");
        assert_eq!(DefinitionKind::Widget.table(), "widgets");
        assert_eq!(DefinitionKind::Dashboard.table(), "dashboards");
    }

    #[tokio::test]
    async fn put_writes_the_body_into_the_kind_table() {
        let mock = Mock::new();
        let recording = mock.add(handlers::record::<DefinitionRow>());
        let store = DefinitionStore::new(client(&mock));
        let name = DefinitionName::parse("commits_per_day").expect("valid name");

        store
            .put(DefinitionKind::Metric, &name, &json!({ "table": "events" }))
            .await
            .expect("put succeeds");

        let rows: Vec<DefinitionRow> = recording.collect().await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "commits_per_day");
        assert_eq!(rows[0].body, r#"{"table":"events"}"#);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src/backend && cargo test -p insight-v3-core definitions`
Expected: FAIL — `definitions` module does not exist.

- [ ] **Step 3: Implement the store**

In `src/definitions.rs`:

```rust
use std::fmt;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

const MAX_NAME_CHARS: usize = 128;
const WRITE_TIMEOUT_SECS: u64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DefinitionKind {
    Metric,
    Widget,
    Dashboard,
}

impl DefinitionKind {
    pub(crate) fn table(&self) -> &'static str {
        match self {
            Self::Metric => "metrics",
            Self::Widget => "widgets",
            Self::Dashboard => "dashboards",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DefinitionName(String);

impl DefinitionName {
    pub(crate) fn parse(value: &str) -> Result<Self, DefinitionError> {
        if value.is_empty() || value.chars().count() > MAX_NAME_CHARS {
            return Err(DefinitionError::Name);
        }

        if !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(DefinitionError::Name);
        }

        Ok(Self(value.to_owned()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Serialize, Deserialize, clickhouse::Row)]
pub(crate) struct DefinitionRow {
    #[serde(with = "clickhouse::serde::uuid")]
    pub(crate) id: Uuid,
    pub(crate) name: String,
    pub(crate) body: String,
    #[serde(with = "clickhouse::serde::chrono::datetime64::millis")]
    pub(crate) updated_at: DateTime<Utc>,
}

pub(crate) struct DefinitionStore {
    client: insight_clickhouse::Client,
    timeout: Duration,
}

impl DefinitionStore {
    pub(crate) fn new(client: insight_clickhouse::Client) -> Self {
        Self {
            client,
            timeout: Duration::from_secs(WRITE_TIMEOUT_SECS),
        }
    }

    pub(crate) async fn put(
        &self,
        kind: DefinitionKind,
        name: &DefinitionName,
        body: &serde_json::Value,
    ) -> Result<(), DefinitionStoreError> {
        let row = DefinitionRow {
            id: Uuid::now_v7(),
            name: name.as_str().to_owned(),
            body: serde_json::to_string(body)?,
            updated_at: Utc::now(),
        };

        let write = async {
            let mut inserter = self.client.inner().insert(kind.table())?;
            inserter.write(&row).await?;
            inserter.end().await
        };

        tokio::time::timeout(self.timeout, write)
            .await
            .map_err(|_| DefinitionStoreError::Timeout)??;

        Ok(())
    }

    pub(crate) async fn get(
        &self,
        kind: DefinitionKind,
        name: &DefinitionName,
    ) -> Result<Option<serde_json::Value>, DefinitionStoreError> {
        let sql = format!(
            "SELECT id, name, body, updated_at FROM {} FINAL WHERE name = ? LIMIT 1",
            kind.table()
        );

        let rows = self
            .client
            .inner()
            .query(&sql)
            .bind(name.as_str())
            .fetch_all::<DefinitionRow>()
            .await?;

        match rows.into_iter().next() {
            Some(row) => Ok(Some(serde_json::from_str(&row.body)?)),
            None => Ok(None),
        }
    }

    pub(crate) async fn list(
        &self,
        kind: DefinitionKind,
    ) -> Result<Vec<String>, DefinitionStoreError> {
        let sql = format!("SELECT DISTINCT name FROM {} FINAL ORDER BY name", kind.table());

        Ok(self.client.inner().query(&sql).fetch_all::<String>().await?)
    }
}

impl fmt::Debug for DefinitionStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DefinitionStore")
            .field("timeout", &self.timeout)
            .finish()
    }
}

#[derive(Debug, Error)]
pub(crate) enum DefinitionError {
    #[error("definition names use letters, digits, underscore and dash, up to 128 characters")]
    Name,
}

#[derive(Debug, Error)]
pub(crate) enum DefinitionStoreError {
    #[error("the definition store timed out")]
    Timeout,
    #[error(transparent)]
    ClickHouse(#[from] clickhouse::error::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
```

Add `mod definitions;` to `src/main.rs`, beside `mod raw_data;`.

- [ ] **Step 4: Add the three tables to the migration**

In `src/migration.rs`, add beside `CREATE_RAW_DATA_TABLE`:

```rust
const CREATE_DEFINITION_TABLE: &str = "CREATE TABLE IF NOT EXISTS {table} (
    id UUID,
    name String,
    body String,
    updated_at DateTime64(3, 'UTC')
)
ENGINE = ReplacingMergeTree(updated_at)
ORDER BY name";

const DEFINITION_TABLES: [&str; 3] = ["metrics", "widgets", "dashboards"];
```

and in `migrate`, after the `raw_data` statement:

```rust
    for table in DEFINITION_TABLES {
        let ddl = CREATE_DEFINITION_TABLE.replace("{table}", table);
        client.inner().query(&ddl).execute().await?;
    }
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd src/backend && cargo test -p insight-v3-core definitions migration`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/backend/services/insight-v3-core/src/definitions.rs \
        src/backend/services/insight-v3-core/src/migration.rs \
        src/backend/services/insight-v3-core/src/main.rs
git commit -m "feat(insight-v3-core): store JSON definitions per kind"
```

---

### Task 2: Definition endpoints

**Files:**
- Create: `src/backend/services/insight-v3-core/src/api/definitions.rs`
- Modify: `src/backend/services/insight-v3-core/src/api.rs`
- Modify: `src/backend/services/insight-v3-core/src/gear.rs`

**Interfaces:**
- Consumes: `DefinitionStore`, `DefinitionKind`, `DefinitionName` from Task 1; `AppState`; the `OperationBuilder` pattern in `src/api/tables.rs`.
- Produces: `PUT /v1/metrics/{name}`, `GET /v1/metrics/{name}`, `GET /v1/metrics`, and the same three for `widgets` and `dashboards`. `AppState::definitions() -> &DefinitionStore`.

- [ ] **Step 1: Write the failing test**

In `src/api/definitions/tests.rs`:

```rust
#[tokio::test]
async fn put_then_get_returns_the_stored_body() {
    let harness = TestHarness::new().await;

    let put = harness
        .put_json("/v1/metrics/commits_per_day", json!({ "table": "events" }))
        .await;
    assert_eq!(put.status(), StatusCode::NO_CONTENT);

    let got = harness.get_json("/v1/metrics/commits_per_day").await;
    assert_eq!(got.status(), StatusCode::OK);
    assert_eq!(got.json().await, json!({ "table": "events" }));
}

#[tokio::test]
async fn get_missing_definition_is_not_found() {
    let harness = TestHarness::new().await;

    let got = harness.get_json("/v1/metrics/nope").await;
    assert_eq!(got.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_name_outside_the_charset_is_rejected() {
    let harness = TestHarness::new().await;

    let put = harness.put_json("/v1/metrics/drop%20table", json!({})).await;
    assert_eq!(put.status(), StatusCode::BAD_REQUEST);
}
```

Build `TestHarness` on the pattern already in `src/api/raw_data/tests.rs` — same mock ClickHouse client and router assembly.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd src/backend && cargo test -p insight-v3-core api::definitions`
Expected: FAIL — no such module.

- [ ] **Step 3: Implement the routes**

In `src/api/definitions.rs`, register three kinds through one helper:

```rust
use std::sync::Arc;

use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{Extension, Json, Router};
use toolkit::api::{OpenApiRegistry, OperationBuilder, ParamLocation, ParamSpec};
use toolkit::errors::CanonicalError;

use crate::api::AppState;
use crate::definitions::{DefinitionKind, DefinitionName};

pub(crate) fn register_routes(
    router: Router,
    openapi: &dyn OpenApiRegistry,
    state: Arc<AppState>,
) -> Router {
    let router = register_kind(router, openapi, state.clone(), DefinitionKind::Metric, "metrics");
    let router = register_kind(router, openapi, state.clone(), DefinitionKind::Widget, "widgets");

    register_kind(router, openapi, state, DefinitionKind::Dashboard, "dashboards")
}

fn register_kind(
    host_router: Router,
    openapi: &dyn OpenApiRegistry,
    state: Arc<AppState>,
    kind: DefinitionKind,
    segment: &str,
) -> Router {
    let name_param = ParamSpec::new("name", ParamLocation::Path).required(true);

    let put = OperationBuilder::put(&format!("/v1/{segment}/{{name}}"))
        .operation_id(&format!("insight_v3_core.{segment}.put"))
        .summary("Create or replace a definition")
        .param(name_param.clone())
        .json_request::<serde_json::Value>(openapi, "The definition body")
        .layer(Extension(state.clone()))
        .layer(Extension(kind))
        .handler(put_definition)
        .register(Router::new(), openapi);

    let get = OperationBuilder::get(&format!("/v1/{segment}/{{name}}"))
        .operation_id(&format!("insight_v3_core.{segment}.get"))
        .summary("Read a definition")
        .param(name_param)
        .layer(Extension(state.clone()))
        .layer(Extension(kind))
        .handler(get_definition)
        .register(Router::new(), openapi);

    let list = OperationBuilder::get(&format!("/v1/{segment}"))
        .operation_id(&format!("insight_v3_core.{segment}.list"))
        .summary("List definition names")
        .layer(Extension(state))
        .layer(Extension(kind))
        .handler(list_definitions)
        .register(Router::new(), openapi);

    host_router.merge(put).merge(get).merge(list)
}

async fn put_definition(
    Extension(state): Extension<Arc<AppState>>,
    Extension(kind): Extension<DefinitionKind>,
    Path(name): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, CanonicalError> {
    let name = DefinitionName::parse(&name).map_err(bad_request)?;
    state
        .definitions()
        .put(kind, &name, &body)
        .await
        .map_err(internal)?;

    Ok(StatusCode::NO_CONTENT)
}

async fn get_definition(
    Extension(state): Extension<Arc<AppState>>,
    Extension(kind): Extension<DefinitionKind>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, CanonicalError> {
    let name = DefinitionName::parse(&name).map_err(bad_request)?;

    match state.definitions().get(kind, &name).await.map_err(internal)? {
        Some(body) => Ok(Json(body).into_response()),
        None => Ok(StatusCode::NOT_FOUND.into_response()),
    }
}

async fn list_definitions(
    Extension(state): Extension<Arc<AppState>>,
    Extension(kind): Extension<DefinitionKind>,
) -> Result<impl IntoResponse, CanonicalError> {
    let names = state.definitions().list(kind).await.map_err(internal)?;

    Ok(Json(serde_json::json!({ "names": names })))
}
```

Write `bad_request` and `internal` as thin wrappers over `CanonicalError`, copying the shape used in `src/api/tables.rs`.

Add the store to `AppState` (a third constructor argument and a `definitions()` accessor), build it in `gear.rs` beside `TableStore`, and call `definitions::register_routes` from `api::register_routes`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src/backend && cargo test -p insight-v3-core api::definitions`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/backend/services/insight-v3-core/src/api/definitions.rs \
        src/backend/services/insight-v3-core/src/api/definitions \
        src/backend/services/insight-v3-core/src/api.rs \
        src/backend/services/insight-v3-core/src/gear.rs
git commit -m "feat(insight-v3-core): serve metric, widget and dashboard definitions"
```

---

### Task 3: Compile a metric to SQL and run it

**Files:**
- Create: `src/backend/services/insight-v3-core/src/metric_query.rs`
- Create: `src/backend/services/insight-v3-core/src/api/metric_run.rs`
- Modify: `src/backend/services/insight-v3-core/src/api.rs`, `src/main.rs`

**Interfaces:**
- Consumes: `DefinitionStore::get`, `DefinitionName`.
- Produces:
  - `MetricQuery` — `serde::Deserialize` over the metric JSON: `{ table: String, fields: Vec<Field>, group_by: Vec<String>, filters: Vec<Filter>, limit: Option<u32> }` where `Field { json: String, r#type: FieldType, agg: Option<Agg>, as_name: String }`, `FieldType { String, Int, Float }`, `Agg { Count, Sum, Avg, Min, Max }`, `Filter { json: String, r#type: FieldType, op: FilterOp, value: serde_json::Value }`, `FilterOp { Eq, Ne, Gt, Gte, Lt, Lte }`.
  - `MetricQuery::compile(&self) -> Result<CompiledQuery, MetricQueryError>` where `CompiledQuery { sql: String, binds: Vec<String> }`.
  - `POST /v1/metrics/{name}/run` returning `{ "columns": [String], "rows": [[serde_json::Value]] }`.

- [ ] **Step 1: Write the failing tests**

In `src/metric_query.rs`:

```rust
#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn query(value: serde_json::Value) -> MetricQuery {
        serde_json::from_value(value).expect("valid metric")
    }

    #[test]
    fn a_grouped_count_compiles_to_json_extraction_over_the_payload() {
        let metric = query(json!({
            "table": "events",
            "fields": [
                { "json": "day", "type": "string", "as_name": "day" },
                { "json": "lines", "type": "int", "agg": "sum", "as_name": "lines" }
            ],
            "group_by": ["day"],
            "filters": [],
            "limit": 100
        }));

        let compiled = metric.compile().expect("compiles");

        assert_eq!(
            compiled.sql,
            "SELECT JSONExtractString(raw_data, 'day') AS `day`, \
             sum(JSONExtractInt(raw_data, 'lines')) AS `lines` \
             FROM `events` GROUP BY `day` ORDER BY `day` LIMIT 100"
        );
        assert!(compiled.binds.is_empty());
    }

    #[test]
    fn filters_bind_their_values() {
        let metric = query(json!({
            "table": "events",
            "fields": [{ "json": "author", "type": "string", "as_name": "author" }],
            "group_by": [],
            "filters": [
                { "json": "event", "type": "string", "op": "eq", "value": "commit" }
            ]
        }));

        let compiled = metric.compile().expect("compiles");

        assert!(compiled.sql.contains("WHERE JSONExtractString(raw_data, 'event') = ?"));
        assert_eq!(compiled.binds, vec!["commit".to_owned()]);
    }

    #[test]
    fn an_identifier_outside_the_charset_is_refused() {
        let metric = query(json!({
            "table": "events`; DROP TABLE events; --",
            "fields": [{ "json": "day", "type": "string", "as_name": "day" }],
            "group_by": [],
            "filters": []
        }));

        assert!(matches!(metric.compile(), Err(MetricQueryError::Identifier(_))));
    }

    #[test]
    fn a_metric_with_no_fields_is_refused() {
        let metric = query(json!({
            "table": "events", "fields": [], "group_by": [], "filters": []
        }));

        assert!(matches!(metric.compile(), Err(MetricQueryError::NoFields)));
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src/backend && cargo test -p insight-v3-core metric_query`
Expected: FAIL — no such module.

- [ ] **Step 3: Implement the compiler**

Rules the implementation must follow:

- Every identifier — `table`, each `json` key, each `as_name`, each `group_by` entry — passes `is_identifier`: non-empty, at most 128 chars, only `[A-Za-z0-9_]`. Anything else returns `MetricQueryError::Identifier(name)`.
- A field renders as `JSONExtractString|JSONExtractInt|JSONExtractFloat(raw_data, '<json>')`, wrapped in `agg(...)` when present, then `` AS `<as_name>` ``.
- `group_by` entries render as backticked `as_name`s and are also the `ORDER BY`.
- Filters render as `<extract> <op> ?` joined by ` AND `, with each value pushed onto `binds` as a string.
- `limit` defaults to 1000 and caps at 10000.
- Empty `fields` returns `MetricQueryError::NoFields`.

```rust
fn is_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= 128
        && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src/backend && cargo test -p insight-v3-core metric_query`
Expected: PASS.

- [ ] **Step 5: Add the run endpoint**

`POST /v1/metrics/{name}/run` — load the definition, `serde_json::from_value::<MetricQuery>`, compile, bind each value in order, `fetch_all` into `Vec<Vec<serde_json::Value>>`, respond `{ "columns": [...], "rows": [...] }`. A missing metric is 404; an uncompilable one is 400 with the compiler's message.

Test it in `src/api/metric_run.rs` with the mock client: a stored metric plus a recorded query, asserting the response carries the columns in field order.

- [ ] **Step 6: Run the tests and commit**

Run: `cd src/backend && cargo test -p insight-v3-core`

```bash
git add src/backend/services/insight-v3-core/src/metric_query.rs \
        src/backend/services/insight-v3-core/src/api/metric_run.rs \
        src/backend/services/insight-v3-core/src/api.rs \
        src/backend/services/insight-v3-core/src/main.rs
git commit -m "feat(insight-v3-core): compile metric definitions to SQL and run them"
```

---

### Task 4: Synthetic data and an end-to-end shell test

**Files:**
- Create: `src/backend/services/insight-v3-core/tests/mvp.sh`
- Create: `src/backend/services/insight-v3-core/tests/fixtures/events.jsonl`

**Interfaces:**
- Consumes: `PUT /v1/tables/{table}`, `POST /v1/raw-data`, every endpoint from Tasks 2 and 3, and `tests/lib/insight_stand/service_token.py` (`open_service_session`, `default_token_url`) for the session the definition endpoints require.
- Produces: a script that proves steps 0-4 of the MVP against the compose stack.

**Credentials:** ingest calls carry the ingest token. Definition calls carry a bearer token minted by `service_token.py` — it POSTs `grant_type=client_credentials` to the authenticator's token listener. The two never share a credential.

- [ ] **Step 1: Write the fixture**

30 lines of synthetic events, three authors over ten days:

```json
{"author":"a@example.com","event":"commit","day":"2026-09-01","lines":42}
{"author":"b@example.com","event":"commit","day":"2026-09-01","lines":17}
{"author":"a@example.com","event":"review","day":"2026-09-02","lines":0}
```

- [ ] **Step 2: Write the script**

Follow `tests/raw_data.sh` for the env-var names and the cleanup trap, but run against the compose stack rather than a standalone service — the definition endpoints need the authenticator, so `./dev-compose.sh up` must be up first. The body:

0. Mint a bearer token: `python3 -c "from insight_stand.service_token import open_service_session; ..."` with `PYTHONPATH=tests/lib`, and export it as `TOKEN`. Fail with a clear message if the authenticator is unreachable — that means the stack is not up.
1. `PUT /v1/tables/events`
2. `POST /v1/raw-data` per fixture line, with the ingest token
   (every call from here down carries `Authorization: Bearer $TOKEN` instead)
3. `PUT /v1/metrics/commits_per_day` with the grouped-count metric from Task 3's first test
4. `POST /v1/metrics/commits_per_day/run`, assert ten rows and the two column names
5. `PUT /v1/widgets/commits_table` — `{"type":"table","metric":"commits_per_day","columns":["day","lines"]}`
6. `PUT /v1/widgets/commits_graph` — `{"type":"line","metric":"commits_per_day","x":"day","y":"lines"}`
7. `PUT /v1/dashboards/engineering` — `{"title":"Engineering","widgets":["commits_table","commits_graph"]}`
8. `GET /v1/dashboards/engineering`, assert both widget names come back

- [ ] **Step 3: Run it against the stack**

Run: `./dev-compose.sh up` from the repo root, then `cd src/backend/services/insight-v3-core && ./tests/mvp.sh`
Expected: exits 0, printing each step. A 401 on a definition call means the token step failed — fix that rather than loosening the endpoint's auth.

- [ ] **Step 4: Commit**

```bash
git add src/backend/services/insight-v3-core/tests/mvp.sh \
        src/backend/services/insight-v3-core/tests/fixtures/events.jsonl
git commit -m "test(insight-v3-core): end-to-end MVP script over synthetic events"
```

---

### Task 5: The custom dashboard route

**Files:**
- Create: `src/frontend/src/routes/portal.custom.index.tsx`
- Create: `src/frontend/src/routes/portal.custom.$name.tsx`
- Create: `src/frontend/src/api/custom.ts`
- Create: `src/frontend/src/queries/custom.ts`
- Test: `src/frontend/src/routes/portal.custom.index.test.tsx`
- Test: `src/frontend/src/routes/portal.custom.$name.test.tsx`

**Interfaces:**
- Consumes: the endpoints from Tasks 2 and 3.
- Produces:
  - `api/custom.ts` — `fetchDashboardNames(): Promise<string[]>` (over `GET /v1/dashboards`, which returns `{ names: [...] }`), `fetchDashboard(name: string): Promise<Dashboard>`, `fetchWidget(name: string): Promise<Widget>`, `runMetric(name: string): Promise<MetricResult>`, with `Dashboard { title: string; widgets: string[] }`, `Widget` a discriminated union on `type` of `TableWidget { type: "table"; metric: string; columns: string[] }` and `LineWidget { type: "line"; metric: string; x: string; y: string }`, `MetricResult { columns: string[]; rows: unknown[][] }`.
  - `queries/custom.ts` — `dashboardNamesQuery()`, `dashboardQuery(name)`, `widgetQuery(name)`, `metricResultQuery(name)` as TanStack query options.
  - Route `/portal/custom`, listing every dashboard name as a link to its page.
  - Route `/portal/custom/$name`, rendering the dashboard title and one block per widget.

- [ ] **Step 1: Write the failing test for the list**

```tsx
it("lists every dashboard as a link", async () => {
  server.use(
    http.get("*/v1/dashboards", () =>
      HttpResponse.json({ names: ["engineering", "delivery"] }),
    ),
  );

  renderRoute("/portal/custom");

  expect(await screen.findByRole("link", { name: "engineering" })).toHaveAttribute(
    "href",
    "/portal/custom/engineering",
  );
  expect(await screen.findByRole("link", { name: "delivery" })).toBeInTheDocument();
});

it("says so when there are no dashboards yet", async () => {
  server.use(http.get("*/v1/dashboards", () => HttpResponse.json({ names: [] })));

  renderRoute("/portal/custom");

  expect(await screen.findByText(/no dashboards yet/i)).toBeInTheDocument();
});
```

- [ ] **Step 2: Write the failing test for one dashboard**

```tsx
it("renders a widget per name in the dashboard", async () => {
  server.use(
    http.get("*/v1/dashboards/engineering", () =>
      HttpResponse.json({ title: "Engineering", widgets: ["commits_table"] }),
    ),
    http.get("*/v1/widgets/commits_table", () =>
      HttpResponse.json({ type: "table", metric: "commits_per_day", columns: ["day", "lines"] }),
    ),
    http.post("*/v1/metrics/commits_per_day/run", () =>
      HttpResponse.json({ columns: ["day", "lines"], rows: [["2026-09-01", 59]] }),
    ),
  );

  renderRoute("/portal/custom/engineering");

  expect(await screen.findByText("Engineering")).toBeInTheDocument();
  expect(await screen.findByRole("cell", { name: "2026-09-01" })).toBeInTheDocument();
  expect(await screen.findByRole("cell", { name: "59" })).toBeInTheDocument();
});
```

Use the mock-server and render helpers already in `src/test/`.

- [ ] **Step 3: Run both to verify they fail**

Run: `cd src/frontend && npm test -- portal.custom`
Expected: FAIL — neither route exists.

- [ ] **Step 4: Implement both routes**

`/portal/custom` fetches the dashboard names and renders a link per name, or a line saying there are none. `/portal/custom/$name` loads the dashboard, then each widget by name, then each widget's metric result. Fetching per widget is deliberate: the page renders its title and the widgets it has while the rest resolve.

- [ ] **Step 5: Run them to verify they pass**

Run: `cd src/frontend && npm test -- portal.custom`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/frontend/src/routes/portal.custom.index.tsx \
        src/frontend/src/routes/portal.custom.index.test.tsx \
        src/frontend/src/routes/portal.custom.$name.tsx \
        src/frontend/src/routes/portal.custom.$name.test.tsx \
        src/frontend/src/api/custom.ts src/frontend/src/queries/custom.ts
git commit -m "feat(frontend): list custom dashboards and render one"
```

---

### Task 6: The two renderers

**Files:**
- Create: `src/frontend/src/components/custom/custom-table.tsx`
- Create: `src/frontend/src/components/custom/custom-line-chart.tsx`
- Create: `src/frontend/src/components/custom/custom-widget.tsx`
- Test: `src/frontend/src/components/custom/custom-widget.test.tsx`

**Interfaces:**
- Consumes: `Widget`, `MetricResult` from Task 5.
- Produces: `CustomWidget({ widget, result, error }): JSX.Element` — switches on `widget.type`, renders `CustomTable` or `CustomLineChart`, and handles the three failure shapes before it draws anything:
  - `error` set — an `role="alert"` box carrying the message, in place of the content.
  - `result.rows` empty — a "no data" line.
  - a type it does not know — a plain message, not a throw.

- [ ] **Step 1: Write the failing test**

```tsx
const result = { columns: ["day", "lines"], rows: [["2026-09-01", 59], ["2026-09-02", 12]] };

it("renders a table widget as rows", () => {
  render(<CustomWidget widget={{ type: "table", metric: "m", columns: ["day", "lines"] }} result={result} />);

  expect(screen.getByRole("columnheader", { name: "day" })).toBeInTheDocument();
  expect(screen.getAllByRole("row")).toHaveLength(3);
});

it("renders a line widget as a chart", () => {
  render(<CustomWidget widget={{ type: "line", metric: "m", x: "day", y: "lines" }} result={result} />);

  expect(screen.getByTestId("custom-line-chart")).toBeInTheDocument();
});

it("shows an error in place of the content when the run failed", () => {
  render(
    <CustomWidget
      widget={{ type: "table", metric: "m", columns: ["day"] }}
      error={new Error("unknown table `evnts`")}
    />,
  );

  expect(screen.getByRole("alert")).toHaveTextContent("unknown table `evnts`");
});

it("says there is no data when the metric returned no rows", () => {
  render(
    <CustomWidget widget={{ type: "table", metric: "m", columns: ["day"] }} result={{ columns: ["day"], rows: [] }} />,
  );

  expect(screen.getByText(/no data/i)).toBeInTheDocument();
});

it("says so when the type is unknown", () => {
  render(<CustomWidget widget={{ type: "sankey", metric: "m" } as never} result={result} />);

  expect(screen.getByText(/unknown widget type/i)).toBeInTheDocument();
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd src/frontend && npm test -- custom-widget`
Expected: FAIL — the components do not exist.

- [ ] **Step 3: Implement the renderers**

`CustomTable` maps `result.columns` to headers and `result.rows` to cells. `CustomLineChart` maps rows to `{ [x]: value, [y]: value }` objects and renders a recharts `LineChart` inside a `ResponsiveContainer`, with `data-testid="custom-line-chart"` on the wrapper. Both take the same `MetricResult`, so a new widget type is a new branch here and nothing else.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src/frontend && npm test -- custom`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/frontend/src/components/custom
git commit -m "feat(frontend): table and line renderers driven by the widget JSON"
```

---

### Task 7: The chat endpoint

**Files:**
- Create: `src/backend/services/insight-v3-core/src/chat.rs`
- Create: `src/backend/services/insight-v3-core/src/api/chat.rs`
- Modify: `src/backend/services/insight-v3-core/src/config.rs`, `src/api.rs`, `src/main.rs`, `config/insight.yaml`

**Interfaces:**
- Consumes: `DefinitionStore`, `MetricQuery` (to validate what the model returns before storing), `TableStore::sample_fields` from Step 0 below.
- Produces:
  - Config field `anthropic_token: SecretString`, alongside `ingest_token`.
  - `ChatClient::new(token: &SecretString, model: String) -> Self`, `ChatClient::propose(&self, message: &str, tables: &[String]) -> Result<Proposal, ChatError>`.
  - `Proposal` is one of two intents the model returns:
    - `Proposal::Answer { reply: String, query: MetricQuery }` — a one-time question. The service runs the query and answers. Nothing is stored.
    - `Proposal::Create { reply: String, metric: Option<(String, serde_json::Value)>, widgets: Vec<(String, serde_json::Value)>, dashboard: Option<(String, serde_json::Value)> }` — the service stores what it accepts.
  - `POST /v1/chat` — `{ "message": String }` in. Out for an answer: `{ "reply": String, "result": { "columns": [String], "rows": [[Value]] } }`. Out for a creation: `{ "reply": String, "created": { "metric": Option<String>, "widgets": [String], "dashboard": Option<String> }, "skipped": [{ "kind": String, "name": String, "reason": "exists" }] }`.
  - The handler calls `DefinitionStore::get` for every name the model proposes. A name already in use is skipped, never overwritten, and comes back in `skipped` so the reply can say so.

- [ ] **Step 0a: Add a canned-reply mode**

The recording has to show the chat working, and no test may call the model. So the chat has two modes, chosen by config:

```yaml
  insight-v3-core:
    config:
      anthropic_token: ""
      chat_mode: "live"   # live | canned
```

In `canned` mode `ChatClient::propose` returns a fixed `Proposal::Create` — one metric, one table widget, one line widget, one dashboard, named after the message's first word — without any network call. Everything downstream is identical: the same validation, the same stores, the same responses. A test asserts that `canned` mode makes no HTTP request and still produces a stored dashboard.

This is what the GIF records. It proves the plumbing, not the model's judgement, and the plan says so rather than implying otherwise.

- [ ] **Step 0b: Sample the field names a table holds**

The chat is useless without them — it will invent field names that compile and return nothing.

Add to `src/tables.rs`:

```rust
impl TableStore {
    /// Field names and inferred types, read from the most recent rows.
    pub(crate) async fn sample_fields(
        &self,
        table: &TableName,
    ) -> Result<Vec<(String, &'static str)>, TableStoreError> {
        let sql = format!(
            "SELECT raw_data FROM `{}` ORDER BY received_at DESC LIMIT 20",
            table.as_str()
        );

        let payloads = self.client.inner().query(&sql).fetch_all::<String>().await?;
        let mut fields: Vec<(String, &'static str)> = Vec::new();

        for payload in payloads {
            let Ok(serde_json::Value::Object(map)) = serde_json::from_str(&payload) else {
                continue;
            };

            for (key, value) in map {
                if fields.iter().any(|(name, _)| name == &key) {
                    continue;
                }

                let kind = match value {
                    serde_json::Value::Number(number) if number.is_i64() => "int",
                    serde_json::Value::Number(_) => "float",
                    _ => "string",
                };

                fields.push((key, kind));
            }
        }

        Ok(fields)
    }
}
```

Test it with the mock client: two rows with overlapping keys yield one entry per key, and an integer field types as `int`.

- [ ] **Step 1: Write the failing test**

Assert on parsing and validation, not on the network:

```rust
#[test]
fn an_answer_intent_carries_a_query_and_stores_nothing() {
    let reply = r#"{"intent":"answer","reply":"About 59 lines on the first day","query":{"table":"events","fields":[{"json":"day","type":"string","as_name":"day"},{"json":"lines","type":"int","agg":"sum","as_name":"lines"}],"group_by":["day"],"filters":[]}}"#;

    match Proposal::parse(reply).expect("parses") {
        Proposal::Answer { reply, query } => {
            assert_eq!(reply, "About 59 lines on the first day");
            query.compile().expect("the query compiles");
        }
        Proposal::Create { .. } => panic!("expected an answer"),
    }
}

#[test]
fn a_create_intent_is_read_out_of_the_model_reply() {
    let reply = r#"{"intent":"create","reply":"Here you go","metric":{"name":"commits_per_day","body":{"table":"events","fields":[{"json":"day","type":"string","as_name":"day"}],"group_by":["day"],"filters":[]}},"widgets":[{"name":"commits_table","body":{"type":"table","metric":"commits_per_day","columns":["day"]}}],"dashboard":{"name":"engineering","body":{"title":"Engineering","widgets":["commits_table"]}}}"#;

    match Proposal::parse(reply).expect("parses") {
        Proposal::Create { reply, widgets, .. } => {
            assert_eq!(reply, "Here you go");
            assert_eq!(widgets.len(), 1);
        }
        Proposal::Answer { .. } => panic!("expected a creation"),
    }
}

#[test]
fn a_metric_the_compiler_refuses_is_not_stored() {
    let reply = r#"{"intent":"create","reply":"x","metric":{"name":"bad","body":{"table":"events`--","fields":[],"group_by":[],"filters":[]}},"widgets":[],"dashboard":null}"#;

    assert!(matches!(Proposal::parse(reply), Err(ChatError::Metric(_))));
}

#[test]
fn prose_around_the_json_is_tolerated() {
    let reply = "Sure!\n```json\n{\"intent\":\"create\",\"reply\":\"ok\",\"widgets\":[],\"dashboard\":null}\n```";

    assert!(matches!(Proposal::parse(reply), Ok(Proposal::Create { .. })));
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd src/backend && cargo test -p insight-v3-core chat`
Expected: FAIL — no such module.

- [ ] **Step 3: Implement the client and the endpoint**

No test in this task calls the model. `ChatClient::propose` is exercised through `Proposal::parse` on fixture replies; the live round trip is verified by hand afterwards.

`ChatClient::propose` posts to `https://api.anthropic.com/v1/messages` with `x-api-key`, `anthropic-version: 2023-06-01`, model `claude-sonnet-5`, and a system prompt that states the two intents, the metric/widget/dashboard JSON shapes, and — per table — its name and the fields `sample_fields` found with their types, and demands a single JSON object in reply. The prompt says: answer a question with `intent: "answer"` and a query; build something with `intent: "create"`.

`Proposal::parse` strips any code fence, deserializes on `intent`, and compiles every query and every proposed metric with `MetricQuery::compile` — a refusal is `ChatError::Metric`, and nothing runs or is stored.

The handler branches on the intent: an answer compiles the query, runs it, and returns `reply` plus `result`; a creation writes each accepted definition through `DefinitionStore::put` and returns what it created. The endpoint sits behind the portal session, with no ingest-token layer.

- [ ] **Step 4: Write the failing test for the existence check**

```rust
#[tokio::test]
async fn a_name_already_in_use_is_skipped_not_overwritten() {
    let harness = TestHarness::new().await;
    harness
        .put_json("/v1/widgets/commits_table", json!({ "type": "table", "metric": "m", "columns": [] }))
        .await;

    let created = harness.chat_creating_widget("commits_table").await;

    assert_eq!(created.skipped, vec![Skipped { kind: "widget".into(), name: "commits_table".into(), reason: "exists".into() }]);
    assert!(created.created.widgets.is_empty());

    let stored = harness.get_json("/v1/widgets/commits_table").await.json().await;
    assert_eq!(stored["metric"], "m");
}
```

- [ ] **Step 5: Run the tests and commit**

Run: `cd src/backend && cargo test -p insight-v3-core`

```bash
git add src/backend/services/insight-v3-core/src/chat.rs \
        src/backend/services/insight-v3-core/src/api/chat.rs \
        src/backend/services/insight-v3-core/src/config.rs \
        src/backend/services/insight-v3-core/src/api.rs \
        src/backend/services/insight-v3-core/src/main.rs \
        src/backend/services/insight-v3-core/config/insight.yaml
git commit -m "feat(insight-v3-core): a chat that writes definitions"
```

---

### Task 8: The chat panel

**Files:**
- Create: `src/frontend/src/components/custom/custom-chat.tsx`
- Modify: `src/frontend/src/routes/portal.custom.index.tsx`
- Modify: `src/frontend/src/routes/portal.custom.$name.tsx`
- Modify: `src/frontend/src/api/custom.ts`
- Test: `src/frontend/src/components/custom/custom-chat.test.tsx`
- Test: `src/frontend/src/routes/portal.custom.index.test.tsx`

**Interfaces:**
- Consumes: `POST /v1/chat` from Task 7.
- Produces: `sendChat(message: string): Promise<ChatReply>` with `ChatReply { reply: string; result?: { columns: string[]; rows: unknown[][] }; created?: { metric?: string; widgets: string[]; dashboard?: string }; skipped?: { kind: string; name: string; reason: string }[] }`, and `CustomChat({ onCreated }: { onCreated: (created: ChatReply["created"]) => void })` — a panel on both the index and the dashboard page.
- Behaviour: a reply carrying `result` renders as a table inside the chat. A reply carrying `created` calls `onCreated`, which invalidates `dashboardNamesQuery`, `dashboardQuery` and `widgetQuery` so the list and the page pick the new definitions up without a reload. When `created.dashboard` is set, the page routes to `/portal/custom/{name}`. Anything in `skipped` is stated in the panel — "commits_table already exists, left as it was".

- [ ] **Step 1: Write the failing test**

```tsx
it("answers a one-time question with a table in the chat", async () => {
  server.use(
    http.post("*/v1/chat", () =>
      HttpResponse.json({
        reply: "59 lines on 2026-09-01",
        result: { columns: ["day", "lines"], rows: [["2026-09-01", 59]] },
      }),
    ),
  );
  const onCreated = vi.fn();

  render(<CustomChat onCreated={onCreated} />);
  await userEvent.type(screen.getByRole("textbox"), "how many lines on the first?");
  await userEvent.click(screen.getByRole("button", { name: /send/i }));

  expect(await screen.findByText("59 lines on 2026-09-01")).toBeInTheDocument();
  expect(await screen.findByRole("cell", { name: "59" })).toBeInTheDocument();
  expect(onCreated).not.toHaveBeenCalled();
});

it("sends the message and shows the reply", async () => {
  server.use(
    http.post("*/v1/chat", () =>
      HttpResponse.json({ reply: "Added a chart", created: { widgets: ["commits_graph"] } }),
    ),
  );
  const onCreated = vi.fn();

  render(<CustomChat onCreated={onCreated} />);
  await userEvent.type(screen.getByRole("textbox"), "chart commits per day");
  await userEvent.click(screen.getByRole("button", { name: /send/i }));

  expect(await screen.findByText("Added a chart")).toBeInTheDocument();
  expect(onCreated).toHaveBeenCalled();
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd src/frontend && npm test -- custom-chat`
Expected: FAIL — the component does not exist.

- [ ] **Step 3: Write the failing test for the list picking up a new dashboard**

```tsx
it("shows a dashboard the chat just created", async () => {
  let names = ["engineering"];
  server.use(
    http.get("*/v1/dashboards", () => HttpResponse.json({ names })),
    http.post("*/v1/chat", () => {
      names = ["engineering", "delivery"];
      return HttpResponse.json({ reply: "Made it", created: { widgets: [], dashboard: "delivery" } });
    }),
  );

  renderRoute("/portal/custom");
  await userEvent.type(screen.getByRole("textbox"), "dashboard about delivery");
  await userEvent.click(screen.getByRole("button", { name: /send/i }));

  expect(await screen.findByRole("link", { name: "delivery" })).toBeInTheDocument();
});
```

- [ ] **Step 4: Implement the panel and wire both routes**

A textarea, a send button, and the exchange so far. On a reply with `created`, call `onCreated`; the index route invalidates `dashboardNamesQuery` so the new dashboard appears in the list, and the dashboard route invalidates its own dashboard and widget queries so a new widget appears without a reload. When `created.dashboard` is set, navigate to `/portal/custom/{name}`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd src/frontend && npm test -- custom`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/frontend/src/components/custom/custom-chat.tsx \
        src/frontend/src/components/custom/custom-chat.test.tsx \
        src/frontend/src/routes/portal.custom.index.tsx \
        src/frontend/src/routes/portal.custom.index.test.tsx \
        src/frontend/src/routes/portal.custom.$name.tsx \
        src/frontend/src/api/custom.ts
git commit -m "feat(frontend): chat panel that creates dashboards and lists them"
```

---

### Task 9: Record the scenarios

**Files:**
- Create: `screenshots-and-etc/` output only — nothing committed.

**Interfaces:**
- Consumes: everything above, running.

- [ ] **Step 1: Bring the stack up**

Run: `./dev-compose.sh up` from the repo root, then `cd src/frontend && npm run dev`.
Expected: the stack is healthy and the dev server serves port 3000.

- [ ] **Step 2: Seed the data and the definitions**

Run: `cd src/backend/services/insight-v3-core && ./tests/mvp.sh`
Expected: exits 0. The `engineering` dashboard now exists.

- [ ] **Step 3: Record the list and the dashboard**

```bash
agent-browser record start screenshots-and-etc/dashboard.webm http://localhost:3000/portal/custom
# log in as dev@company.nonpresent / insight-dev, click through to the dashboard
agent-browser record stop
ffmpeg -i screenshots-and-etc/dashboard.webm -vf "fps=10,scale=1280:-1:flags=lanczos" \
       -loop 0 screenshots-and-etc/dashboard.gif
```

- [ ] **Step 4: Record the chat**

With `chat_mode: canned`, record a one-time question answered as a table in the panel, then a creation that adds a dashboard to the list and routes to it. Save as `screenshots-and-etc/chat.gif`.

- [ ] **Step 5: Record the failure states**

Store a metric naming a table that does not exist, put a widget on it, and record the error box. Save as `screenshots-and-etc/failures.gif`.

- [ ] **Step 6: Report**

Three GIFs in `screenshots-and-etc/`, none committed, with a line each saying which scenario it shows.

---

## Order and what each task proves

| Task | Proves |
|------|--------|
| 1 | Definitions persist, per kind |
| 2 | They can be written and read over HTTP |
| 3 | A metric is a query, and it runs |
| 4 | Steps 0-4 of the MVP work end to end against a live service |
| 5 | Dashboards are listed at /portal/custom, and one renders at its URL |
| 6 | One table and one graph from JSON, plus the error and empty states |
| 7 | The chat answers a question, and writes the same definitions |
| 8 | It does so from the page, and a new dashboard shows up in the list at once |
| 9 | Three GIFs showing the scenarios end to end, against a real login |

Tasks 1-4 are backend and can run while 5-6 are built against the mocked endpoints. Task 7 needs Task 3's compiler; Task 8 needs Task 7.
