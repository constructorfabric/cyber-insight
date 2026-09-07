#!/usr/bin/env bash
set -euo pipefail

service_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
backend_dir="$(cd "$service_dir/../.." && pwd)"
clickhouse_url="${INSIGHT_V3_CORE_TEST_CLICKHOUSE_URL:-http://127.0.0.1:18123}"
clickhouse_database="${INSIGHT_V3_CORE_TEST_CLICKHOUSE_DATABASE:-insight}"
clickhouse_user="${INSIGHT_V3_CORE_TEST_CLICKHOUSE_USER:-}"
clickhouse_password="${INSIGHT_V3_CORE_TEST_CLICKHOUSE_PASSWORD:-}"
port="${INSIGHT_V3_CORE_TEST_PORT:-18086}"
token="${INSIGHT_V3_CORE_TEST_TOKEN:-synthetic-test-token}"
table_name="synthetic.events.$$"
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

app_env=(
  env
  "APP__gears__insight_v3_core__config__clickhouse_url=$clickhouse_url"
  "APP__gears__insight_v3_core__config__clickhouse_database=$clickhouse_database"
  "APP__gears__insight_v3_core__config__ingest_token=$token"
)
clickhouse_curl=(--fail --silent --show-error --connect-timeout 2 --max-time 10)

if [[ -n "$clickhouse_user" || -n "$clickhouse_password" ]]; then
  app_env+=(
    "APP__gears__insight_v3_core__config__clickhouse_user=$clickhouse_user"
    "APP__gears__insight_v3_core__config__clickhouse_password=$clickhouse_password"
  )
  clickhouse_curl+=(--user "$clickhouse_user:$clickhouse_password")
fi

clickhouse_query() {
  curl "${clickhouse_curl[@]}" \
    --data-binary "$1" \
    "$clickhouse_url/?database=$clickhouse_database"
}

for _ in 1 2; do
  "${app_env[@]}" cargo run --quiet --manifest-path "$backend_dir/Cargo.toml" \
    --package insight-v3-core -- \
    --config "$service_dir/config/insight.yaml" migrate
done

"${app_env[@]}" \
  "APP__gears__api_gateway__config__bind_addr=127.0.0.1:$port" \
  cargo run --quiet --manifest-path "$backend_dir/Cargo.toml" \
  --package insight-v3-core -- \
  --config "$service_dir/config/insight.yaml" run >"$log_file" 2>&1 &
pid=$!

ready="false"
for _ in {1..240}; do
  if ! kill -0 "$pid" 2>/dev/null; then
    cat "$log_file"
    exit 1
  fi

  if curl --fail --silent --connect-timeout 2 --max-time 5 \
    "http://127.0.0.1:$port/health" >/dev/null; then
    ready="true"
    break
  fi
  sleep 0.25
done
if [[ "$ready" != "true" ]]; then
  cat "$log_file"
  exit 1
fi

raw_values=(
  '{"nested":[1,true,null]}'
  '[1,2,3]'
  '"scalar"'
  '42'
)
request_body="{\"table\":\"$table_name\",\"raw_data\":${raw_values[0]}}"
status="$(curl --silent --connect-timeout 2 --max-time 10 \
  --output /dev/null --write-out '%{http_code}' \
  --header 'content-type: application/json' \
  --data-binary "$request_body" \
  "http://127.0.0.1:$port/v1/raw-data")"
[[ "$status" == "401" ]]

status="$(curl --silent --connect-timeout 2 --max-time 10 \
  --output /dev/null --write-out '%{http_code}' \
  --header 'content-type: application/json' \
  --header 'authorization: Bearer incorrect-token' \
  --data-binary "$request_body" \
  "http://127.0.0.1:$port/v1/raw-data")"
[[ "$status" == "401" ]]

before="$(clickhouse_query \
  "SELECT count() FROM raw_data WHERE table_name = '$table_name'")"
[[ "$before" == "0" ]]

for raw_value in "${raw_values[@]}"; do
  request_body="{\"table\":\"$table_name\",\"raw_data\":$raw_value}"
  status="$(curl --silent --connect-timeout 2 --max-time 10 \
    --output /dev/null --write-out '%{http_code}' \
    --header 'content-type: application/json' \
    --header "authorization: Bearer $token" \
    --data-binary "$request_body" \
    "http://127.0.0.1:$port/v1/raw-data")"
  [[ "$status" == "204" ]]
done

stored="$(clickhouse_query "SELECT
  count(),
  countIf(raw_data = '{\"nested\":[1,true,null]}'),
  countIf(raw_data = '[1,2,3]'),
  countIf(raw_data = '\"scalar\"'),
  countIf(raw_data = '42')
FROM raw_data
WHERE table_name = '$table_name'")"
[[ "$stored" == $'4\t1\t1\t1\t1' ]]
