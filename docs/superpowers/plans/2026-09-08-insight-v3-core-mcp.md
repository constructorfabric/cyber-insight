# An MCP server on insight-v3-core — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** an MCP client authors metrics, widgets and dashboards in `insight-v3-core`, runs a metric to see its rows, and lists the table catalogue — over a second MCP server on `/mcp/v3`, authorized by its own OAuth scope.

**Architecture:** the six domain sequences the REST handlers perform today move into a `custom` module over a borrowed `Surfaces` view of the four stores they need. The REST handlers become skeletons that map one typed error to `CanonicalError`; a new MCP server maps the same error to tool errors. The MCP server runs on its own listener because the gears api-gateway authenticates the whole REST router against its own audience, and it authorizes from the MCP access token because the identity round-trip is unavailable to a bearer caller.

**Tech Stack:** Rust (axum, `rmcp` 3.2 streamable-http, `jsonwebtoken` 10, `schemars` 1.2), the gears toolkit, MariaDB via sea-orm, ClickHouse, nginx + Lua at the edge, Helm, Docker Compose.

**Spec:** [docs/superpowers/specs/2026-09-08-insight-v3-core-mcp-design.md](../specs/2026-09-08-insight-v3-core-mcp-design.md)

## Global Constraints

- Everything lands on `feat/insight-v3-core-raw-data`, one commit per task, conventional-commit subjects. No new branch, no new PR, never `git commit --amend`.
- Backend commands run from `src/backend`: `cargo test -p insight-v3-core`, `cargo test -p authenticator`, `cargo test -p routegen`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt`.
- Comments only where they express a constraint the code cannot, tagged `SAFETY:`, `INVARIANT:` or `WORKAROUND:`. Zero comments is the expected outcome for most files.
- A module past ~400 lines becomes a directory. `src/mcp/` is a directory from the first commit for this reason.
- Newtypes at boundaries; exhaustive `match` on our own enums with no `_` arm; `pub(crate)` before `pub`; import groups std / external / workspace / `crate::` separated by blank lines.
- Test names state the rule. Table-driven over copy-paste. In tests, `type R = Result<(), Box<dyn Error>>`.
- No production-derived information anywhere — code, tests, fixtures, commit messages, docs. All example values synthetic: `example.invalid`, `localhost`, `Example Corp`.
- The new scope is exactly `mcp:author`. The new resource path is exactly `/mcp/v3`. The new default bind address is exactly `0.0.0.0:8087`.
- MCP is disabled unless configured: `mcp.enabled` defaults to false, and an enabled server with no `public_url` fails config validation rather than opening the port.
- The REST path keeps `require_admin` untouched. No task changes how the portal authorizes.
- `rmcp`, `schemars`, `jsonwebtoken`, `tokio-util`, `url` and `base64` are already workspace dependencies — add them to the service with `{ workspace = true }`, never a fresh version.

---

### Task 1: The custom-surface operations, extracted

The REST handlers hold the domain sequences inline, mixed with `CanonicalError` construction. The MCP tools need the same sequences with different error mapping. Extract once, consume twice.

**Files:**
- Create: `src/backend/services/insight-v3-core/src/custom.rs`
- Create: `src/backend/services/insight-v3-core/src/custom/tests.rs`
- Modify: `src/backend/services/insight-v3-core/src/api.rs` (add `surfaces()` to `AppState`)
- Modify: `src/backend/services/insight-v3-core/src/api/definitions.rs` (handlers call `custom`)
- Modify: `src/backend/services/insight-v3-core/src/api/metric_run.rs` (handler calls `custom`)
- Modify: `src/backend/services/insight-v3-core/src/main.rs` (add `mod custom;`)

**Interfaces:**
- Consumes: `crate::definitions::{Definitions, DefinitionKind, DefinitionName, DefinitionStoreError}`, `crate::metric_query::{MetricQuery, MetricQueryError, MetricRunError, MetricRunner, People, RunResult}`, `crate::catalog::{Catalog, CatalogError, TableSchema}`, `crate::widget::{Widget, WidgetError}`.
- Produces:
  ```rust
  pub(crate) struct Surfaces<'a> { /* definitions, metrics, people, catalog */ }
  impl<'a> Surfaces<'a> {
      pub(crate) fn new(
          definitions: &'a dyn Definitions,
          metrics: &'a MetricRunner,
          people: &'a People,
          catalog: &'a Catalog,
      ) -> Self;
      pub(crate) async fn list(&self, kind: DefinitionKind) -> Result<Vec<String>, CustomError>;
      pub(crate) async fn get(&self, kind: DefinitionKind, name: &DefinitionName) -> Result<serde_json::Value, CustomError>;
      pub(crate) async fn put(&self, kind: DefinitionKind, name: &DefinitionName, body: &serde_json::Value) -> Result<(), CustomError>;
      pub(crate) async fn delete(&self, kind: DefinitionKind, name: &DefinitionName) -> Result<(), CustomError>;
      pub(crate) async fn run_metric(&self, name: &DefinitionName) -> Result<RunResult, CustomError>;
      pub(crate) async fn tables(&self) -> Result<Vec<TableSchema>, CustomError>;
  }
  pub(crate) enum CustomError {
      NotFound { kind: DefinitionKind, name: String },
      InUse { used_by: Vec<String> },
      Widget(WidgetError),
      Body(serde_json::Error),
      Compile(MetricQueryError),
      Run(MetricRunError),
      Store(DefinitionStoreError),
      Catalog(CatalogError),
  }
  impl CustomError { pub(crate) fn is_about_the_caller(&self) -> bool; }
  ```
  `AppState::surfaces(&self) -> Surfaces<'_>`.

`get` returns the body rather than `Option`, folding the absent case into `CustomError::NotFound` — every caller treated `None` as an error already.

`delete` returns `()`: an absent definition is `NotFound`, matching what the REST handler answered with `404` and giving the MCP client a message instead of a silent success.

- [ ] **Step 1: Write the failing tests**

Create `src/custom/tests.rs`:

```rust
use std::error::Error;

use serde_json::json;

use super::*;
use crate::catalog::Catalog;
use crate::definitions::memory::MemoryDefinitions;
use crate::metric_query::{MetricRunner, People};

type R = Result<(), Box<dyn Error>>;

struct Fixture {
    definitions: MemoryDefinitions,
    metrics: MetricRunner,
    people: People,
    catalog: Catalog,
}

impl Fixture {
    fn new() -> Self {
        let client = || {
            insight_clickhouse::Client::new(insight_clickhouse::Config::new(
                "http://clickhouse.invalid",
                "insight",
            ))
        };

        Self {
            definitions: MemoryDefinitions::new(),
            metrics: MetricRunner::new(client()),
            people: People::new("identity"),
            catalog: Catalog::new(client(), "insight".to_owned()),
        }
    }

    fn surfaces(&self) -> Surfaces<'_> {
        Surfaces::new(&self.definitions, &self.metrics, &self.people, &self.catalog)
    }
}

fn name(value: &str) -> DefinitionName {
    DefinitionName::parse(value).expect("test name is valid")
}

fn metric_body() -> serde_json::Value {
    json!({
        "table": "events",
        "fields": [
            {"json": "actor", "type": "string", "as_name": "actor"},
            {"json": "actor", "type": "string", "agg": "count", "as_name": "total"}
        ],
        "group_by": ["actor"]
    })
}

#[tokio::test]
async fn reading_a_definition_that_was_never_stored_reports_it_missing() -> R {
    let fixture = Fixture::new();

    let error = fixture
        .surfaces()
        .get(DefinitionKind::Metric, &name("absent"))
        .await
        .expect_err("an unstored metric has no body");

    assert!(matches!(error, CustomError::NotFound { .. }), "{error:?}");
    assert!(error.is_about_the_caller());

    Ok(())
}

#[tokio::test]
async fn a_widget_naming_a_column_its_metric_does_not_produce_is_refused() -> R {
    let fixture = Fixture::new();
    let surfaces = fixture.surfaces();
    surfaces
        .put(DefinitionKind::Metric, &name("per-actor"), &metric_body())
        .await?;

    let error = surfaces
        .put(
            DefinitionKind::Widget,
            &name("chart"),
            &json!({"type": "line", "metric": "per-actor", "x": "actor", "y": "lines"}),
        )
        .await
        .expect_err("the metric produces `total`, not `lines`");

    assert!(matches!(error, CustomError::Widget(_)), "{error:?}");

    Ok(())
}

#[tokio::test]
async fn a_widget_naming_a_metric_that_is_not_stored_is_refused() -> R {
    let fixture = Fixture::new();

    let error = fixture
        .surfaces()
        .put(
            DefinitionKind::Widget,
            &name("chart"),
            &json!({"type": "table", "metric": "absent", "columns": []}),
        )
        .await
        .expect_err("a widget cannot draw a metric that is not there");

    assert!(matches!(error, CustomError::Widget(_)), "{error:?}");

    Ok(())
}

#[tokio::test]
async fn a_metric_a_widget_still_draws_is_kept_and_its_dependents_named() -> R {
    let fixture = Fixture::new();
    let surfaces = fixture.surfaces();
    surfaces
        .put(DefinitionKind::Metric, &name("per-actor"), &metric_body())
        .await?;
    surfaces
        .put(
            DefinitionKind::Widget,
            &name("chart"),
            &json!({"type": "line", "metric": "per-actor", "x": "actor", "y": "total"}),
        )
        .await?;

    let error = surfaces
        .delete(DefinitionKind::Metric, &name("per-actor"))
        .await
        .expect_err("a metric in use is kept");

    match error {
        CustomError::InUse { used_by } => assert_eq!(used_by, vec!["chart".to_owned()]),
        other => panic!("should refuse as in use: {other:?}"),
    }

    Ok(())
}

#[tokio::test]
async fn a_dashboard_holds_widgets_so_nothing_reports_it_as_a_dependent() -> R {
    let fixture = Fixture::new();
    let surfaces = fixture.surfaces();
    surfaces
        .put(
            DefinitionKind::Dashboard,
            &name("board"),
            &json!({"title": "Example board", "widgets": []}),
        )
        .await?;

    surfaces.delete(DefinitionKind::Dashboard, &name("board")).await?;

    assert!(surfaces.list(DefinitionKind::Dashboard).await?.is_empty());

    Ok(())
}

#[tokio::test]
async fn running_a_metric_whose_body_is_not_a_query_reports_the_body() -> R {
    let fixture = Fixture::new();
    let surfaces = fixture.surfaces();
    surfaces
        .put(DefinitionKind::Metric, &name("broken"), &json!({"table": "events"}))
        .await?;

    let error = surfaces
        .run_metric(&name("broken"))
        .await
        .expect_err("a query with no fields does not deserialize");

    assert!(matches!(error, CustomError::Body(_)), "{error:?}");

    Ok(())
}

#[tokio::test]
async fn listing_names_them_in_the_order_the_store_gives() -> R {
    let fixture = Fixture::new();
    let surfaces = fixture.surfaces();
    surfaces
        .put(DefinitionKind::Metric, &name("alpha"), &metric_body())
        .await?;
    surfaces
        .put(DefinitionKind::Metric, &name("beta"), &metric_body())
        .await?;

    assert_eq!(
        surfaces.list(DefinitionKind::Metric).await?,
        vec!["alpha".to_owned(), "beta".to_owned()]
    );

    Ok(())
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src/backend && cargo test -p insight-v3-core custom::tests`
Expected: FAIL — `custom` is not a module of this crate.

- [ ] **Step 3: Write `src/custom.rs`**

```rust
//! The metric, widget and dashboard operations, over the stores they need.

use serde_json::Value;
use thiserror::Error;

use crate::catalog::{Catalog, CatalogError, TableSchema};
use crate::definitions::{
    DefinitionKind, DefinitionName, DefinitionStoreError, Definitions,
};
use crate::metric_query::{
    MetricQuery, MetricQueryError, MetricRunError, MetricRunner, People, RunResult,
};
use crate::widget::{Widget, WidgetError};

#[cfg(test)]
mod tests;

#[derive(Debug, Error)]
pub(crate) enum CustomError {
    #[error("{} `{name}` was not found", kind.table())]
    NotFound { kind: DefinitionKind, name: String },
    #[error("still in use by {}", used_by.join(", "))]
    InUse { used_by: Vec<String> },
    #[error(transparent)]
    Widget(WidgetError),
    #[error("definition body is not valid: {0}")]
    Body(serde_json::Error),
    #[error(transparent)]
    Compile(MetricQueryError),
    #[error(transparent)]
    Run(MetricRunError),
    #[error(transparent)]
    Store(DefinitionStoreError),
    #[error(transparent)]
    Catalog(CatalogError),
}

impl CustomError {
    pub(crate) fn is_about_the_caller(&self) -> bool {
        match self {
            Self::NotFound { .. }
            | Self::InUse { .. }
            | Self::Widget(_)
            | Self::Body(_)
            | Self::Compile(_) => true,
            Self::Run(_) | Self::Store(_) | Self::Catalog(_) => false,
        }
    }
}

#[derive(Debug)]
pub(crate) struct Surfaces<'a> {
    definitions: &'a dyn Definitions,
    metrics: &'a MetricRunner,
    people: &'a People,
    catalog: &'a Catalog,
}

impl<'a> Surfaces<'a> {
    pub(crate) fn new(
        definitions: &'a dyn Definitions,
        metrics: &'a MetricRunner,
        people: &'a People,
        catalog: &'a Catalog,
    ) -> Self {
        Self {
            definitions,
            metrics,
            people,
            catalog,
        }
    }

    pub(crate) async fn list(&self, kind: DefinitionKind) -> Result<Vec<String>, CustomError> {
        self.definitions.list(kind).await.map_err(CustomError::Store)
    }

    pub(crate) async fn get(
        &self,
        kind: DefinitionKind,
        name: &DefinitionName,
    ) -> Result<Value, CustomError> {
        self.definitions
            .get(kind, name)
            .await
            .map_err(CustomError::Store)?
            .ok_or_else(|| CustomError::NotFound {
                kind,
                name: name.as_str().to_owned(),
            })
    }

    pub(crate) async fn put(
        &self,
        kind: DefinitionKind,
        name: &DefinitionName,
        body: &Value,
    ) -> Result<(), CustomError> {
        if kind == DefinitionKind::Widget {
            self.check_widget(body).await?;
        }

        self.definitions
            .put(kind, name, body)
            .await
            .map_err(CustomError::Store)
    }

    pub(crate) async fn delete(
        &self,
        kind: DefinitionKind,
        name: &DefinitionName,
    ) -> Result<(), CustomError> {
        let used_by = self.dependents_of(kind, name).await?;
        if !used_by.is_empty() {
            return Err(CustomError::InUse { used_by });
        }

        let removed = self
            .definitions
            .delete(kind, name)
            .await
            .map_err(CustomError::Store)?;

        if removed {
            return Ok(());
        }

        Err(CustomError::NotFound {
            kind,
            name: name.as_str().to_owned(),
        })
    }

    pub(crate) async fn run_metric(
        &self,
        name: &DefinitionName,
    ) -> Result<RunResult, CustomError> {
        let body = self.get(DefinitionKind::Metric, name).await?;

        let metric: MetricQuery = serde_json::from_value(body).map_err(CustomError::Body)?;
        let compiled = metric.compile(self.people).map_err(CustomError::Compile)?;

        self.metrics.run(&compiled).await.map_err(CustomError::Run)
    }

    pub(crate) async fn tables(&self) -> Result<Vec<TableSchema>, CustomError> {
        self.catalog.tables().await.map_err(CustomError::Catalog)
    }

    async fn check_widget(&self, body: &Value) -> Result<(), CustomError> {
        let widget: Widget =
            serde_json::from_value(body.clone()).map_err(|error| CustomError::Widget(error.into()))?;

        let metric_name = DefinitionName::parse(widget.metric())
            .map_err(|_| CustomError::Widget(WidgetError::NoMetric(widget.metric().to_owned())))?;

        let stored = self
            .definitions
            .get(DefinitionKind::Metric, &metric_name)
            .await
            .map_err(CustomError::Store)?
            .ok_or_else(|| CustomError::Widget(WidgetError::NoMetric(widget.metric().to_owned())))?;

        let metric: MetricQuery =
            serde_json::from_value(stored).map_err(|error| CustomError::Widget(error.into()))?;

        widget.check_against(&metric).map_err(CustomError::Widget)
    }

    async fn dependents_of(
        &self,
        kind: DefinitionKind,
        name: &DefinitionName,
    ) -> Result<Vec<String>, CustomError> {
        let (holder, needle) = match kind {
            DefinitionKind::Metric => (DefinitionKind::Widget, "metric"),
            DefinitionKind::Widget => (DefinitionKind::Dashboard, "widgets"),
            DefinitionKind::Dashboard => return Ok(Vec::new()),
        };

        let mut used_by = Vec::new();
        for holder_name in self.list(holder).await? {
            let Ok(parsed) = DefinitionName::parse(&holder_name) else {
                continue;
            };
            let Some(body) = self
                .definitions
                .get(holder, &parsed)
                .await
                .map_err(CustomError::Store)?
            else {
                continue;
            };

            let names = match body.get(needle) {
                Some(Value::String(one)) => vec![one.as_str()],
                Some(Value::Array(many)) => many.iter().filter_map(Value::as_str).collect(),
                _ => Vec::new(),
            };
            if names.contains(&name.as_str()) {
                used_by.push(holder_name);
            }
        }

        Ok(used_by)
    }
}
```

Add `mod custom;` to `src/main.rs` in module-name order (after `mod config;`).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src/backend && cargo test -p insight-v3-core custom::tests`
Expected: PASS, 7 tests.

- [ ] **Step 5: Add `AppState::surfaces` and rewire the REST handlers**

In `src/api.rs`, beside the other accessors:

```rust
pub(crate) fn surfaces(&self) -> crate::custom::Surfaces<'_> {
    crate::custom::Surfaces::new(
        self.definitions.as_ref(),
        &self.metrics,
        &self.people,
        &self.catalog,
    )
}
```

In `src/api/definitions.rs`, delete `dependents_of`, `check_widget` and `widget_error`, and add one mapper used by every handler in the file:

```rust
fn custom_error(error: crate::custom::CustomError) -> CanonicalError {
    use crate::custom::CustomError;

    match error {
        CustomError::NotFound { kind, name } => DefinitionApiError::not_found(format!(
            "{} `{name}` was not found",
            kind.table()
        ))
        .with_resource(&name)
        .create(),
        CustomError::InUse { used_by } => DefinitionApiError::failed_precondition()
            .with_precondition_violation(
                "name",
                format!("still in use by {}", used_by.join(", ")),
                "in_use",
            )
            .create(),
        CustomError::Widget(source) => DefinitionApiError::invalid_argument()
            .with_field_violation("body", source.to_string(), "INVALID")
            .create(),
        CustomError::Body(source) => DefinitionApiError::invalid_argument()
            .with_field_violation("body", source.to_string(), "INVALID")
            .create(),
        CustomError::Compile(source) => DefinitionApiError::invalid_argument()
            .with_field_violation("body", source.to_string(), "INVALID")
            .create(),
        CustomError::Store(source) => definition_store_error(source),
        CustomError::Run(source) => {
            tracing::error!(error = ?source, "metric query execution failed");
            CanonicalError::internal("metric query execution failed").create()
        }
        CustomError::Catalog(source) => {
            tracing::error!(error = ?source, "table catalogue read failed");
            CanonicalError::internal("table catalogue read failed").create()
        }
    }
}
```

`put_definition` becomes:

```rust
async fn put_definition(
    Extension(state): Extension<Arc<AppState>>,
    Extension(kind): Extension<DefinitionKind>,
    Path(name): Path<String>,
    headers: axum::http::HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Result<Response, CanonicalError> {
    crate::api::require_admin(&state, &headers, || {
        DefinitionApiError::permission_denied()
            .with_reason(crate::api::ADMIN_ONLY)
            .create()
    })
    .await?;

    let name = DefinitionName::parse(&name).map_err(definition_error)?;

    state
        .surfaces()
        .put(kind, &name, &body)
        .await
        .map_err(custom_error)?;

    Ok(StatusCode::NO_CONTENT.into_response())
}
```

`delete_definition` keeps its `require_admin` and name parse, then:

```rust
    match state.surfaces().delete(kind, &name).await {
        Ok(()) => Ok(StatusCode::NO_CONTENT.into_response()),
        Err(crate::custom::CustomError::NotFound { .. }) => {
            Ok(StatusCode::NOT_FOUND.into_response())
        }
        Err(other) => Err(custom_error(other)),
    }
```

`get_definition` and the list handler call `state.surfaces().get(..)` / `.list(..)`, mapping through `custom_error`; `get_definition` no longer needs its own not-found construction.

`src/api/chat.rs` calls `check_widget` — repoint it at `state.surfaces().put(..)`'s validation by calling the same `Surfaces::put` for the widgets it writes, or, where it must stage all writes atomically through `put_all`, call `Surfaces::check_widget`; make that method `pub(crate)` if the chat needs it directly.

`src/api/metric_run.rs`: the handler body after `require_admin` and the name parse becomes

```rust
    let result = state
        .surfaces()
        .run_metric(&name)
        .await
        .map_err(custom_error)?;

    Ok(Json(result).into_response())
```

with a `custom_error` mapper of the same shape built on `MetricRunApiError`. Delete `invalid_metric_body`, `compile_error` and `run_error`, keeping `definition_store_error` and `metric_not_found` only if the mapper still uses them.

- [ ] **Step 6: Run the whole service suite**

Run: `cd src/backend && cargo test -p insight-v3-core`
Expected: PASS. The existing `api::definitions::tests` and `api::metric_run::tests` are the safety net for this extraction — if any assertion on a status code or message changed, the extraction changed behavior and must be corrected, not the test.

- [ ] **Step 7: Lint and format**

Run: `cd src/backend && cargo fmt && cargo clippy -p insight-v3-core --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add src/backend/services/insight-v3-core/src
git commit -m "refactor(insight-v3-core): hold the custom-surface operations in one place"
```

---

### Task 2: MCP configuration

**Files:**
- Modify: `src/backend/services/insight-v3-core/src/config.rs`
- Modify: `src/backend/services/insight-v3-core/config/insight.yaml`
- Modify: `src/backend/services/insight-v3-core/Cargo.toml`

**Interfaces:**
- Produces: `crate::config::McpConfig { enabled: bool, bind_addr: String, public_url: String, allow_insecure_private_network: bool }`, `GearConfig::mcp`, `ValidatedConfig::mcp(&self) -> &McpConfig`.

- [ ] **Step 1: Write the failing tests**

Append to `src/config.rs`'s existing `#[cfg(test)] mod tests` block (create it if the file has none):

```rust
#[test]
fn a_disabled_mcp_server_needs_no_public_url() {
    let mcp = McpConfig::default();

    assert!(validate_mcp(&mcp).is_ok());
}

#[test]
fn an_enabled_mcp_server_without_a_public_url_is_refused() {
    let mcp = McpConfig {
        enabled: true,
        ..McpConfig::default()
    };

    assert!(matches!(validate_mcp(&mcp), Err(ConfigError::Empty(field)) if field == "mcp.public_url"));
}

#[test]
fn an_enabled_mcp_server_with_an_unparseable_bind_address_is_refused() {
    let mcp = McpConfig {
        enabled: true,
        bind_addr: "not-an-address".to_owned(),
        public_url: "https://insight.example.invalid".to_owned(),
        allow_insecure_private_network: false,
    };

    assert!(matches!(validate_mcp(&mcp), Err(ConfigError::McpBindAddr)));
}

#[test]
fn an_enabled_mcp_server_is_accepted_with_an_origin_and_an_address() {
    let mcp = McpConfig {
        enabled: true,
        bind_addr: "0.0.0.0:8087".to_owned(),
        public_url: "https://insight.example.invalid".to_owned(),
        allow_insecure_private_network: false,
    };

    assert!(validate_mcp(&mcp).is_ok());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src/backend && cargo test -p insight-v3-core config::tests`
Expected: FAIL — `McpConfig` does not exist.

- [ ] **Step 3: Implement the config**

In `src/config.rs`, add the constant beside the other module-top constants:

```rust
const DEFAULT_MCP_BIND_ADDR: &str = "0.0.0.0:8087";
```

Add the struct:

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub(crate) struct McpConfig {
    pub(crate) enabled: bool,
    pub(crate) bind_addr: String,
    pub(crate) public_url: String,
    pub(crate) allow_insecure_private_network: bool,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bind_addr: DEFAULT_MCP_BIND_ADDR.to_owned(),
            public_url: String::new(),
            allow_insecure_private_network: false,
        }
    }
}
```

Add `pub(crate) mcp: McpConfig` to `GearConfig` and to `ValidatedConfig`, `mcp: McpConfig::default()` to `GearConfig::default()`, `mcp: self.mcp` to the `ValidatedConfig` the `validate` method returns, and the accessor:

```rust
pub(crate) fn mcp(&self) -> &McpConfig {
    &self.mcp
}
```

Add the validator and call it from `GearConfig::validate` before the `Ok(..)`:

```rust
fn validate_mcp(mcp: &McpConfig) -> Result<(), ConfigError> {
    if !mcp.enabled {
        return Ok(());
    }

    require_non_empty("mcp.public_url", &mcp.public_url)?;
    mcp.bind_addr
        .parse::<std::net::SocketAddr>()
        .map_err(|_| ConfigError::McpBindAddr)?;

    Ok(())
}
```

Add the error variant to `ConfigError`:

```rust
    #[error("mcp.bind_addr is not a socket address")]
    McpBindAddr,
```

In `config/insight.yaml`, under `insight-v3-core: config:`, add:

```yaml
      mcp:
        enabled: false
        bind_addr: "0.0.0.0:8087"
        public_url: ""
        allow_insecure_private_network: false
```

In the service `Cargo.toml`, add to `[dependencies]`:

```toml
jsonwebtoken = { workspace = true }
rmcp = { workspace = true }
schemars = { workspace = true }
tokio-util = { workspace = true }
url = { workspace = true }
```

and to `[dev-dependencies]`:

```toml
base64 = { workspace = true }
p256 = { version = "0.14", features = ["pkcs8", "pem", "ecdsa", "getrandom"] }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src/backend && cargo test -p insight-v3-core config::tests`
Expected: PASS, 4 tests.

- [ ] **Step 5: Commit**

```bash
git add src/backend/services/insight-v3-core
git commit -m "feat(insight-v3-core): configure an MCP listener, off unless asked for"
```

---

### Task 3: The token verifier

**Files:**
- Create: `src/backend/services/insight-v3-core/src/mcp/auth.rs`
- Create: `src/backend/services/insight-v3-core/src/mcp/mod.rs`
- Create: `src/backend/services/insight-v3-core/src/mcp/auth/tests.rs`
- Modify: `src/backend/services/insight-v3-core/src/main.rs` (add `mod mcp;`)

**Interfaces:**
- Produces:
  ```rust
  pub(crate) const MCP_SCOPE: &str = "mcp:author";
  pub(crate) const MCP_PATH: &str = "/mcp/v3";
  pub(crate) struct TokenVerifier;             // Clone
  impl TokenVerifier {
      pub(crate) fn new(public_url: &str, allow_insecure_private_network: bool) -> anyhow::Result<Self>;
      pub(crate) async fn verify(&self, token: &str) -> Result<(), AuthFailure>;
      pub(crate) fn challenge(&self, status: StatusCode, error: &str) -> Response;
  }
  pub(crate) enum AuthFailure { Unauthorized, InsufficientScope, Unavailable }
  pub(crate) fn validate_public_url(raw: &str, allow_insecure_private_network: bool) -> anyhow::Result<()>;
  pub(crate) fn bearer_token(headers: &HeaderMap) -> Option<&str>;
  pub(crate) async fn authenticate(State(verifier): State<TokenVerifier>, headers: HeaderMap, request: Request, next: Next) -> Response;
  ```

The verifier is the analytics one with two values changed: audience `{issuer}/mcp/v3` and required scope `mcp:author`. Port `TokenVerifier`, `TokenVerifierInner`, `JwksRefresh`, `AuthFailure`, `validate_public_url`, `bearer_token`, `authenticate`, the JWKS fetch with its `MAX_JWKS_BYTES` cap and `JWKS_REFRESH_COOLDOWN`, and the private-network guard from `src/backend/services/analytics/src/mcp.rs`, substituting `MCP_PATH` for the hardcoded `/mcp` and `MCP_SCOPE` for `mcp:query`.

- [ ] **Step 1: Write the failing tests**

Create `src/mcp/auth/tests.rs`:

```rust
use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::http::StatusCode;
use axum::routing::get;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use p256::SecretKey;
use p256::elliptic_curve::Generate as _;
use p256::elliptic_curve::sec1::ToSec1Point as _;
use p256::pkcs8::{EncodePrivateKey as _, LineEnding};
use serde_json::{Value, json};
use tokio::task::JoinHandle;

use super::*;

type R = Result<(), Box<dyn Error>>;

struct SigningMaterial {
    key: EncodingKey,
    jwks: Value,
}

fn signing_material() -> Result<SigningMaterial, Box<dyn Error>> {
    let secret = SecretKey::generate();
    let pem = secret.to_pkcs8_pem(LineEnding::LF)?;
    let key = EncodingKey::from_ec_pem(pem.as_bytes())?;
    let point = secret.public_key().to_sec1_point(false);
    let x = point.x().ok_or("public key has no x coordinate")?;
    let y = point.y().ok_or("public key has no y coordinate")?;

    Ok(SigningMaterial {
        key,
        jwks: json!({
            "keys": [{
                "kty": "EC",
                "crv": "P-256",
                "use": "sig",
                "alg": "ES256",
                "kid": "test-key",
                "x": B64.encode(x),
                "y": B64.encode(y),
            }]
        }),
    })
}

fn claims(issuer: &str) -> Result<Value, Box<dyn Error>> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

    Ok(json!({
        "sub": "test-user",
        "tenant_id": "test-tenant",
        "roles": "user admin",
        "sub_type": "user",
        "sid": "test-session",
        "iss": issuer,
        "aud": format!("{issuer}{MCP_PATH}"),
        "scope": format!("openid {MCP_SCOPE}"),
        "iat": now,
        "exp": now + 600,
        "jti": "test-token",
    }))
}

fn sign(material: &SigningMaterial, claims: &Value) -> Result<String, Box<dyn Error>> {
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some("test-key".to_owned());

    Ok(encode(&header, claims, &material.key)?)
}

async fn spawn_issuer(jwks: Value) -> Result<(String, JoinHandle<()>), Box<dyn Error>> {
    let app = axum::Router::new().route(
        "/.well-known/jwks.json",
        get(move || {
            let jwks = jwks.clone();
            async move { Json(jwks) }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    Ok((format!("http://{address}"), server))
}

#[tokio::test]
async fn a_correctly_issued_token_is_accepted() -> R {
    let material = signing_material()?;
    let (issuer, server) = spawn_issuer(material.jwks.clone()).await?;
    let verifier = TokenVerifier::new(&issuer, true)?;

    let token = sign(&material, &claims(&issuer)?)?;

    assert!(verifier.verify(&token).await.is_ok());

    server.abort();
    Ok(())
}

#[tokio::test]
async fn a_token_minted_for_another_resource_is_refused() -> R {
    let material = signing_material()?;
    let (issuer, server) = spawn_issuer(material.jwks.clone()).await?;
    let verifier = TokenVerifier::new(&issuer, true)?;

    let mut claims = claims(&issuer)?;
    claims["aud"] = json!(format!("{issuer}/mcp"));
    let token = sign(&material, &claims)?;

    assert!(matches!(
        verifier.verify(&token).await,
        Err(AuthFailure::Unauthorized)
    ));

    server.abort();
    Ok(())
}

#[tokio::test]
async fn a_token_carrying_only_the_read_only_scope_is_refused() -> R {
    let material = signing_material()?;
    let (issuer, server) = spawn_issuer(material.jwks.clone()).await?;
    let verifier = TokenVerifier::new(&issuer, true)?;

    let mut claims = claims(&issuer)?;
    claims["scope"] = json!("openid mcp:query");
    let token = sign(&material, &claims)?;

    assert!(matches!(
        verifier.verify(&token).await,
        Err(AuthFailure::InsufficientScope)
    ));

    server.abort();
    Ok(())
}

#[tokio::test]
async fn an_expired_token_is_refused() -> R {
    let material = signing_material()?;
    let (issuer, server) = spawn_issuer(material.jwks.clone()).await?;
    let verifier = TokenVerifier::new(&issuer, true)?;

    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let mut claims = claims(&issuer)?;
    claims["iat"] = json!(now - 1200);
    claims["exp"] = json!(now - 600);
    let token = sign(&material, &claims)?;

    assert!(matches!(
        verifier.verify(&token).await,
        Err(AuthFailure::Unauthorized)
    ));

    server.abort();
    Ok(())
}

#[tokio::test]
async fn a_token_from_another_issuer_is_refused() -> R {
    let material = signing_material()?;
    let (issuer, server) = spawn_issuer(material.jwks.clone()).await?;
    let verifier = TokenVerifier::new(&issuer, true)?;

    let mut claims = claims(&issuer)?;
    claims["iss"] = json!("https://elsewhere.example.invalid");
    let token = sign(&material, &claims)?;

    assert!(matches!(
        verifier.verify(&token).await,
        Err(AuthFailure::Unauthorized)
    ));

    server.abort();
    Ok(())
}

#[test]
fn a_public_url_that_is_not_an_https_origin_is_refused() {
    for raw in [
        "",
        "not-a-url",
        "ftp://insight.example.invalid",
        "https://insight.example.invalid/path",
        "https://insight.example.invalid?query=1",
    ] {
        assert!(
            validate_public_url(raw, false).is_err(),
            "should reject: {raw:?}"
        );
    }
}

#[test]
fn a_challenge_names_this_server_s_metadata_document_and_scope() {
    let verifier = TokenVerifier::new("https://insight.example.invalid", false)
        .expect("a plain https origin is a valid public URL");

    let response = verifier.challenge(StatusCode::UNAUTHORIZED, "invalid_token");
    let header = response
        .headers()
        .get(axum::http::header::WWW_AUTHENTICATE)
        .and_then(|value| value.to_str().ok())
        .expect("a challenge carries WWW-Authenticate");

    assert!(header.contains("oauth-protected-resource/mcp/v3"), "{header}");
    assert!(header.contains(MCP_SCOPE), "{header}");
}

#[test]
fn a_bearer_header_yields_its_token_and_anything_else_yields_none() {
    let mut headers = axum::http::HeaderMap::new();
    assert_eq!(bearer_token(&headers), None);

    headers.insert(
        axum::http::header::AUTHORIZATION,
        axum::http::HeaderValue::from_static("Basic abc"),
    );
    assert_eq!(bearer_token(&headers), None);

    headers.insert(
        axum::http::header::AUTHORIZATION,
        axum::http::HeaderValue::from_static("Bearer abc"),
    );
    assert_eq!(bearer_token(&headers), Some("abc"));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src/backend && cargo test -p insight-v3-core mcp::auth`
Expected: FAIL — `mcp` is not a module of this crate.

- [ ] **Step 3: Write `src/mcp/auth.rs` and `src/mcp/mod.rs`**

`src/mcp/mod.rs` starts as the module root only:

```rust
//! The MCP server: the custom surfaces, for a client that speaks MCP.

pub(crate) mod auth;
```

`src/mcp/auth.rs` is the analytics verifier, ported. Copy `src/backend/services/analytics/src/mcp.rs` lines covering `MAX_JWKS_BYTES`, `JWKS_REFRESH_COOLDOWN`, `McpAccessClaims`, `TokenVerifier`, `TokenVerifierInner`, `JwksRefresh`, `AuthFailure`, `authenticate`, `bearer_token`, `validate_public_url` and the private-network guard, then apply exactly these substitutions:

- add at module top: `pub(crate) const MCP_SCOPE: &str = "mcp:author";` and `pub(crate) const MCP_PATH: &str = "/mcp/v3";`
- `audience: format!("{issuer}/mcp")` becomes `audience: format!("{issuer}{MCP_PATH}")`
- `resource_metadata: format!("{issuer}/.well-known/oauth-protected-resource/mcp")` becomes `resource_metadata: format!("{issuer}/.well-known/oauth-protected-resource{MCP_PATH}")`
- the scope check compares against `MCP_SCOPE`
- the challenge builder writes `Bearer resource_metadata="…", scope="{MCP_SCOPE}"`
- every item is `pub(crate)`, and `#[cfg(test)] mod tests;` goes at the foot of the file

Add `mod mcp;` to `src/main.rs` in module-name order.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src/backend && cargo test -p insight-v3-core mcp::auth`
Expected: PASS, 8 tests.

- [ ] **Step 5: Lint and commit**

```bash
cd src/backend && cargo fmt && cargo clippy -p insight-v3-core --all-targets -- -D warnings
git add src/backend/services/insight-v3-core
git commit -m "feat(insight-v3-core): verify an MCP access token minted for this server"
```

---

### Task 4: The tools

**Files:**
- Create: `src/backend/services/insight-v3-core/src/mcp/tools.rs`
- Create: `src/backend/services/insight-v3-core/src/mcp/tools/tests.rs`
- Modify: `src/backend/services/insight-v3-core/src/mcp/mod.rs`

**Interfaces:**
- Consumes: `crate::custom::{CustomError, Surfaces}`, `crate::definitions::{DefinitionKind, DefinitionName, Definitions}`, `crate::metric_query::{MetricRunner, People}`, `crate::catalog::Catalog`.
- Produces:
  ```rust
  pub(crate) struct CustomSurfaces { /* Clone */ }
  impl CustomSurfaces {
      pub(crate) fn new(
          definitions: Arc<dyn Definitions>,
          metrics: MetricRunner,
          people: People,
          catalog: Catalog,
      ) -> Self;
  }
  ```
  with the eight `#[tool]` methods and a `ServerHandler` impl.

`CustomSurfaces` owns its dependencies rather than borrowing, because `StreamableHttpService` needs a `'static` factory. It hands out a `Surfaces<'_>` per call.

- [ ] **Step 1: Write the failing tests**

Create `src/mcp/tools/tests.rs`:

```rust
use std::error::Error;
use std::sync::Arc;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use serde_json::json;

use super::*;
use crate::definitions::memory::MemoryDefinitions;

type R = Result<(), Box<dyn Error>>;

fn surfaces() -> CustomSurfaces {
    let client = || {
        insight_clickhouse::Client::new(insight_clickhouse::Config::new(
            "http://clickhouse.invalid",
            "insight",
        ))
    };

    CustomSurfaces::new(
        Arc::new(MemoryDefinitions::new()),
        MetricRunner::new(client()),
        People::new("identity"),
        Catalog::new(client(), "insight".to_owned()),
    )
}

fn metric_body() -> serde_json::Value {
    json!({
        "table": "events",
        "fields": [
            {"json": "actor", "type": "string", "as_name": "actor"},
            {"json": "actor", "type": "string", "agg": "count", "as_name": "total"}
        ],
        "group_by": ["actor"]
    })
}

fn assert_tool_error_contains(result: &CallToolResult, expected: &str) {
    assert_eq!(result.is_error, Some(true), "should be an error: {result:?}");
    let rendered = format!("{:?}", result.content);
    assert!(
        rendered.contains(expected),
        "should mention {expected:?}: {rendered}"
    );
}

#[tokio::test]
async fn the_server_announces_its_eight_tools() {
    let tools = CustomSurfaces::tool_router().list_all();

    let mut names: Vec<&str> = tools.iter().map(|tool| tool.name.as_ref()).collect();
    names.sort_unstable();

    assert_eq!(
        names,
        [
            "delete_definition",
            "get_definition",
            "list_definitions",
            "list_tables",
            "put_dashboard",
            "put_metric",
            "put_widget",
            "run_metric",
        ]
    );
}

#[tokio::test]
async fn a_stored_metric_is_listed_and_read_back() -> R {
    let surfaces = surfaces();

    let put = surfaces
        .put_metric(Parameters(PutRequest {
            name: "per-actor".to_owned(),
            body: metric_body(),
        }))
        .await;
    assert_ne!(put.is_error, Some(true), "{put:?}");

    let listed = surfaces
        .list_definitions(Parameters(KindRequest {
            kind: ToolKind::Metric,
        }))
        .await;
    assert!(format!("{:?}", listed.content).contains("per-actor"));

    let read = surfaces
        .get_definition(Parameters(NamedRequest {
            kind: ToolKind::Metric,
            name: "per-actor".to_owned(),
        }))
        .await;
    assert!(format!("{:?}", read.content).contains("events"));

    Ok(())
}

#[tokio::test]
async fn reading_a_definition_that_was_never_stored_says_so() {
    let result = surfaces()
        .get_definition(Parameters(NamedRequest {
            kind: ToolKind::Dashboard,
            name: "absent".to_owned(),
        }))
        .await;

    assert_tool_error_contains(&result, "was not found");
}

#[tokio::test]
async fn a_name_the_store_would_not_accept_is_refused_before_any_read() {
    let result = surfaces()
        .get_definition(Parameters(NamedRequest {
            kind: ToolKind::Metric,
            name: "not a valid name".to_owned(),
        }))
        .await;

    assert_tool_error_contains(&result, "name");
}

#[tokio::test]
async fn a_widget_drawing_a_column_its_metric_does_not_produce_is_refused() -> R {
    let surfaces = surfaces();
    surfaces
        .put_metric(Parameters(PutRequest {
            name: "per-actor".to_owned(),
            body: metric_body(),
        }))
        .await;

    let result = surfaces
        .put_widget(Parameters(PutRequest {
            name: "chart".to_owned(),
            body: json!({"type": "line", "metric": "per-actor", "x": "actor", "y": "lines"}),
        }))
        .await;

    assert_tool_error_contains(&result, "lines");

    Ok(())
}

#[tokio::test]
async fn a_metric_a_widget_still_draws_is_not_deleted() -> R {
    let surfaces = surfaces();
    surfaces
        .put_metric(Parameters(PutRequest {
            name: "per-actor".to_owned(),
            body: metric_body(),
        }))
        .await;
    surfaces
        .put_widget(Parameters(PutRequest {
            name: "chart".to_owned(),
            body: json!({"type": "line", "metric": "per-actor", "x": "actor", "y": "total"}),
        }))
        .await;

    let result = surfaces
        .delete_definition(Parameters(NamedRequest {
            kind: ToolKind::Metric,
            name: "per-actor".to_owned(),
        }))
        .await;

    assert_tool_error_contains(&result, "chart");

    Ok(())
}

#[tokio::test]
async fn a_dashboard_is_stored_and_then_removed() -> R {
    let surfaces = surfaces();
    surfaces
        .put_dashboard(Parameters(PutRequest {
            name: "board".to_owned(),
            body: json!({"title": "Example board", "widgets": []}),
        }))
        .await;

    let removed = surfaces
        .delete_definition(Parameters(NamedRequest {
            kind: ToolKind::Dashboard,
            name: "board".to_owned(),
        }))
        .await;
    assert_ne!(removed.is_error, Some(true), "{removed:?}");

    let listed = surfaces
        .list_definitions(Parameters(KindRequest {
            kind: ToolKind::Dashboard,
        }))
        .await;
    assert!(!format!("{:?}", listed.content).contains("board"));

    Ok(())
}

#[tokio::test]
async fn running_a_metric_that_was_never_stored_says_so() {
    let result = surfaces()
        .run_metric(Parameters(NameRequest {
            name: "absent".to_owned(),
        }))
        .await;

    assert_tool_error_contains(&result, "was not found");
}

#[tokio::test]
async fn a_catalogue_that_cannot_be_read_is_a_tool_error_not_a_panic() {
    let result = surfaces().list_tables().await;

    assert_eq!(result.is_error, Some(true), "{result:?}");
}

#[tokio::test]
async fn the_server_instructions_name_the_discovery_tool() {
    let info = rmcp::ServerHandler::get_info(&surfaces());

    let instructions = info.instructions.unwrap_or_default();
    assert!(instructions.contains("list_tables"), "{instructions}");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src/backend && cargo test -p insight-v3-core mcp::tools`
Expected: FAIL — `mcp::tools` does not exist.

- [ ] **Step 3: Write `src/mcp/tools.rs`**

```rust
use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, Implementation, ServerCapabilities, ServerInfo,
};
use rmcp::{ServerHandler, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::catalog::{Catalog, Layer, TableSchema};
use crate::custom::{CustomError, Surfaces};
use crate::definitions::{DefinitionKind, DefinitionName, Definitions};
use crate::metric_query::{MetricRunner, People};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ToolKind {
    Metric,
    Widget,
    Dashboard,
}

impl ToolKind {
    fn kind(self) -> DefinitionKind {
        match self {
            Self::Metric => DefinitionKind::Metric,
            Self::Widget => DefinitionKind::Widget,
            Self::Dashboard => DefinitionKind::Dashboard,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct KindRequest {
    /// Which of the three kinds of definition to list.
    pub(crate) kind: ToolKind,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct NamedRequest {
    pub(crate) kind: ToolKind,
    /// The definition's name: letters, digits, `_` and `-`, up to 128 characters.
    pub(crate) name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct NameRequest {
    pub(crate) name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct PutRequest {
    pub(crate) name: String,
    /// The definition body, replacing whatever this name holds.
    pub(crate) body: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct TableEntry {
    database: String,
    table: String,
    layer: String,
    columns: Vec<ColumnEntry>,
}

#[derive(Debug, Serialize)]
struct ColumnEntry {
    name: String,
    r#type: String,
}

#[derive(Clone)]
pub(crate) struct CustomSurfaces {
    definitions: Arc<dyn Definitions>,
    metrics: MetricRunner,
    people: People,
    catalog: Catalog,
    tool_router: ToolRouter<Self>,
}

impl std::fmt::Debug for CustomSurfaces {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("CustomSurfaces").finish()
    }
}

impl CustomSurfaces {
    pub(crate) fn new(
        definitions: Arc<dyn Definitions>,
        metrics: MetricRunner,
        people: People,
        catalog: Catalog,
    ) -> Self {
        Self {
            definitions,
            metrics,
            people,
            catalog,
            tool_router: Self::tool_router(),
        }
    }

    fn surfaces(&self) -> Surfaces<'_> {
        Surfaces::new(
            self.definitions.as_ref(),
            &self.metrics,
            &self.people,
            &self.catalog,
        )
    }

    async fn write(&self, kind: ToolKind, request: PutRequest) -> CallToolResult {
        let name = match parse_name(&request.name) {
            Ok(name) => name,
            Err(result) => return result,
        };

        match self
            .surfaces()
            .put(kind.kind(), &name, &request.body)
            .await
        {
            Ok(()) => CallToolResult::structured(serde_json::json!({"stored": request.name})),
            Err(error) => tool_error(&error),
        }
    }
}

#[tool_router]
impl CustomSurfaces {
    #[tool(
        name = "list_definitions",
        description = "Names every stored metric, widget or dashboard. Start here before writing one, so an existing definition is replaced deliberately rather than by accident."
    )]
    async fn list_definitions(
        &self,
        Parameters(KindRequest { kind }): Parameters<KindRequest>,
    ) -> CallToolResult {
        match self.surfaces().list(kind.kind()).await {
            Ok(names) => CallToolResult::structured(serde_json::json!({"names": names})),
            Err(error) => tool_error(&error),
        }
    }

    #[tool(
        name = "get_definition",
        description = "Reads one definition's stored body."
    )]
    async fn get_definition(
        &self,
        Parameters(NamedRequest { kind, name }): Parameters<NamedRequest>,
    ) -> CallToolResult {
        let parsed = match parse_name(&name) {
            Ok(parsed) => parsed,
            Err(result) => return result,
        };

        match self.surfaces().get(kind.kind(), &parsed).await {
            Ok(body) => CallToolResult::structured(body),
            Err(error) => tool_error(&error),
        }
    }

    #[tool(
        name = "put_metric",
        description = "Creates or replaces a metric. The body names a table and the fields to read from it: {\"table\": \"events\", \"fields\": [{\"json\": \"actor\", \"type\": \"string\", \"as_name\": \"actor\"}, {\"json\": \"actor\", \"type\": \"string\", \"agg\": \"count\", \"as_name\": \"total\"}], \"group_by\": [\"actor\"]}. A field reads either a key inside the row's JSON payload (`json`) or a typed column of the table (`column`). Optional `database`, `filters`, `order_by` and `limit`. Call list_tables first to learn what a table holds."
    )]
    async fn put_metric(&self, Parameters(request): Parameters<PutRequest>) -> CallToolResult {
        self.write(ToolKind::Metric, request).await
    }

    #[tool(
        name = "put_widget",
        description = "Creates or replaces a widget. A widget draws one metric's columns by the `as_name` that metric gives them: {\"type\": \"table\", \"metric\": \"per-actor\", \"columns\": [\"actor\", \"total\"]} or {\"type\": \"line\", \"metric\": \"per-actor\", \"x\": \"actor\", \"y\": \"total\"}. A widget naming a column its metric does not produce is refused."
    )]
    async fn put_widget(&self, Parameters(request): Parameters<PutRequest>) -> CallToolResult {
        self.write(ToolKind::Widget, request).await
    }

    #[tool(
        name = "put_dashboard",
        description = "Creates or replaces a dashboard. A dashboard has a title and names the widgets it holds: {\"title\": \"Example board\", \"widgets\": [\"chart\"]}."
    )]
    async fn put_dashboard(&self, Parameters(request): Parameters<PutRequest>) -> CallToolResult {
        self.write(ToolKind::Dashboard, request).await
    }

    #[tool(
        name = "delete_definition",
        description = "Removes one definition. A metric a widget still draws, or a widget a dashboard still holds, is kept and its dependents named — delete the dependents first."
    )]
    async fn delete_definition(
        &self,
        Parameters(NamedRequest { kind, name }): Parameters<NamedRequest>,
    ) -> CallToolResult {
        let parsed = match parse_name(&name) {
            Ok(parsed) => parsed,
            Err(result) => return result,
        };

        match self.surfaces().delete(kind.kind(), &parsed).await {
            Ok(()) => CallToolResult::structured(serde_json::json!({"deleted": name})),
            Err(error) => tool_error(&error),
        }
    }

    #[tool(
        name = "run_metric",
        description = "Compiles a stored metric and runs it, returning its rows. Use this to answer a question from the data, and to check a metric produces what a widget expects to draw."
    )]
    async fn run_metric(
        &self,
        Parameters(NameRequest { name }): Parameters<NameRequest>,
    ) -> CallToolResult {
        let parsed = match parse_name(&name) {
            Ok(parsed) => parsed,
            Err(result) => return result,
        };

        match self.surfaces().run_metric(&parsed).await {
            Ok(result) => match serde_json::to_value(&result) {
                Ok(value) => CallToolResult::structured(value),
                Err(error) => tool_error_message(&format!("metric result could not be encoded: {error}")),
            },
            Err(error) => tool_error(&error),
        }
    }

    #[tool(
        name = "list_tables",
        description = "Every database and table this server can see, each with its columns and the layer it belongs to. Call this before writing a metric, so the metric names a table and columns that exist."
    )]
    async fn list_tables(&self) -> CallToolResult {
        match self.surfaces().tables().await {
            Ok(tables) => {
                let entries: Vec<TableEntry> = tables.iter().map(table_entry).collect();
                match serde_json::to_value(&entries) {
                    Ok(value) => CallToolResult::structured(serde_json::json!({"tables": value})),
                    Err(error) => {
                        tool_error_message(&format!("table catalogue could not be encoded: {error}"))
                    }
                }
            }
            Err(error) => tool_error(&error),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for CustomSurfaces {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new("insight-custom-surfaces", env!("CARGO_PKG_VERSION"))
                    .with_title("Insight custom surfaces"),
            )
            .with_instructions(
                "Author metrics, widgets and dashboards. Call list_tables to learn what data \
                 exists, put_metric to define a query over it, run_metric to see its rows, then \
                 put_widget to draw those rows and put_dashboard to hold the widgets. A widget \
                 names its metric's columns by their as_name.",
            )
    }
}

fn table_entry(schema: &TableSchema) -> TableEntry {
    TableEntry {
        database: schema.database.clone(),
        table: schema.table.clone(),
        layer: layer_name(schema.layer).to_owned(),
        columns: schema
            .columns
            .iter()
            .map(|(name, r#type)| ColumnEntry {
                name: name.clone(),
                r#type: r#type.clone(),
            })
            .collect(),
    }
}

fn layer_name(layer: Layer) -> &'static str {
    match layer {
        Layer::Bronze => "bronze",
        Layer::Silver => "silver",
        Layer::Gold => "gold",
        Layer::Identity => "identity",
        Layer::Ingest => "ingest",
        Layer::Other => "other",
    }
}

fn parse_name(raw: &str) -> Result<DefinitionName, CallToolResult> {
    DefinitionName::parse(raw).map_err(|error| tool_error_message(&error.to_string()))
}

fn tool_error(error: &CustomError) -> CallToolResult {
    if !error.is_about_the_caller() {
        tracing::error!(%error, "an MCP tool call failed");
    }

    tool_error_message(&error.to_string())
}

fn tool_error_message(message: &str) -> CallToolResult {
    CallToolResult::error(vec![rmcp::model::ContentBlock::text(message.to_owned())])
}
```

Add `pub(crate) mod tools;` to `src/mcp/mod.rs`.

`Catalog` and `MetricRunner` must be `Clone` for `CustomSurfaces` to be. If either is not, derive `Clone` on it — both hold only an `insight_clickhouse::Client` and plain values.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src/backend && cargo test -p insight-v3-core mcp::tools`
Expected: PASS, 11 tests.

- [ ] **Step 5: Lint and commit**

```bash
cd src/backend && cargo fmt && cargo clippy -p insight-v3-core --all-targets -- -D warnings
git add src/backend/services/insight-v3-core
git commit -m "feat(insight-v3-core): author a metric, a widget and a dashboard over MCP"
```

---

### Task 5: The listener

**Files:**
- Modify: `src/backend/services/insight-v3-core/src/mcp/mod.rs`
- Create: `src/backend/services/insight-v3-core/src/mcp/tests.rs`
- Modify: `src/backend/services/insight-v3-core/src/gear.rs`

**Interfaces:**
- Consumes: `crate::config::McpConfig`, `crate::mcp::tools::CustomSurfaces`, `crate::mcp::auth::{TokenVerifier, authenticate, validate_public_url}`.
- Produces:
  ```rust
  pub(crate) fn router(config: &McpConfig, surfaces: CustomSurfaces, cancellation: CancellationToken) -> anyhow::Result<Router>;
  pub(crate) async fn start(config: &McpConfig, surfaces: CustomSurfaces, cancellation: CancellationToken) -> anyhow::Result<()>;
  ```

`MAX_REQUEST_BODY_BYTES` is a new module constant, `1024 * 1024`. Hosts allowed are `["localhost"]`, matching the `Host: localhost` the gateway sets on a bearer route.

- [ ] **Step 1: Write the failing tests**

Create `src/mcp/tests.rs`:

```rust
use std::error::Error;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tokio_util::sync::CancellationToken;
use tower::ServiceExt as _;

use super::*;
use crate::config::McpConfig;
use crate::definitions::memory::MemoryDefinitions;

type R = Result<(), Box<dyn Error>>;

fn surfaces() -> crate::mcp::tools::CustomSurfaces {
    let client = || {
        insight_clickhouse::Client::new(insight_clickhouse::Config::new(
            "http://clickhouse.invalid",
            "insight",
        ))
    };

    crate::mcp::tools::CustomSurfaces::new(
        Arc::new(MemoryDefinitions::new()),
        crate::metric_query::MetricRunner::new(client()),
        crate::metric_query::People::new("identity"),
        crate::catalog::Catalog::new(client(), "insight".to_owned()),
    )
}

fn enabled_config() -> McpConfig {
    McpConfig {
        enabled: true,
        bind_addr: "127.0.0.1:0".to_owned(),
        public_url: "http://localhost:3000".to_owned(),
        allow_insecure_private_network: false,
    }
}

#[tokio::test]
async fn a_disabled_server_binds_nothing_and_reports_success() -> R {
    let config = McpConfig::default();

    start(&config, surfaces(), CancellationToken::new()).await?;

    Ok(())
}

#[tokio::test]
async fn an_enabled_server_with_no_public_url_refuses_to_start() {
    let config = McpConfig {
        enabled: true,
        ..McpConfig::default()
    };

    assert!(start(&config, surfaces(), CancellationToken::new()).await.is_err());
}

#[tokio::test]
async fn a_request_without_a_bearer_token_is_challenged() -> R {
    let router = router(&enabled_config(), surfaces(), CancellationToken::new())?;

    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(crate::mcp::auth::MCP_PATH)
                .header("host", "localhost")
                .body(Body::empty())?,
        )
        .await?;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(
        response
            .headers()
            .contains_key(axum::http::header::WWW_AUTHENTICATE),
        "a challenge names where to authorize"
    );

    Ok(())
}

#[tokio::test]
async fn a_request_carrying_a_token_this_server_cannot_verify_is_refused() -> R {
    let router = router(&enabled_config(), surfaces(), CancellationToken::new())?;

    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(crate::mcp::auth::MCP_PATH)
                .header("host", "localhost")
                .header("authorization", "Bearer not-a-token")
                .body(Body::empty())?,
        )
        .await?;

    assert!(
        response.status() == StatusCode::UNAUTHORIZED
            || response.status() == StatusCode::SERVICE_UNAVAILABLE,
        "unexpected status: {}",
        response.status()
    );

    Ok(())
}

#[tokio::test]
async fn an_enabled_server_binds_its_address() -> R {
    let cancellation = CancellationToken::new();

    start(&enabled_config(), surfaces(), cancellation.clone()).await?;
    cancellation.cancel();

    Ok(())
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src/backend && cargo test -p insight-v3-core mcp::tests`
Expected: FAIL — `router` and `start` do not exist.

- [ ] **Step 3: Implement the listener in `src/mcp/mod.rs`**

```rust
//! The MCP server: the custom surfaces, for a client that speaks MCP.

use std::sync::Arc;

use axum::Router;
use axum::middleware;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use tokio_util::sync::CancellationToken;

use crate::config::McpConfig;

pub(crate) mod auth;
pub(crate) mod tools;

#[cfg(test)]
mod tests;

const MAX_REQUEST_BODY_BYTES: usize = 1024 * 1024;

pub(crate) fn router(
    config: &McpConfig,
    surfaces: tools::CustomSurfaces,
    cancellation: CancellationToken,
) -> anyhow::Result<Router> {
    let service: StreamableHttpService<tools::CustomSurfaces, LocalSessionManager> =
        StreamableHttpService::new(
            move || Ok(surfaces.clone()),
            Arc::default(),
            StreamableHttpServerConfig::default()
                .with_legacy_session_mode(false)
                .with_json_response(true)
                .with_allowed_hosts(["localhost"])
                .with_max_request_body_bytes(MAX_REQUEST_BODY_BYTES)
                .with_cancellation_token(cancellation),
        );

    let verifier = auth::TokenVerifier::new(
        &config.public_url,
        config.allow_insecure_private_network,
    )?;

    Ok(Router::new()
        .nest_service(auth::MCP_PATH, service)
        .layer(middleware::from_fn_with_state(
            verifier,
            auth::authenticate,
        )))
}

pub(crate) async fn start(
    config: &McpConfig,
    surfaces: tools::CustomSurfaces,
    cancellation: CancellationToken,
) -> anyhow::Result<()> {
    if !config.enabled {
        return Ok(());
    }

    let router = router(config, surfaces, cancellation.child_token())?;

    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await?;
    tracing::info!(bind_addr = %config.bind_addr, "custom-surface MCP server listening");

    tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, router)
            .with_graceful_shutdown(cancellation.cancelled_owned())
            .await
        {
            tracing::error!(%error, "custom-surface MCP server stopped unexpectedly");
        }
    });

    Ok(())
}
```

In `src/gear.rs`, after `self.runtime.set(runtime)` succeeds, build the tool state from the same pieces the runtime holds and start the listener:

```rust
        crate::mcp::start(
            config.mcp(),
            crate::mcp::tools::CustomSurfaces::new(
                definitions,
                crate::metric_query::MetricRunner::new(config.clickhouse_query_client()),
                crate::metric_query::People::new(config.identity_database()),
                crate::catalog::Catalog::new(
                    config.clickhouse_query_client(),
                    config.clickhouse_database(),
                ),
            ),
            ctx.cancellation_token().child_token(),
        )
        .await?;
```

`definitions` is the `Arc<dyn Definitions>` already built above; clone it into `RuntimeState` so both the REST router and the MCP server share the one store.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src/backend && cargo test -p insight-v3-core mcp`
Expected: PASS — the auth, tools and listener suites together.

- [ ] **Step 5: Build the whole workspace, lint, commit**

```bash
cd src/backend && cargo fmt && cargo clippy -p insight-v3-core --all-targets -- -D warnings && cargo test -p insight-v3-core
git add src/backend/services/insight-v3-core
git commit -m "feat(insight-v3-core): serve the custom surfaces on their own MCP listener"
```

---

### Task 6: The authenticator learns a second resource

**Files:**
- Modify: `src/backend/services/authenticator/src/mcp_oauth/types.rs`
- Modify: `src/backend/services/authenticator/src/mcp_oauth/handlers.rs`
- Modify: `src/backend/services/authenticator/src/api/mod.rs`
- Modify: `src/backend/services/authenticator/src/mcp_oauth/grant_tests.rs`
- Modify: `src/backend/services/authenticator/tests/e2e_mcp_oauth.rs`

**Interfaces:**
- Produces:
  ```rust
  pub const MCP_SCOPE: &str = "mcp:query";
  pub const MCP_AUTHOR_SCOPE: &str = "mcp:author";
  pub struct McpResource { pub path: &'static str, pub scope: &'static str }
  pub const MCP_RESOURCES: [McpResource; 2];
  ```
  and, in `handlers.rs`, `fn mcp_resource_for(state: &AppState, resource: &str) -> Option<&'static McpResource>`.

- [ ] **Step 1: Write the failing tests**

In `src/mcp_oauth/grant_tests.rs`, add:

```rust
#[test]
fn each_mcp_resource_pairs_with_exactly_one_scope() {
    let mut pairs: Vec<(&str, &str)> = MCP_RESOURCES
        .iter()
        .map(|resource| (resource.path, resource.scope))
        .collect();
    pairs.sort_unstable();

    assert_eq!(pairs, [("/mcp", "mcp:query"), ("/mcp/v3", "mcp:author")]);
}

#[test]
fn the_authoring_scope_is_not_valid_for_the_read_only_resource() {
    let read_only = MCP_RESOURCES
        .iter()
        .find(|resource| resource.path == "/mcp")
        .expect("the read-only resource is declared");

    assert_ne!(read_only.scope, MCP_AUTHOR_SCOPE);
}
```

In `tests/e2e_mcp_oauth.rs`, add cases against the running authenticator, following the file's existing helpers:

```rust
#[tokio::test]
async fn authorizing_a_resource_that_is_not_declared_is_refused() -> R {
    // Drive /auth/oauth/authorize with resource = "{origin}/mcp/v4" and assert
    // the error body names "invalid_target", using this file's existing
    // client-registration and authorize helpers.
}

#[tokio::test]
async fn authorizing_the_authoring_resource_with_the_read_only_scope_is_refused() -> R {
    // resource = "{origin}/mcp/v3", scope = "mcp:query" -> "invalid_scope".
}

#[tokio::test]
async fn the_authoring_resource_has_its_own_metadata_document() -> R {
    // GET /.well-known/oauth-protected-resource/mcp/v3 -> 200, and the body's
    // "resource" ends with "/mcp/v3" and "scopes_supported" is ["mcp:author"].
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src/backend && cargo test -p authenticator mcp_oauth`
Expected: FAIL — `MCP_RESOURCES` and `MCP_AUTHOR_SCOPE` do not exist.

- [ ] **Step 3: Implement the resource set**

In `types.rs`, keep `MCP_SCOPE` and add:

```rust
pub const MCP_AUTHOR_SCOPE: &str = "mcp:author";

#[derive(Debug, Clone, Copy)]
pub struct McpResource {
    pub path: &'static str,
    pub scope: &'static str,
}

pub const MCP_RESOURCES: [McpResource; 2] = [
    McpResource {
        path: "/mcp",
        scope: MCP_SCOPE,
    },
    McpResource {
        path: "/mcp/v3",
        scope: MCP_AUTHOR_SCOPE,
    },
];
```

In `handlers.rs`, replace `resource_url` with a lookup and keep a single-resource helper for the paths that still need a default:

```rust
fn mcp_resource_for(state: &AppState, resource: &str) -> Option<&'static McpResource> {
    let origin = public_origin(state);

    MCP_RESOURCES
        .iter()
        .find(|candidate| resource == endpoint(&origin, candidate.path))
}

fn resource_url(state: &AppState, resource: &McpResource) -> String {
    endpoint(&public_origin(state), resource.path)
}
```

In the authorize validation, replace the two rejections:

```rust
    let resource = required(query.resource, "resource")?;
    let Some(known) = mcp_resource_for(state, &resource) else {
        return Err(failure("invalid_target", "resource is not supported"));
    };
    let scope = query.scope.unwrap_or_else(|| known.scope.to_owned());
    if scope != known.scope {
        return Err(failure("invalid_scope", "scope is not supported"));
    }
```

In `authorization_server_metadata`, advertise the union:

```rust
            "scopes_supported": MCP_RESOURCES.iter().map(|r| r.scope).collect::<Vec<_>>(),
```

Make `protected_resource_metadata` resource-aware by reading the request path:

```rust
pub async fn protected_resource_metadata(
    Extension(state): Extension<Arc<AppState>>,
    uri: axum::http::Uri,
) -> Response {
    if !state.cfg.mcp_oauth.enabled {
        return StatusCode::NOT_FOUND.into_response();
    }

    let suffix = uri
        .path()
        .strip_prefix("/.well-known/oauth-protected-resource")
        .unwrap_or_default();
    let requested = if suffix.is_empty() { "/mcp" } else { suffix };
    let Some(resource) = MCP_RESOURCES
        .iter()
        .find(|candidate| candidate.path == requested)
    else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let origin = public_origin(&state);
    json_response(
        StatusCode::OK,
        &json!({
            "resource": resource_url(&state, resource),
            "authorization_servers": [origin],
            "scopes_supported": [resource.scope],
            "bearer_methods_supported": ["header"],
        }),
    )
}
```

Every other `resource_url(&state)` call site takes the resource the surrounding code already has: the authorize flow and the token flows carry `pending.resource` / `grant.resource` strings and compare them, so they need no change beyond compiling against the new signature.

In `src/api/mod.rs`, register the third metadata path beside the existing two, copying the `/.well-known/oauth-protected-resource/mcp` builder verbatim and changing only the path to `/.well-known/oauth-protected-resource/mcp/v3` and the operation id.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src/backend && cargo test -p authenticator`
Expected: PASS.

- [ ] **Step 5: Lint and commit**

```bash
cd src/backend && cargo fmt && cargo clippy -p authenticator --all-targets -- -D warnings
git add src/backend/services/authenticator
git commit -m "feat(authenticator): issue MCP grants for more than one resource"
```

---

### Task 7: The edge routes the second server

**Files:**
- Modify: `src/backend/tools/routegen/src/schema.rs`
- Modify: `src/backend/tools/routegen/src/emit.rs`
- Modify: `src/backend/tools/routegen/tests/golden.rs`
- Modify: `src/backend/services/gateway/lua/gateway.lua`
- Modify: `src/backend/services/gateway/lua/errors.lua`

**Interfaces:**
- Produces: `Route::mcp_scope: Option<String>` and `ResolvedRoute::mcp_scope: String` (defaulting to `mcp:query`); `fn mcp_resource_metadata_url(raw: Option<&str>, path: &str) -> Result<Option<String>, McpPublicUrlError>`; Lua `pass_bearer(resource_metadata_url, scope)`.

A bearer route's challenge must name its own metadata document and its own scope. Today both are one global value, so a `401` from a second MCP server would send the client to the first server's document.

- [ ] **Step 1: Write the failing tests**

In `tests/golden.rs`, add a case with two bearer routes, following the file's existing golden-comparison helper:

```rust
#[test]
fn each_bearer_route_challenges_with_its_own_metadata_document_and_scope() {
    let yaml = r#"
version: 1
routes:
  - prefix: /mcp
    upstream: http://analytics:8086
    auth: bearer
  - prefix: /mcp/v3
    upstream: http://insight-v3-core:8087
    auth: bearer
    mcp_scope: "mcp:author"
"#;

    let config = RouteConfig::parse(yaml).expect("the document is valid");
    let settings = Settings {
        mcp_public_url: Some("https://insight.example.invalid".to_owned()),
        ..Settings::default()
    };
    let conf = emit(&config, &settings).expect("the config emits");

    assert!(
        conf.contains(
            r#"pass_bearer("https://insight.example.invalid/.well-known/oauth-protected-resource/mcp", "mcp:query")"#
        ),
        "{conf}"
    );
    assert!(
        conf.contains(
            r#"pass_bearer("https://insight.example.invalid/.well-known/oauth-protected-resource/mcp/v3", "mcp:author")"#
        ),
        "{conf}"
    );
    assert!(
        conf.contains("location = /.well-known/oauth-protected-resource/mcp/v3 {"),
        "{conf}"
    );
}

#[test]
fn a_bearer_route_without_a_declared_scope_keeps_the_read_only_one() {
    let yaml = r#"
version: 1
routes:
  - prefix: /mcp
    upstream: http://analytics:8086
    auth: bearer
"#;

    let config = RouteConfig::parse(yaml).expect("the document is valid");

    assert_eq!(config.resolved_routes()[0].mcp_scope, "mcp:query");
}
```

Adjust the import list and the `emit`/`Settings` call shape to match what `tests/golden.rs` already uses.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src/backend && cargo test -p routegen`
Expected: FAIL — `mcp_scope` is not a field, and `deny_unknown_fields` rejects it in the YAML.

- [ ] **Step 3: Implement**

In `schema.rs`, add the constant and the field:

```rust
pub const DEFAULT_MCP_SCOPE: &str = "mcp:query";
```

to `Route`:

```rust
    #[serde(default)]
    pub mcp_scope: Option<String>,
```

to `ResolvedRoute`:

```rust
    pub mcp_scope: String,
```

and in `Route::resolve`:

```rust
            mcp_scope: self
                .mcp_scope
                .clone()
                .unwrap_or_else(|| DEFAULT_MCP_SCOPE.to_owned()),
```

In `emit.rs`, give the metadata-URL builder a path:

```rust
fn mcp_resource_metadata_url(
    raw: Option<&str>,
    path: &str,
) -> Result<Option<String>, McpPublicUrlError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let url = Url::parse(raw)?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(McpPublicUrlError::InvalidOrigin);
    }

    Ok(Some(format!(
        "{}/.well-known/oauth-protected-resource{path}",
        url.as_str().trim_end_matches('/')
    )))
}
```

Keep the existing global emission as the fallback by calling it with `"/mcp"`.

Emit the well-known locations from the bearer routes rather than a fixed list — replace the hardcoded array with:

```rust
    let mut well_known = vec![
        "/.well-known/oauth-authorization-server".to_owned(),
        "/.well-known/oauth-protected-resource".to_owned(),
        "/.well-known/jwks.json".to_owned(),
    ];
    for route in routes {
        if route.auth == Authentication::Bearer {
            let path = format!("/.well-known/oauth-protected-resource{}", route.prefix);
            if !well_known.contains(&path) {
                well_known.push(path);
            }
        }
    }

    for path in &well_known {
```

In `emit_api_location_block`, pass the route's own values to Lua:

```rust
        Authentication::Bearer => {
            let metadata =
                mcp_resource_metadata_url(settings.mcp_public_url.as_deref(), &route.prefix)?
                    .unwrap_or_default();
            writeln!(
                c,
                "            access_by_lua_block {{ require(\"gateway\").pass_bearer(\"{metadata}\", \"{}\") }}",
                route.mcp_scope
            )?;
        }
```

`emit_api_location_block` needs `settings` in scope; thread it through from the caller alongside `config`.

In `gateway.lua`:

```lua
function _M.pass_bearer(resource_metadata_url, scope)
    local authorization = ngx.var.http_authorization
    local scheme, token = string.match(authorization or "", "^(%S+) (%S+)$")
    if not scheme or string.lower(scheme) ~= "bearer" or not token then
        local metadata = resource_metadata_url
        if metadata == nil or metadata == "" then
            metadata = cfg.mcp_resource_metadata_url
        end
        return errors.bearer_unauthorized(metadata, scope)
    end

    ngx.req.clear_header("Cookie")
    set_request_context()
end
```

In `errors.lua`:

```lua
function _M.bearer_unauthorized(resource_metadata_url, scope)
    local required_scope = scope or "mcp:query"
    local challenge = 'Bearer scope="' .. required_scope .. '"'
    if resource_metadata_url then
        challenge = 'Bearer resource_metadata="' .. resource_metadata_url
            .. '", scope="' .. required_scope .. '"'
    end
    ...
```

keeping the rest of the function as it stands.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src/backend && cargo test -p routegen`
Expected: PASS. Existing golden files change — regenerate them the way the file's own instructions say, and read the diff before accepting it: every changed line should be a `pass_bearer(...)` call gaining two arguments.

- [ ] **Step 5: Run the gateway's own tests**

Run: `cd src/backend/services/gateway/tests && ./run-e2e.sh`
Expected: PASS. If the harness is unavailable on this machine, say so rather than reporting a pass.

- [ ] **Step 6: Commit**

```bash
git add src/backend/tools/routegen src/backend/services/gateway/lua
git commit -m "feat(gateway): challenge each MCP route with its own resource and scope"
```

---

### Task 8: Deployment wiring

**Files:**
- Modify: `deploy/compose/gateway/routes.yaml`
- Modify: `docker-compose.yml`
- Modify: `deploy/compose/insight-init.sh`
- Modify: `src/backend/services/gateway/helm/values.yaml`
- Modify: `charts/insight/values.yaml`
- Modify: `charts/insight/values.schema.json`
- Modify: `src/backend/services/insight-v3-core/helm/values.yaml`
- Modify: `src/backend/services/insight-v3-core/helm/templates/deployment.yaml`
- Modify: `src/backend/services/insight-v3-core/helm/templates/service.yaml`

- [ ] **Step 1: Add the compose route**

In `deploy/compose/gateway/routes.yaml`, after the `/api/v3` route:

```yaml
  - prefix: /mcp/v3
    upstream: http://insight-v3-core:8087
    auth: bearer
    mcp_scope: "mcp:author"
    timeout_ms: 40000
```

- [ ] **Step 2: Add the Helm route**

In `src/backend/services/gateway/helm/values.yaml`, after the `/mcp` route:

```yaml
    - prefix: /mcp/v3
      upstream: 'http://{{ .Release.Name }}-v3-core:8087'
      auth: bearer
      mcpScope: "mcp:author"
      timeoutMs: 40000
      mcpOnly: true
```

and extend the route-rendering template in `configmap.yaml` to emit `mcp_scope` when `mcpScope` is set, mirroring how it already emits `auth` and `timeoutMs`.

- [ ] **Step 3: Configure the service in compose**

In `docker-compose.yml`, on the `insight-v3-core` service, add:

```yaml
      APP__gears__insight_v3_core__config__mcp__enabled: "${INSIGHT_V3_MCP_ENABLED:-false}"
      APP__gears__insight_v3_core__config__mcp__bind_addr: "0.0.0.0:8087"
      APP__gears__insight_v3_core__config__mcp__public_url: "${INSIGHT_V3_MCP_PUBLIC_URL:-}"
      APP__gears__insight_v3_core__config__mcp__allow_insecure_private_network: "${INSIGHT_V3_MCP_ALLOW_INSECURE_PRIVATE_NETWORK:-false}"
```

In `deploy/compose/insight-init.sh`, set `INSIGHT_V3_MCP_ENABLED` and `INSIGHT_V3_MCP_PUBLIC_URL` alongside the existing `MCP_ENABLED` block, so a stand that enables MCP enables both servers against the same public origin.

- [ ] **Step 4: Configure the service in the chart**

In `src/backend/services/insight-v3-core/helm/values.yaml`, add an `mcp` block with `enabled`, `port: 8087`, `publicUrl` and `allowInsecurePrivateNetwork`. In `deployment.yaml`, emit the four `APP__gears__insight_v3_core__config__mcp__*` env vars, guarded on `mcp.enabled`, following how the file already emits the ClickHouse and identity settings. In `service.yaml`, expose the port under the same guard.

In `charts/insight/values.yaml`, add the umbrella keys beside the existing `v3-core` block, and in `values.schema.json` add their types so a typo fails `helm template` rather than at runtime.

- [ ] **Step 5: Verify the templates render**

Run: `helm template insight charts/insight --set global.mcp.enabled=true --set global.mcp.publicUrl=https://insight.example.invalid --set v3Core.mcp.enabled=true --set v3Core.mcp.publicUrl=https://insight.example.invalid > /dev/null`
Expected: renders with no error. Then grep the rendered output for `mcp/v3` and confirm the gateway config carries the new route and the v3-core deployment carries the four env vars.

- [ ] **Step 6: Run the chart's own tests**

Run: the pytest suite under `src/backend/services/insight-v3-core/helm/tests/` if one exists, otherwise the umbrella chart tests the repo already runs in CI.
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add deploy charts docker-compose.yml src/backend/services/gateway/helm src/backend/services/insight-v3-core/helm
git commit -m "feat(deploy): route /mcp/v3 to the custom-surface MCP server"
```

---

### Task 9: The end-to-end test

**Files:**
- Create: `src/backend/services/insight-v3-core/tests/mcp.sh`
- Modify: `src/backend/services/insight-v3-core/tests/ci.sh`

Follow `tests/mvp.sh` for shape: `set -euo pipefail`, a `BASE_URL` with a default, `curl` with `--fail-with-body`, a synthetic fixture, and a cleanup trap that removes what the test created.

- [ ] **Step 1: Write the test script**

The script must:

1. Read the MCP access token from `INSIGHT_MCP_TOKEN`, skipping with a clear message if it is unset — a browser authorization cannot be automated here.
2. `POST {BASE_URL}/mcp/v3` with an `initialize` request and assert the response names `insight-custom-surfaces`.
3. `tools/list` and assert all eight tool names are present.
4. `tools/call` `list_tables` and assert the response carries at least one table.
5. `tools/call` `put_metric` with a synthetic metric over the ingest table the fixture wrote, then `run_metric` and assert the rows come back.
6. `tools/call` `put_widget` naming a column the metric does not produce, and assert the call returns a tool error mentioning that column.
7. `tools/call` `put_widget` correctly, then `put_dashboard` holding it.
8. `tools/call` `delete_definition` on the metric and assert it is refused and names the widget.
9. Delete dashboard, then widget, then metric, and assert each succeeds.
10. `POST {BASE_URL}/mcp/v3` with no `Authorization` header and assert `401` with a `WWW-Authenticate` header naming `oauth-protected-resource/mcp/v3`.

- [ ] **Step 2: Bring up the stack and run it**

```bash
./dev-compose.sh up
INSIGHT_MCP_TOKEN=... src/backend/services/insight-v3-core/tests/mcp.sh
```

Expected: every assertion passes. Run it for real; do not commit an unrun script.

- [ ] **Step 3: Register it with the service's CI script**

Add `mcp.sh` to `tests/ci.sh` next to the other scripts, guarded so it skips rather than fails when `INSIGHT_MCP_TOKEN` is absent.

- [ ] **Step 4: Commit**

```bash
git add src/backend/services/insight-v3-core/tests
git commit -m "test(insight-v3-core): drive the MCP server end to end"
```

---

## Self-review

**Spec coverage.** Section 1 (own listener) → Task 5. Section 2 (verifier, new scope and resource, token-as-admin-proof) → Tasks 3 and 5. Section 3 (resource set, paired scopes, per-resource metadata) → Task 6. Section 4 (eight tools, per-kind put schemas, reused validation, domain errors as tool errors) → Tasks 1 and 4. Section 5 (gateway route, per-route metadata URL, chart values) → Tasks 7 and 8. Section 6 (unit, authenticator, end-to-end) → Tasks 1, 2, 3, 4, 5, 6, 7 and 9. Section 7 (out of scope) → no task, correctly. Section 8 (known defect) → no task, deliberately.

**Type consistency.** `Surfaces` is constructed by `Surfaces::new(&dyn Definitions, &MetricRunner, &People, &Catalog)` in Tasks 1, 4 and 5. `CustomError` variants are matched in Tasks 1 and 4 with the same names. `CustomSurfaces::new(Arc<dyn Definitions>, MetricRunner, People, Catalog)` is used identically in Tasks 4 and 5. `MCP_PATH` and `MCP_SCOPE` are defined in Task 3 and consumed in Tasks 4 and 5. `mcp_scope` is added to the schema in Task 7 and consumed by the YAML in Task 8.

**Open risk carried into execution.** Task 4 assumes `MetricRunner` and `Catalog` are `Clone`; Task 4's step 3 says to derive it if they are not. Task 8 assumes the gateway Helm chart renders routes from a values list; if that template turns out to hardcode the route set, the route is added there instead, and the task's deliverable is unchanged.
