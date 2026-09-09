# Time ranges for the v3 custom pages — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** one metric definition answers yesterday through last year, with the window picked while reading a dashboard.

**Architecture:** the metric declares which field is its clock; the dashboard declares which windows it offers; the runner resolves a token to bounds and a bucket at request time and injects both into the compiled SQL. Nothing is precomputed and nothing is stored per window.

**Tech Stack:** Rust (axum, `clickhouse` 0.15, `chrono`, `sea-orm`), ClickHouse, React 19 + TanStack Query/Router, Vitest, `cargo test`.

**Spec:** [../specs/2026-09-09-insight-v3-time-ranges-design.md](../specs/2026-09-09-insight-v3-time-ranges-design.md) — read it before Task 1. Every task below implements a numbered section of it.

## Global Constraints

- **Tokens on the wire are ISO-shaped, labels are human.** `PDC`, `P7D`, `P30D`, `PMC`, `PQC`, `P1Y`, `inf`, or an ISO 8601 interval `<from>/<to>`. Never a human label.
- **Grain is derived from the range, never declared on a metric:** `PDC`→hour, `P7D`/`P30D`/`PMC`→day, `PQC`→week, `P1Y`/`inf`→month.
- **Windows are half-open:** `>= from AND < to`. Never `BETWEEN`.
- **Windows anchor to the newest row in the data**, not to wall-clock now.
- **The timezone default is `UTC`.** An absent `tz` behaves exactly as UTC.
- **An absent request body means unbounded, unbucketed, UTC** — byte-identical SQL to today.
- **`FINAL` is emitted only for engines ending in `ReplacingMergeTree`.** On plain `MergeTree` it is `Code: 181 ILLEGAL_FINAL`, and every v3 ingest table is plain `MergeTree`.
- **No comments** unless the fact is invisible from the code and would cause a bug unstated. This repo's existing doc-comments are the house style for module and type headers; do not add inline commentary.
- **Conventional commits**, `feat(insight-v3-core):` / `test(insight-v3-core):`. Never amend; new commit on top.

## Change from the spec, decided while planning

Spec section 5 has the handler resolve bounds in Rust and bind them as unix seconds. **`chrono-tz` is not a workspace dependency** (`chrono` is, without a timezone database), so per-user zones would mean adding one.

Instead **all window arithmetic is emitted as ClickHouse SQL over the anchor**, with the zone as a literal: `toStartOfDay(<anchor>, 'Europe/Belgrade') - INTERVAL 1 DAY`. Three reasons this is better and not merely cheaper: no new dependency; one engine computes both the bounds and the buckets, so they cannot disagree about a DST boundary; and the anchor query and the window become one round trip instead of two.

The cost is that the zone reaches SQL as a literal rather than a bind, so it must be character-checked before interpolation (Task 2). An unknown-but-well-formed zone is refused by ClickHouse and surfaces as a 400.

## File Structure

| File | Responsibility |
|---|---|
| `src/backend/services/insight-v3-core/src/window.rs` | **New.** `Range` token parsing, `Grain`, and the SQL fragments for bounds and bucket. Pure string/typed logic, no I/O. |
| `.../src/window/tests.rs` | **New.** Token → SQL and token → grain tests. |
| `.../src/metric_query.rs` | `FieldType::Datetime`, `MetricQuery::time`, `max_range`, `compile(people, window, replacing)`, bucket injection, `column_names` including the bucket, the anchor query. |
| `.../src/catalog.rs` | Reads each table's `engine` alongside its columns, under the same cache. |
| `.../src/custom.rs` | `run_metric` takes a request, resolves the anchor, builds the window, passes the engine. |
| `.../src/api/metric_run.rs` | The run endpoint gains a body and two new error mappings. |
| `.../src/mcp/tools.rs` | `put_metric` documents `time`/`max_range`; `run_metric` accepts a range. |
| `.../src/chat.rs` | Validation compile passes an unbounded window; the system prompt teaches the clock. |
| `src/frontend/src/api/custom-client.ts` | `runMetric(name, range?, tz?)`, `Dashboard.time_ranges`. |
| `src/frontend/src/queries/custom.ts` | `metricResultQuery(name, range, tz)` — range and zone in the key. |
| `src/frontend/src/lib/portal/portal-search.ts` | `range` on `PortalSearch` and its validator. |
| `src/frontend/src/components/custom/range-picker.tsx` | **New.** The custom zone's picker: toggle group over a board's tokens plus a calendar for an interval. |
| `src/frontend/src/routes/portal.custom.$name.tsx` | Header picker, range passed to each slot, all-time badge. |

---

### Task 1: A metric can declare which field is its clock

Spec section 3. No windowing yet — this task only makes a timestamp expressible.

**Files:**
- Modify: `src/backend/services/insight-v3-core/src/metric_query.rs`
- Test: `src/backend/services/insight-v3-core/src/metric_query.rs` (its `mod tests`)

**Interfaces:**
- Consumes: nothing.
- Produces: `FieldType::Datetime`; `MetricQuery::time: Option<TimeSource>`; `MetricQuery::max_range(&self) -> Option<&str>`; `MetricQuery::time_sql(&self, qualifier: Option<&str>) -> Option<Result<String, MetricQueryError>>`.

- [ ] **Step 1: Write the failing tests**

In `metric_query.rs`'s `mod tests`:

```rust
#[test]
fn a_json_clock_is_parsed_out_of_the_payload() {
    let metric = query(json!({
        "table": "events",
        "time": { "json": "committed_at", "type": "datetime" },
        "fields": [{ "json": "sha", "type": "string", "agg": "count", "as_name": "commits" }]
    }));

    let sql = metric
        .time_sql(None)
        .expect("a clock was declared")
        .expect("it compiles");

    assert_eq!(
        sql,
        "parseDateTimeBestEffort(JSONExtractString(raw_data, 'committed_at'))"
    );
}

#[test]
fn a_column_clock_is_read_as_it_stands() {
    let metric = query(json!({
        "table": "silver.class_git_pull_requests",
        "time": { "column": "created_on", "type": "datetime" },
        "fields": [{ "column": "pr_id", "type": "int", "agg": "count", "as_name": "prs" }]
    }));

    let sql = metric
        .time_sql(None)
        .expect("a clock was declared")
        .expect("it compiles");

    assert_eq!(sql, "`created_on`");
}

#[test]
fn a_metric_without_a_clock_has_no_time_sql() {
    let metric = query(json!({
        "table": "events",
        "fields": [{ "json": "sha", "type": "string", "agg": "count", "as_name": "commits" }]
    }));

    assert!(metric.time_sql(None).is_none());
}

#[test]
fn a_clock_naming_both_sources_is_refused() {
    let metric = query(json!({
        "table": "events",
        "time": { "json": "committed_at", "column": "created_on", "type": "datetime" },
        "fields": [{ "json": "sha", "type": "string", "agg": "count", "as_name": "commits" }]
    }));

    assert!(metric.time_sql(None).expect("declared").is_err());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p insight-v3-core metric_query::tests::a_json_clock`
Expected: FAIL — `no method named time_sql`.

- [ ] **Step 3: Add the type and the field**

In `metric_query.rs`, extend `FieldType` and its `extract`:

```rust
pub(crate) enum FieldType {
    String,
    Int,
    Float,
    Datetime,
}
```

```rust
    fn extract(self, json: &str, qualifier: Option<&str>) -> String {
        let function = match self {
            Self::String | Self::Datetime => "JSONExtractString",
            Self::Int => "JSONExtractInt",
            Self::Float => "JSONExtractFloat",
        };
        let payload = match qualifier {
            Some(alias) => format!("`{alias}`.raw_data"),
            None => "raw_data".to_owned(),
        };
        let read = format!("{function}({payload}, '{json}')");
        match self {
            Self::Datetime => format!("parseDateTimeBestEffort({read})"),
            Self::String | Self::Int | Self::Float => read,
        }
    }
```

Add the clock beside the other `MetricQuery` fields:

```rust
    #[serde(default)]
    time: Option<TimeSource>,
    #[serde(default)]
    max_range: Option<String>,
```

```rust
#[derive(Debug, Deserialize)]
pub(crate) struct TimeSource {
    #[serde(default)]
    json: Option<String>,
    #[serde(default)]
    column: Option<String>,
    #[serde(default = "datetime_type")]
    r#type: FieldType,
}

fn datetime_type() -> FieldType {
    FieldType::Datetime
}

impl TimeSource {
    fn sql(&self, qualifier: Option<&str>) -> Result<String, MetricQueryError> {
        let source = Source::resolve(self.json.as_deref(), self.column.as_deref())
            .ok_or_else(|| MetricQueryError::FieldSource("a clock".to_owned()))?;
        source.sql(self.r#type, qualifier)
    }
}
```

And the accessors:

```rust
    pub(crate) fn time_sql(
        &self,
        qualifier: Option<&str>,
    ) -> Option<Result<String, MetricQueryError>> {
        self.time.as_ref().map(|time| time.sql(qualifier))
    }

    pub(crate) fn max_range(&self) -> Option<&str> {
        self.max_range.as_deref()
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p insight-v3-core metric_query::tests`
Expected: PASS, and every pre-existing test in that module still passes — `Datetime` is additive.

- [ ] **Step 5: Check for stray comments, then commit**

Run the `/comment-review` skill over the diff and delete any comment restating the code.

```bash
git add src/backend/services/insight-v3-core/src/metric_query.rs
git commit -m "feat(insight-v3-core): a metric can declare which field is its clock"
```

---

### Task 2: Range tokens resolve to SQL bounds and a bucket

Spec section 4 and the planning change above. Pure — no database, no I/O.

**Files:**
- Create: `src/backend/services/insight-v3-core/src/window.rs`
- Create: `src/backend/services/insight-v3-core/src/window/tests.rs`
- Modify: `src/backend/services/insight-v3-core/src/main.rs` (declare `mod window;`)

**Interfaces:**
- Consumes: nothing.
- Produces: `Range::parse(token: &str) -> Result<Bound, WindowError>`; `Bound`; `Grain`; `Window::new(bound: Bound, tz: Option<&str>) -> Result<Window, WindowError>`; `Window::unbounded() -> Window`; `Window::without_bucket(self) -> Window`; `Window::grain(&self) -> Grain`; `Window::bounds_sql(&self, anchor: &str) -> Option<(String, String)>`; `Window::bucket_sql(&self, time: &str) -> Option<String>`; `Window::span_days(&self) -> Option<i64>`; `WindowError`.

- [ ] **Step 1: Write the failing tests**

Create `src/backend/services/insight-v3-core/src/window/tests.rs`:

```rust
use super::*;

fn window(token: &str) -> Window {
    Window::new(Range::parse(token).expect("a known token"), None).expect("valid")
}

#[test]
fn every_token_parses_and_carries_its_grain() {
    assert_eq!(window("PDC").grain(), Grain::Hour);
    assert_eq!(window("P7D").grain(), Grain::Day);
    assert_eq!(window("P30D").grain(), Grain::Day);
    assert_eq!(window("PMC").grain(), Grain::Day);
    assert_eq!(window("PQC").grain(), Grain::Week);
    assert_eq!(window("P1Y").grain(), Grain::Month);
    assert_eq!(window("inf").grain(), Grain::Month);
}

#[test]
fn an_unknown_token_is_refused() {
    assert!(matches!(
        Range::parse("yesterday"),
        Err(WindowError::UnknownRange(_))
    ));
}

#[test]
fn a_complete_period_is_not_a_rolling_one() {
    let (month_from, month_to) = window("PMC").bounds_sql("anchor").expect("bounded");
    let (rolling_from, rolling_to) = window("P30D").bounds_sql("anchor").expect("bounded");

    assert_ne!(month_from, rolling_from);
    assert_ne!(month_to, rolling_to);
    assert_eq!(
        month_from,
        "toStartOfMonth(toStartOfMonth(anchor) - INTERVAL 1 DAY)"
    );
    assert_eq!(month_to, "toStartOfMonth(anchor)");
    assert_eq!(rolling_to, "anchor");
}

#[test]
fn yesterday_is_the_complete_day_before_the_anchor() {
    let (from, to) = window("PDC").bounds_sql("anchor").expect("bounded");
    assert_eq!(from, "toStartOfDay(anchor) - INTERVAL 1 DAY");
    assert_eq!(to, "toStartOfDay(anchor)");
}

#[test]
fn unbounded_has_no_bounds_but_still_buckets() {
    let all = window("inf");
    assert!(all.bounds_sql("anchor").is_none());
    assert_eq!(
        all.bucket_sql("`created_on`").expect("a bucket"),
        "toStartOfMonth(`created_on`)"
    );
}

#[test]
fn an_iso_interval_is_a_range() {
    let interval = window("2026-08-01/2026-09-01");
    let (from, to) = interval.bounds_sql("anchor").expect("bounded");

    assert_eq!(from, "toDateTime('2026-08-01 00:00:00')");
    assert_eq!(to, "toDateTime('2026-09-01 00:00:00')");
    assert_eq!(interval.grain(), Grain::Day);
    assert_eq!(interval.span_days(), Some(31));
}

#[test]
fn a_reversed_interval_is_refused() {
    assert!(matches!(
        Range::parse("2026-09-01/2026-08-01"),
        Err(WindowError::ReversedInterval)
    ));
}

#[test]
fn a_zone_reaches_every_truncation_and_the_bounds() {
    let belgrade =
        Window::new(Range::parse("PDC").expect("token"), Some("Europe/Belgrade")).expect("valid");
    let (from, to) = belgrade.bounds_sql("anchor").expect("bounded");

    assert_eq!(
        from,
        "toStartOfDay(anchor, 'Europe/Belgrade') - INTERVAL 1 DAY"
    );
    assert_eq!(to, "toStartOfDay(anchor, 'Europe/Belgrade')");
    assert_eq!(
        belgrade.bucket_sql("`t`").expect("a bucket"),
        "toStartOfHour(`t`, 'Europe/Belgrade')"
    );
}

#[test]
fn a_zone_that_could_break_out_of_its_quotes_is_refused() {
    assert!(matches!(
        Window::new(Range::parse("P7D").expect("token"), Some("UTC'; DROP")),
        Err(WindowError::InvalidTimezone(_))
    ));
}

#[test]
fn an_unbucketed_window_keeps_its_bounds() {
    let total = window("P30D").without_bucket();
    assert!(total.bounds_sql("anchor").is_some());
    assert!(total.bucket_sql("`t`").is_none());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p insight-v3-core window::`
Expected: FAIL — `unresolved module or unlinked crate 'window'`.

- [ ] **Step 3: Write the module's types**

Create `src/backend/services/insight-v3-core/src/window.rs`:

```rust
//! A range token, and the ClickHouse expressions it becomes.
//!
//! Every bound and every bucket is emitted as SQL over an anchor expression
//! rather than computed here: the same engine then decides where a day starts
//! for the filter and for the grouping, so the two cannot disagree across a
//! DST boundary, and no timezone database is needed in this process.

use std::fmt::Write as _;

use thiserror::Error;

const MAX_TIMEZONE_CHARS: usize = 64;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Grain {
    Hour,
    Day,
    Week,
    Month,
}

impl Grain {
    fn function(self) -> &'static str {
        match self {
            Self::Hour => "toStartOfHour",
            Self::Day => "toStartOfDay",
            Self::Week => "toStartOfWeek",
            Self::Month => "toStartOfMonth",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Range {
    PreviousDay,
    Rolling { days: i64, grain: Grain },
    PreviousMonth,
    PreviousQuarter,
    Unbounded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Bound {
    Token(Range),
    Dates { from: String, to: String, days: i64 },
}

#[derive(Debug, Error)]
pub(crate) enum WindowError {
    #[error("`{0}` is not a known range")]
    UnknownRange(String),
    #[error("an interval's start must come before its end")]
    ReversedInterval,
    #[error("`{0}` is not a usable timezone name")]
    InvalidTimezone(String),
}
```

- [ ] **Step 4: Write the parser and the zone check**

```rust
impl Range {
    pub(crate) fn parse(token: &str) -> Result<Bound, WindowError> {
        match token {
            "PDC" => Ok(Bound::Token(Self::PreviousDay)),
            "P7D" => Ok(Bound::Token(Self::Rolling { days: 7, grain: Grain::Day })),
            "P30D" => Ok(Bound::Token(Self::Rolling { days: 30, grain: Grain::Day })),
            "PMC" => Ok(Bound::Token(Self::PreviousMonth)),
            "PQC" => Ok(Bound::Token(Self::PreviousQuarter)),
            "P1Y" => Ok(Bound::Token(Self::Rolling { days: 365, grain: Grain::Month })),
            "inf" => Ok(Bound::Token(Self::Unbounded)),
            other => parse_interval(other),
        }
    }
}

fn parse_interval(token: &str) -> Result<Bound, WindowError> {
    let (from, to) = token
        .split_once('/')
        .ok_or_else(|| WindowError::UnknownRange(token.to_owned()))?;
    let start = chrono::NaiveDate::parse_from_str(from, "%Y-%m-%d")
        .map_err(|_| WindowError::UnknownRange(token.to_owned()))?;
    let end = chrono::NaiveDate::parse_from_str(to, "%Y-%m-%d")
        .map_err(|_| WindowError::UnknownRange(token.to_owned()))?;
    if end <= start {
        return Err(WindowError::ReversedInterval);
    }

    Ok(Bound::Dates {
        from: from.to_owned(),
        to: to.to_owned(),
        days: (end - start).num_days(),
    })
}

fn checked_timezone(tz: &str) -> Result<String, WindowError> {
    let usable = !tz.is_empty()
        && tz.chars().count() <= MAX_TIMEZONE_CHARS
        && tz
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '+' | '-'));

    if usable {
        Ok(tz.to_owned())
    } else {
        Err(WindowError::InvalidTimezone(tz.to_owned()))
    }
}
```

- [ ] **Step 5: Write the emitters**

```rust
#[derive(Debug, Clone)]
pub(crate) struct Window {
    bound: Option<Bound>,
    grain: Grain,
    bucket: bool,
    tz: Option<String>,
}

impl Window {
    pub(crate) fn new(bound: Bound, tz: Option<&str>) -> Result<Self, WindowError> {
        let grain = match &bound {
            Bound::Token(Range::PreviousDay) => Grain::Hour,
            Bound::Token(Range::Rolling { grain, .. }) => *grain,
            Bound::Token(Range::PreviousMonth) => Grain::Day,
            Bound::Token(Range::PreviousQuarter) => Grain::Week,
            Bound::Token(Range::Unbounded) => Grain::Month,
            Bound::Dates { .. } => Grain::Day,
        };

        Ok(Self {
            bound: Some(bound),
            grain,
            bucket: true,
            tz: tz.map(checked_timezone).transpose()?,
        })
    }

    pub(crate) fn unbounded() -> Self {
        Self { bound: None, grain: Grain::Month, bucket: false, tz: None }
    }

    pub(crate) fn without_bucket(mut self) -> Self {
        self.bucket = false;
        self
    }

    pub(crate) fn grain(&self) -> Grain {
        self.grain
    }

    fn truncate(&self, function: &str, inner: &str) -> String {
        let mut sql = format!("{function}({inner}");
        if let Some(tz) = &self.tz {
            let _ = write!(sql, ", '{tz}'");
        }
        sql.push(')');
        sql
    }

    pub(crate) fn bucket_sql(&self, time: &str) -> Option<String> {
        self.bucket.then(|| self.truncate(self.grain.function(), time))
    }

    pub(crate) fn bounds_sql(&self, anchor: &str) -> Option<(String, String)> {
        match self.bound.as_ref()? {
            Bound::Token(Range::Unbounded) => None,
            Bound::Token(Range::PreviousDay) => {
                let start = self.truncate("toStartOfDay", anchor);
                Some((format!("{start} - INTERVAL 1 DAY"), start))
            }
            Bound::Token(Range::Rolling { days, .. }) => {
                Some((format!("{anchor} - INTERVAL {days} DAY"), anchor.to_owned()))
            }
            Bound::Token(Range::PreviousMonth) => {
                let start = self.truncate("toStartOfMonth", anchor);
                let previous = format!("{start} - INTERVAL 1 DAY");
                Some((self.truncate("toStartOfMonth", &previous), start))
            }
            Bound::Token(Range::PreviousQuarter) => {
                let start = self.truncate("toStartOfQuarter", anchor);
                let previous = format!("{start} - INTERVAL 1 DAY");
                Some((self.truncate("toStartOfQuarter", &previous), start))
            }
            Bound::Dates { from, to, .. } => Some((
                format!("toDateTime('{from} 00:00:00')"),
                format!("toDateTime('{to} 00:00:00')"),
            )),
        }
    }

    pub(crate) fn span_days(&self) -> Option<i64> {
        match self.bound.as_ref()? {
            Bound::Token(Range::PreviousDay) => Some(1),
            Bound::Token(Range::Rolling { days, .. }) => Some(*days),
            Bound::Token(Range::PreviousMonth) => Some(31),
            Bound::Token(Range::PreviousQuarter) => Some(92),
            Bound::Token(Range::Unbounded) => None,
            Bound::Dates { days, .. } => Some(*days),
        }
    }
}
```

Declare the module in `main.rs` beside the others:

```rust
mod window;
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p insight-v3-core window::`
Expected: PASS, 10 tests.

Run: `cargo clippy -p insight-v3-core --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add src/backend/services/insight-v3-core/src/window.rs \
        src/backend/services/insight-v3-core/src/window/tests.rs \
        src/backend/services/insight-v3-core/src/main.rs
git commit -m "feat(insight-v3-core): range tokens become ClickHouse bounds and a bucket"
```

---

### Task 3: The catalogue knows each table's engine

Spec section 12. Needed before `FINAL` can be emitted safely.

**Files:**
- Modify: `src/backend/services/insight-v3-core/src/catalog.rs`
- Test: `src/backend/services/insight-v3-core/src/catalog.rs` (its `mod tests`)

**Interfaces:**
- Consumes: nothing.
- Produces: `TableSchema::engine: String`; `TableSchema::is_replacing(&self) -> bool`; `Catalog::engine_of(&self, name: &str) -> Result<Option<String>, CatalogError>`.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn a_replacing_engine_is_recognised_including_its_replicated_form() {
    assert!(!schema_with_engine("MergeTree").is_replacing());
    assert!(schema_with_engine("ReplacingMergeTree").is_replacing());
    assert!(schema_with_engine("ReplicatedReplacingMergeTree").is_replacing());
    assert!(!schema_with_engine("View").is_replacing());
}
```

Add the helper beside the module's other test helpers:

```rust
fn schema_with_engine(engine: &str) -> TableSchema {
    TableSchema {
        database: "silver".to_owned(),
        table: "class_git_pull_requests".to_owned(),
        layer: Layer::Silver,
        columns: vec![("pr_id".to_owned(), "Int64".to_owned())],
        engine: engine.to_owned(),
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p insight-v3-core catalog::tests::a_replacing_engine`
Expected: FAIL — `TableSchema` has no field `engine`.

- [ ] **Step 3: Read the engine**

Add `pub(crate) engine: String` to `TableSchema`, and a second query beside `LIST_COLUMNS`:

```rust
const LIST_ENGINES: &str = "SELECT database, name, engine
FROM system.tables
WHERE database NOT IN ('system', 'information_schema', 'INFORMATION_SCHEMA', 'default')";
```

```rust
#[derive(Debug, clickhouse::Row, serde::Deserialize)]
struct EngineRow {
    database: String,
    name: String,
    engine: String,
}
```

In `load`, fetch `LIST_ENGINES` into a `HashMap<(String, String), String>` and pass it to `schema_of`, which reads its table's engine out of the map. A table with no engine row keeps an empty string, which `is_replacing` reads as false.

- [ ] **Step 4: Expose the predicate and the lookup**

```rust
impl TableSchema {
    pub(crate) fn is_replacing(&self) -> bool {
        self.engine.ends_with("ReplacingMergeTree")
    }
}
```

```rust
    pub(crate) async fn engine_of(&self, name: &str) -> Result<Option<String>, CatalogError> {
        Ok(self
            .tables()
            .await?
            .into_iter()
            .find(|schema| schema.is_named(name))
            .map(|schema| schema.engine))
    }
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p insight-v3-core catalog::`
Expected: PASS. Every existing catalogue test still passes — the field is additive, and the cached-listing test already asserts no second query is issued on a cache hit.

- [ ] **Step 6: Commit**

```bash
git add src/backend/services/insight-v3-core/src/catalog.rs
git commit -m "feat(insight-v3-core): the catalogue reads each table's engine"
```

---

### Task 4: `compile` takes a window and injects the bucket

Spec sections 5, 7 and 12. The core of the feature.

**Files:**
- Modify: `src/backend/services/insight-v3-core/src/metric_query.rs`
- Modify: `src/backend/services/insight-v3-core/src/custom.rs:195`
- Modify: `src/backend/services/insight-v3-core/src/chat.rs:235`
- Test: `src/backend/services/insight-v3-core/src/metric_query.rs` (its `mod tests`)

**Interfaces:**
- Consumes: `Window`, `Range`, `Bound` (Task 2); `time_sql`, `max_range` (Task 1).
- Produces: `MetricQuery::compile(&self, people: &People, window: &Window, replacing: bool) -> Result<CompiledQuery, MetricQueryError>`; `column_names` including `"bucket"` when a clock is declared; `MetricQueryError::WindowWithoutClock`; `MetricQueryError::RangeTooWide { cap: String }`; `MetricQueryError::ClockAlreadyFiltered(String)`; `BUCKET_ALIAS`; `ANCHOR_PLACEHOLDER`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn a_window_becomes_a_half_open_filter_and_a_bucket() {
    let metric = query(json!({
        "table": "silver.class_git_pull_requests",
        "time": { "column": "created_on", "type": "datetime" },
        "fields": [{ "column": "pr_id", "type": "int", "agg": "count", "as_name": "prs" }]
    }));
    let window = Window::new(Range::parse("P30D").expect("token"), None).expect("valid");

    let compiled = metric.compile(&people(), &window, false).expect("compiles");

    assert!(
        compiled.sql.contains("toStartOfDay(`created_on`) AS `bucket`"),
        "{}",
        compiled.sql
    );
    assert!(
        compiled.sql.contains("`created_on` >=") && compiled.sql.contains("`created_on` <"),
        "{}",
        compiled.sql
    );
    assert!(!compiled.sql.contains("BETWEEN"), "{}", compiled.sql);
    assert!(compiled.sql.contains("GROUP BY `bucket`"), "{}", compiled.sql);
    assert!(compiled.sql.contains("ORDER BY `bucket`"), "{}", compiled.sql);
}

#[test]
fn an_unbounded_window_compiles_what_it_compiles_today() {
    let metric = query(json!({
        "table": "events",
        "fields": [{ "json": "sha", "type": "string", "agg": "count", "as_name": "commits" }]
    }));

    let compiled = metric.compile(&people(), &Window::unbounded(), false).expect("compiles");

    assert_eq!(
        compiled.sql,
        "SELECT count(JSONExtractString(raw_data, 'sha')) AS `commits` FROM `events` LIMIT 1000"
    );
}

#[test]
fn a_total_keeps_its_bounds_and_loses_its_bucket() {
    let metric = query(json!({
        "table": "silver.class_git_pull_requests",
        "time": { "column": "closed_on", "type": "datetime" },
        "fields": [{ "column": "pr_id", "type": "int", "agg": "count", "as_name": "merged" }]
    }));
    let window = Window::new(Range::parse("P30D").expect("token"), None)
        .expect("valid")
        .without_bucket();

    let compiled = metric.compile(&people(), &window, false).expect("compiles");

    assert!(!compiled.sql.contains("bucket"), "{}", compiled.sql);
    assert!(compiled.sql.contains("`closed_on` >="), "{}", compiled.sql);
    assert!(!compiled.sql.contains("GROUP BY"), "{}", compiled.sql);
}

#[test]
fn the_bucket_is_a_column_a_widget_may_draw() {
    let metric = query(json!({
        "table": "events",
        "time": { "json": "committed_at", "type": "datetime" },
        "fields": [{ "json": "sha", "type": "string", "agg": "count", "as_name": "commits" }]
    }));

    assert_eq!(metric.column_names(), vec!["bucket", "commits"]);
}

#[test]
fn a_clockless_metric_offers_no_bucket_column() {
    let metric = query(json!({
        "table": "events",
        "fields": [{ "json": "sha", "type": "string", "agg": "count", "as_name": "commits" }]
    }));

    assert_eq!(metric.column_names(), vec!["commits"]);
}

#[test]
fn a_window_over_a_clockless_metric_is_refused() {
    let metric = query(json!({
        "table": "events",
        "fields": [{ "json": "sha", "type": "string", "agg": "count", "as_name": "commits" }]
    }));
    let window = Window::new(Range::parse("P30D").expect("token"), None).expect("valid");

    assert!(matches!(
        metric.compile(&people(), &window, false),
        Err(MetricQueryError::WindowWithoutClock)
    ));
}

#[test]
fn a_window_wider_than_the_metrics_cap_is_refused() {
    let metric = query(json!({
        "table": "events",
        "time": { "json": "committed_at", "type": "datetime" },
        "max_range": "P30D",
        "fields": [{ "json": "sha", "type": "string", "agg": "count", "as_name": "commits" }]
    }));
    let window = Window::new(Range::parse("P1Y").expect("token"), None).expect("valid");

    assert!(matches!(
        metric.compile(&people(), &window, false),
        Err(MetricQueryError::RangeTooWide { .. })
    ));
}

#[test]
fn a_metric_may_not_filter_the_field_it_is_measured_over() {
    let metric = query(json!({
        "table": "silver.class_git_pull_requests",
        "time": { "column": "created_on", "type": "datetime" },
        "fields": [{ "column": "pr_id", "type": "int", "agg": "count", "as_name": "prs" }],
        "filters": [{ "column": "created_on", "type": "string", "op": "gte",
                      "value": "2026-01-01" }]
    }));
    let window = Window::new(Range::parse("P30D").expect("token"), None).expect("valid");

    assert!(matches!(
        metric.compile(&people(), &window, false),
        Err(MetricQueryError::ClockAlreadyFiltered(_))
    ));
}

#[test]
fn final_is_emitted_for_a_replacing_table_and_never_otherwise() {
    let metric = query(json!({
        "table": "silver.class_git_pull_requests",
        "fields": [{ "column": "pr_id", "type": "int", "agg": "count", "as_name": "prs" }]
    }));

    let replacing = metric.compile(&people(), &Window::unbounded(), true).expect("compiles");
    let plain = metric.compile(&people(), &Window::unbounded(), false).expect("compiles");

    assert!(
        replacing.sql.contains("`silver`.`class_git_pull_requests` FINAL"),
        "{}",
        replacing.sql
    );
    assert!(!plain.sql.contains("FINAL"), "{}", plain.sql);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p insight-v3-core metric_query::tests`
Expected: FAIL — `compile` takes 1 argument, 3 supplied.

- [ ] **Step 3: Add the constants and the errors**

```rust
pub(crate) const BUCKET_ALIAS: &str = "bucket";
/// Stands in for the resolved anchor timestamp until `MetricRunner::run`
/// substitutes it, so `compile` needs no database round trip of its own.
pub(crate) const ANCHOR_PLACEHOLDER: &str = "__anchor";
```

```rust
    #[error("this metric declares no time field, so it cannot answer a time range")]
    WindowWithoutClock,
    #[error("this metric answers at most {cap}")]
    RangeTooWide { cap: String },
    #[error("`{0}` is this metric's time field, so a filter on it fights the range")]
    ClockAlreadyFiltered(String),
```

**Why this third error exists.** Spec section 10 says the dashboard range *substitutes* rather than ANDs, and this is the one place that rule can be broken. A metric declaring `created_on` as its clock *and* carrying its own `created_on >= '2026-01-01'` filter would have both predicates ANDed, so the picker would appear not to work — the exact Metabase failure the spec is written against. Refusing the definition is the only version of "substitutes" that holds, because there is nothing to substitute into.

- [ ] **Step 4: Widen `compile`**

```rust
    pub(crate) fn compile(
        &self,
        people: &People,
        window: &Window,
        replacing: bool,
    ) -> Result<CompiledQuery, MetricQueryError> {
```

After `selection(qualifier)?` and before the group-by check, resolve the clock and prepend the bucket:

```rust
        let clock = self.time_sql(qualifier).transpose()?;
        let bucket = clock
            .as_deref()
            .and_then(|time| window.bucket_sql(time));
        let mut group_by: Vec<String> = self.group_by.clone();
        if let Some(bucket) = &bucket {
            select_parts.insert(0, format!("{bucket} AS `{BUCKET_ALIAS}`"));
            group_by.insert(0, BUCKET_ALIAS.to_owned());
        }
```

Replace the three later reads of `self.group_by` with `group_by`, and let the identifier/`as_names` check skip `BUCKET_ALIAS` — it is a select alias this function created, not one the caller declared.

Add the window predicate beside the metric's own filters:

```rust
        if let Some(time) = clock.as_deref() {
            for filter in &self.filters {
                if filter.source()?.sql(filter.r#type, qualifier)? == time {
                    return Err(MetricQueryError::ClockAlreadyFiltered(
                        filter.source()?.name().to_owned(),
                    ));
                }
            }
        }
        if let Some((from, to)) = window.bounds_sql(ANCHOR_PLACEHOLDER) {
            let Some(time) = clock.as_deref() else {
                return Err(MetricQueryError::WindowWithoutClock);
            };
            if let (Some(cap), Some(span)) = (self.max_range(), window.span_days())
                && cap_days(cap).is_some_and(|allowed| span > allowed)
            {
                return Err(MetricQueryError::RangeTooWide { cap: cap.to_owned() });
            }
            where_parts.push(format!("{time} >= {from}"));
            where_parts.push(format!("{time} < {to}"));
        }
```

Append `FINAL` where the engine replaces, after the alias is written:

```rust
        if replacing {
            from.push_str(" FINAL");
        }
```

- [ ] **Step 5: Include the bucket in the result columns and parse the cap**

```rust
    pub(crate) fn column_names(&self) -> Vec<String> {
        let mut names = Vec::with_capacity(self.fields.len() + 1);
        if self.time.is_some() {
            names.push(BUCKET_ALIAS.to_owned());
        }
        names.extend(self.fields.iter().map(|field| field.as_name.clone()));
        names
    }
```

Beside `is_identifier`:

```rust
fn cap_days(cap: &str) -> Option<i64> {
    let digits: String = cap.chars().filter(char::is_ascii_digit).collect();
    let count: i64 = digits.parse().ok()?;
    match cap.chars().last()? {
        'D' => Some(count),
        'M' => Some(count * 31),
        'Y' => Some(count * 366),
        _ => None,
    }
}
```

- [ ] **Step 6: Update both call sites so the tree compiles**

`custom.rs`, in `run_metric` — the real window arrives in Task 5:

```rust
        let compiled = metric
            .compile(self.metrics.people(), &Window::unbounded(), false)
            .map_err(CustomError::Compile)?;
```

`chat.rs`, in `compile_named_metric` — a validation compile is not answering a question:

```rust
    metric.compile(people, &Window::unbounded(), false)?;
```

- [ ] **Step 7: Run the tests to verify they pass**

Run: `cargo test -p insight-v3-core`
Expected: PASS across the crate. `an_unbounded_window_compiles_what_it_compiles_today` is the regression guard: if any pre-existing SQL assertion moved, the injection is firing when it must not.

Run: `cargo clippy -p insight-v3-core --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 8: Check for stray comments, then commit**

Run the `/comment-review` skill over the diff.

```bash
git add src/backend/services/insight-v3-core/src/metric_query.rs \
        src/backend/services/insight-v3-core/src/custom.rs \
        src/backend/services/insight-v3-core/src/chat.rs
git commit -m "feat(insight-v3-core): compile a metric against a window"
```

---

### Task 5: Running a metric resolves its anchor and its window

Spec sections 5, 6 and the nullable-clock rule in section 3.

**Files:**
- Modify: `src/backend/services/insight-v3-core/src/metric_query.rs` (`Anchor`, `MetricRunner::anchor`, anchor substitution in `run`, `RunResult::excluded`)
- Modify: `src/backend/services/insight-v3-core/src/custom.rs`
- Modify: `src/backend/services/insight-v3-core/src/api/metric_run.rs`
- Test: `src/backend/services/insight-v3-core/src/api/metric_run/tests.rs`

**Interfaces:**
- Consumes: everything from Tasks 1-4.
- Produces: `RunRequest { range: Option<String>, tz: Option<String>, bucket: Option<bool> }`; `Surfaces::run_metric(&self, name: &DefinitionName, request: &RunRequest) -> Result<RunResult, CustomError>`; `MetricRunner::anchor(&self, from: &str, time: &str, replacing: bool) -> Result<Anchor, MetricRunError>`; `MetricRunner::run_with(&self, compiled: &CompiledQuery, metric: &MetricQuery, window: &Window, replacing: bool) -> Result<RunResult, MetricRunError>`; `RunResult::excluded: Option<u64>`; `Anchor { latest: Option<DateTime<Utc>>, null_clocks: u64 }`; `CustomError::Window(WindowError)`.

- [ ] **Step 1: Write the failing tests**

In `api/metric_run/tests.rs`, following the module's existing harness (extend its request helper to take an optional body):

```rust
#[tokio::test]
async fn a_run_with_no_body_is_the_run_it_always_was() {
    let stand = stand().await;
    stand.put_metric("commits", clockless_metric()).await;

    let (status, body) = stand.run("commits", None).await;

    assert_eq!(status, StatusCode::OK);
    let columns = body["columns"].as_array().expect("columns");
    assert!(columns.iter().all(|column| column != "bucket"), "{body}");
}

#[tokio::test]
async fn a_range_over_a_clockless_metric_is_a_400() {
    let stand = stand().await;
    stand.put_metric("commits", clockless_metric()).await;

    let (status, _) = stand.run("commits", Some(json!({ "range": "P30D" }))).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn an_unknown_range_token_is_a_400_naming_the_field() {
    let stand = stand().await;
    stand.put_metric("commits", clocked_metric()).await;

    let (status, body) = stand.run("commits", Some(json!({ "range": "yesterday" }))).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.to_string().contains("range"), "{body}");
}

#[tokio::test]
async fn a_range_wider_than_the_cap_is_a_400() {
    let stand = stand().await;
    stand.put_metric("commits", capped_metric()).await;

    let (status, _) = stand.run("commits", Some(json!({ "range": "P1Y" }))).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_zone_that_could_break_out_of_its_quotes_is_a_400() {
    let stand = stand().await;
    stand.put_metric("commits", clocked_metric()).await;

    let (status, _) = stand
        .run("commits", Some(json!({ "range": "P30D", "tz": "UTC'; DROP" })))
        .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
}
```

Add the three fixtures beside the module's other helpers: `clockless_metric()` is `{"table":"events","fields":[…]}`, `clocked_metric()` adds `"time":{"json":"committed_at","type":"datetime"}`, `capped_metric()` adds `"max_range":"P30D"` to that.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p insight-v3-core api::metric_run::tests`
Expected: FAIL — the harness has no `run` with a body, and `RunRequest` does not exist.

- [ ] **Step 3: Resolve the anchor in one query**

In `metric_query.rs`, beside `MetricRunner::run`:

```rust
#[derive(Debug, Clone, Copy)]
pub(crate) struct Anchor {
    pub(crate) latest: Option<chrono::DateTime<chrono::Utc>>,
    pub(crate) null_clocks: u64,
}

#[derive(Debug, clickhouse::Row, serde::Deserialize)]
struct AnchorRow {
    latest: chrono::DateTime<chrono::Utc>,
    null_clocks: u64,
    rows: u64,
}

impl MetricRunner {
    pub(crate) async fn anchor(
        &self,
        from: &str,
        time: &str,
        replacing: bool,
    ) -> Result<Anchor, MetricRunError> {
        let table = if replacing {
            format!("{from} FINAL")
        } else {
            from.to_owned()
        };
        let sql = format!(
            "SELECT max({time}) AS latest, countIf({time} IS NULL) AS null_clocks, \
             count() AS rows FROM {table}"
        );
        let row: AnchorRow = self.client.query(&sql).fetch_one().await?;

        // ClickHouse answers `max()` over an empty set with the epoch rather
        // than null, so an empty table is told apart by its row count.
        Ok(Anchor {
            latest: (row.rows > 0).then_some(row.latest),
            null_clocks: row.null_clocks,
        })
    }
}
```

In `run`, substitute the placeholder before the query goes out, and carry the excluded count into the result:

```rust
        let sql = compiled.sql.replace(ANCHOR_PLACEHOLDER, &anchor_literal);
```

where `anchor_literal` is `toDateTime('YYYY-MM-DD HH:MM:SS')` built from `Anchor::latest`. Add `pub(crate) excluded: Option<u64>` to `RunResult`.

- [ ] **Step 4: Thread the request through `Surfaces`**

In `custom.rs`:

```rust
#[derive(Debug, Default, Deserialize)]
pub(crate) struct RunRequest {
    #[serde(default)]
    pub(crate) range: Option<String>,
    #[serde(default)]
    pub(crate) tz: Option<String>,
    #[serde(default)]
    pub(crate) bucket: Option<bool>,
}
```

```rust
    pub(crate) async fn run_metric(
        &self,
        name: &DefinitionName,
        request: &RunRequest,
    ) -> Result<RunResult, CustomError> {
        let body = self.get(DefinitionKind::Metric, name).await?;
        let metric: MetricQuery = serde_json::from_value(body).map_err(CustomError::Body)?;

        let replacing = self
            .catalog
            .engine_of(&metric.qualified())
            .await
            .map_err(CustomError::Catalog)?
            .is_some_and(|engine| engine.ends_with("ReplacingMergeTree"));

        let window = match &request.range {
            None => Window::unbounded(),
            Some(token) => {
                let bound = Range::parse(token).map_err(CustomError::Window)?;
                let window =
                    Window::new(bound, request.tz.as_deref()).map_err(CustomError::Window)?;
                if request.bucket == Some(false) {
                    window.without_bucket()
                } else {
                    window
                }
            }
        };

        let compiled = metric
            .compile(self.metrics.people(), &window, replacing)
            .map_err(CustomError::Compile)?;

        self.metrics
            .run_with(&compiled, &metric, &window, replacing)
            .await
            .map_err(CustomError::Run)
    }
```

`run_with` is `run` plus the anchor step: it calls `anchor` only when the window has bounds, substitutes the placeholder, runs, and sets `excluded`.

Add `CustomError::Window(WindowError)` whose `is_about_the_caller()` is `true`.

- [ ] **Step 5: Accept the body at the edge**

In `api/metric_run.rs`:

```rust
async fn run_metric(
    Extension(state): Extension<Arc<AppState>>,
    Path(name): Path<String>,
    headers: axum::http::HeaderMap,
    body: Option<Json<RunRequest>>,
) -> Result<Response, CanonicalError> {
```

Pass `body.map(|Json(request)| request).unwrap_or_default()` through, and add the mapping:

```rust
        CustomError::Window(source) => MetricRunApiError::invalid_argument()
            .with_field_violation("range", source.to_string(), "INVALID")
            .create(),
```

In `compile_error`, map `WindowWithoutClock` and `RangeTooWide` onto the `range` field rather than `body`, so the message points at what the caller sent.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p insight-v3-core`
Expected: PASS.

Run: `cargo clippy -p insight-v3-core --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 7: Verify against the dev warehouse, not only the test stand**

Every test above asserts strings. The emitted SQL has not yet run anywhere. Open a tunnel to insight-dev ClickHouse using the command in the workspace's `insight-dev-data` skill — do not improvise it, the credential and the username are both non-obvious — then run the compiled shape by hand:

```sql
SELECT toStartOfDay(created_on) AS bucket, count(pr_id) AS opened
FROM silver.class_git_pull_requests FINAL
WHERE created_on >= toStartOfMonth(toStartOfMonth(<anchor>) - INTERVAL 1 DAY)
  AND created_on <  toStartOfMonth(<anchor>)
GROUP BY bucket ORDER BY bucket
```

Confirm two things: the row count is non-zero, and the same query with `P30D` bounds returns a different count. Close the tunnel afterwards.

- [ ] **Step 8: Commit**

```bash
git add src/backend/services/insight-v3-core/src/custom.rs \
        src/backend/services/insight-v3-core/src/metric_query.rs \
        src/backend/services/insight-v3-core/src/api/metric_run.rs \
        src/backend/services/insight-v3-core/src/api/metric_run/tests.rs
git commit -m "feat(insight-v3-core): a metric run takes a range, a zone and an anchor"
```

---

### Task 6: MCP and the chat author metrics with a clock

Spec section 9.

**Files:**
- Modify: `src/backend/services/insight-v3-core/src/mcp/tools.rs:239` and `:281`
- Modify: `src/backend/services/insight-v3-core/src/chat.rs` (the system prompt)
- Test: `src/backend/services/insight-v3-core/src/mcp/tools/tests.rs`

**Interfaces:**
- Consumes: `RunRequest` (Task 5).
- Produces: the `run_metric` MCP tool accepting `range`, `tz` and `bucket`.

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn the_run_tool_forwards_a_range() {
    let server = server().await;
    server.put_metric("prs", clocked_metric()).await;

    let result = server
        .call("run_metric", json!({ "name": "prs", "range": "PMC" }))
        .await;

    assert!(result.is_ok(), "{result:?}");
    assert!(columns_of(&result).iter().any(|column| column == "bucket"));
}

#[tokio::test]
async fn the_run_tool_rejects_an_unknown_range() {
    let server = server().await;
    server.put_metric("prs", clocked_metric()).await;

    let result = server
        .call("run_metric", json!({ "name": "prs", "range": "last_month" }))
        .await;

    assert!(result.is_err(), "{result:?}");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p insight-v3-core mcp::tools::tests::the_run_tool`
Expected: FAIL — the tool's request type has no `range`.

- [ ] **Step 3: Widen the tool's request**

Give the `run_metric` tool's request type the three optional fields and pass them straight into `Surfaces::run_metric`.

- [ ] **Step 4: Extend both descriptions**

The description is the only schema an MCP client reads, so the vocabulary has to be in it.

`put_metric` — after the existing field vocabulary, add: an optional `time` names the field the metric is measured over, `{"json": "committed_at", "type": "datetime"}` for a key inside the payload or `{"column": "created_on"}` for a typed column; a metric without it cannot answer a time range; and an optional `max_range` such as `P1Y` caps the widest window it will answer.

`run_metric` — add: an optional `range` is one of `PDC`, `P7D`, `P30D`, `PMC`, `PQC`, `P1Y`, `inf`, or an ISO interval such as `2026-08-01/2026-09-01`; an optional `tz` such as `Europe/Belgrade` moves the day boundary and defaults to UTC; and `bucket: false` returns one row for the whole window instead of one row per bucket.

- [ ] **Step 5: Teach the chat prompt**

In `chat.rs`, the system prompt already names every table on the stand. Add one instruction: when a table has a column or payload key holding a timestamp, the metric must declare it as `time`, because a metric without a clock cannot be filtered to a window and its widget is drawn with an "All time" badge.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p insight-v3-core mcp:: chat::`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/backend/services/insight-v3-core/src/mcp/tools.rs \
        src/backend/services/insight-v3-core/src/mcp/tools/tests.rs \
        src/backend/services/insight-v3-core/src/chat.rs
git commit -m "feat(insight-v3-core): MCP and the chat author metrics with a clock"
```

---

### Task 7: The client sends the range, and the cache keys on it

Spec section 8. This is the task that prevents a wrong number.

**Files:**
- Modify: `src/frontend/src/api/custom-client.ts:257`
- Modify: `src/frontend/src/queries/custom.ts:97`
- Modify: `src/frontend/src/lib/portal/portal-search.ts:21,86`
- Test: `src/frontend/src/queries/custom.test.ts`, `src/frontend/src/lib/portal/portal-search.test.ts`

**Interfaces:**
- Consumes: the endpoint from Task 5.
- Produces: `runMetric(name: string, range?: string, tz?: string): Promise<MetricResult>`; `metricResultQuery(name: string, range?: string, tz?: string)`; `PortalSearch.range?: string`.

- [ ] **Step 1: Write the failing tests**

```ts
it("keys a metric result on its range and zone", () => {
  const thirty = metricResultQuery("commits", "P30D");
  const year = metricResultQuery("commits", "P1Y");
  const zoned = metricResultQuery("commits", "P30D", "Europe/Belgrade");

  expect(thirty.queryKey).not.toEqual(year.queryKey);
  expect(thirty.queryKey).not.toEqual(zoned.queryKey);
});

it("keeps a metric result key stable without a range", () => {
  expect(metricResultQuery("commits").queryKey).toEqual([
    "custom",
    "metric-result",
    "commits",
    undefined,
    undefined,
  ]);
});
```

```ts
it("keeps a known range token and drops an unknown one", () => {
  expect(validatePortalSearch({ range: "P30D" }).range).toBe("P30D");
  expect(validatePortalSearch({ range: "2026-08-01/2026-09-01" }).range).toBe(
    "2026-08-01/2026-09-01"
  );
  expect(validatePortalSearch({ range: "yesterday" }).range).toBeUndefined();
  expect(validatePortalSearch({ range: "2026-09-01/2026-08-01" }).range).toBeUndefined();
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `pnpm --dir src/frontend vitest run src/queries/custom.test.ts src/lib/portal/portal-search.test.ts`
Expected: FAIL — `metricResultQuery` takes one argument; `range` is not on `PortalSearch`.

- [ ] **Step 3: Send it**

```ts
export async function runMetric(
  name: string,
  range?: string,
  tz?: string
): Promise<MetricResult> {
  const res = await fetchWithAuth(
    `${BASE}/metrics/${encodeURIComponent(name)}/run`,
    {
      method: "POST",
      ...(range
        ? {
            headers: { "content-type": "application/json" },
            body: JSON.stringify({ range, ...(tz ? { tz } : {}) }),
          }
        : {}),
    }
  );
  return readJson<MetricResult>(res);
}
```

An absent range sends no body at all, which is what keeps today's behaviour byte-identical.

- [ ] **Step 4: Key on it**

```ts
export function metricResultQuery(name: string, range?: string, tz?: string) {
  return queryOptions({
    queryKey: ["custom", "metric-result", name, range, tz],
    queryFn: () => runMetric(name, range, tz),
  });
}
```

- [ ] **Step 5: Let the URL carry it**

Add `range?: string` to `PortalSearch`, and validate it in `validatePortalSearch` using the module's existing `ISO_DATE`:

```ts
const RANGE_TOKENS = new Set(["PDC", "P7D", "P30D", "PMC", "PQC", "P1Y", "inf"]);

function rangeToken(raw: unknown): string | undefined {
  const value = str(raw);
  if (!value) return undefined;
  if (RANGE_TOKENS.has(value)) return value;

  const [from, to] = value.split("/");
  const bounded =
    from && to && ISO_DATE.test(from) && ISO_DATE.test(to) && from < to;
  return bounded ? value : undefined;
}
```

and `range: rangeToken(raw.range)` in the returned object.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `pnpm --dir src/frontend vitest run src/queries src/lib/portal`
Expected: PASS.

Run: `pnpm --dir src/frontend tsc --noEmit`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add src/frontend/src/api/custom-client.ts \
        src/frontend/src/queries/custom.ts \
        src/frontend/src/queries/custom.test.ts \
        src/frontend/src/lib/portal/portal-search.ts \
        src/frontend/src/lib/portal/portal-search.test.ts
git commit -m "feat(frontend): a custom metric result is keyed on its range and zone"
```

---

### Task 8: The custom zone gets its own range picker

Spec sections 4 and 8. **This is a new component, not a reuse.** `PeriodSelectorBar`'s props are typed `PeriodValue` and it calls `resolveDateRange` internally for its label, so it cannot render a board's token list. It is the model to copy — the same `ToggleGroup`, `Popover` and `Calendar` primitives — not the component to import.

**Files:**
- Create: `src/frontend/src/components/custom/range-picker.tsx`
- Create: `src/frontend/src/components/custom/range-picker.test.tsx`

**Interfaces:**
- Consumes: `toISODate` from `api/period-to-date-range`.
- Produces: `RANGE_LABELS: Record<string, string>`; `labelFor(range: string): string`; `RangePicker({ tokens, value, onChange }: { tokens: string[]; value: string; onChange: (range: string) => void })`.

- [ ] **Step 1: Write the failing tests**

```tsx
it("labels each token in human words and reports the token", async () => {
  const onChange = vi.fn();
  render(
    <RangePicker tokens={["PDC", "P30D", "P1Y"]} value="P30D" onChange={onChange} />
  );

  expect(screen.getByRole("radio", { name: "Yesterday" })).toBeInTheDocument();
  expect(screen.getByRole("radio", { name: "Last 30 days" })).toBeInTheDocument();

  await userEvent.click(screen.getByRole("radio", { name: "Last year" }));

  expect(onChange).toHaveBeenCalledWith("P1Y");
});

it("renders only the tokens the board offers", () => {
  render(<RangePicker tokens={["PDC"]} value="PDC" onChange={vi.fn()} />);

  expect(screen.queryByRole("radio", { name: "Last quarter" })).toBeNull();
});

it("labels an interval value with its dates", () => {
  render(
    <RangePicker
      tokens={["P30D"]}
      value="2026-08-01/2026-09-01"
      onChange={vi.fn()}
    />
  );

  expect(screen.getByText("2026-08-01 – 2026-09-01")).toBeInTheDocument();
});

it("emits an ISO interval from the calendar", async () => {
  const onChange = vi.fn();
  render(<RangePicker tokens={["P30D"]} value="P30D" onChange={onChange} />);

  await userEvent.click(screen.getByRole("button", { name: /custom range/i }));
  await pickDays("2026-08-01", "2026-09-01");

  expect(onChange).toHaveBeenCalledWith("2026-08-01/2026-09-01");
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `pnpm --dir src/frontend vitest run src/components/custom/range-picker.test.tsx`
Expected: FAIL — the module does not exist.

- [ ] **Step 3: Write the labels**

```tsx
export const RANGE_LABELS: Record<string, string> = {
  PDC: "Yesterday",
  P7D: "Last 7 days",
  P30D: "Last 30 days",
  PMC: "Last month",
  PQC: "Last quarter",
  P1Y: "Last year",
  inf: "All time",
};

export function labelFor(range: string): string {
  return RANGE_LABELS[range] ?? range.replace("/", " – ");
}
```

- [ ] **Step 4: Write the control**

A `ToggleGroup` of `ToggleGroupItem`s over `tokens`, each labelled through `labelFor`, with `value` marking the active one; plus a `Popover` holding the same `Calendar` the period bar uses, triggered by a button labelled "Custom range". On a completed calendar selection emit one token, not a pair:

```tsx
  const commit = (from: Date, to: Date) =>
    onChange(`${toISODate(from)}/${toISODate(to)}`);
```

When `value` is an interval rather than a token, no toggle item is active and the trigger shows `labelFor(value)`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `pnpm --dir src/frontend vitest run src/components/custom/range-picker.test.tsx`
Expected: PASS, 4 tests.

- [ ] **Step 6: Commit**

```bash
git add src/frontend/src/components/custom/range-picker.tsx \
        src/frontend/src/components/custom/range-picker.test.tsx
git commit -m "feat(frontend): a range picker over the tokens a board offers"
```

---

### Task 9: The dashboard drives every widget from one picker

Spec sections 4, 8 and 10 — the last of them is the badge.

**Files:**
- Modify: `src/frontend/src/routes/portal.custom.$name.tsx:38-170`
- Modify: `src/frontend/src/api/custom-client.ts:93` (`Dashboard.time_ranges`, `Dashboard.default_range`)
- Test: `src/frontend/src/routes/portal.custom.$name.test.tsx`

**Interfaces:**
- Consumes: `RangePicker`, `labelFor` (Task 8); `metricResultQuery`, `PortalSearch.range` (Task 7).
- Produces: nothing further.

- [ ] **Step 1: Write the failing tests**

```tsx
it("shows no picker for a board that offers no ranges", async () => {
  renderDashboard({ title: "Engineering", items: [{ widget: "commits" }] });

  expect(await screen.findByText("Engineering")).toBeInTheDocument();
  expect(screen.queryByRole("radio")).toBeNull();
});

it("opens on the board's default range", async () => {
  renderDashboard({
    title: "Engineering",
    time_ranges: ["PDC", "P30D", "P1Y"],
    default_range: "P30D",
    items: [{ widget: "commits" }],
  });

  expect(
    await screen.findByRole("radio", { name: "Last 30 days", checked: true })
  ).toBeInTheDocument();
});

it("re-runs every widget on the picked range", async () => {
  const runs = spyOnRunMetric();
  renderDashboard({
    title: "Engineering",
    time_ranges: ["P30D", "P1Y"],
    default_range: "P30D",
    items: [{ widget: "commits" }, { widget: "prs" }],
  });

  await userEvent.click(await screen.findByRole("radio", { name: "Last year" }));

  expect(runs).toHaveBeenCalledWith("commits", "P1Y", undefined);
  expect(runs).toHaveBeenCalledWith("prs", "P1Y", undefined);
});

it("badges a widget whose metric has no clock", async () => {
  renderDashboard({
    title: "Engineering",
    time_ranges: ["P30D"],
    default_range: "P30D",
    items: [{ widget: "commits_ever" }],
  });

  expect(await screen.findByText("All time")).toBeInTheDocument();
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `pnpm --dir src/frontend vitest run "src/routes/portal.custom.\$name.test.tsx"`
Expected: FAIL — no picker is rendered.

- [ ] **Step 3: Widen the `Dashboard` type**

```ts
export interface Dashboard {
  title: string;
  items?: DashboardItem[];
  widgets?: string[];
  /** The windows this board offers, as range tokens. */
  time_ranges?: string[];
  /** Which of them it opens on. */
  default_range?: string;
}
```

- [ ] **Step 4: Wire the header**

In `CustomDashboardBody`, read the range from `usePortalSearch().range`, fall back to `dashboard.default_range`, and write a change back through `useSetPortalSearch` so a link carries it:

```tsx
      <header className="mb-4 flex flex-wrap items-center justify-between gap-2">
        <h1 className={TEXT_TITLE}>{dashboard.title}</h1>
        {dashboard.time_ranges?.length ? (
          <RangePicker
            tokens={dashboard.time_ranges}
            value={range}
            onChange={(next) => setSearch({ range: next })}
          />
        ) : null}
      </header>
```

- [ ] **Step 5: Pass the range into every slot, and badge the clockless**

`DashboardWidgetSlot` takes `range` and forwards it to `metricResultQuery(metric, range)`. The badge comes off the widget's own metric definition, which the slot fetches beside the widget:

```tsx
  const metricState = useQuery({
    ...metricQuery(metric ?? ""),
    enabled: Boolean(metric),
  });
  const clockless = metricState.data ? !("time" in metricState.data) : false;
```

Render an "All time" `Badge` in the card header when `clockless` is true.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `pnpm --dir src/frontend vitest run src/routes src/components/custom`
Expected: PASS.

Run: `pnpm --dir src/frontend tsc --noEmit && pnpm --dir src/frontend lint`
Expected: clean.

- [ ] **Step 7: Verify in a real browser**

Per the workspace's UI-testing rule, drive the portal with `agent-browser` headless — not a repo-local browser skill. Open a custom dashboard that declares `time_ranges`, switch from `P30D` to `P1Y`, and confirm three things by screenshot: the chart's x-axis relabels from days to months, the URL carries `?range=P1Y`, and a clockless widget keeps its "All time" badge while its neighbours change.

- [ ] **Step 8: Commit**

```bash
git add "src/frontend/src/routes/portal.custom.\$name.tsx" \
        "src/frontend/src/routes/portal.custom.\$name.test.tsx" \
        src/frontend/src/api/custom-client.ts
git commit -m "feat(frontend): one picker drives every widget on a custom dashboard"
```

---

### Task 10: The demo definitions the PR ships with

Spec section 14. Two metrics, two widgets and a board, so a reviewer sees the feature rather than reading about it.

**Files:**
- Modify: `src/backend/services/insight-v3-core/tests/mvp.sh` — the MVP's existing seeding script, run as `./tests/mvp.sh` and already the thing that creates the `engineering` board. Extend it; do not add a second mechanism.
- Test: `src/backend/services/insight-v3-core/src/api/definitions/tests.rs`

**Interfaces:**
- Consumes: everything above.
- Produces: definitions `prs_opened`, `prs_merged`, widgets `prs_opened_line`, `prs_merged_total`, dashboard `prs_board`.

- [ ] **Step 1: Write the definitions**

The columns are read, not guessed: `silver.class_git_pull_requests` carries `created_on`, `updated_on` and `closed_on` as `Nullable(DateTime)`, 65,014 rows spanning 2016-02-29 to 2026-09-07, and `state` has three values.

```json
{
  "table": "silver.class_git_pull_requests",
  "time": { "column": "created_on", "type": "datetime" },
  "max_range": "P1Y",
  "fields": [{ "column": "pr_id", "type": "int", "agg": "count", "as_name": "opened" }]
}
```

```json
{
  "table": "silver.class_git_pull_requests",
  "time": { "column": "closed_on", "type": "datetime" },
  "max_range": "P1Y",
  "fields": [{ "column": "pr_id", "type": "int", "agg": "count", "as_name": "merged" }],
  "filters": [{ "column": "state", "type": "string", "op": "eq", "value": "MERGED" }]
}
```

```json
{ "type": "line", "metric": "prs_opened", "x": "bucket", "y": "opened" }
```

```json
{ "type": "stat", "metric": "prs_merged", "value": "merged" }
```

```json
{
  "title": "Pull requests",
  "time_ranges": ["PDC", "P7D", "P30D", "PMC", "PQC", "P1Y", "inf"],
  "default_range": "P30D",
  "items": [
    { "heading": "Opened" },
    { "widget": "prs_opened_line" },
    { "widget": "prs_merged_total" }
  ]
}
```

- [ ] **Step 2: Write the failing test**

The line widget names `bucket`, which only validates because Task 4 put it in `column_names` — so this test is the end-to-end proof of that decision.

```rust
#[tokio::test]
async fn a_line_widget_may_draw_the_injected_bucket() {
    let stand = stand().await;
    stand.put_metric("prs_opened", prs_opened()).await;

    let (status, _) = stand
        .put_json(
            "/v1/widgets/prs_opened_line",
            json!({ "type": "line", "metric": "prs_opened", "x": "bucket", "y": "opened" }),
        )
        .await;

    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn a_line_widget_on_a_clockless_metric_is_still_refused() {
    let stand = stand().await;
    stand.put_metric("commits", clockless_metric()).await;

    let (status, _) = stand
        .put_json(
            "/v1/widgets/commits_line",
            json!({ "type": "line", "metric": "commits", "x": "bucket", "y": "commits" }),
        )
        .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p insight-v3-core api::definitions::tests`
Expected: PASS both.

- [ ] **Step 4: Commit**

```bash
git add src/backend/services/insight-v3-core/src/api/definitions/tests.rs \
        src/backend/services/insight-v3-core/tests/mvp.sh
git commit -m "feat(insight-v3-core): a pull-request board demonstrating the ranges"
```

---

## Verification before claiming done

Run all of it and paste the output rather than summarising it:

```bash
cargo test -p insight-v3-core
cargo clippy -p insight-v3-core --all-targets -- -D warnings
pnpm --dir src/frontend vitest run
pnpm --dir src/frontend tsc --noEmit
pnpm --dir src/frontend lint
```

Then the two checks no test covers, because both assert against strings rather than against a running system: the warehouse query in Task 5 Step 7, and the browser pass in Task 9 Step 7.

## Out of scope, on purpose

- **The per-user timezone store and profile page.** Spec section 14. `tz` is accepted on every run here and defaults to UTC; nothing above changes when the setting arrives.
- **An indexed clock.** No silver or ingest table is sorted by date, so every window is a full scan. `max_range` bounds the damage; a sort key or a materialized column is upstream of this service.
- **`argMax` deduplication.** `FINAL` is chosen in Task 4. Revisit only if it shows up in a slow query.
- **Showing the excluded-row count.** Task 5 puts `excluded` on the run result because a nullable clock drops rows silently, but nothing renders it. Spec section 3 asks for it to exist, not to be displayed; a widget that surfaces it is a later change.
