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

healthcheck() {
  local status
  status="$(curl --connect-timeout 2 --max-time 5 --silent \
    --output /dev/null --write-out '%{http_code}' "$1")" || return
  [[ "$status" == "200" ]]
}

APP__gears__api_gateway__config__bind_addr="127.0.0.1:$port" \
  APP__gears__insight_v3_core__config__clickhouse_url="http://127.0.0.1:18123" \
  APP__gears__insight_v3_core__config__clickhouse_database="insight" \
  APP__gears__insight_v3_core__config__ingest_token="health-test-token" \
  cargo run --quiet --manifest-path "$backend_dir/Cargo.toml" \
  --package insight-v3-core -- \
  --config "$service_dir/config/insight.yaml" run >"$log_file" 2>&1 &
pid=$!

for _ in {1..120}; do
  if ! kill -0 "$pid" 2>/dev/null; then
    cat "$log_file"
    exit 1
  fi

  if healthcheck "http://127.0.0.1:$port/health" && \
     healthcheck "http://127.0.0.1:$port/healthz"; then
    exit 0
  fi

  sleep 0.25
done

cat "$log_file"
exit 1
