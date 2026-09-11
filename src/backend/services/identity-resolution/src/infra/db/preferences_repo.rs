//! Per-person preferences, scoped to the tenant the person signed into.
//!
//! One row per (tenant, person). A person with no row has expressed no
//! preference, which the caller reads as the default rather than as an error.

use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement};
use uuid::Uuid;

use crate::domain::timezone::Timezone;

/// The zone this person chose in this tenant, or `None` when they never did.
///
/// # Errors
///
/// Returns an error if the query fails, or if a stored name is no longer a
/// zone this build knows.
pub async fn timezone_of(
    db: &DatabaseConnection,
    tenant_id: Uuid,
    person_id: Uuid,
) -> anyhow::Result<Option<Timezone>> {
    const SQL: &str = r"
        SELECT timezone
        FROM user_preferences
        WHERE insight_tenant_id = ?
          AND person_id         = ?
    ";

    let stmt = Statement::from_sql_and_values(
        DbBackend::MySql,
        SQL,
        [
            tenant_id.as_bytes().to_vec().into(),
            person_id.as_bytes().to_vec().into(),
        ],
    );

    let Some(row) = db.query_one_raw(stmt).await? else {
        return Ok(None);
    };
    let stored: String = row.try_get("", "timezone")?;

    Ok(Some(Timezone::parse(&stored)?))
}

/// Records this person's zone, replacing whatever they chose before.
///
/// # Errors
///
/// Returns an error if the write fails.
pub async fn set_timezone(
    db: &DatabaseConnection,
    tenant_id: Uuid,
    person_id: Uuid,
    timezone: &Timezone,
) -> anyhow::Result<()> {
    const SQL: &str = r"
        INSERT INTO user_preferences (insight_tenant_id, person_id, timezone, updated_at)
        VALUES (?, ?, ?, UTC_TIMESTAMP(6))
        ON DUPLICATE KEY UPDATE timezone = VALUES(timezone), updated_at = VALUES(updated_at)
    ";

    let stmt = Statement::from_sql_and_values(
        DbBackend::MySql,
        SQL,
        [
            tenant_id.as_bytes().to_vec().into(),
            person_id.as_bytes().to_vec().into(),
            timezone.as_str().into(),
        ],
    );
    db.execute_raw(stmt).await?;

    Ok(())
}
