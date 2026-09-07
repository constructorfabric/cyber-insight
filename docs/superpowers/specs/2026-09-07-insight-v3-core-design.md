# Insight v3 Core Design

## Goal

Introduce `insight-v3-core` as a runnable Rust microservice that follows the same gears-rust host pattern as the analytics service and accepts token-authenticated raw JSON for durable storage in ClickHouse.

## Architecture

`insight-v3-core` is a member of the existing backend Cargo workspace. Its binary loads the standard gears configuration and starts `toolkit::bootstrap::run_server`. An `InsightV3CoreGear` owns the ClickHouse client, registers the ingestion route, and exposes it through the toolkit REST capability.

The host links the same API gateway and authentication pipeline gears as analytics. This preserves the expected service shape for later authenticated APIs without inventing a second runtime pattern.

## Runtime contract

The API gateway host listens on port `8086` and owns the public `/health` and `/healthz` endpoints. Both endpoints must return HTTP `200` when the process is ready.

`POST /v1/raw-data` accepts an `Authorization: Bearer <token>` header and a JSON body with two fields: `table`, a non-empty logical source label, and `raw_data`, any valid JSON value. `ingest_token` is a static per-service-instance secret supplied only through the environment-backed service configuration; there is no token issuance, retrieval, refresh, OAuth, or JWT flow. The route is public to the gateway authentication pipeline and compares the supplied Bearer value with that configured secret. Missing, malformed, duplicate, or incorrect authorization headers return HTTP `401` with `WWW-Authenticate: Bearer`; the token is never logged or returned. A successful synchronous insert returns HTTP `204`. Invalid labels or oversized input return a client error, while ClickHouse failures return a generic server error and are logged internally.

The request's `table` value is data, not a physical identifier. Every request is inserted into the fixed ClickHouse table `raw_data`, with the logical label stored in its `table_name` column. This prevents callers from selecting or injecting physical table names while preserving the requested source-table information.

The physical schema is:

```sql
CREATE TABLE IF NOT EXISTS raw_data (
    id UUID,
    table_name String,
    raw_data String,
    received_at DateTime64(3, 'UTC')
)
ENGINE = MergeTree
ORDER BY (table_name, received_at, id)
```

The service serializes `raw_data` as valid JSON text, generates the row ID and reception timestamp, and inserts only after authentication and validation. A dedicated `migrate` subcommand applies the idempotent table DDL; ordinary server boot never changes the schema.

Configuration lives in `services/insight-v3-core/config/insight.yaml`. The ClickHouse URL, database, optional credentials, and ingestion token are gear configuration values and can be overridden with `APP__gears__insight_v3_core__config__*` environment variables. The service refuses to start or migrate when the URL, database, or token is empty. Logging and graceful shutdown are provided by the toolkit bootstrap.

## Container

The service ships as a multi-stage Debian-based image built from the backend workspace context. The runtime contains only the compiled binary, service configuration, shared entrypoint, and required certificates. It runs as the existing non-root `appuser` convention and exposes port `8086`.

Adding the workspace member requires the dependency-cache stages of sibling Rust-service Dockerfiles to copy and stub the new manifest, keeping their existing image builds valid.

## Verification

A process-level smoke test starts the real binary with test-only configuration and checks `/health` and `/healthz`. Unit and HTTP-contract tests cover token rejection, request validation, serialization, and ClickHouse insert behavior. End-to-end verification starts a real ClickHouse 25.7.5 container, applies the migration, posts raw JSON through the service API, and queries the stored row. Verification also runs the workspace tests, Clippy with warnings denied, a Docker build, and container health requests.

## Out of scope

Docker Compose, Helm, platform-gateway routing, CI image publication, arbitrary caller-selected physical tables, read APIs, queues, and background processing remain out of scope.
