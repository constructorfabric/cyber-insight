use std::fmt;
use std::time::Duration;

use clickhouse::sql::Identifier;
use thiserror::Error;

pub(crate) const MAX_TABLE_NAME_CHARS: usize = 128;
const TABLE_CREATE_TIMEOUT_SECS: u64 = 35;
const CREATE_TABLE: &str = "CREATE TABLE IF NOT EXISTS ? (
    id UUID,
    table_name String,
    raw_data String,
    received_at DateTime64(3, 'UTC')
)
ENGINE = MergeTree
ORDER BY (table_name, received_at, id)";
/// The ingest tables, told apart from every other service's tables in the
/// same database by the ORDER BY that `CREATE_TABLE` above gives them.
const LIST_TABLES: &str = "SELECT name FROM system.tables
WHERE database = currentDatabase()
  AND sorting_key = 'table_name, received_at, id'
ORDER BY name";

#[derive(Debug, Clone)]
pub(crate) struct TableName(String);

impl TableName {
    pub(crate) fn parse(value: &str) -> Result<Self, TableError> {
        if value.is_empty() {
            return Err(TableError::EmptyName);
        }
        if value.chars().count() > MAX_TABLE_NAME_CHARS {
            return Err(TableError::NameTooLong);
        }

        let mut bytes = value.bytes();
        let Some(first) = bytes.next() else {
            return Err(TableError::EmptyName);
        };
        if !(first.is_ascii_alphabetic() || first == b'_')
            || !bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(TableError::InvalidName);
        }

        Ok(Self(value.to_owned()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn into_string(self) -> String {
        self.0
    }
}

pub(crate) struct TableStore {
    client: insight_clickhouse::Client,
    timeout: Duration,
}

impl TableStore {
    pub(crate) fn new(client: insight_clickhouse::Client) -> Self {
        Self {
            client,
            timeout: Duration::from_secs(TABLE_CREATE_TIMEOUT_SECS),
        }
    }

    pub(crate) async fn create(&self, table: &TableName) -> Result<(), TableStoreError> {
        let query = self
            .client
            .inner()
            .query(CREATE_TABLE)
            .bind(Identifier(table.as_str()));

        tokio::time::timeout(self.timeout, query.execute())
            .await
            .map_err(|_| TableStoreError::Timeout)??;

        Ok(())
    }

    /// Every table data has been ingested into.
    pub(crate) async fn list(&self) -> Result<Vec<String>, TableStoreError> {
        Ok(self
            .client
            .inner()
            .query(LIST_TABLES)
            .fetch_all::<String>()
            .await?)
    }

    /// Field names and inferred types, read from the most recent rows.
    pub(crate) async fn sample_fields(
        &self,
        table: &TableName,
    ) -> Result<Vec<(String, &'static str)>, TableStoreError> {
        let sql = format!(
            "SELECT raw_data FROM `{}` ORDER BY received_at DESC LIMIT 20",
            table.as_str()
        );

        let payloads = self
            .client
            .inner()
            .query(&sql)
            .fetch_all::<String>()
            .await?;
        let mut fields: Vec<(String, &'static str)> = Vec::new();

        for payload in payloads {
            let Ok(serde_json::Value::Object(map)) = serde_json::from_str(&payload) else {
                continue;
            };

            for (key, value) in map {
                if fields.iter().any(|(name, _)| name == &key) {
                    continue;
                }

                let kind = match value {
                    serde_json::Value::Number(number) if number.is_i64() => "int",
                    serde_json::Value::Number(_) => "float",
                    _ => "string",
                };

                fields.push((key, kind));
            }
        }

        Ok(fields)
    }
}

impl fmt::Debug for TableStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TableStore")
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, Error)]
pub(crate) enum TableError {
    #[error("table must not be blank")]
    EmptyName,
    #[error("table must be at most {MAX_TABLE_NAME_CHARS} characters")]
    NameTooLong,
    #[error(
        "table must start with an ASCII letter or underscore and contain only ASCII letters, digits, or underscores"
    )]
    InvalidName,
}

#[derive(Debug, Error)]
pub(crate) enum TableStoreError {
    #[error("table creation timed out")]
    Timeout,
    #[error("table creation failed")]
    ClickHouse(#[from] clickhouse::error::Error),
}

#[cfg(test)]
mod tests {
    use clickhouse::test::{Mock, handlers};

    use super::*;

    #[test]
    fn physical_table_names_use_portable_identifiers() {
        assert!(TableName::parse("events_2026").is_ok());

        for value in ["", " events", "events.daily", "9events", "naïve"] {
            assert!(
                TableName::parse(value).is_err(),
                "should reject table name: {value:?}"
            );
        }
    }

    #[tokio::test]
    async fn table_creation_uses_the_fixed_raw_data_schema() {
        let mock = Mock::new();
        let recording = mock.add(handlers::record_ddl());
        let client =
            insight_clickhouse::Client::new(insight_clickhouse::Config::new(mock.url(), "insight"));
        let store = TableStore::new(client);
        let table = TableName::parse("events_2026").unwrap_or_else(|error| panic!("name: {error}"));

        store
            .create(&table)
            .await
            .unwrap_or_else(|error| panic!("table should be created: {error}"));
        let query = recording.query().await;

        assert!(query.contains("CREATE TABLE IF NOT EXISTS `events_2026`"));
        assert!(query.contains("id UUID"));
        assert!(query.contains("table_name String"));
        assert!(query.contains("raw_data String"));
        assert!(query.contains("received_at DateTime64(3, 'UTC')"));
        assert!(query.contains("ORDER BY (table_name, received_at, id)"));
    }

    #[test]
    fn listing_selects_only_tables_shaped_like_an_ingest_table() {
        // The database holds tables from every other service too, so the
        // ingest schema's own ORDER BY is what separates ours from theirs.
        assert!(LIST_TABLES.contains("system.tables"));
        assert!(LIST_TABLES.contains("database = currentDatabase()"));
        assert!(LIST_TABLES.contains("table_name, received_at, id"));
    }

    #[tokio::test]
    async fn listing_returns_the_table_names() {
        let mock = Mock::new();
        mock.add(handlers::provide(vec![
            "events".to_owned(),
            "raw_data".to_owned(),
        ]));
        let client =
            insight_clickhouse::Client::new(insight_clickhouse::Config::new(mock.url(), "insight"));
        let store = TableStore::new(client);

        let names = store
            .list()
            .await
            .unwrap_or_else(|error| panic!("tables should list: {error}"));

        assert_eq!(names, vec!["events".to_owned(), "raw_data".to_owned()]);
    }

    #[tokio::test]
    async fn overlapping_rows_yield_one_entry_per_field_typed_from_its_value() {
        let mock = Mock::new();
        mock.add(handlers::provide(vec![
            r#"{"day":"2026-09-01","lines":59}"#.to_owned(),
            r#"{"day":"2026-09-02","author":"nda"}"#.to_owned(),
        ]));
        let client =
            insight_clickhouse::Client::new(insight_clickhouse::Config::new(mock.url(), "insight"));
        let store = TableStore::new(client);
        let table = TableName::parse("events_2026").unwrap_or_else(|error| panic!("name: {error}"));

        let fields = store
            .sample_fields(&table)
            .await
            .unwrap_or_else(|error| panic!("fields should be sampled: {error}"));

        assert_eq!(
            fields,
            vec![
                ("day".to_owned(), "string"),
                ("lines".to_owned(), "int"),
                ("author".to_owned(), "string"),
            ]
        );
    }
}
