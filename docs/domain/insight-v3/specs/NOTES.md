# Notes — Insight v3

## Areas

1. Data ingestion
2. Identity resolution
3. Access control
4. Create metrics, widgets, and dashboards
5. AI (including answer one time questions)
6. Review data ingestion status
7. Review platform usage
8. Alerts
9. Query optimization
10. Download data from Insight (API, manually in reports)

## MVP check

Built from scratch in `insight-v3-core`. Nothing from analytics.

- [ ] 0. Pick the data — synthetic JSON events
- [ ] 1. Send it to the DB — `POST /v1/raw-data` (exists)
- [ ] 2. Create a metric in JSON, at runtime
- [ ] 3. Create two widgets in JSON, at runtime — one table, one graph
- [ ] 4. Create a dashboard in JSON
- [ ] 5. Render it at `/portal/custom/{dashboard-name}`
- [ ] 6. AI chat in the UI: answers a one-time question from the data, and does 2, 3 and 4 from a message

### Decided

- Metrics, widgets and dashboards are JSON, so a new one is added without a code change.
- One table per kind: `metrics`, `widgets`, `dashboards`.
- A widget names a metric; the metric carries the query over `raw_data`.
- The chat calls the model directly from `insight-v3-core`, with the token in service config. The chat engine gear comes later.
- A metric's JSON is a structured query the service interprets — table, select, group by, where. No SQL in the definition.
- The chat writes through the same endpoints the UI uses, so it needs the portal session, not the ingest token.
- For the MVP, definitions are global: no owner column, everyone on the stand sees the same ones. Per-user and sharing are feature scope, not MVP.
- The portal session guards every definition endpoint. Ingest keeps its own token.
- Time is whatever a metric filters on. No dashboard period selector in the MVP.
- The chat prompt carries the table names and the field names sampled from each table, so it stops guessing.
- The chat checks whether a name is taken before creating, and refuses to overwrite. A direct `PUT` still replaces.
- A widget whose metric fails shows the error in place of its content; an empty result says so.
- `/portal/custom` lists all dashboards and links to each. The chat panel sits there too.
- A dashboard created in the chat appears in that list at once, and the page routes to it.

### Handoff

- Work lands on `feat/insight-v3-core-raw-data` ([PR #3255](https://github.com/constructorfabric/insight/pull/3255)), one commit per task.
- The end-to-end script runs against `./dev-compose.sh up`, and mints its session token with `tests/lib/insight_stand/service_token.py`.
- The chat is tested against fixtures. No live model call, no key in the repo.
- Screenshots and screen recordings land in `screenshots-and-etc/` — gitignored, never committed.
- The chat has a `chat_mode: canned` config for recording: a fixed proposal, no network call, same validation and storage as live.
- Recording drives the real Keycloak login as `dev@company.nonpresent` / `insight-dev`, against `npm run dev` proxied to the compose stack.
- Plan: [docs/superpowers/plans/2026-09-07-insight-v3-mvp.md](../../../superpowers/plans/2026-09-07-insight-v3-mvp.md)

### Assumed

- `/portal/custom/*` sits behind the same portal session as every other portal page.
- Done means the dashboard opens in a browser on the dev stand.

### To build

| # | Piece |
|---|-------|
| 2 | `metrics` table, `PUT /v1/metrics/{name}`, `GET /v1/metrics/{name}` |
| 2 | Metric run: `POST /v1/metrics/{name}/run`, returning rows and columns |
| 3 | `widgets` table, `PUT /v1/widgets/{name}`, `GET /v1/widgets/{name}` |
| 4 | `dashboards` table, `PUT /v1/dashboards/{name}`, `GET /v1/dashboards/{name}` |
| 5 | Route `/portal/custom/$name`, a table renderer and a graph renderer |
| 6 | `POST /v1/chat` plus a chat panel on the page, writing definitions through the endpoints above |

Plan: [docs/superpowers/plans/2026-09-07-insight-v3-mvp.md](../../../superpowers/plans/2026-09-07-insight-v3-mvp.md)
