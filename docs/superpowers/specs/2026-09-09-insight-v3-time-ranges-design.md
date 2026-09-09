# Time ranges for the v3 custom pages

**Goal:** one metric definition answers "yesterday", "last 30 days", "last month",
"last quarter" and "last year", with the range picked while looking at a dashboard
rather than written into the metric.

**Why:** the ask is to *review* a number over several windows — "I need to be able to
review metrics from yesterday, for last month, last quarter, last 30 days, last year".
Today a metric's window can only be literal dates typed into its filters, so a metric
named for last month means the dates whoever wrote it typed, and shows the same rows
next month.

**Scope:** dashboards. Alerts need a self-contained window, because an alert has no
viewer to pick one; they are named below where they change the design, and are not
built here.

---

## 1. What is missing today

`MetricQuery` has no time concept at all — `database`, `table`, `fields`, `group_by`,
`filters`, `order_by`, `limit`
([metric_query.rs:23](../../../src/backend/services/insight-v3-core/src/metric_query.rs)).
Four separate pieces are absent:

| | Today | Consequence |
|---|---|---|
| A date type | `FieldType` is `string`/`int`/`float` (`metric_query.rs:340`) | A timestamp is not expressible as one |
| Bucketing | A column compiles to a bare `` `col` ``; no `toDate`/`toStartOf` anywhere (`metric_query.rs:328`) | No day granularity either |
| Relative values | Filter values bind as `Str`/`Int`/`Float` literals (`metric_query.rs:429`) | No `now()`, so a window is frozen dates |
| A range parameter | `POST /v1/metrics/{name}/run` takes a path name and no body (`api/metric_run.rs:35`) | A caller cannot ask for a window |

Nothing is precomputed: the handler reads the definition, compiles it, runs it on
ClickHouse and returns
(`custom.rs:191`). The only cache in the service is the table catalogue, 5 min
(`catalog.rs:10`). On the client, results are keyed `["custom","metric-result",name]`
(`queries/custom.ts:98`) under a 1 h `staleTime` (`query-client.ts:25`).

Two facts make the change cheap. Every ingest table already carries
`received_at DateTime64(3, 'UTC')` inside its sort key `(table_name, received_at, id)`
(`tables.rs:9`). Connector tables in bronze/silver/gold carry their own time columns
under arbitrary names, and the catalogue already exposes every column with its
ClickHouse type (`catalog.rs:32`), so a clock is discoverable rather than guessed.

## 2. Decision: the range belongs to the view, the clock to the metric

Ten decisions, taken 2026-09-09:

1. **The range lives on the dashboard, not in the metric.** A metric is one definition;
   the picker re-runs it. The alternative — a relative window inside the metric — is
   viable and cheaper (nothing is precomputed, so it would recompute correctly on every
   open), but it multiplies the catalogue by the number of ranges and cannot answer
   "the same chart, one year instead".
2. **The window filters event time, and the metric names the field.** The date the thing
   happened, not the date the row arrived. A backfill lands two years of commits with
   today's `received_at`, and every window on ingest time would then be wrong.
3. **The range picks the grain.** The metric declares no grain.
4. **The range list is per-dashboard, not global.** The portal already carries a
   period picker for every other zone, and it is deliberately switched off for this one
   (`components/portal/portal-topbar.tsx:29`: *"Nothing under Manage or Custom reads
   scope, cohort or period"*). It stays off. A global bar cannot take a per-board option
   list, and a board about yesterday's CI runs should be able to open on yesterday.
5. **Day boundaries follow a per-user timezone the user can change**, held server-side
   and edited on a profile page, defaulting to UTC. The store and the page are net-new
   and are delivered separately — see section 13; this design depends only on `tz`
   arriving on the request. UTC alone was recorded first and reversed: in Belgrade (UTC+2) "yesterday" would
   run 02:00–02:00 local, so a commit made Monday 23:00 counts as Tuesday and every
   daily chart is shifted. The cost is accepted knowingly — two viewers of one dashboard
   in different zones see different numbers, and the zone joins the cache key.
6. **Windows resolve against the newest row in the data, not wall-clock now.** Rill
   resolves dashboard ranges against `latest` and alert ranges against a watermark,
   because connectors sync in batches. With `now()`, a connector that last synced on
   Friday makes "yesterday" an empty chart on Monday, and that reads as a bug rather
   than as stale data.
7. **Tokens are ISO-shaped on the wire, human-readable in the UI.** The API carries
   `P30D`; the picker shows "Last 30 days".
8. **A custom range is in scope now**, not deferred. The existing picker already ships a
   calendar, and a token set without it would leave that calendar as dead UI.
9. **A widget may not pin its own range**, and a metric may cap how wide a window it
   will answer. Pinning is what every product that allows it has the silent-wrong-number
   bug for; the cap is Rill's `max_query_time_range`, and it is the cheap half of the
   scan problem in section 11.
10. **A widget outside the window says so with a badge in its card header**, not a
   caption. A caption under a chart gets skipped by the eye, which is the failure being
   guarded against.

The split in decisions 1–4 and the reference point in decision 6 are what Rill does, and Rill is the only one of the three
reviewed products that does *only* this. Its metrics view declares the clock and
nothing about ranges — `timeseries` (which timestamp column), `smallest_time_grain`,
`first_day_of_week`, `first_month_of_year`, `max_query_time_range`. Its explore
dashboard declares the ranges — a `time_ranges` list and `defaults.time_range`. A
measure is `expression: count(*)`, with no time in it.

Superset and Metabase both allow a range in the chart *and* on the dashboard, and both
have the failure that follows. Superset stores a chart's own `time_range` and grain in
its `params`, dashboard native filters add on top, and a broken panel renders blank
with no error. In Metabase, dashboard filters are **ANDed** onto a card's own filters
rather than substituted, so a card nobody mapped keeps showing unfiltered numbers with
no sign it is doing so — observed hands-on, not read.

Two rules follow from that, and section 10 applies them: the dashboard range
**substitutes**, never ANDs; and a widget that is not under the picker says so on its
face.

## 3. The metric gains a clock

```json
{
  "table": "github_commits",
  "time": { "json": "committed_at", "type": "datetime" },
  "max_range": "P1Y",
  "fields": [
    { "json": "sha", "type": "string", "agg": "count", "as_name": "commits" }
  ]
}
```

One table can have more than one legitimate clock, which is why this sits on the metric
and not on the table: `silver.class_git_pull_requests` carries `created_on`, `updated_on`
and `closed_on`, so "PRs opened" and "PRs merged" are the same table read through
different clocks.

**A nullable clock drops rows in silence.** All three of those columns are
`Nullable(DateTime)`, and 931 of 65,014 rows have a null `closed_on` — an open PR. The
half-open predicate excludes every one of them, correctly for "merged last month" and
invisibly for anyone who expected a total. A metric whose clock is nullable must say so
where the badge in section 10 says it, and the count of excluded rows belongs in the
run result rather than nowhere.

`time` is optional and names exactly one of `json` or `column`, resolved by the existing
`Source::resolve`. `FieldType` gains `datetime`: a `json` source extracts and parses,
`parseDateTimeBestEffort(JSONExtractString(raw_data, 'committed_at'))`; a `column`
source stays the bare backticked identifier, since it is already a ClickHouse date type.

`max_range` is Rill's `max_query_time_range`: the widest window this metric will
answer, as an ISO duration. A request for more is refused with a 400 naming the cap,
which is a better failure than a thirty-second timeout (decision 9).

The metric declares no range and no grain — decisions 1 and 3.

## 4. The dashboard gains a range list

```json
{
  "title": "Engineering",
  "time_ranges": ["PDC", "P7D", "P30D", "PMC", "PQC", "P1Y", "inf"],
  "default_range": "P30D",
  "items": [{ "widget": "commits_over_time" }, { "widget": "commits_total" }]
}
```

A closed set of tokens, not a grammar. Rill's own keywords are marked legacy in its
docs, superseded by a parsed expression language with reference points and snapping.
That is a parser; this needs eight names.

Decision 7 asks for ISO on the wire, and plain ISO 8601 gets most of the way: a duration
says "rolling N". It cannot say "the previous complete calendar month" — which is why
Rill needed complete-period keywords beside its durations, and why three tokens here
borrow that `<grain>C` shape rather than inventing a second style.

| Token | Meaning | UI label | Grain |
|---|---|---|---|
| `PDC` | previous complete day | Yesterday | hour |
| `P7D` | rolling 7 days | Last 7 days | day |
| `P30D` | rolling 30 days | Last 30 days | day |
| `PMC` | previous complete calendar month | Last month | day |
| `PQC` | previous complete calendar quarter | Last quarter | week |
| `P1Y` | rolling 365 days | Last year | month |
| `inf` | unbounded | All time | month |
| `<from>/<to>` | ISO 8601 interval | the dates | from its span |

`inf` is unbounded but still buckets, by month — decision 3 gives every range a grain,
and month is the only readable one over an unknown span.

`PMC` and `P30D` are deliberately separate tokens, as are `PDC` and `PT24H`. They are
different numbers and are routinely conflated.

**The custom range is one token, not a second parameter** (decision 8): an ISO 8601
interval, `2026-08-01/2026-09-01`. One `range` field carries either form, which keeps
the wire, the URL, the cache key and a future alert's stored window all single-valued.
Its grain comes from its span, by the same thresholds the fixed tokens use. The server
validates it: the client's `validateDateRange` and `MAX_DATE_RANGE_DAYS` are a courtesy
to the person typing, never a guarantee to the handler.

A widget carries no range — decision 9. Chart kind stays the whole of a widget's job.

## 5. Compilation: the bucket is injected, not declared

`MetricQuery::compile(&self, people)` becomes
`compile(&self, people, window: &Window)`, where a `Window` is either unbounded or a
resolved `{ from, to, grain, tz }`. Resolution happens in the handler, once per request,
so compilation stays a pure function of definition plus window.

**Resolution takes a query of its own.** Decision 6 anchors a token to the newest row
rather than to the clock, so the handler first runs `SELECT max(<time expr>)` over the
metric's table and offsets from that. `PDC` means the day before the last day with data.
Two consequences to build deliberately: it is a second round trip per run, and on an
empty table `max()` is null — the window is then empty and the widget shows no data,
which is the honest answer for a table with nothing in it.

**The cap is checked before the query, not after.** A resolved window wider than the
metric's `max_range` is refused with a 400 naming the cap (decision 9).

Two edits to the assembled SQL:

- **Select and group.** Prepend one select part,
  `<truncate>(<time expr>) AS bucket` — where `<truncate>` is the grain's function
  from the table in section 4, `toStartOfHour` through `toStartOfMonth` — and prepend
  `bucket` to `group_by`.
  `group_by` entries are select aliases rendered backticked, and `ORDER BY` already
  falls back to the group list when `order_by` is absent (`metric_query.rs:664`), so
  time ordering comes free.
- **Filter.** `WHERE <time expr> >= ? AND <time expr> < ?`. Half-open, not `BETWEEN` —
  `BETWEEN` includes the upper bound and double-counts the row on the boundary between
  two adjacent windows.

Bounds bind through the existing mechanism as unix seconds, with the SQL side wrapping
them `toDateTime64(?, 3, 'UTC')`. That is a constant expression folded before the scan,
so a real `DateTime` column keeps its index; worth confirming on the target ClickHouse
version rather than assumed.

Decision 5 puts a timezone on every one of those calls: `toStartOfDay(<time expr>,
'Europe/Belgrade')`, with the bounds and the calendar arithmetic for `PDC`/`PMC`/`PQC`
computed in the same zone. The default is `UTC`, so a request that names no zone behaves
as the first draft of this spec did.

Both existing call sites pass a window: `Surfaces::run_metric` (`custom.rs:195`) the
one the request resolved, and the chat's validation compile (`chat.rs:235`) an unbounded
one, since it is checking a definition rather than answering a question.

## 6. The run endpoint gains a body

```
POST /v1/metrics/commits/run  { "range": "P30D", "tz": "Europe/Belgrade" }
                                                    → ~30 rows, one per day
POST /v1/metrics/commits/run  { "range": "P1Y" }    → ~12 rows, one per month
POST /v1/metrics/commits/run  { "range": "2026-08-01/2026-09-01" }
                                                    → one row per day of August
POST /v1/metrics/commits/run  { "range": "P30D", "bucket": false }
                                                    → 1 row
POST /v1/metrics/commits/run                        → today's behaviour
```

An absent body means unbounded, unbucketed and UTC, which is exactly what the endpoint
does now, so no existing caller changes. An absent `tz` means UTC.

`bucket: false` is what a "total" is: the same definition, no time bucket, aggregated
over the picked window. `inf` plus `bucket: false` is an all-time total. Neither is a
different kind of metric, which is why neither needs a field on one.

## 7. Widget validation must know about the injected column

`Widget::check_against` refuses a widget naming a column its metric does not produce,
reading `MetricQuery::column_names()` (`widget.rs:71`, `metric_query.rs:528`), which
returns field `as_name`s only. A line widget with `x: "bucket"` would therefore be
refused at write time, making every time chart unwritable.

`column_names()` must include the bucket alias whenever the metric declares a `time`.
This is the one place where forgetting the injection breaks the feature silently at
authoring time rather than at run time.

## 8. Frontend

The picker is built from parts the portal already has, but not from its global period
state — decision 4 makes the option list a property of the board.

- **Reuse the control, not the state.** `PeriodSelectorBar`
  (`components/widgets/period-selector-bar.tsx`) is the existing toggle group plus
  custom-range calendar. `usePortalPeriod` is not reused: it is zone-global, keyed on a
  shared `PeriodValue` union, and reading it here would mean extending that union across
  every v2 zone's switch for tokens only this zone understands. `PeriodValue` stays
  untouched.
- **Placement.** The dashboard header, which today holds only the title
  (`routes/portal.custom.$name.tsx:99`). Options come from the board's `time_ranges`,
  the initial value from `default_range`. A board that declares neither shows no picker.
- **The picked token in the URL.** `PortalSearch` is a validated allowlist and drops
  what it does not recognise (`lib/portal/portal-search.ts:83`), so a `range` field must
  be added there and in `validatePortalSearch` or the param is silently stripped on
  navigation.
- **The token travels, not a resolved pair.** The client sends `range: "PMC"` and the
  server resolves it — including for a custom range, which travels as one ISO interval.
  `resolveDateRange` behind the existing bar is local-timezone aware by design ("a
  `week` means 7 local days"); if the client resolved the window, bounds would be cut in
  the browser's zone while buckets were truncated in the requested one. Only the server
  ever resolves a window. It is also what lets an alert store a window, which a frozen
  pair of dates cannot.
- **Human labels, ISO values** (decision 7). The label table in section 4 lives on the
  client; nothing but the token crosses the wire or appears in the URL.
- **The calendar stays enabled** (decision 8), writing an ISO interval into the same
  `range` param rather than a second pair of fields.
- **The timezone is a per-user setting the user can change** (decision 5), sent as `tz`
  on every run. The existing per-user period default is kept in `localStorage`
  (`hooks/use-period.ts`), and the zone follows that pattern — which makes it per
  device, not per person. See the open question in section 13.
- **The range and the zone both go in the query key.** Today's key is
  `["custom","metric-result",name]` with a 1 h `staleTime`; without them, switching from
  `P1Y` to `PDC` serves the year's rows from cache. This is the one change here that
  produces a wrong number rather than an error.
- `runMetric(name)` gains the range and the zone (`api/custom-client.ts:257`).

## 9. MCP and chat

- `put_metric`'s description documents `time`, or an authored metric has no clock
  (`mcp/tools.rs:239`).
- `run_metric` gains an optional range (`mcp/tools.rs:281`).
- The chat system prompt instructs declaring a clock when the table has one. Without
  it, every metric the assistant writes ignores the picker — and the assistant is how
  most of them will be written.

## 10. Metrics with no clock

Every definition that exists today has none. They compile unbounded and ignore the
picker, which keeps them correct — they are all-time totals now and stay all-time
totals.

They must not be silent about it. A widget whose metric declares no `time` carries an
"All time" **badge in the card header** (decision 10). The dashboard range substitutes into the metric's
window; it is never ANDed onto the metric's own filters. Both rules exist because of the
Metabase behaviour in section 2, where the absence of a marker is the whole bug.

## 11. Performance: named, not solved

**Every window predicate is a full scan, on both kinds of table.** On a v3 ingest table
the clock is inside the JSON payload, so it cannot use an index — `received_at` is in
the sort key, `committed_at` is a parsed expression over `raw_data`. The modelled layer
is no better, and that was checked rather than assumed: `silver.class_git_pull_requests`
is `ReplacingMergeTree` sorted by `unique_key`, with no partition key, so a filter on
`created_on` reads all 65,282 parts' rows too. A real `DateTime` column is not the same
thing as an indexed one.

`max_range` (section 3) is therefore the only guard this design ships: it bounds the
worst window a board can ask for, without making any scan cheaper. The real fix — a
sort key or a materialized column carrying the declared clock — is a change to table
shape, upstream of this service, and out of scope here.

The duplicate-row problem the same check turned up is fixed here rather than later —
section 12.

## 12. Duplicate rows: `FINAL`, but only where it is legal

`silver.class_git_pull_requests` is `ReplacingMergeTree(_version)`, and the compiler
emits no `FINAL`. An updated row is written beside its old version and collapsed by a
background merge later, so until that merge runs both are readable: `total_rows` is
65,282 against a `count()` of 65,014 — 268 phantom rows, and a `count` metric over the
table is over-counting by whatever has not merged yet, with or without a time window.
This predates time ranges and is fixed in the same change by decision.

**`FINAL` cannot be emitted unconditionally.** On a plain `MergeTree` table it is a hard
error, not a no-op:

```
SELECT count() FROM silver.to_ai_cost FINAL
Code: 181. DB::Exception: Storage MergeTree doesn't support FINAL. (ILLEGAL_FINAL)
```

That is not a corner case. Every v3 ingest table is created `ENGINE = MergeTree`
(`tables.rs:14`), so a blanket `FINAL` would break the custom zone's own data on the
first query — the exact surface this design exists to serve. The silver layer is mixed
too: 58 `ReplacingMergeTree`, 2 `MergeTree`, 1 `View`, and a `View` does accept `FINAL`.

**So the engine decides, and the catalogue must know it.** `Catalog` reads column names
and types from `system.columns` (`catalog.rs:114`); it gains the table's `engine` from
`system.tables` in the same load, cached under the same 5-minute TTL. The compiler emits
`FINAL` when that engine ends in `ReplacingMergeTree` — covering the `Replicated`
variants — and omits it otherwise.

This makes compilation depend on the catalogue, which it does not today. `compile` stays
pure: the engine is resolved before it, beside the window, and passed in.

`argMax(<column>, _version)` per key is the faster alternative and is available here
(the table declares its version column). It is not chosen: it needs a per-metric
rewrite of every aggregate, whereas `FINAL` is correct for any `Replacing` table without
knowing what the metric aggregates. Revisit if `FINAL` shows up in a slow query.

## 13. Testing

- One compile test per token, asserting the emitted SQL and the resolved bounds.
- `PMC` and `P30D` resolve to different bounds, as do `PDC` and `PT24H`.
- A row whose timestamp equals `to` is excluded, and appears in the next window exactly
  once.
- Bounds anchor to `max(<time>)`, not to now: with the newest row three days old, `PDC`
  covers the day before that row, and an empty table yields an empty window rather than
  an error.
- One token resolves to different bounds under two zones, and to the UTC bounds when no
  zone is sent.
- An ISO interval is accepted as a `range`; a reversed or malformed one is refused by
  the handler, not only by the client.
- A window wider than `max_range` is refused with a 400 naming the cap.
- `column_names()` contains the bucket alias when `time` is declared and does not when
  it is absent; a line widget on `bucket` passes `check_against`.
- A run with no body emits the SQL it emits today.
- Two ranges of one metric do not share a client cache entry, and neither do two zones.
- `validatePortalSearch` keeps a valid `range` and drops an unknown one.
- A dashboard declaring no `time_ranges` renders no picker.
- A metric over a `ReplacingMergeTree` table compiles with `FINAL`; one over a
  `MergeTree` table compiles without it, and a v3 ingest table is the `MergeTree` case.
- A metric whose clock is nullable reports how many rows the window excluded.

## 14. A dependency, and what ships with it

**The per-user timezone needs a home that does not exist yet.** Decision 5 puts it
server-side with a profile page to change it, and nothing in the product holds a user
preference today: identity offers `GET /v1/me`, which is identity and roles and is
read-only (`identity-resolution/src/api/me.rs:50`), the portal has no profile or
settings route at all, and the one existing preference — the period default — lives in
`localStorage` (`hooks/use-period.ts`). A store, a read, a write and a new route is more
frontend work than this whole feature.

So it is a separate spec and a separate PR. This design depends on one thing only: `tz`
arriving in the run request, defaulting to UTC when absent. Time ranges ship first and
are correct in UTC; the profile page moves the default off UTC afterwards, and no code
here changes when it lands.

**What the shipping PR demonstrates**, now that the table has been read:

| Widget | Metric | Proves |
|---|---|---|
| line | PRs opened per bucket, clock `created_on` | grain switching, a real `DateTime` clock |
| stat | PRs merged in the window, clock `closed_on`, `bucket: false` | totals are a range plus no bucket |
| stat with badge | an existing clockless metric | the section 10 badge |

Both metrics read `silver.class_git_pull_requests`, whose columns were read off
insight-dev: `created_on`, `updated_on`, `closed_on` are `Nullable(DateTime)`,
`_airbyte_extracted_at` is `DateTime64(3)` and is ingest time — the column decision 2
exists to keep out of a window. 65,014 rows spanning 2016-02-29 to 2026-09-07, so every
token in section 4 returns something.
