//! Live-DB tests for the per-person preference store.
//!
//! Never `#[ignore]`d — see the invariant in [`super::test_fixture`]; they
//! skip at runtime when `INTEGRATION_TESTS_MARIADB_URL` is unset. Every case
//! writes under a tenant of its own, so they are parallel-safe.

use uuid::Uuid;

use super::preferences_repo::{set_timezone, timezone_of};
use super::test_fixture::fixture_or_skip;
use crate::domain::timezone::Timezone;

type TestResult = anyhow::Result<()>;

fn zone(name: &str) -> anyhow::Result<Timezone> {
    Timezone::parse(name).map_err(|error| anyhow::anyhow!("`{name}` is a zone: {error}"))
}

#[tokio::test]
async fn a_person_who_never_chose_a_zone_has_no_preference() -> TestResult {
    let Some(fixture) = fixture_or_skip().await? else {
        return Ok(());
    };

    let found = timezone_of(&fixture.db, fixture.tenant, Uuid::now_v7()).await?;

    assert_eq!(found, None);
    Ok(())
}

#[tokio::test]
async fn a_saved_zone_is_there_on_the_next_request() -> TestResult {
    let Some(fixture) = fixture_or_skip().await? else {
        return Ok(());
    };
    let person = Uuid::now_v7();

    set_timezone(
        &fixture.db,
        fixture.tenant,
        person,
        &zone("Europe/Belgrade")?,
    )
    .await?;
    let found = timezone_of(&fixture.db, fixture.tenant, person).await?;

    assert_eq!(found, Some(zone("Europe/Belgrade")?));
    Ok(())
}

#[tokio::test]
async fn choosing_again_replaces_the_earlier_choice() -> TestResult {
    let Some(fixture) = fixture_or_skip().await? else {
        return Ok(());
    };
    let person = Uuid::now_v7();

    set_timezone(
        &fixture.db,
        fixture.tenant,
        person,
        &zone("Europe/Belgrade")?,
    )
    .await?;
    set_timezone(&fixture.db, fixture.tenant, person, &zone("Asia/Tokyo")?).await?;
    let found = timezone_of(&fixture.db, fixture.tenant, person).await?;

    assert_eq!(found, Some(zone("Asia/Tokyo")?));
    Ok(())
}

#[tokio::test]
async fn two_people_in_one_tenant_keep_their_own_zones() -> TestResult {
    let Some(fixture) = fixture_or_skip().await? else {
        return Ok(());
    };
    let one = Uuid::now_v7();
    let other = Uuid::now_v7();

    set_timezone(&fixture.db, fixture.tenant, one, &zone("Europe/Belgrade")?).await?;
    set_timezone(&fixture.db, fixture.tenant, other, &zone("Asia/Tokyo")?).await?;

    assert_eq!(
        timezone_of(&fixture.db, fixture.tenant, one).await?,
        Some(zone("Europe/Belgrade")?)
    );
    assert_eq!(
        timezone_of(&fixture.db, fixture.tenant, other).await?,
        Some(zone("Asia/Tokyo")?)
    );
    Ok(())
}

#[tokio::test]
async fn one_subject_signed_into_two_tenants_keeps_two_zones() -> TestResult {
    let Some(fixture) = fixture_or_skip().await? else {
        return Ok(());
    };
    let elsewhere = fixture.in_another_tenant();
    let person = Uuid::now_v7();

    set_timezone(
        &fixture.db,
        fixture.tenant,
        person,
        &zone("Europe/Belgrade")?,
    )
    .await?;

    assert_eq!(
        timezone_of(&elsewhere.db, elsewhere.tenant, person).await?,
        None
    );
    Ok(())
}
