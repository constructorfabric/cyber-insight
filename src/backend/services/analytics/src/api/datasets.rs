//! `GET /v1/datasets` and `GET /v1/datasets/{key}` — what a query may be built over.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Extension, Path};
use axum::http::HeaderMap;
use toolkit_canonical_errors::CanonicalError;

use super::error::DatasetError;
use super::{ADMIN_ONLY, AppState, require_admin};
use crate::domain::datasets;
use crate::domain::datasets::describe::{
    DatasetDescription, DatasetListDescription, describe, describe_all,
};

// INVARIANT: declarations are installation-wide, so neither handler reads the
// session's tenant. Both are gated like `POST /v1/query`: a description of a
// surface a caller cannot query is nothing to build from.
pub async fn list_datasets(
    Extension(state): Extension<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<DatasetListDescription>, CanonicalError> {
    require_admin(&state, &headers, admin_only).await?;

    listing()
}

pub async fn get_dataset(
    Extension(state): Extension<Arc<AppState>>,
    headers: HeaderMap,
    Path(key): Path<String>,
) -> Result<Json<DatasetDescription>, CanonicalError> {
    require_admin(&state, &headers, admin_only).await?;

    description(&key)
}

fn listing() -> Result<Json<DatasetListDescription>, CanonicalError> {
    let declared = datasets::product_datasets().map_err(|error| {
        tracing::error!(error = %error, "the shipped dataset declarations are unusable");
        CanonicalError::internal("failed to list the datasets").create()
    })?;

    Ok(Json(describe_all(declared)))
}

fn description(key: &str) -> Result<Json<DatasetDescription>, CanonicalError> {
    let declared = datasets::dataset(key).ok_or_else(|| {
        DatasetError::not_found("no such dataset")
            .with_resource(key)
            .create()
    })?;

    Ok(Json(describe(declared)))
}

fn admin_only() -> CanonicalError {
    DatasetError::permission_denied()
        .with_reason(ADMIN_ONLY)
        .create()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use toolkit_canonical_errors::Problem;

    use super::*;

    #[test]
    fn a_key_this_build_declares_no_dataset_for_is_a_not_found_naming_it() {
        let refusal = description("git_tags").expect_err("an undeclared key is refused");
        let problem =
            serde_json::to_value(Problem::from(refusal)).expect("the envelope serializes");

        assert_eq!(problem["status"], 404);
        assert_eq!(
            problem["context"]["resource_type"],
            "gts.cf.insight.analytics_api.dataset.v1~"
        );
        assert_eq!(problem["context"]["resource_name"], "git_tags");
    }

    #[test]
    fn a_declared_key_answers_the_description_the_listing_carries() {
        let Json(listing) = listing().expect("the declarations load");
        let Json(one) = description("git_commits").expect("git_commits is declared");

        assert_eq!(
            listing
                .datasets
                .iter()
                .find(|dataset| dataset.key == "git_commits"),
            Some(&one),
            "the listing and the detail describe the dataset differently"
        );
    }
}
