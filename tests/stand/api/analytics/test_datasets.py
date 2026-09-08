"""`GET /v1/datasets` — what a query may be built over.

    GET /v1/datasets         200 admin · every declared key, described
                             403 everybody else
    GET /v1/datasets/{key}   200 admin · the same object the listing carried
                             403 everybody else
                             404 · a key this build declares no dataset for

Deployed-path because the declarations are validated against the warehouse's
column catalog at boot: a 200 is evidence the running image's datasets loaded,
which is the precondition every `POST /v1/query` depends on.
"""

from __future__ import annotations

import pytest
from insight_stand import ApiClient, PersonaSession, analytics_path

from ..schemas import ProblemDocument
from ..schemas.analytics import QueryDataset, QueryDatasetList

DATASETS = analytics_path("/v1/datasets")

#: The datasets this build declares.
GIT_COMMITS = "git_commits"
GIT_FILE_CHANGES = "git_file_changes"

#: Well-formed as a key — lowercase snake_case — so the refusal is the
#: declaration's and not a spelling rejection dressed as one.
UNKNOWN_DATASET = "stand_does_not_exist"


def _dataset_path(key: str) -> str:
    return analytics_path(f"/v1/datasets/{key}")


@pytest.fixture(scope="module")
def api(admin_operator_session: PersonaSession) -> ApiClient:
    """Gated with the query surface it describes, so every read is the operator's."""
    return admin_operator_session.client


def _listing(api: ApiClient) -> QueryDatasetList:
    response = api.get(DATASETS)
    assert response.status_code == 200, f"status={response.status_code} {response.text[:300]}"
    return response.parse(QueryDatasetList)


@pytest.mark.reliability
def test_the_listing_describes_every_dataset_a_query_may_name(api: ApiClient) -> None:
    """Axes are asserted non-empty because a key with none lists a dataset nothing
    can be asked of."""
    listing = _listing(api)

    keys = [dataset.key for dataset in listing.datasets]
    assert set(keys) >= {GIT_COMMITS, GIT_FILE_CHANGES}, (
        f"the build declares datasets the listing does not carry: {keys}"
    )
    assert len(keys) == len(set(keys)), f"a dataset is listed twice: {keys}"

    for dataset in listing.datasets:
        assert dataset.time_fields, f"{dataset.key}: no time field, so no window can be bound"
        assert dataset.dimensions, f"{dataset.key}: no dimension, so nothing can be grouped"
        assert dataset.measurables, f"{dataset.key}: no measurable, so nothing can be folded"
        assert sum(field.default for field in dataset.time_fields) == 1, (
            f"{dataset.key}: a window binds to exactly one default time field: "
            f"{dataset.time_fields}"
        )
        for dimension in dataset.dimensions:
            if dimension.label is not None:
                assert dimension.label == f"{dimension.field}_label", (
                    f"{dataset.key}.{dimension.field}: the label column is named off "
                    f"the contract: {dimension.label}"
                )


@pytest.mark.reliability
@pytest.mark.parametrize("key", [GIT_COMMITS, GIT_FILE_CHANGES])
def test_a_dataset_reads_back_as_the_object_the_listing_carried(api: ApiClient, key: str) -> None:
    """A difference between the two reads is a describe path that has drifted."""
    listed = {dataset.key: dataset for dataset in _listing(api).datasets}
    assert key in listed, f"{key} is not in the listing"

    response = api.get(_dataset_path(key))
    assert response.status_code == 200, f"status={response.status_code} {response.text[:300]}"

    assert response.parse(QueryDataset) == listed[key], (
        f"{key}: the detail and the listing describe the dataset differently"
    )


@pytest.mark.reliability
def test_a_key_this_build_declares_no_dataset_for_is_404(api: ApiClient) -> None:
    """404 rather than the 400 `POST /v1/query` answers for the same unknown key:
    there it is a field of a request, here it is the resource addressed."""
    response = api.get(_dataset_path(UNKNOWN_DATASET))

    assert response.status_code == 404, f"status={response.status_code} {response.text[:300]}"
    assert response.parse(ProblemDocument).status == 404


@pytest.mark.security
def test_a_lead_is_refused_both_reads(lead_session: PersonaSession) -> None:
    """The gate is inside the handler, so this is the only place it is observable."""
    for path in (DATASETS, _dataset_path(GIT_COMMITS)):
        response = lead_session.client.get(path)
        assert response.status_code == 403, (
            f"{path} answered {response.status_code} for a lead: {response.text[:300]}"
        )
        assert response.parse(ProblemDocument).status == 403


@pytest.mark.security
def test_a_description_carries_nothing_about_where_the_rows_live(api: ApiClient) -> None:
    """A declaration also names the database, relation, tenancy column and row
    identity; a description that leaked one would hand over the warehouse shape."""
    body = api.get(DATASETS).text

    for internal in ("database", "relation", "read_discipline", "tenant_field", "row_identity"):
        assert internal not in body, f"the listing leaks `{internal}`: {body[:300]}"
