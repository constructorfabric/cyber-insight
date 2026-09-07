//! Compiles a metric's JSON definition into `ClickHouse` SQL and runs it.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fmt::Write as _;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

const MAX_IDENTIFIER_CHARS: usize = 128;
const DEFAULT_LIMIT: u32 = 1000;
const MAX_LIMIT: u32 = 10000;
const FETCH_TIMEOUT_SECS: u64 = 30;
const MAX_RESULT_BYTES: usize = 5 * 1024 * 1024;

#[derive(Debug, Deserialize)]
pub(crate) struct MetricQuery {
    table: String,
    fields: Vec<Field>,
    #[serde(default)]
    group_by: Vec<String>,
    #[serde(default)]
    filters: Vec<Filter>,
    #[serde(default)]
    limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct Field {
    json: String,
    r#type: FieldType,
    #[serde(default)]
    agg: Option<Agg>,
    as_name: String,
}

#[derive(Debug, Deserialize)]
struct Filter {
    json: String,
    r#type: FieldType,
    op: FilterOp,
    value: serde_json::Value,
}

impl Filter {
    fn bind(&self) -> Result<FilterBind, MetricQueryError> {
        match self.r#type {
            FieldType::String => self.value.as_str().map(|v| FilterBind::Str(v.to_owned())),
            FieldType::Int => self.value.as_i64().map(FilterBind::Int),
            FieldType::Float => self.value.as_f64().map(FilterBind::Float),
        }
        .ok_or_else(|| MetricQueryError::FilterValue(self.json.clone()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum FieldType {
    String,
    Int,
    Float,
}

impl FieldType {
    fn extract(self, json: &str) -> String {
        let function = match self {
            Self::String => "JSONExtractString",
            Self::Int => "JSONExtractInt",
            Self::Float => "JSONExtractFloat",
        };
        format!("{function}(raw_data, '{json}')")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Agg {
    Count,
    Sum,
    Avg,
    Min,
    Max,
}

impl Agg {
    fn sql(self) -> &'static str {
        match self {
            Self::Count => "count",
            Self::Sum => "sum",
            Self::Avg => "avg",
            Self::Min => "min",
            Self::Max => "max",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum FilterOp {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
}

impl FilterOp {
    fn sql(self) -> &'static str {
        match self {
            Self::Eq => "=",
            Self::Ne => "!=",
            Self::Gt => ">",
            Self::Gte => ">=",
            Self::Lt => "<",
            Self::Lte => "<=",
        }
    }
}

/// A single filter value, typed per its declared [`FieldType`].
///
/// `MetricQuery::compile` validates each filter value against its declared
/// type up front, so binding a numeric filter produces a numeric SQL literal
/// rather than a quoted string compared against a `JSONExtractInt`/`Float`
/// expression.
#[derive(Debug, Clone)]
pub(crate) enum FilterBind {
    Str(String),
    Int(i64),
    Float(f64),
}

impl FilterBind {
    fn as_display_string(&self) -> String {
        match self {
            Self::Str(value) => value.clone(),
            Self::Int(value) => value.to_string(),
            Self::Float(value) => value.to_string(),
        }
    }

    fn bind_onto(&self, query: clickhouse::query::Query) -> clickhouse::query::Query {
        match self {
            Self::Str(value) => query.bind(value),
            Self::Int(value) => query.bind(value),
            Self::Float(value) => query.bind(value),
        }
    }
}

/// Compares against the string form asserted by `MetricQuery::compile`'s
/// unit tests, without stringifying the value used to actually bind the
/// query (see [`FilterBind::bind_onto`]).
impl PartialEq<String> for FilterBind {
    fn eq(&self, other: &String) -> bool {
        self.as_display_string() == *other
    }
}

#[derive(Debug)]
pub(crate) struct CompiledQuery {
    pub(crate) sql: String,
    pub(crate) binds: Vec<FilterBind>,
    column_types: HashMap<String, FieldType>,
}

#[derive(Debug, Error)]
pub(crate) enum MetricQueryError {
    #[error("`{0}` must be 1-128 characters of letters, digits or underscore")]
    Identifier(String),
    #[error("a metric must select at least one field")]
    NoFields,
    #[error("filter value for `{0}` does not match its declared type")]
    FilterValue(String),
}

impl MetricQuery {
    pub(crate) fn compile(&self) -> Result<CompiledQuery, MetricQueryError> {
        if !is_identifier(&self.table) {
            return Err(MetricQueryError::Identifier(self.table.clone()));
        }
        if self.fields.is_empty() {
            return Err(MetricQueryError::NoFields);
        }

        let mut select_parts = Vec::with_capacity(self.fields.len());
        let mut as_names = HashSet::with_capacity(self.fields.len());
        let mut column_types = HashMap::with_capacity(self.fields.len());
        for field in &self.fields {
            if !is_identifier(&field.json) {
                return Err(MetricQueryError::Identifier(field.json.clone()));
            }
            if !is_identifier(&field.as_name) {
                return Err(MetricQueryError::Identifier(field.as_name.clone()));
            }
            as_names.insert(field.as_name.as_str());
            column_types.insert(field.as_name.clone(), field.r#type);

            let extraction = field.r#type.extract(&field.json);
            let expression = match field.agg {
                Some(agg) => format!("{}({extraction})", agg.sql()),
                None => extraction,
            };
            select_parts.push(format!("{expression} AS `{}`", field.as_name));
        }

        for group in &self.group_by {
            if !is_identifier(group) || !as_names.contains(group.as_str()) {
                return Err(MetricQueryError::Identifier(group.clone()));
            }
        }

        let mut where_parts = Vec::with_capacity(self.filters.len());
        let mut binds = Vec::with_capacity(self.filters.len());
        for filter in &self.filters {
            if !is_identifier(&filter.json) {
                return Err(MetricQueryError::Identifier(filter.json.clone()));
            }
            let extraction = filter.r#type.extract(&filter.json);
            where_parts.push(format!("{extraction} {} ?", filter.op.sql()));
            binds.push(filter.bind()?);
        }

        let mut sql = format!("SELECT {} FROM `{}`", select_parts.join(", "), self.table);
        if !where_parts.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&where_parts.join(" AND "));
        }
        if !self.group_by.is_empty() {
            let backticked: Vec<String> = self
                .group_by
                .iter()
                .map(|group| format!("`{group}`"))
                .collect();
            sql.push_str(" GROUP BY ");
            sql.push_str(&backticked.join(", "));
            sql.push_str(" ORDER BY ");
            sql.push_str(&backticked.join(", "));
        }
        let limit = self.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);
        let _ = write!(sql, " LIMIT {limit}");

        Ok(CompiledQuery {
            sql,
            binds,
            column_types,
        })
    }
}

fn is_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= MAX_IDENTIFIER_CHARS
        && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `ClickHouse`'s `JSON` format serialises wide integers as JSON strings;
/// parse declared `int`/`float` columns back into numbers.
fn coerce_value(value: serde_json::Value, field_type: FieldType) -> serde_json::Value {
    let serde_json::Value::String(text) = value else {
        return value;
    };

    match field_type {
        FieldType::String => {}
        FieldType::Int => {
            if let Ok(number) = text.parse::<i64>() {
                return serde_json::Value::Number(number.into());
            }
        }
        FieldType::Float => {
            if let Some(number) = text
                .parse::<f64>()
                .ok()
                .and_then(serde_json::Number::from_f64)
            {
                return serde_json::Value::Number(number);
            }
        }
    }

    serde_json::Value::String(text)
}

/// The result of running a [`CompiledQuery`] against `ClickHouse`.
#[derive(Debug, Serialize)]
pub(crate) struct RunResult {
    pub(crate) columns: Vec<String>,
    pub(crate) rows: Vec<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
struct ResultMeta {
    name: String,
}

#[derive(Debug, Deserialize)]
struct ClickHouseJsonResult {
    meta: Vec<ResultMeta>,
    data: Vec<serde_json::Map<String, serde_json::Value>>,
}

pub(crate) struct MetricRunner {
    client: insight_clickhouse::Client,
    fetch_timeout: Duration,
}

impl MetricRunner {
    pub(crate) fn new(client: insight_clickhouse::Client) -> Self {
        Self {
            client,
            fetch_timeout: Duration::from_secs(FETCH_TIMEOUT_SECS),
        }
    }

    pub(crate) async fn run(&self, compiled: &CompiledQuery) -> Result<RunResult, MetricRunError> {
        let mut query = self.client.query(&compiled.sql);
        for bind in &compiled.binds {
            query = bind.bind_onto(query);
        }
        let mut cursor = query.fetch_bytes("JSON")?;

        let fetch = async {
            let mut bytes = Vec::new();
            while let Some(chunk) = cursor.next().await? {
                let next_len = bytes.len().saturating_add(chunk.len());
                if next_len > MAX_RESULT_BYTES {
                    return Err(MetricRunError::ResultTooLarge);
                }
                bytes.extend_from_slice(&chunk);
            }
            Ok::<_, MetricRunError>(bytes)
        };

        let bytes = tokio::time::timeout(self.fetch_timeout, fetch)
            .await
            .map_err(|_| MetricRunError::Timeout)??;

        let parsed: ClickHouseJsonResult = serde_json::from_slice(&bytes)?;
        let columns: Vec<String> = parsed.meta.into_iter().map(|column| column.name).collect();
        let rows = parsed
            .data
            .into_iter()
            .map(|mut row| {
                columns
                    .iter()
                    .map(|name| {
                        let value = row.remove(name).unwrap_or(serde_json::Value::Null);
                        match compiled.column_types.get(name) {
                            Some(field_type) => coerce_value(value, *field_type),
                            None => value,
                        }
                    })
                    .collect()
            })
            .collect();

        Ok(RunResult { columns, rows })
    }
}

impl fmt::Debug for MetricRunner {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MetricRunner")
            .field("fetch_timeout", &self.fetch_timeout)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Error)]
pub(crate) enum MetricRunError {
    #[error("metric query timed out")]
    Timeout,
    #[error("metric query result exceeded the size limit")]
    ResultTooLarge,
    #[error(transparent)]
    ClickHouse(clickhouse::error::Error),
    #[error(transparent)]
    InvalidResponse(#[from] serde_json::Error),
}

impl From<clickhouse::error::Error> for MetricRunError {
    fn from(error: clickhouse::error::Error) -> Self {
        match error {
            clickhouse::error::Error::TimedOut => Self::Timeout,
            error => Self::ClickHouse(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn query(value: serde_json::Value) -> MetricQuery {
        serde_json::from_value(value).unwrap_or_else(|error| panic!("valid metric: {error}"))
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

        let compiled = metric
            .compile()
            .unwrap_or_else(|error| panic!("compiles: {error}"));

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

        let compiled = metric
            .compile()
            .unwrap_or_else(|error| panic!("compiles: {error}"));

        assert!(
            compiled
                .sql
                .contains("WHERE JSONExtractString(raw_data, 'event') = ?")
        );
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

        assert!(matches!(
            metric.compile(),
            Err(MetricQueryError::Identifier(_))
        ));
    }

    #[test]
    fn a_metric_with_no_fields_is_refused() {
        let metric = query(json!({
            "table": "events", "fields": [], "group_by": [], "filters": []
        }));

        assert!(matches!(metric.compile(), Err(MetricQueryError::NoFields)));
    }

    #[test]
    fn numeric_filters_bind_the_numeric_value_not_its_json_encoding() {
        let metric = query(json!({
            "table": "events",
            "fields": [{ "json": "lines", "type": "int", "as_name": "lines" }],
            "group_by": [],
            "filters": [
                { "json": "lines", "type": "int", "op": "gt", "value": 5 }
            ]
        }));

        let compiled = metric
            .compile()
            .unwrap_or_else(|error| panic!("compiles: {error}"));

        assert_eq!(compiled.binds, vec!["5".to_owned()]);
    }

    #[test]
    fn a_filter_value_that_does_not_match_its_declared_type_is_refused() {
        let metric = query(json!({
            "table": "events",
            "fields": [{ "json": "lines", "type": "int", "as_name": "lines" }],
            "group_by": [],
            "filters": [
                { "json": "lines", "type": "int", "op": "gt", "value": "not-a-number" }
            ]
        }));

        assert!(matches!(
            metric.compile(),
            Err(MetricQueryError::FilterValue(_))
        ));
    }

    #[test]
    fn coerces_string_typed_clickhouse_numbers_to_json_numbers() {
        assert_eq!(coerce_value(json!("132"), FieldType::Int), json!(132));
        assert_eq!(coerce_value(json!("12.5"), FieldType::Float), json!(12.5));
        assert_eq!(
            coerce_value(json!("2026-09-01"), FieldType::String),
            json!("2026-09-01")
        );
    }

    #[test]
    fn group_by_must_reference_a_selected_field() {
        let metric = query(json!({
            "table": "events",
            "fields": [{ "json": "day", "type": "string", "as_name": "day" }],
            "group_by": ["not_selected"],
            "filters": []
        }));

        assert!(matches!(
            metric.compile(),
            Err(MetricQueryError::Identifier(_))
        ));
    }
}
