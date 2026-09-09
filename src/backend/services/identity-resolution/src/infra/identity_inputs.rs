//! ClickHouse reader for `identity.identity_inputs` — the raw observation
//! stream that feeds the persons-seed. Concrete `IdentityInputsReader` over the
//! shared `insight-clickhouse` client. Verified against a live dev ClickHouse
//! (the persons-seed reads its whole input through this).
//!
//! NOTE: this materializes the filtered input into a `Vec` rather than
//! streaming row-by-row. Fine at current
//! deployment sizes; row-streaming is deferred to the hardening pass (#1753).

use std::time::Duration;

use async_trait::async_trait;
use clickhouse::Row;
use insight_clickhouse::{Client, Config};
use sea_orm::prelude::DateTime;
use serde::Deserialize;
use uuid::Uuid;

/// A full input scan can outrun the client's 30s default; the seed run as a
/// whole is bounded by `SEED_TIMEOUT`, so give the read generous headroom.
const READ_TIMEOUT: Duration = Duration::from_mins(5);
const ACCOUNT_READ_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_CORRECTION_ACCOUNTS: usize = 10_000;
const MAX_CORRECTION_OBSERVATIONS: usize = 100_000;
const ACCOUNT_READ_CHUNK: usize = 200;

use crate::domain::seed::{IdentityInputRow, SourceAccountKey};
use crate::domain::seed_service::IdentityInputsReader;

/// Verbatim shape from `ClickHouseIdentityInputsReader`: rows ordered so the
/// FIRST per account is the latest (`_synced_at DESC`), which is exactly what
/// `build_profiles` expects. `insight_source_id` is `toString`-ed and reparsed
/// — wrapped in `ifNull` because the column is `Nullable(String)` in the dbt
/// table (`toString` of a Nullable stays Nullable, which the strict decoder
/// rejects against the non-null `String` field); a NULL becomes `''` and fails
/// the UUID reparse, failing the seed.
///
/// HOTFIX(#1550) — TEMPORARY. This is
/// the ANCHOR of the hotfix: every piece of code whose behavior exists only
/// because of it carries the literal tag `HOTFIX(#1550)` — grep for it to
/// find the full blast radius when unwinding the hotfix. The dbt
/// producer writes `insight_tenant_id` *hashed* — sipHash128 of whatever raw
/// string the connector was configured with (`identity_inputs_from_history.sql`,
/// documented there as a TEMPORARY cross-source join key) — so the stored tenant
/// never equals the caller's tenant and persons-seed silently read 0 rows. There
/// is no reliable representation to match against (connector configs are
/// free-form strings), so the tenant filter is DROPPED for now: Insight
/// deployments are single-tenant, all `identity_inputs` rows belong to the
/// deployment, and the seed writes its output under the caller's tenant
/// regardless of what the rows carry (`run_seed` binds the request tenant,
/// never the row's).
///
/// MULTI-TENANT PREREQUISITE: the tenant filter MUST come back before any
/// multi-tenant deployment — without it every tenant's seed would read (and
/// re-file under itself) all other tenants' rows. Restoring it requires the
/// producer side to be fixed first (dbt resolves real tenant UUIDs instead of
/// hashing free-form connector strings), then reinstate
/// `WHERE insight_tenant_id = ?` here.
///
/// The text columns have mixed nullability in `identity_inputs` (e.g.
/// `insight_source_type` is `String`, `source_account_id` is `Nullable(String)`),
/// and the clickhouse decoder is strict in both directions — so most are coerced
/// to a non-null `String` with `ifNull(col, '')` and decoded uniformly.
/// `source_account_id` is the exception: it is decoded as `Option<String>` and a
/// NULL fails the read rather than silently minting a `''` pseudo-account.
/// Crucially the aliases DIFFER from the source column names (`val`, `op_type`,
/// …): a same-name `ifNull(value,'') AS value` would shadow the `value`
/// referenced in `WHERE` and can trip a ClickHouse "Cyclic aliases" error.
/// `is_delete` is derived from
/// `operation_type`. DELETE rows are closure signals that arrive with an
/// empty `value` by the write contract, so the non-empty filter applies to
/// UPSERT rows only — value-filtering DELETEs would drop every tombstone.
const STREAM_SQL: &str = r"
    SELECT
        ifNull(insight_source_type, '')  AS source_type,
        ifNull(toString(insight_source_id), '') AS source_id,
        source_account_id                AS account_id,
        ifNull(value_type, '')           AS val_type,
        ifNull(value, '')                AS val,
        toString(_synced_at)             AS synced_at,
        ifNull(operation_type, '')       AS op_type
    FROM identity.identity_inputs
    WHERE (operation_type = 'UPSERT' AND value IS NOT NULL AND value != '')
       OR operation_type = 'DELETE'
    ORDER BY
        insight_source_type,
        insight_source_id,
        source_account_id,
        _synced_at DESC,
        value_type,
        value
";

#[derive(Debug, Row, Deserialize)]
struct InputRow {
    source_type: String,
    source_id: String,
    account_id: Option<String>,
    val_type: String,
    val: String,
    synced_at: String,
    op_type: String,
}

/// Reads `identity_inputs` from ClickHouse via the shared client.
pub struct ClickHouseIdentityInputsReader {
    client: Client,
}

impl ClickHouseIdentityInputsReader {
    pub(crate) async fn accounts(
        &self,
        accounts: &[SourceAccountKey],
    ) -> anyhow::Result<Vec<IdentityInputRow>> {
        tokio::time::timeout(ACCOUNT_READ_TIMEOUT, self.read_accounts(accounts)).await?
    }

    async fn read_accounts(
        &self,
        accounts: &[SourceAccountKey],
    ) -> anyhow::Result<Vec<IdentityInputRow>> {
        anyhow::ensure!(
            accounts.len() <= MAX_CORRECTION_ACCOUNTS,
            "correction exceeds the account evidence limit"
        );
        let mut result = Vec::new();
        for chunk in accounts.chunks(ACCOUNT_READ_CHUNK) {
            let predicate = vec!["(insight_source_type = ? AND insight_source_id = toUUID(?) AND source_account_id = ?)"; chunk.len()].join(" OR ");
            let sql = format!(
                "SELECT ifNull(insight_source_type, '') AS source_type, ifNull(toString(insight_source_id), '') AS source_id, source_account_id AS account_id, ifNull(value_type, '') AS val_type, ifNull(value, '') AS val, toString(_synced_at) AS synced_at, ifNull(operation_type, '') AS op_type FROM identity.identity_inputs WHERE ({predicate}) ORDER BY _synced_at DESC, _version DESC, value_type, value LIMIT 1 BY insight_source_type, insight_source_id, source_account_id, value_type LIMIT 100001"
            );
            let mut query = self.client.query(&sql);
            for account in chunk {
                query = query
                    .bind(&account.source_type)
                    .bind(account.source_id.to_string())
                    .bind(&account.account_id);
            }
            let rows: Vec<InputRow> = query.fetch_all().await?;
            anyhow::ensure!(
                result.len() + rows.len() <= MAX_CORRECTION_OBSERVATIONS,
                "correction exceeds the observation limit"
            );
            for row in rows {
                result.push(map_row(row)?);
            }
        }
        Ok(result)
    }

    #[must_use]
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    /// Build a reader from connection settings (empty user → no auth).
    #[must_use]
    pub fn connect(url: &str, database: &str, user: &str, password: &str) -> Self {
        let mut config = Config::new(url, database).with_query_timeout(READ_TIMEOUT);
        if !user.is_empty() {
            config = config.with_auth(user, password);
        }
        Self::new(Client::new(config))
    }
}

#[async_trait]
impl IdentityInputsReader for ClickHouseIdentityInputsReader {
    async fn stream(&self, tenant_id: Uuid) -> anyhow::Result<Vec<IdentityInputRow>> {
        // tenant_id is intentionally unused while the HOTFIX(#1550) drops the
        // tenant filter — kept so the `IdentityInputsReader` trait stays
        // stable for when the filter comes back.
        let _ = tenant_id;
        let rows: Vec<InputRow> = self.client.query(STREAM_SQL).fetch_all().await?;
        rows.into_iter().map(map_row).collect()
    }
}

fn map_row(r: InputRow) -> anyhow::Result<IdentityInputRow> {
    let account_id = r.account_id.ok_or_else(|| {
        anyhow::anyhow!(
            "NULL source_account_id in identity_inputs (source_type={}, source_id={}, \
             value_type={}): refusing to fold it into a '' pseudo-account; fix the producer row",
            r.source_type,
            r.source_id,
            r.val_type,
        )
    })?;
    Ok(IdentityInputRow {
        source_type: r.source_type,
        source_id: Uuid::parse_str(&r.source_id)?,
        source_account_id: account_id,
        value_type: r.val_type,
        value: r.val,
        synced_at: parse_ch_datetime(&r.synced_at)?,
        is_delete: r.op_type == "DELETE",
    })
}

/// Parse a `ClickHouse` `toString(DateTime[64])` value: `"2026-07-16 12:34:56"`
/// or `"…56.123456"`. Tries the fractional form first, then the plain form.
fn parse_ch_datetime(s: &str) -> anyhow::Result<DateTime> {
    DateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f")
        .or_else(|_| DateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S"))
        .map_err(|e| anyhow::anyhow!("unparseable _synced_at '{s}': {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn account_reads_scope_native_ids_and_keep_the_latest_explicit_clear()
    -> anyhow::Result<()> {
        let Ok(url) = std::env::var("INTEGRATION_TESTS_IDENTITY_INPUTS_URL") else {
            return Ok(());
        };
        let reader = ClickHouseIdentityInputsReader::connect(&url, "default", "", "");
        reader
            .client
            .query("CREATE DATABASE IF NOT EXISTS identity")
            .execute()
            .await?;
        reader.client.query("CREATE TABLE IF NOT EXISTS identity.identity_inputs (insight_source_type String, insight_source_id UUID, source_account_id Nullable(String), value_type String, value Nullable(String), _synced_at DateTime64(6), operation_type String, _version UInt64) ENGINE = MergeTree ORDER BY (insight_source_type, insight_source_id)").execute().await?;
        let source_id = Uuid::now_v7();
        for (kind, operation, value, version) in [
            ("directory", "UPSERT", "Old Name", 1_u64),
            ("directory", "DELETE", "", 2),
            ("activity", "UPSERT", "Other source", 3),
        ] {
            reader.client.query("INSERT INTO identity.identity_inputs VALUES (?, toUUID(?), 'shared-id', 'person_display_name', ?, toDateTime64('2026-01-01 00:00:00', 6), ?, ?)")
                .bind(kind).bind(source_id.to_string()).bind(value).bind(operation).bind(version).execute().await?;
        }
        let account = SourceAccountKey {
            source_type: "directory".to_owned(),
            source_id,
            account_id: "shared-id".to_owned(),
        };
        let rows = reader.accounts(&[account]).await?;
        assert_eq!(rows.len(), 1);
        assert!(rows[0].is_delete);
        assert!(rows[0].value.is_empty());
        Ok(())
    }

    #[test]
    fn stream_sql_keeps_empty_value_delete_rows() {
        assert!(
            STREAM_SQL.contains("OR operation_type = 'DELETE'"),
            "DELETE closure signals carry an empty value and must not be value-filtered"
        );
        assert!(
            STREAM_SQL.contains("operation_type = 'UPSERT' AND value IS NOT NULL"),
            "the non-empty filter applies to UPSERT rows only"
        );
    }

    #[test]
    fn parses_clickhouse_datetime_with_and_without_fraction() -> anyhow::Result<()> {
        let with_frac = parse_ch_datetime("2026-07-16 12:34:56.123456")?;
        let no_frac = parse_ch_datetime("2026-07-16 12:34:56")?;
        assert_eq!(
            with_frac.format("%Y-%m-%d %H:%M:%S").to_string(),
            "2026-07-16 12:34:56"
        );
        assert_eq!(
            no_frac.format("%Y-%m-%d %H:%M:%S").to_string(),
            "2026-07-16 12:34:56"
        );
        assert!(parse_ch_datetime("not-a-date").is_err());
        Ok(())
    }

    #[test]
    fn a_null_account_id_fails_the_read_rather_than_minting_a_pseudo_account() -> anyhow::Result<()>
    {
        let row = InputRow {
            source_type: "bamboohr".to_owned(),
            source_id: Uuid::now_v7().to_string(),
            account_id: None,
            val_type: "email".to_owned(),
            val: "person@inputs.test".to_owned(),
            synced_at: "2026-01-02 03:04:05.678".to_owned(),
            op_type: "UPSERT".to_owned(),
        };

        let refused = map_row(row)
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();

        anyhow::ensure!(
            refused.contains("NULL source_account_id"),
            "an accountless row must name itself in the failure, not fold into '': {refused:?}"
        );
        Ok(())
    }
}
