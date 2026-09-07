# Insight v3 Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a runnable `insight-v3-core` Rust microservice with a Docker image, working health endpoints, and token-authenticated raw JSON ingestion into ClickHouse.

**Architecture:** Add a gears-rust host modeled on `services/analytics`, with a REST-capable gear and the standard authenticated system-gear pipeline. The API gateway host owns `/health` and `/healthz`. The gear exposes one manually token-authenticated route and writes through a self-managed ClickHouse client to the fixed `raw_data` table. A `migrate` subcommand owns schema creation; normal server startup is schema-read-only.

**Tech Stack:** Rust 1.95, Tokio, gears-rust toolkit, Axum, Clap, ClickHouse 25.7, Docker

**Spec:** `docs/superpowers/specs/2026-09-07-insight-v3-core-design.md`

## Global Constraints

- Use Rust edition 2024 and the backend workspace dependency versions.
- Use the service name `insight-v3-core`, gear name `insight-v3-core`, and HTTP port `8086`.
- Follow the analytics service's bootstrap, configuration, authentication, and Docker conventions.
- Do not add orchestration, Helm, Compose, gateway routing, CI publication, read APIs, or arbitrary caller-selected physical tables.
- Run the image as a non-root user.
- Treat the ingestion token as a static per-instance env secret with no issuance or exchange flow, compare it in constant time, and never log or return it.
- Treat request `table` as a logical source label stored in `raw_data.table_name`; never interpolate it into SQL.

---

### Task 1: Runnable gears service

**Files:**

- Create: `src/backend/services/insight-v3-core/Cargo.toml`
- Create: `src/backend/services/insight-v3-core/src/main.rs`
- Create: `src/backend/services/insight-v3-core/src/gear.rs`
- Create: `src/backend/services/insight-v3-core/config/insight.yaml`
- Create: `src/backend/services/insight-v3-core/Dockerfile`
- Create: `src/backend/services/insight-v3-core/tests/health.sh`
- Modify: `src/backend/Cargo.toml`
- Modify: `src/backend/Cargo.lock`
- Modify: sibling Rust-service Dockerfiles under `src/backend/services/`

**Interfaces:**

- Consumes: `toolkit::bootstrap::{AppConfig, run_server}`, `toolkit::{Gear, GearCtx, RestApiCapability}`, and the inventory-linked system gears used by analytics.
- Produces: the `insight-v3-core` binary, the `InsightV3CoreGear` registration, and HTTP `200` responses at `/health` and `/healthz` on port `8086`.

- [ ] **Step 1: Add the failing process smoke test**

```bash
#!/usr/bin/env bash
set -euo pipefail

service_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
backend_dir="$(cd "$service_dir/../.." && pwd)"
port="${INSIGHT_V3_CORE_TEST_PORT:-18086}"
log_file="$(mktemp)"
pid=""

cleanup() {
  if [[ -n "$pid" ]]; then
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
  fi
  rm -f "$log_file"
}
trap cleanup EXIT

APP__gears__api_gateway__config__bind_addr="127.0.0.1:$port" \
  cargo run --quiet --manifest-path "$backend_dir/Cargo.toml" \
  --package insight-v3-core -- \
  --config "$service_dir/config/insight.yaml" run >"$log_file" 2>&1 &
pid=$!

for _ in {1..120}; do
  if ! kill -0 "$pid" 2>/dev/null; then
    cat "$log_file"
    exit 1
  fi

  if curl --fail --silent "http://127.0.0.1:$port/health" >/dev/null && \
     curl --fail --silent "http://127.0.0.1:$port/healthz" >/dev/null; then
    exit 0
  fi

  sleep 0.25
done

cat "$log_file"
exit 1
```

- [ ] **Step 2: Run the smoke test and verify it fails because the service package does not exist**

Run: `PATH="$HOME/.cargo/bin:$PATH" bash src/backend/services/insight-v3-core/tests/health.sh`

Expected: non-zero exit with Cargo reporting that package `insight-v3-core` does not exist.

- [ ] **Step 3: Implement the minimal analytics-style gears host and Docker image**

Register `services/insight-v3-core` in the workspace; add `main.rs`, an empty REST-capable `InsightV3CoreGear`, the analytics-shaped authenticated config on port `8086`, and the multi-stage Dockerfile. Update each sibling Rust-service Docker dependency-cache stage so the expanded workspace still resolves.

- [ ] **Step 4: Run the process smoke test and verify both health endpoints pass**

Run: `PATH="$HOME/.cargo/bin:$PATH" bash src/backend/services/insight-v3-core/tests/health.sh`

Expected: exit `0` after both `/health` and `/healthz` return HTTP `200`.

- [ ] **Step 5: Run package verification**

Run: `cargo fmt --all --check`

Run: `cargo test --package insight-v3-core`

Run: `cargo clippy --package insight-v3-core --all-targets -- -D warnings`

Expected: every command exits `0`.

- [ ] **Step 6: Build and smoke-test the container**

Run: `docker build -f services/insight-v3-core/Dockerfile -t insight-v3-core:dev .`

Run the image with `-p 18086:8086`, then request `/health` and `/healthz` and remove the container.

Expected: the image builds, the container stays running, and both endpoints return HTTP `200`.

- [ ] **Step 7: Commit the implementation**

```bash
git add src/backend docs/superpowers
git commit -m "feat(insight-v3-core): scaffold the service"
```

---

### Task 2: Token-authenticated raw-data ingestion

**Files:**

- Create: `src/backend/services/insight-v3-core/src/api.rs`
- Create: `src/backend/services/insight-v3-core/src/config.rs`
- Create: `src/backend/services/insight-v3-core/src/raw_data.rs`
- Create: `src/backend/services/insight-v3-core/src/migration.rs`
- Create: `src/backend/services/insight-v3-core/tests/raw_data.sh`
- Modify: `src/backend/services/insight-v3-core/Cargo.toml`
- Modify: `src/backend/services/insight-v3-core/src/main.rs`
- Modify: `src/backend/services/insight-v3-core/src/gear.rs`
- Modify: `src/backend/services/insight-v3-core/config/insight.yaml`
- Modify: `src/backend/services/insight-v3-core/tests/health.sh`
- Modify: `src/backend/Cargo.lock`

**Interfaces:**

- Consumes: `APP__gears__insight_v3_core__config__ingest_token`, ClickHouse connection settings, `Authorization: Bearer`, and `{ "table": string, "raw_data": JSON }`.
- Produces: `POST /v1/raw-data`, HTTP `204` on a committed insert, and rows in the fixed ClickHouse table `raw_data(id, table_name, raw_data, received_at)`.

- [ ] **Step 1: Add failing configuration and authentication tests**

Cover required non-empty token/ClickHouse settings, redacted secret formatting if effective config can be printed, exactly one valid Bearer header, case-insensitive Bearer scheme, constant-time token verification, and rejection of missing, duplicate, malformed, empty, or wrong tokens.

Run: `cargo test --package insight-v3-core config api`

Expected: non-zero exit because the configuration and ingestion modules do not exist.

- [ ] **Step 2: Add failing request and store tests**

Cover the public HTTP contract: arbitrary JSON shapes are accepted in `raw_data`; a trimmed non-empty logical table label is stored; blank or overlong labels and oversized bodies are rejected; unauthorized requests never write; ClickHouse failures do not expose backend details. Use the ClickHouse crate test mock to assert the exact inserted row and serialized JSON.

Run: `cargo test --package insight-v3-core raw_data`

Expected: non-zero exit because the request/store implementation does not exist.

- [ ] **Step 3: Implement typed configuration and ClickHouse storage**

Add gear configuration for `clickhouse_url`, `clickhouse_database`, optional user/password, and `ingest_token`. Validate required values at both server initialization and migration entrypoints. Build the shared ClickHouse client with optional credentials. Represent stored rows with generated UUID v7, UTC millisecond reception time, logical table name, and serialized JSON text. Keep SQL identifiers fixed.

- [ ] **Step 4: Implement the protected REST route**

Register `POST /v1/raw-data` through `OperationBuilder` as an exposed anonymous gateway route whose handler enforces its own Bearer token. Apply a bounded request body, validate and normalize the logical table label, serialize any valid JSON value, synchronously finish the ClickHouse insert, and return `204`. Return canonical client/auth/server errors, add `WWW-Authenticate: Bearer` on `401`, and never include the secret or ClickHouse error text in responses.

- [ ] **Step 5: Add the idempotent migration path**

Add `insight-v3-core migrate` and execute `CREATE TABLE IF NOT EXISTS raw_data` with the schema in the design document. Keep DDL out of ordinary server startup.

- [ ] **Step 6: Make the focused tests pass**

Run: `cargo fmt --all --check`

Run: `cargo test --package insight-v3-core`

Run: `cargo clippy --package insight-v3-core --all-targets -- -D warnings`

Expected: every command exits `0`.

- [ ] **Step 7: Verify against real ClickHouse**

Start `clickhouse/clickhouse-server:25.7.5` on an isolated Docker network. Run the migration, start the actual service with env-provided token and ClickHouse settings, prove unauthenticated and incorrect-token requests return `401`, post nested/object/array/scalar JSON successfully, and query ClickHouse to prove the logical table label and JSON text were stored in `raw_data`.

Expected: migration succeeds repeatedly, rejected requests add no rows, successful requests return `204`, and stored rows match their request bodies.

- [ ] **Step 8: Run full verification and commit**

Run: `cargo test --workspace --quiet`

Run: `python3 scripts/ci/tests/test_changed.py`

Run: `git diff --check`

Expected: every command exits `0`.

```bash
git add src/backend docs/superpowers
git commit -m "feat(insight-v3-core): ingest raw JSON with an instance secret"
```
