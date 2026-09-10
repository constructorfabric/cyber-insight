#!/usr/bin/env bash
set -euo pipefail

service_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fixture_file="$service_dir/tests/fixtures/events.jsonl"

port="${INSIGHT_V3_CORE_PORT:-8087}"
base_url="http://127.0.0.1:$port"

clickhouse_url="${INSIGHT_V3_CORE_TEST_CLICKHOUSE_URL:-http://127.0.0.1:${CLICKHOUSE_HTTP_PORT:-8123}}"
clickhouse_database="${INSIGHT_V3_CORE_TEST_CLICKHOUSE_DATABASE:-insight}"
clickhouse_user="${CLICKHOUSE_USER:-insight}"
clickhouse_password="${CLICKHOUSE_PASSWORD:-insight-local}"

table_name="mvp_events_$$"
metric_name="commits_per_day_$$"
widget_table_name="commits_table_$$"
widget_graph_name="commits_graph_$$"
dashboard_name="engineering_$$"

log_file="$(mktemp)"
body_file="$(mktemp)"
table_created="false"
definitions_created="false"

cleanup() {
  status=$?
  trap - EXIT
  if [[ "$table_created" == "true" ]]; then
    curl --silent --show-error --connect-timeout 2 --max-time 10 \
      --user "$clickhouse_user:$clickhouse_password" \
      --data-binary "DROP TABLE IF EXISTS $table_name" \
      "$clickhouse_url/?database=$clickhouse_database" >/dev/null 2>&1 || true
  fi
  if [[ "$definitions_created" == "true" ]]; then
    curl --silent --show-error --connect-timeout 2 --max-time 10 \
      --user "$clickhouse_user:$clickhouse_password" \
      --data-binary "ALTER TABLE metrics DELETE WHERE name = '$metric_name'" \
      "$clickhouse_url/?database=$clickhouse_database" >/dev/null 2>&1 || true
    curl --silent --show-error --connect-timeout 2 --max-time 10 \
      --user "$clickhouse_user:$clickhouse_password" \
      --data-binary "ALTER TABLE widgets DELETE WHERE name IN ('$widget_table_name', '$widget_graph_name')" \
      "$clickhouse_url/?database=$clickhouse_database" >/dev/null 2>&1 || true
    curl --silent --show-error --connect-timeout 2 --max-time 10 \
      --user "$clickhouse_user:$clickhouse_password" \
      --data-binary "ALTER TABLE dashboards DELETE WHERE name = '$dashboard_name'" \
      "$clickhouse_url/?database=$clickhouse_database" >/dev/null 2>&1 || true
  fi
  if [[ "$status" != "0" ]] && [[ -s "$log_file" ]]; then
    cat "$log_file" >&2
  fi
  rm -f "$log_file" "$body_file"
  exit "$status"
}
trap cleanup EXIT

step() {
  printf '\n== step %s: %s ==\n' "$1" "$2"
}

expect_equal() {
  local expected="$1"
  local actual="$2"
  local context="$3"

  if [[ "$actual" == "$expected" ]]; then
    return 0
  fi

  printf '%s: expected %q, got %q\n' "$context" "$expected" "$actual" >&2
  return 1
}

http() {
  local method="$1"
  local path="$2"
  shift 2
  curl --silent --show-error --connect-timeout 2 --max-time 10 \
    --request "$method" \
    --output "$body_file" --write-out '%{http_code}' \
    "$@" \
    "$base_url$path"
}

step 0 "read the ingest token and confirm the stack is reachable"

token="${INSIGHT_V3_INGEST_TOKEN:-}"
if [[ -z "$token" ]]; then
  echo "INSIGHT_V3_INGEST_TOKEN is not set. Export it to the value insight-v3-core was started with." >&2
  exit 1
fi
if (( ${#token} < 32 )); then
  echo "INSIGHT_V3_INGEST_TOKEN must be at least 32 bytes (got ${#token})." >&2
  exit 1
fi
echo "ingest token present (${#token} bytes)"

status="$(http GET /health)"
if [[ "$status" != "200" ]]; then
  echo "insight-v3-core is not reachable at $base_url (GET /health returned $status)." >&2
  echo "This script does not start the stack — bring it up yourself first." >&2
  exit 1
fi
echo "stack reachable at $base_url"

step 1 "PUT /v1/tables/$table_name"
status="$(http PUT "/v1/tables/$table_name" --header "X-Insight-Token: $token")"
expect_equal "204" "$status" "table creation"
table_created="true"
echo "-> $status"

step 2 "POST /v1/raw-data for each fixture line"
line_count=0
while IFS= read -r line || [[ -n "$line" ]]; do
  [[ -z "$line" ]] && continue
  line_count=$((line_count + 1))
  request_body="$(jq -c --arg table "$table_name" '{table: $table, raw_data: .}' <<<"$line")"
  status="$(http POST /v1/raw-data \
    --header "X-Insight-Token: $token" \
    --header 'content-type: application/json' \
    --data-binary "$request_body")"
  expect_equal "204" "$status" "raw-data insertion (line $line_count)"
done <"$fixture_file"
expect_equal "30" "$line_count" "fixture line count"
echo "-> ingested $line_count events"

step 3 "PUT /v1/metrics/$metric_name"
metric_body="$(jq -n --arg table "$table_name" '{
  table: $table,
  fields: [
    {json: "day", type: "string", as_name: "day"},
    {json: "lines", type: "int", agg: "sum", as_name: "lines"}
  ],
  group_by: ["day"],
  filters: [],
  limit: 100
}')"
status="$(http PUT "/v1/metrics/$metric_name" \
  --header 'content-type: application/json' \
  --data-binary "$metric_body")"
expect_equal "204" "$status" "metric definition"
definitions_created="true"
echo "-> $status"

step 4 "POST /v1/metrics/$metric_name/run"
status="$(http POST "/v1/metrics/$metric_name/run")"
response_body="$(cat "$body_file")"
expect_equal "200" "$status" "metric run"
columns="$(jq -r '.columns | join(",")' <<<"$response_body")"
expect_equal "day,lines" "$columns" "metric run columns"
row_count="$(jq '.rows | length' <<<"$response_body")"
expect_equal "10" "$row_count" "metric run row count"
day_one_lines="$(jq -r '.rows[] | select(.[0] == "2026-09-01") | .[1]' <<<"$response_body")"
expect_equal "59" "$day_one_lines" "commits_per_day sum for 2026-09-01"
echo "-> $status, $row_count rows, columns=$columns"

step 5 "PUT /v1/widgets/$widget_table_name"
status="$(http PUT "/v1/widgets/$widget_table_name" \
  --header 'content-type: application/json' \
  --data-binary "$(jq -n --arg metric "$metric_name" '{type: "table", metric: $metric, columns: ["day", "lines"]}')")"
expect_equal "204" "$status" "table widget"
echo "-> $status"

step 6 "PUT /v1/widgets/$widget_graph_name"
status="$(http PUT "/v1/widgets/$widget_graph_name" \
  --header 'content-type: application/json' \
  --data-binary "$(jq -n --arg metric "$metric_name" '{type: "line", metric: $metric, x: "day", y: "lines"}')")"
expect_equal "204" "$status" "line widget"
echo "-> $status"

step 7 "PUT /v1/dashboards/$dashboard_name"
status="$(http PUT "/v1/dashboards/$dashboard_name" \
  --header 'content-type: application/json' \
  --data-binary "$(jq -n --arg t "$widget_table_name" --arg g "$widget_graph_name" \
    '{title: "Engineering", widgets: [$t, $g]}')")"
expect_equal "204" "$status" "dashboard definition"
echo "-> $status"

step 8 "GET /v1/dashboards/$dashboard_name"
status="$(http GET "/v1/dashboards/$dashboard_name")"
response_body="$(cat "$body_file")"
expect_equal "200" "$status" "dashboard read"
has_table="$(jq --arg n "$widget_table_name" '.widgets | index($n) != null' <<<"$response_body")"
has_graph="$(jq --arg n "$widget_graph_name" '.widgets | index($n) != null' <<<"$response_body")"
expect_equal "true" "$has_table" "dashboard widgets include $widget_table_name"
expect_equal "true" "$has_graph" "dashboard widgets include $widget_graph_name"
echo "-> $status, widgets=$(jq -c '.widgets' <<<"$response_body")"

echo
echo "All 9 steps passed (table=$table_name)."
