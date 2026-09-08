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
const FACT_ALIAS: &str = "__f";
const NAME_CTE: &str = "__person_name";
const EMAIL_CTE: &str = "__people_by_email";
const ID_CTE: &str = "__people_by_id";
const IDENTITY_TABLE: &str = "identity_persons";
const MAX_RESULT_BYTES: usize = 5 * 1024 * 1024;

#[derive(Debug, Deserialize)]
pub(crate) struct MetricQuery {
    #[serde(default)]
    database: Option<String>,
    table: String,
    fields: Vec<Field>,
    #[serde(default)]
    group_by: Vec<String>,
    #[serde(default)]
    filters: Vec<Filter>,
    #[serde(default)]
    order_by: Option<OrderBy>,
    #[serde(default)]
    limit: Option<u32>,
}

/// How to sort the rows. Without it the grouping's own columns order the
/// result, which cannot answer "the most" or "the largest".
#[derive(Debug, Deserialize)]
struct OrderBy {
    /// One of the query's own `as_name` values.
    field: String,
    #[serde(default)]
    direction: Direction,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Direction {
    #[default]
    Asc,
    Desc,
}

impl Direction {
    fn sql(self) -> &'static str {
        match self {
            Self::Asc => "ASC",
            Self::Desc => "DESC",
        }
    }
}

#[derive(Debug, Deserialize)]
struct Field {
    #[serde(default)]
    json: Option<String>,
    #[serde(default)]
    column: Option<String>,
    r#type: FieldType,
    #[serde(default)]
    agg: Option<Agg>,
    as_name: String,
    /// What this column holds a person by, when it holds one.
    #[serde(default)]
    person: Option<PersonHandle>,
}

/// Which handle a person column carries.
///
/// A fact table names a person the way its source system did - the address
/// they committed under, the id an API returned. Neither is the name anyone
/// would recognise them by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum PersonHandle {
    Email,
    Id,
}

impl PersonHandle {
    fn cte(self) -> &'static str {
        match self {
            Self::Email => EMAIL_CTE,
            Self::Id => ID_CTE,
        }
    }

    /// The handle as the identity side spells it: ids are compared as text,
    /// because the fact table stores one as a UUID, a String, or neither.
    fn key(self, read: &str) -> String {
        match self {
            Self::Email => read.to_owned(),
            Self::Id => format!("toString({read})"),
        }
    }
}

/// Where a person's name is resolved from.
///
/// The identity database on this stand. Only the database is configurable:
/// the table and the `value_type` rows inside it are identity's own schema,
/// not something a stand chooses.
#[derive(Debug, Clone)]
pub(crate) struct People {
    database: String,
}

impl People {
    pub(crate) fn new(database: impl Into<String>) -> Self {
        Self {
            database: database.into(),
        }
    }

    /// The `WITH` clauses the joins read from.
    ///
    /// A person accumulates a row per name they have ever had, so the latest
    /// one wins by `argMax` before anything joins to it. Joining the rows
    /// directly multiplies every fact by that history - it turned 752 merged
    /// pull requests into 18,800.
    fn prelude(&self, handles: &[PersonHandle]) -> Result<String, MetricQueryError> {
        if !is_identifier(&self.database) {
            return Err(MetricQueryError::Identifier(self.database.clone()));
        }
        let persons = format!("`{}`.`{IDENTITY_TABLE}`", self.database);

        let mut ctes = vec![format!(
            "{NAME_CTE} AS (SELECT `person_id`, argMax(`value_effective`, `created_at`) AS `display_name` FROM {persons} WHERE `value_type` = 'display_name' GROUP BY `person_id`)"
        )];
        for handle in handles {
            ctes.push(match handle {
                PersonHandle::Email => format!(
                    "{EMAIL_CTE} AS (SELECT `h`.`handle` AS `handle`, `n`.`display_name` AS `display_name` FROM (SELECT `value_effective` AS `handle`, argMax(`person_id`, `created_at`) AS `person_id` FROM {persons} WHERE `value_type` = 'email' GROUP BY `value_effective`) AS `h` INNER JOIN {NAME_CTE} AS `n` ON `n`.`person_id` = `h`.`person_id`)"
                ),
                PersonHandle::Id => format!(
                    "{ID_CTE} AS (SELECT toString(`person_id`) AS `handle`, `display_name` FROM {NAME_CTE})"
                ),
            });
        }

        Ok(format!("WITH {} ", ctes.join(", ")))
    }
}

#[derive(Debug, Deserialize)]
struct Filter {
    #[serde(default)]
    json: Option<String>,
    #[serde(default)]
    column: Option<String>,
    r#type: FieldType,
    op: FilterOp,
    value: serde_json::Value,
}

impl Filter {
    fn source(&self) -> Result<Source<'_>, MetricQueryError> {
        Source::resolve(self.json.as_deref(), self.column.as_deref())
            .ok_or_else(|| MetricQueryError::FieldSource("a filter".to_owned()))
    }

    fn bind(&self, source: Source<'_>) -> Result<FilterBind, MetricQueryError> {
        match self.r#type {
            FieldType::String => self.value.as_str().map(|v| FilterBind::Str(v.to_owned())),
            FieldType::Int => self.value.as_i64().map(FilterBind::Int),
            FieldType::Float => self.value.as_f64().map(FilterBind::Float),
        }
        .ok_or_else(|| MetricQueryError::FilterValue(source.name().to_owned()))
    }
}

impl Field {
    fn source(&self) -> Result<Source<'_>, MetricQueryError> {
        Source::resolve(self.json.as_deref(), self.column.as_deref())
            .ok_or_else(|| MetricQueryError::FieldSource(format!("field `{}`", self.as_name)))
    }

    /// What this field selects.
    ///
    /// Counting rows reads no value at all, so `count` alone is `count()` —
    /// the most natural aggregate there is, and one the language could not
    /// express while every field had to name a source. Every other
    /// aggregate, and every plain field, still reads exactly one.
    fn expression(&self, qualifier: Option<&str>) -> Result<String, MetricQueryError> {
        if self.agg == Some(Agg::Count) && self.json.is_none() && self.column.is_none() {
            return Ok("count()".to_owned());
        }

        Ok(self.aggregated(self.source()?.sql(self.r#type, qualifier)?))
    }

    fn aggregated(&self, read: String) -> String {
        match self.agg {
            Some(agg) => format!("{}({read})", agg.sql()),
            None => read,
        }
    }
}

/// Where a value is read from: a key inside the `raw_data` payload, or a
/// typed column of the table itself.
#[derive(Debug, Clone, Copy)]
enum Source<'a> {
    Json(&'a str),
    Column(&'a str),
}

impl<'a> Source<'a> {
    fn resolve(json: Option<&'a str>, column: Option<&'a str>) -> Option<Self> {
        match (json, column) {
            (Some(json), None) => Some(Self::Json(json)),
            (None, Some(column)) => Some(Self::Column(column)),
            (None, None) | (Some(_), Some(_)) => None,
        }
    }

    fn name(self) -> &'a str {
        match self {
            Self::Json(name) | Self::Column(name) => name,
        }
    }

    fn sql(
        self,
        field_type: FieldType,
        qualifier: Option<&str>,
    ) -> Result<String, MetricQueryError> {
        if !is_identifier(self.name()) {
            return Err(MetricQueryError::Identifier(self.name().to_owned()));
        }
        Ok(match self {
            Self::Json(json) => field_type.extract(json, qualifier),
            Self::Column(column) => match qualifier {
                Some(alias) => format!("`{alias}`.`{column}`"),
                None => format!("`{column}`"),
            },
        })
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
    fn extract(self, json: &str, qualifier: Option<&str>) -> String {
        let function = match self {
            Self::String => "JSONExtractString",
            Self::Int => "JSONExtractInt",
            Self::Float => "JSONExtractFloat",
        };
        let payload = match qualifier {
            Some(alias) => format!("`{alias}`.raw_data"),
            None => "raw_data".to_owned(),
        };
        format!("{function}({payload}, '{json}')")
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

/// What the field list compiled to.
struct Selection<'a> {
    parts: Vec<String>,
    as_names: HashSet<&'a str>,
    column_types: HashMap<String, FieldType>,
    /// The joins a person's name needs, if any field asked for one.
    joins: String,
    handles: Vec<PersonHandle>,
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
    #[error("`{0}` must name one of this query's as_name values")]
    GroupBy(String),
    #[error("`{0}` cannot order the rows: it is not one of this query's as_name values")]
    OrderBy(String),
    #[error("a metric must select at least one field")]
    NoFields,
    #[error("filter value for `{0}` does not match its declared type")]
    FilterValue(String),
    #[error("{0} must name exactly one of `json` or `column`")]
    FieldSource(String),
}

impl MetricQuery {
    /// The table alone, with any database it was written with stripped off.
    pub(crate) fn table(&self) -> &str {
        self.split().1
    }

    /// The database, whether it came in its own field or qualified the table.
    pub(crate) fn database(&self) -> Option<&str> {
        self.split().0
    }

    /// A table written `database.table` is read as both.
    ///
    /// The map the model is shown, and the lookup tool it calls, both address
    /// a table as `database.table` - so it writes the qualified name in the
    /// table field, and refusing that only spends a round trip teaching it a
    /// distinction the wire format makes and nothing else does. Split only on
    /// a single dot with an identifier either side; anything else stays whole
    /// and is refused by the identifier check as before.
    fn split(&self) -> (Option<&str>, &str) {
        if self.database.is_some() {
            return (self.database.as_deref(), &self.table);
        }

        match self.table.split_once('.') {
            Some((database, table)) if is_identifier(database) && is_identifier(table) => {
                (Some(database), table)
            }
            _ => (None, &self.table),
        }
    }

    /// The table as the query addressed it, database and all.
    pub(crate) fn qualified(&self) -> String {
        match self.split() {
            (Some(database), table) => format!("{database}.{table}"),
            (None, table) => table.to_owned(),
        }
    }

    /// The columns a result carries, in order — each field's `as_name`. What
    /// a widget must name to draw anything.
    pub(crate) fn column_names(&self) -> Vec<String> {
        self.fields
            .iter()
            .map(|field| field.as_name.clone())
            .collect()
    }

    /// Each field as it is selected, with whatever joining in a person's
    /// name takes with it.
    fn selection(&self, qualifier: Option<&str>) -> Result<Selection<'_>, MetricQueryError> {
        let mut selection = Selection {
            parts: Vec::with_capacity(self.fields.len()),
            as_names: HashSet::with_capacity(self.fields.len()),
            column_types: HashMap::with_capacity(self.fields.len()),
            joins: String::new(),
            handles: Vec::new(),
        };

        for (index, field) in self.fields.iter().enumerate() {
            if !is_identifier(&field.as_name) {
                return Err(MetricQueryError::Identifier(field.as_name.clone()));
            }
            selection.as_names.insert(field.as_name.as_str());
            selection
                .column_types
                .insert(field.as_name.clone(), field.r#type);

            let expression = match field.person {
                Some(handle) => {
                    if !selection.handles.contains(&handle) {
                        selection.handles.push(handle);
                    }
                    let read = field.source()?.sql(field.r#type, qualifier)?;
                    let alias = format!("__p{index}");
                    let _ = write!(
                        selection.joins,
                        " LEFT JOIN {} AS `{alias}` ON `{alias}`.`handle` = {}",
                        handle.cte(),
                        handle.key(&read)
                    );
                    // Identity knows nobody by this handle: show what the
                    // table itself says, never a blank.
                    field.aggregated(format!(
                        "coalesce(nullIf(`{alias}`.`display_name`, ''), {read})"
                    ))
                }
                None => field.expression(qualifier)?,
            };
            selection
                .parts
                .push(format!("{expression} AS `{}`", field.as_name));
        }

        Ok(selection)
    }

    pub(crate) fn compile(&self, people: &People) -> Result<CompiledQuery, MetricQueryError> {
        let (database, table) = self.split();
        if !is_identifier(table) {
            return Err(MetricQueryError::Identifier(self.table.clone()));
        }
        if let Some(database) = self.database()
            && !is_identifier(database)
        {
            return Err(MetricQueryError::Identifier(database.to_owned()));
        }
        if self.fields.is_empty() {
            return Err(MetricQueryError::NoFields);
        }

        // Resolving a name joins another table in, and then a bare column
        // could mean either side - so every read carries the fact table's
        // alias exactly when there is something to be ambiguous with.
        let qualifier = self
            .fields
            .iter()
            .any(|field| field.person.is_some())
            .then_some(FACT_ALIAS);
        let Selection {
            parts: select_parts,
            as_names,
            column_types,
            joins,
            handles,
        } = self.selection(qualifier)?;

        for group in &self.group_by {
            if !is_identifier(group) {
                return Err(MetricQueryError::Identifier(group.clone()));
            }
            if !as_names.contains(group.as_str()) {
                return Err(MetricQueryError::GroupBy(group.clone()));
            }
        }

        let mut where_parts = Vec::with_capacity(self.filters.len());
        let mut binds = Vec::with_capacity(self.filters.len());
        for filter in &self.filters {
            let source = filter.source()?;
            let read = source.sql(filter.r#type, qualifier)?;
            where_parts.push(format!("{read} {} ?", filter.op.sql()));
            binds.push(filter.bind(source)?);
        }

        let mut from = match database {
            Some(database) => format!("`{database}`.`{table}`"),
            None => format!("`{table}`"),
        };
        if let Some(alias) = qualifier {
            let _ = write!(from, " AS `{alias}`");
        }
        let prelude = if handles.is_empty() {
            String::new()
        } else {
            people.prelude(&handles)?
        };
        let mut sql = format!(
            "{prelude}SELECT {} FROM {from}{joins}",
            select_parts.join(", ")
        );
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

            if self.order_by.is_none() {
                sql.push_str(" ORDER BY ");
                sql.push_str(&backticked.join(", "));
            }
        }
        if let Some(order) = &self.order_by {
            if !as_names.contains(order.field.as_str()) {
                return Err(MetricQueryError::OrderBy(order.field.clone()));
            }
            let _ = write!(sql, " ORDER BY `{}` {}", order.field, order.direction.sql());
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
    people: People,
}

impl MetricRunner {
    pub(crate) fn new(client: insight_clickhouse::Client, people: People) -> Self {
        Self {
            client,
            fetch_timeout: Duration::from_secs(FETCH_TIMEOUT_SECS),
            people,
        }
    }

    /// Where the queries this runs resolve a person's name from.
    pub(crate) fn people(&self) -> &People {
        &self.people
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

    fn people() -> People {
        People::new("identity")
    }

    fn merged_by_author(person: &str) -> MetricQuery {
        query(json!({
            "table": "silver.class_git_pull_requests",
            "fields": [
                { "column": "author_email", "type": "string", "as_name": "author", "person": person },
                { "agg": "count", "column": "pr_id", "type": "int", "as_name": "merged" }
            ],
            "group_by": ["author"],
            "filters": [{ "column": "state", "type": "string", "op": "eq", "value": "MERGED" }],
            "order_by": { "field": "merged", "direction": "desc" }
        }))
    }

    #[test]
    fn a_person_column_selects_the_name_identity_knows() {
        let compiled = merged_by_author("email")
            .compile(&people())
            .unwrap_or_else(|error| panic!("compiles: {error}"));

        assert!(
            compiled.sql.contains(
                "coalesce(nullIf(`__p0`.`display_name`, ''), `__f`.`author_email`) AS `author`"
            ),
            "{}",
            compiled.sql
        );
        assert!(
            compiled.sql.contains(
                "LEFT JOIN __people_by_email AS `__p0` ON `__p0`.`handle` = `__f`.`author_email`"
            ),
            "{}",
            compiled.sql
        );
        assert!(
            compiled
                .sql
                .contains("FROM `silver`.`class_git_pull_requests` AS `__f`"),
            "{}",
            compiled.sql
        );
    }

    #[test]
    fn the_latest_name_wins_before_anything_joins_to_it() {
        // A person carries a row per name they have ever had. Joining those
        // rows directly multiplies every fact by that history.
        let compiled = merged_by_author("email")
            .compile(&people())
            .unwrap_or_else(|error| panic!("compiles: {error}"));

        assert!(
            compiled.sql.starts_with(
                "WITH __person_name AS (SELECT `person_id`, argMax(`value_effective`, `created_at`) AS `display_name` FROM `identity`.`identity_persons` WHERE `value_type` = 'display_name' GROUP BY `person_id`)"
            ),
            "{}",
            compiled.sql
        );
    }

    #[test]
    fn resolving_a_name_qualifies_every_other_read() {
        let compiled = merged_by_author("email")
            .compile(&people())
            .unwrap_or_else(|error| panic!("compiles: {error}"));

        assert!(
            compiled.sql.contains("count(`__f`.`pr_id`) AS `merged`"),
            "{}",
            compiled.sql
        );
        assert!(
            compiled.sql.contains("WHERE `__f`.`state` = ?"),
            "{}",
            compiled.sql
        );
        assert_eq!(compiled.binds, vec!["MERGED".to_owned()]);
    }

    #[test]
    fn a_query_that_names_no_person_joins_nothing() {
        let compiled = query(json!({
            "table": "silver.class_git_pull_requests",
            "fields": [{ "column": "author_email", "type": "string", "as_name": "author" }],
            "group_by": ["author"],
            "filters": []
        }))
        .compile(&people())
        .unwrap_or_else(|error| panic!("compiles: {error}"));

        assert!(!compiled.sql.contains("WITH "), "{}", compiled.sql);
        assert!(!compiled.sql.contains("JOIN"), "{}", compiled.sql);
        assert!(!compiled.sql.contains("__f"), "{}", compiled.sql);
    }

    #[test]
    fn a_person_id_is_compared_as_text() {
        // The same column is a UUID in one table and a String in the next.
        let compiled = query(json!({
            "table": "silver.class_git_pull_requests",
            "fields": [
                { "column": "author_person_id", "type": "string", "as_name": "author", "person": "id" },
                { "agg": "count", "type": "int", "as_name": "prs" }
            ],
            "group_by": ["author"],
            "filters": []
        }))
        .compile(&people())
        .unwrap_or_else(|error| panic!("compiles: {error}"));

        assert!(
            compiled.sql.contains(
                "LEFT JOIN __people_by_id AS `__p0` ON `__p0`.`handle` = toString(`__f`.`author_person_id`)"
            ),
            "{}",
            compiled.sql
        );
        assert!(
            compiled
                .sql
                .contains("__people_by_id AS (SELECT toString(`person_id`) AS `handle`"),
            "{}",
            compiled.sql
        );
    }

    #[test]
    fn two_person_columns_resolve_one_each() {
        let compiled = query(json!({
            "table": "silver.class_git_pr_review_events",
            "fields": [
                { "column": "author_email", "type": "string", "as_name": "author", "person": "email" },
                { "column": "actor_person_id", "type": "string", "as_name": "reviewer", "person": "id" },
                { "agg": "count", "type": "int", "as_name": "reviews" }
            ],
            "group_by": ["author", "reviewer"],
            "filters": []
        }))
        .compile(&people())
        .unwrap_or_else(|error| panic!("compiles: {error}"));

        assert!(compiled.sql.contains("AS `__p0`"), "{}", compiled.sql);
        assert!(compiled.sql.contains("AS `__p1`"), "{}", compiled.sql);
        assert_eq!(
            compiled.sql.matches("LEFT JOIN").count(),
            2,
            "{}",
            compiled.sql
        );
    }

    #[test]
    fn a_person_inside_the_payload_resolves_the_same_way() {
        let compiled = query(json!({
            "table": "events",
            "fields": [
                { "json": "author", "type": "string", "as_name": "author", "person": "email" },
                { "agg": "sum", "json": "lines", "type": "int", "as_name": "lines" }
            ],
            "group_by": ["author"],
            "filters": []
        }))
        .compile(&people())
        .unwrap_or_else(|error| panic!("compiles: {error}"));

        assert!(
            compiled
                .sql
                .contains("ON `__p0`.`handle` = JSONExtractString(`__f`.raw_data, 'author')"),
            "{}",
            compiled.sql
        );
    }

    #[test]
    fn the_identity_database_is_whatever_the_stand_configured() {
        let compiled = merged_by_author("email")
            .compile(&People::new("identity_two"))
            .unwrap_or_else(|error| panic!("compiles: {error}"));

        assert!(
            compiled.sql.contains("`identity_two`.`identity_persons`"),
            "{}",
            compiled.sql
        );
    }

    #[test]
    fn an_identity_database_outside_the_charset_is_refused() {
        assert!(matches!(
            merged_by_author("email").compile(&People::new("identity`; DROP TABLE x; --")),
            Err(MetricQueryError::Identifier(_))
        ));
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
            .compile(&people())
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
            .compile(&people())
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
            metric.compile(&people()),
            Err(MetricQueryError::Identifier(_))
        ));
    }

    #[test]
    fn ordering_by_a_selected_value_beats_the_grouping_order() {
        // "Which author changed the most lines" is unanswerable without
        // this: ordering by the grouped column returns whoever sorts first
        // alphabetically, and the reply presents it as the largest.
        let metric = query(json!({
            "table": "events",
            "fields": [
                { "json": "author", "type": "string", "as_name": "author" },
                { "json": "lines", "type": "int", "agg": "sum", "as_name": "total_lines" }
            ],
            "group_by": ["author"],
            "filters": [],
            "order_by": { "field": "total_lines", "direction": "desc" },
            "limit": 1
        }));

        let compiled = metric
            .compile(&people())
            .unwrap_or_else(|error| panic!("the query compiles: {error}"));

        assert!(
            compiled.sql.contains("ORDER BY `total_lines` DESC"),
            "{}",
            compiled.sql
        );
        // One ORDER BY, not the grouping's as well.
        assert_eq!(
            compiled.sql.matches("ORDER BY").count(),
            1,
            "{}",
            compiled.sql
        );
        assert!(compiled.sql.ends_with(" LIMIT 1"), "{}", compiled.sql);
    }

    #[test]
    fn ordering_defaults_to_ascending() {
        let metric = query(json!({
            "table": "events",
            "fields": [{ "json": "day", "type": "string", "as_name": "day" }],
            "group_by": [],
            "filters": [],
            "order_by": { "field": "day" }
        }));

        let compiled = metric
            .compile(&people())
            .unwrap_or_else(|error| panic!("the query compiles: {error}"));

        assert!(
            compiled.sql.contains("ORDER BY `day` ASC"),
            "{}",
            compiled.sql
        );
    }

    #[test]
    fn ordering_by_a_column_the_query_does_not_select_is_refused() {
        let metric = query(json!({
            "table": "events",
            "fields": [{ "json": "day", "type": "string", "as_name": "day" }],
            "group_by": [],
            "filters": [],
            "order_by": { "field": "lines" }
        }));

        assert!(matches!(
            metric.compile(&people()),
            Err(MetricQueryError::OrderBy(_))
        ));
    }

    #[test]
    fn counting_rows_needs_no_column() {
        // "How many rows are in this table" is the first thing anyone asks,
        // and it reads no value.
        let metric = query(json!({
            "table": "bronze_github.commits",
            "fields": [{ "agg": "count", "type": "int", "as_name": "rows" }],
            "group_by": [],
            "filters": []
        }));

        let compiled = metric
            .compile(&people())
            .unwrap_or_else(|error| panic!("the query compiles: {error}"));

        assert!(
            compiled.sql.contains("count() AS `rows`"),
            "{}",
            compiled.sql
        );
    }

    #[test]
    fn counting_one_column_still_names_it() {
        let metric = query(json!({
            "table": "events",
            "fields": [{ "agg": "count", "column": "author", "type": "string", "as_name": "n" }],
            "group_by": [],
            "filters": []
        }));

        let compiled = metric
            .compile(&people())
            .unwrap_or_else(|error| panic!("the query compiles: {error}"));

        assert!(
            compiled.sql.contains("count(`author`) AS `n`"),
            "{}",
            compiled.sql
        );
    }

    #[test]
    fn an_aggregate_that_is_not_count_still_needs_a_source() {
        // sum() of nothing is not a question.
        let metric = query(json!({
            "table": "events",
            "fields": [{ "agg": "sum", "type": "int", "as_name": "total" }],
            "group_by": [],
            "filters": []
        }));

        assert!(matches!(
            metric.compile(&people()),
            Err(MetricQueryError::FieldSource(_))
        ));
    }

    #[test]
    fn a_table_written_with_its_database_is_read_as_both() {
        // The map and the lookup tool both address a table as
        // `database.table`, so the model writes it that way.
        let metric = query(json!({
            "table": "bronze_github.commits",
            "fields": [{ "column": "sha", "type": "string", "as_name": "sha" }],
            "group_by": [],
            "filters": []
        }));

        let compiled = metric
            .compile(&people())
            .unwrap_or_else(|error| panic!("the query compiles: {error}"));

        assert!(
            compiled.sql.contains("FROM `bronze_github`.`commits`"),
            "{}",
            compiled.sql
        );
        assert_eq!(metric.database(), Some("bronze_github"));
        assert_eq!(metric.table(), "commits");
    }

    #[test]
    fn a_database_field_wins_over_a_qualified_table() {
        let metric = query(json!({
            "database": "silver",
            "table": "class_git_commits",
            "fields": [{ "column": "sha", "type": "string", "as_name": "sha" }],
            "group_by": [],
            "filters": []
        }));

        assert_eq!(metric.database(), Some("silver"));
        assert_eq!(metric.table(), "class_git_commits");
    }

    #[test]
    fn a_table_with_two_dots_is_still_refused() {
        // Splitting only rescues the one shape the model writes; anything
        // else stays whole and fails the identifier check.
        let metric = query(json!({
            "table": "a.b.c",
            "fields": [{ "column": "x", "type": "string", "as_name": "x" }],
            "group_by": [],
            "filters": []
        }));

        assert!(matches!(
            metric.compile(&people()),
            Err(MetricQueryError::Identifier(_))
        ));
    }

    #[test]
    fn a_metric_with_no_fields_is_refused() {
        let metric = query(json!({
            "table": "events", "fields": [], "group_by": [], "filters": []
        }));

        assert!(matches!(
            metric.compile(&people()),
            Err(MetricQueryError::NoFields)
        ));
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
            .compile(&people())
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
            metric.compile(&people()),
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
            metric.compile(&people()),
            Err(MetricQueryError::GroupBy(_))
        ));
    }

    #[test]
    fn a_database_qualifies_the_table() {
        let metric = query(json!({
            "database": "silver",
            "table": "git_commits",
            "fields": [{ "column": "author", "type": "string", "as_name": "author" }]
        }));

        assert_eq!(metric.database(), Some("silver"));

        let compiled = metric
            .compile(&people())
            .unwrap_or_else(|error| panic!("compiles: {error}"));

        assert!(
            compiled.sql.contains("FROM `silver`.`git_commits`"),
            "{}",
            compiled.sql
        );
    }

    #[test]
    fn a_query_without_a_database_still_reads_the_bare_table() {
        let metric = query(json!({
            "table": "events",
            "fields": [{ "json": "day", "type": "string", "as_name": "day" }]
        }));

        assert_eq!(metric.database(), None);

        let compiled = metric
            .compile(&people())
            .unwrap_or_else(|error| panic!("compiles: {error}"));

        assert!(compiled.sql.contains("FROM `events`"), "{}", compiled.sql);
    }

    #[test]
    fn a_field_naming_a_column_reads_it_without_json_extraction() {
        let metric = query(json!({
            "database": "silver",
            "table": "git_commits",
            "fields": [{ "column": "lines_changed", "type": "int", "as_name": "lines" }]
        }));

        let compiled = metric
            .compile(&people())
            .unwrap_or_else(|error| panic!("compiles: {error}"));

        assert!(
            compiled.sql.contains("`lines_changed` AS `lines`"),
            "{}",
            compiled.sql
        );
        assert!(!compiled.sql.contains("JSONExtract"), "{}", compiled.sql);
    }

    #[test]
    fn an_aggregate_wraps_a_column_as_it_wraps_an_extraction() {
        let metric = query(json!({
            "database": "silver",
            "table": "git_commits",
            "fields": [
                { "column": "lines_changed", "type": "int", "agg": "sum", "as_name": "total" }
            ]
        }));

        let compiled = metric
            .compile(&people())
            .unwrap_or_else(|error| panic!("compiles: {error}"));

        assert!(
            compiled.sql.contains("sum(`lines_changed`) AS `total`"),
            "{}",
            compiled.sql
        );
    }

    #[test]
    fn a_filter_on_a_column_compares_it_and_still_binds_the_value() {
        let metric = query(json!({
            "database": "silver",
            "table": "git_commits",
            "fields": [{ "column": "author", "type": "string", "as_name": "author" }],
            "filters": [
                { "column": "event", "type": "string", "op": "eq", "value": "commit" }
            ]
        }));

        let compiled = metric
            .compile(&people())
            .unwrap_or_else(|error| panic!("compiles: {error}"));

        assert!(
            compiled.sql.contains("WHERE `event` = ?"),
            "{}",
            compiled.sql
        );
        assert_eq!(compiled.binds, vec!["commit".to_owned()]);
    }

    #[test]
    fn a_field_naming_both_a_json_key_and_a_column_is_refused() {
        let metric = query(json!({
            "table": "git_commits",
            "fields": [
                { "json": "lines", "column": "lines_changed", "type": "int", "as_name": "lines" }
            ]
        }));

        assert!(matches!(
            metric.compile(&people()),
            Err(MetricQueryError::FieldSource(_))
        ));
    }

    #[test]
    fn a_field_naming_neither_a_json_key_nor_a_column_is_refused() {
        let metric = query(json!({
            "table": "git_commits",
            "fields": [{ "type": "int", "as_name": "lines" }]
        }));

        assert!(matches!(
            metric.compile(&people()),
            Err(MetricQueryError::FieldSource(_))
        ));
    }

    #[test]
    fn a_filter_naming_neither_a_json_key_nor_a_column_is_refused() {
        let metric = query(json!({
            "table": "git_commits",
            "fields": [{ "column": "author", "type": "string", "as_name": "author" }],
            "filters": [{ "type": "string", "op": "eq", "value": "commit" }]
        }));

        assert!(matches!(
            metric.compile(&people()),
            Err(MetricQueryError::FieldSource(_))
        ));
    }

    #[test]
    fn a_database_outside_the_identifier_charset_is_refused() {
        let metric = query(json!({
            "database": "silver`; DROP TABLE git_commits; --",
            "table": "git_commits",
            "fields": [{ "json": "author", "type": "string", "as_name": "author" }]
        }));

        assert!(matches!(
            metric.compile(&people()),
            Err(MetricQueryError::Identifier(_))
        ));
    }
}
