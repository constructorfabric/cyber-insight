"""`POST /v1/query` — the query contract over the declared datasets.

    POST /v1/query   200 admin · 403 everybody else
                     400 (unknown dataset, undeclared dimension, a limit over
                          the cap, a window over the cap, an ordered test over a
                          dimension, a body naming a tenant)

Deployed-path because no unit test reaches these: the session's tenant becoming
the scan's leading predicate, the compiled statement being SQL a real ClickHouse
accepts, the gold relation the declaration binds having been built, and the
admin gate — which lives inside the handler, so the edge admits the request and
the refusal comes from the role check.

No expected number is written into this file; the 200 cases reconcile two
independent queries against each other.
"""

from __future__ import annotations

import pytest
from insight_stand import ApiClient, ApiResponse, Manifest, PersonaSession, analytics_path
from insight_stand.api import JsonValue

from ..schemas import ProblemDocument
from ..schemas.analytics import QueryAnswer
from . import query_window

QUERY = analytics_path("/v1/query")

#: The datasets this build declares. A key the service does not carry is the
#: other half of the pair, and it is well-formed so a refusal is the
#: declaration's rather than a spelling rejection dressed as one.
GIT_COMMITS = "git_commits"
GIT_FILE_CHANGES = "git_file_changes"
UNKNOWN_DATASET = "stand_does_not_exist"


def _query(
    manifest: Manifest, *, grain: str | None = None, **overrides: JsonValue
) -> dict[str, JsonValue]:
    start, end = query_window(manifest)
    time: dict[str, JsonValue] = {"from": start, "to": end}
    if grain is not None:
        time["grain"] = grain

    body: dict[str, JsonValue] = {
        "dataset": GIT_COMMITS,
        "aggregates": [{"name": "commits", "fn": "count"}],
        "time": time,
    }
    body.update(overrides)
    return body


def _post(api: ApiClient, body: dict[str, JsonValue]) -> ApiResponse:
    return api.post(QUERY, json_body=body)


def _violated_fields(response: ApiResponse) -> list[str]:
    """Every request field the refusal names, so a caller could repair the query."""
    violations = response.parse(ProblemDocument).context.get("field_violations")
    assert isinstance(violations, list), (
        f"a refusal must carry field_violations: {response.text[:300]}"
    )

    fields: list[str] = []
    for violation in violations:
        assert isinstance(violation, dict), f"a violation must be an object: {violation}"
        field = violation.get("field")
        assert isinstance(field, str), f"a violation must name a field: {violation}"
        fields.append(field)
    return fields


def _column_names(answer: QueryAnswer) -> list[str]:
    return [column.name for column in answer.columns]


def _answered(response: ApiResponse) -> QueryAnswer:
    """A 200 with at least one row: the seed guarantees commits in the window, so
    an empty table would be a scan that reached the wrong rows, not a quiet stand."""
    assert response.status_code == 200, f"status={response.status_code} {response.text[:300]}"
    answer = response.parse(QueryAnswer)
    assert answer.rows, f"the seeded window answered no rows: {response.text[:300]}"
    for row in answer.rows:
        assert len(row) == len(answer.columns), (
            f"a row carries {len(row)} values for {len(answer.columns)} columns: {row}"
        )
    return answer


@pytest.fixture(scope="module")
def api(admin_operator_session: PersonaSession) -> ApiClient:
    """The surface is admin-only, so every case below queries as the operator."""
    return admin_operator_session.client


@pytest.mark.requires_seed("dev_lead")
@pytest.mark.reliability
def test_a_bucketed_query_answers_a_typed_table_whose_rows_match_its_columns(
    api: ApiClient, stand_manifest: Manifest
) -> None:
    body = _query(
        stand_manifest,
        grain="month",
        group_by=[{"axis": "dimension", "field": "repository"}, {"axis": "time"}],
        order=[{"by": "commits", "dir": "desc"}],
        limit=200,
    )

    answer = _answered(_post(api, body))
    assert _column_names(answer) == ["repository", "repository_label", "time", "commits"], (
        f"the answer's columns are not the ones the query asked for: {answer.columns}"
    )
    assert [column.kind for column in answer.columns] == [
        "dimension",
        "label",
        "bucket",
        "aggregate",
    ]
    assert not answer.flags.truncated, "a 200-row ceiling over the seed's repositories truncated"


@pytest.mark.requires_seed("dev_lead")
@pytest.mark.reliability
def test_the_buckets_of_a_grouped_count_sum_to_the_same_window_folded_whole(
    api: ApiClient, stand_manifest: Manifest
) -> None:
    """A bucketed and an ungrouped count read the same rows, so they must agree
    whatever the stand was seeded with."""
    total_rows = _answered(_post(api, _query(stand_manifest))).rows
    assert len(total_rows) == 1, f"an ungrouped query answers one row, got {total_rows}"
    total = total_rows[0][0]
    assert isinstance(total, int) and total > 0, f"the seeded window counts no commit: {total}"

    bucketed_body = _query(stand_manifest, grain="month", group_by=[{"axis": "time"}], limit=10_000)
    bucketed = _answered(_post(api, bucketed_body))

    summed = sum(row[1] for row in bucketed.rows)
    assert summed == total, (
        f"the monthly buckets sum to {summed} and the same window folded whole "
        f"is {total} — grouping changed which rows were counted"
    )


@pytest.mark.requires_seed("dev_lead")
@pytest.mark.reliability
def test_a_filter_on_a_dimension_value_reads_the_rows_its_group_counted(
    api: ApiClient, stand_manifest: Manifest
) -> None:
    """A group-by answers one count per `source`; an `eq` filter on each value
    must answer the same count, or a filter and a group read a dimension
    differently. The two queries are independent scans."""
    grouped = _answered(
        _post(api, _query(stand_manifest, group_by=[{"axis": "dimension", "field": "source"}]))
    )
    assert _column_names(grouped) == ["source", "source_label", "commits"]

    for source, _label, expected in grouped.rows:
        filtered = _answered(
            _post(
                api,
                _query(
                    stand_manifest,
                    filters=[{"field": "source", "op": "eq", "value": source}],
                ),
            )
        )
        assert filtered.rows[0][0] == expected, (
            f"source={source!r}: grouped count {expected}, filtered count {filtered.rows[0][0]}"
        )


@pytest.mark.requires_seed("dev_lead")
@pytest.mark.reliability
def test_a_pattern_over_a_dimension_reads_the_rows_its_exact_values_would(
    api: ApiClient, stand_manifest: Manifest
) -> None:
    """A `match` alternation over `source` must count what an `in` over the same
    two values counts, and a `like` on one value what `eq` counts: three
    independent scans that read one set of rows."""
    grouped = _answered(
        _post(api, _query(stand_manifest, group_by=[{"axis": "dimension", "field": "source"}]))
    )
    sources = [row[0] for row in grouped.rows]
    first = sources[0]

    liked = _answered(
        _post(
            api, _query(stand_manifest, filters=[{"field": "source", "op": "like", "value": first}])
        )
    )
    exact = _answered(
        _post(
            api, _query(stand_manifest, filters=[{"field": "source", "op": "eq", "value": first}])
        )
    )
    assert liked.rows[0][0] == exact.rows[0][0], f"like vs eq on {first!r}"

    pattern = "^(" + "|".join(sources) + ")$"
    matched = _answered(
        _post(
            api,
            _query(stand_manifest, filters=[{"field": "source", "op": "match", "value": pattern}]),
        )
    )
    listed = _answered(
        _post(
            api,
            _query(stand_manifest, filters=[{"field": "source", "op": "in", "values": sources}]),
        )
    )
    assert matched.rows[0][0] == listed.rows[0][0], f"match vs in over {sources}"


@pytest.mark.requires_seed("dev_lead")
@pytest.mark.reliability
def test_a_limit_below_the_group_count_is_reported_truncated_and_at_it_is_not(
    api: ApiClient, stand_manifest: Manifest
) -> None:
    """The group count comes from the stand itself: an unclipped daily count
    says how many buckets there are, then one below it must truncate and
    exactly it must not."""
    whole = _answered(
        _post(api, _query(stand_manifest, grain="day", group_by=[{"axis": "time"}], limit=10_000))
    )
    assert not whole.flags.truncated, "the seed has more than 10000 days"
    buckets = len(whole.rows)
    if buckets < 2:
        pytest.skip("the seeded window holds one bucket, so nothing can be clipped")

    clipped = _answered(
        _post(
            api,
            _query(stand_manifest, grain="day", group_by=[{"axis": "time"}], limit=buckets - 1),
        )
    )
    assert clipped.flags.truncated, f"{buckets} buckets behind a limit of {buckets - 1}"
    assert len(clipped.rows) == buckets - 1

    exact = _answered(
        _post(api, _query(stand_manifest, grain="day", group_by=[{"axis": "time"}], limit=buckets))
    )
    assert not exact.flags.truncated, f"{buckets} buckets behind a limit of {buckets}"
    assert len(exact.rows) == buckets


@pytest.mark.requires_seed("dev_lead")
@pytest.mark.versatility
@pytest.mark.parametrize("grain", ["day", "week", "month"])
def test_every_declared_grain_answers_a_bucket_column(
    api: ApiClient, stand_manifest: Manifest, grain: str
) -> None:
    """Shape only: bucket boundaries are the compiler's rendered-SQL goldens' job."""
    body = _query(stand_manifest, grain=grain, group_by=[{"axis": "time"}], limit=10_000)

    answer = _answered(_post(api, body))
    assert _column_names(answer) == ["time", "commits"], f"grain={grain}"


@pytest.mark.reliability
@pytest.mark.parametrize(
    ("label", "overrides", "field"),
    [
        ("a dataset this build does not declare", {"dataset": UNKNOWN_DATASET}, "dataset"),
        (
            "a dimension the dataset does not declare",
            {"group_by": [{"axis": "dimension", "field": "branch"}]},
            "group_by[0].field",
        ),
        ("a row ceiling over the cap", {"limit": 1_000_000}, "limit"),
        (
            "a window wider than the cap",
            {"time": {"from": "2020-01-01", "to": "2026-01-01"}},
            "time.to",
        ),
        (
            "an ordered test over a dimension",
            {"filters": [{"field": "repository", "op": "gt", "value": "a"}]},
            "filters[0].op",
        ),
        (
            "a pattern test over a measurable",
            {"filters": [{"field": "lines_added", "op": "like", "value": "1%"}]},
            "filters[0].op",
        ),
        (
            "a pattern that is not a regular expression",
            {"filters": [{"field": "repository", "op": "match", "value": "("}]},
            "filters[0].value",
        ),
        (
            "an aggregate over a column that is not a measurable",
            {"aggregates": [{"name": "added", "fn": "sum", "field": "message"}]},
            "aggregates[0].field",
        ),
    ],
)
def test_a_query_the_dataset_cannot_answer_is_refused_naming_the_field(
    api: ApiClient,
    stand_manifest: Manifest,
    label: str,
    overrides: dict[str, JsonValue],
    field: str,
) -> None:
    """400 rather than 404 throughout, the unknown dataset included: this is a
    statement about the request, not about a missing resource."""
    body = _query(stand_manifest)
    body.update(overrides)

    response = _post(api, body)

    assert response.status_code == 400, (
        f"{label}: status={response.status_code} {response.text[:300]}"
    )
    assert field in _violated_fields(response), (
        f"{label}: the refusal did not name {field!r}: {response.text[:300]}"
    )


@pytest.mark.requires_seed("dev_lead")
@pytest.mark.versatility
def test_the_file_change_dataset_answers_over_its_own_declared_dimensions(
    api: ApiClient, stand_manifest: Manifest
) -> None:
    """`file_extension` is a dimension of this dataset alone, so a 200 is evidence
    the query reached THIS relation rather than the commit one."""
    body = _query(
        stand_manifest,
        grain="month",
        dataset=GIT_FILE_CHANGES,
        group_by=[{"axis": "dimension", "field": "file_extension"}, {"axis": "time"}],
        aggregates=[
            {"name": "changes", "fn": "count"},
            {
                "name": "added_to_tests",
                "fn": "sum",
                "field": "lines_added",
                "filter": {"field": "category", "op": "eq", "value": "test"},
            },
        ],
        limit=500,
    )

    answer = _answered(_post(api, body))
    assert _column_names(answer) == [
        "file_extension",
        "file_extension_label",
        "time",
        "changes",
        "added_to_tests",
    ]


@pytest.mark.reliability
def test_a_dimension_belongs_to_its_dataset_and_not_to_the_build(
    api: ApiClient, stand_manifest: Manifest
) -> None:
    """The other half of the pair above: the same axis refused on the dataset that
    does not declare it, which is what proves validation is dataset-scoped."""
    body = _query(stand_manifest, group_by=[{"axis": "dimension", "field": "file_extension"}])

    response = _post(api, body)

    assert response.status_code == 400, f"status={response.status_code} {response.text[:300]}"
    assert "group_by[0].field" in _violated_fields(response), (
        f"the refusal did not name the axis: {response.text[:300]}"
    )


@pytest.mark.reliability
@pytest.mark.parametrize(
    ("label", "overrides"),
    [
        (
            "an operand belonging to another filter operator",
            {"filters": [{"field": "source", "op": "eq", "values": ["github"]}]},
        ),
        (
            "a fold naming a column its variant does not read",
            {"aggregates": [{"name": "commits", "fn": "count", "field": "lines_added"}]},
        ),
        (
            "a fold missing the column its variant reads",
            {"aggregates": [{"name": "added", "fn": "sum"}]},
        ),
        (
            "a group axis that names no axis",
            {"group_by": [{"dimension": "repository"}]},
        ),
    ],
)
def test_an_operand_from_another_variant_is_refused_before_anything_validates(
    api: ApiClient, stand_manifest: Manifest, label: str, overrides: dict[str, JsonValue]
) -> None:
    """Each body pairs a tag with an operand another variant takes, so the refusal
    is the body extractor's rather than any dataset rule's."""
    body = _query(stand_manifest)
    body.update(overrides)

    response = _post(api, body)

    assert response.status_code in {400, 422}, (
        f"{label}: status={response.status_code} {response.text[:300]}"
    )


@pytest.mark.security
def test_a_lead_is_refused_the_query_surface(
    lead_session: PersonaSession, stand_manifest: Manifest
) -> None:
    """An ordinary authenticated caller, not an anonymous one: the datasets carry
    person columns and declare no visibility policy yet, so nobody below the
    operator gets a row — a well-formed query included."""
    response = lead_session.client.post(QUERY, json_body=_query(stand_manifest))

    assert response.status_code == 403, (
        f"answered {response.status_code} for a lead: {response.text[:300]}"
    )
    assert response.parse(ProblemDocument).status == 403


@pytest.mark.security
def test_a_query_cannot_name_a_tenant_of_its_own(api: ApiClient, stand_manifest: Manifest) -> None:
    """The contract refuses every key it does not declare, so a query has no lever
    to scope itself to another tenant."""
    body = _query(stand_manifest)
    body["tenant_id"] = "00000000-0000-0000-0000-000000000000"

    response = _post(api, body)

    assert response.status_code in {400, 422}, (
        f"an off-contract key was accepted: status={response.status_code} {response.text[:300]}"
    )
