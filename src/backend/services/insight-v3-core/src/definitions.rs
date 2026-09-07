use std::fmt;
use std::time::Duration;

use chrono::{DateTime, Utc};
use clickhouse::sql::Identifier;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

const MAX_NAME_CHARS: usize = 128;
const WRITE_TIMEOUT_SECS: u64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DefinitionKind {
    Metric,
    Widget,
    Dashboard,
}

impl DefinitionKind {
    pub(crate) fn table(self) -> &'static str {
        match self {
            Self::Metric => "metrics",
            Self::Widget => "widgets",
            Self::Dashboard => "dashboards",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DefinitionName(String);

impl DefinitionName {
    pub(crate) fn parse(value: &str) -> Result<Self, DefinitionError> {
        if value.is_empty() || value.chars().count() > MAX_NAME_CHARS {
            return Err(DefinitionError::Name);
        }

        if !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(DefinitionError::Name);
        }

        Ok(Self(value.to_owned()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Serialize, Deserialize, clickhouse::Row)]
pub(crate) struct DefinitionRow {
    #[serde(with = "clickhouse::serde::uuid")]
    pub(crate) id: Uuid,
    pub(crate) name: String,
    pub(crate) body: String,
    #[serde(with = "clickhouse::serde::chrono::datetime64::millis")]
    pub(crate) updated_at: DateTime<Utc>,
}

pub(crate) struct DefinitionStore {
    client: insight_clickhouse::Client,
    timeout: Duration,
}

impl DefinitionStore {
    pub(crate) fn new(client: insight_clickhouse::Client) -> Self {
        Self {
            client,
            timeout: Duration::from_secs(WRITE_TIMEOUT_SECS),
        }
    }

    pub(crate) async fn put(
        &self,
        kind: DefinitionKind,
        name: &DefinitionName,
        body: &serde_json::Value,
    ) -> Result<(), DefinitionStoreError> {
        let row = DefinitionRow {
            id: Uuid::now_v7(),
            name: name.as_str().to_owned(),
            body: serde_json::to_string(body)?,
            updated_at: Utc::now(),
        };

        let write = async {
            let mut inserter = self
                .client
                .inner()
                .insert::<DefinitionRow>(kind.table())
                .await?;
            inserter.write(&row).await?;
            inserter.end().await
        };

        tokio::time::timeout(self.timeout, write)
            .await
            .map_err(|_| DefinitionStoreError::Timeout)??;

        Ok(())
    }

    pub(crate) async fn get(
        &self,
        kind: DefinitionKind,
        name: &DefinitionName,
    ) -> Result<Option<serde_json::Value>, DefinitionStoreError> {
        let rows = self
            .client
            .inner()
            .query("SELECT id, name, body, updated_at FROM ? FINAL WHERE name = ? LIMIT 1")
            .bind(Identifier(kind.table()))
            .bind(name.as_str())
            .fetch_all::<DefinitionRow>()
            .await?;

        match rows.into_iter().next() {
            Some(row) => Ok(Some(serde_json::from_str(&row.body)?)),
            None => Ok(None),
        }
    }

    pub(crate) async fn list(
        &self,
        kind: DefinitionKind,
    ) -> Result<Vec<String>, DefinitionStoreError> {
        Ok(self
            .client
            .inner()
            .query("SELECT DISTINCT name FROM ? FINAL ORDER BY name")
            .bind(Identifier(kind.table()))
            .fetch_all::<String>()
            .await?)
    }
}

impl fmt::Debug for DefinitionStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DefinitionStore")
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, Error)]
pub(crate) enum DefinitionError {
    #[error("definition names use letters, digits, underscore and dash, up to 128 characters")]
    Name,
}

#[derive(Debug, Error)]
pub(crate) enum DefinitionStoreError {
    #[error("the definition store timed out")]
    Timeout,
    #[error("definition store operation failed")]
    ClickHouse(#[source] clickhouse::error::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl From<clickhouse::error::Error> for DefinitionStoreError {
    fn from(error: clickhouse::error::Error) -> Self {
        match error {
            clickhouse::error::Error::TimedOut => Self::Timeout,
            error => Self::ClickHouse(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use clickhouse::test::{Mock, handlers};
    use serde_json::json;

    use super::*;

    fn client(mock: &Mock) -> insight_clickhouse::Client {
        insight_clickhouse::Client::new(insight_clickhouse::Config::new(mock.url(), "insight"))
    }

    #[test]
    fn names_reject_anything_outside_the_identifier_charset() {
        assert!(DefinitionName::parse("commits_per_day").is_ok());
        assert!(DefinitionName::parse("").is_err());
        assert!(DefinitionName::parse("drop table").is_err());
        assert!(DefinitionName::parse("a`b").is_err());
        assert!(DefinitionName::parse(&"a".repeat(129)).is_err());
    }

    #[test]
    fn each_kind_has_its_own_table() {
        assert_eq!(DefinitionKind::Metric.table(), "metrics");
        assert_eq!(DefinitionKind::Widget.table(), "widgets");
        assert_eq!(DefinitionKind::Dashboard.table(), "dashboards");
    }

    #[tokio::test]
    async fn put_writes_the_body_into_the_kind_table() {
        let mock = Mock::new();
        let recording = mock.add(handlers::record::<DefinitionRow>());
        let store = DefinitionStore::new(client(&mock));
        let name = DefinitionName::parse("commits_per_day")
            .unwrap_or_else(|error| panic!("name must parse: {error}"));

        store
            .put(DefinitionKind::Metric, &name, &json!({ "table": "events" }))
            .await
            .unwrap_or_else(|error| panic!("put must succeed: {error}"));

        let rows: Vec<DefinitionRow> = recording.collect().await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "commits_per_day");
        assert_eq!(rows[0].body, r#"{"table":"events"}"#);
    }

    #[tokio::test]
    async fn get_returns_the_stored_body_for_a_matching_row() {
        let mock = Mock::new();
        mock.add(handlers::provide(vec![DefinitionRow {
            id: Uuid::now_v7(),
            name: "commits_per_day".to_owned(),
            body: r#"{"table":"events"}"#.to_owned(),
            updated_at: Utc::now(),
        }]));
        let store = DefinitionStore::new(client(&mock));
        let name = DefinitionName::parse("commits_per_day")
            .unwrap_or_else(|error| panic!("name must parse: {error}"));

        let body = store
            .get(DefinitionKind::Metric, &name)
            .await
            .unwrap_or_else(|error| panic!("get must succeed: {error}"));

        assert_eq!(body, Some(json!({ "table": "events" })));
    }

    #[tokio::test]
    async fn get_returns_none_when_no_row_matches() {
        let mock = Mock::new();
        mock.add(handlers::provide(Vec::<DefinitionRow>::new()));
        let store = DefinitionStore::new(client(&mock));
        let name = DefinitionName::parse("nope")
            .unwrap_or_else(|error| panic!("name must parse: {error}"));

        let body = store
            .get(DefinitionKind::Metric, &name)
            .await
            .unwrap_or_else(|error| panic!("get must succeed: {error}"));

        assert_eq!(body, None);
    }

    #[tokio::test]
    async fn get_reads_final_with_the_bound_name_and_kind_table() {
        let mock = Mock::new();
        let recording = mock.add(handlers::record_ddl());
        let store = DefinitionStore::new(client(&mock));
        let name = DefinitionName::parse("commits_per_day")
            .unwrap_or_else(|error| panic!("name must parse: {error}"));

        store
            .get(DefinitionKind::Widget, &name)
            .await
            .unwrap_or_else(|error| panic!("get must succeed: {error}"));
        let sql = recording.query().await;

        assert!(sql.contains("FROM `widgets` FINAL"), "sql was: {sql}");
        assert!(sql.contains("WHERE name = "), "sql was: {sql}");
        assert!(sql.contains("commits_per_day"), "sql was: {sql}");
    }

    #[tokio::test]
    async fn list_returns_the_names_from_the_response() {
        let mock = Mock::new();
        mock.add(handlers::provide(vec!["a".to_owned(), "b".to_owned()]));
        let store = DefinitionStore::new(client(&mock));

        let names = store
            .list(DefinitionKind::Dashboard)
            .await
            .unwrap_or_else(|error| panic!("list must succeed: {error}"));

        assert_eq!(names, vec!["a".to_owned(), "b".to_owned()]);
    }

    #[tokio::test]
    async fn list_selects_distinct_names_ordered_from_the_kind_table() {
        let mock = Mock::new();
        let recording = mock.add(handlers::record_ddl());
        let store = DefinitionStore::new(client(&mock));

        store
            .list(DefinitionKind::Dashboard)
            .await
            .unwrap_or_else(|error| panic!("list must succeed: {error}"));
        let sql = recording.query().await;

        assert_eq!(
            sql,
            "SELECT DISTINCT name FROM `dashboards` FINAL ORDER BY name"
        );
    }

    #[test]
    fn clickhouse_timeout_is_normalized_to_definition_store_timeout() {
        let error = DefinitionStoreError::from(clickhouse::error::Error::TimedOut);

        assert!(matches!(error, DefinitionStoreError::Timeout));
    }
}
