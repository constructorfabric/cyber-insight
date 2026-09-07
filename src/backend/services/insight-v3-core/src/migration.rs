use thiserror::Error;

const CREATE_RAW_DATA_TABLE: &str = "CREATE TABLE IF NOT EXISTS raw_data (
    id UUID,
    table_name String,
    raw_data String,
    received_at DateTime64(3, 'UTC')
)
ENGINE = MergeTree
ORDER BY (table_name, received_at, id)";

const CREATE_DEFINITION_TABLE: &str = "CREATE TABLE IF NOT EXISTS {table} (
    id UUID,
    name String,
    body String,
    updated_at DateTime64(3, 'UTC')
)
ENGINE = ReplacingMergeTree(updated_at)
ORDER BY name";

const DEFINITION_TABLES: [&str; 3] = ["metrics", "widgets", "dashboards"];

pub(crate) async fn migrate(client: &insight_clickhouse::Client) -> Result<(), MigrationError> {
    client
        .inner()
        .query(CREATE_RAW_DATA_TABLE)
        .execute()
        .await?;

    for table in DEFINITION_TABLES {
        let ddl = CREATE_DEFINITION_TABLE.replace("{table}", table);
        client.inner().query(&ddl).execute().await?;
    }

    Ok(())
}

#[derive(Debug, Error)]
#[error("failed to migrate the raw_data table")]
pub(crate) struct MigrationError(#[from] clickhouse::error::Error);

#[cfg(test)]
mod tests {
    use clickhouse::test::{Mock, handlers};

    use super::*;

    #[tokio::test]
    async fn migration_creates_the_raw_data_table() {
        let mock = Mock::new();
        let recording = mock.add(handlers::record_ddl());
        let client =
            insight_clickhouse::Client::new(insight_clickhouse::Config::new(mock.url(), "insight"));
        mock.add(handlers::record_ddl());
        mock.add(handlers::record_ddl());
        mock.add(handlers::record_ddl());

        migrate(&client)
            .await
            .unwrap_or_else(|error| panic!("migration must succeed: {error}"));
        let ddl = recording.query().await;

        assert!(ddl.contains("CREATE TABLE IF NOT EXISTS raw_data"));
        assert!(ddl.contains("table_name String"));
        assert!(ddl.contains("raw_data String"));
        assert!(ddl.contains("received_at DateTime64(3, 'UTC')"));
        assert!(ddl.contains("ORDER BY (table_name, received_at, id)"));
    }

    #[tokio::test]
    async fn migration_creates_the_three_definition_tables() {
        let mock = Mock::new();
        mock.add(handlers::record_ddl());
        let metrics = mock.add(handlers::record_ddl());
        let widgets = mock.add(handlers::record_ddl());
        let dashboards = mock.add(handlers::record_ddl());
        let client =
            insight_clickhouse::Client::new(insight_clickhouse::Config::new(mock.url(), "insight"));

        migrate(&client)
            .await
            .unwrap_or_else(|error| panic!("migration must succeed: {error}"));

        for (recording, table) in [
            (metrics, "metrics"),
            (widgets, "widgets"),
            (dashboards, "dashboards"),
        ] {
            let ddl = recording.query().await;

            assert!(ddl.contains(&format!("CREATE TABLE IF NOT EXISTS {table}")));
            assert!(ddl.contains("id UUID"));
            assert!(ddl.contains("name String"));
            assert!(ddl.contains("body String"));
            assert!(ddl.contains("updated_at DateTime64(3, 'UTC')"));
            assert!(ddl.contains("ENGINE = ReplacingMergeTree(updated_at)"));
            assert!(ddl.contains("ORDER BY name"));
        }
    }
}
