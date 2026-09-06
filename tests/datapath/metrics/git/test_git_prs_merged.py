"""Pull requests merged, credited to the request's author and dated by the close.

Each connector says a request landed its own way — GitHub's and GitLab's merged_at, and
Bitbucket's terminal update event — and silver normalises all three to one state and one
close time. Gold counts a request in the merged state with a known close time exactly once,
so an open one, a closed-but-never-merged one and a re-synced duplicate all stay out.
"""

from __future__ import annotations

import pytest
from insight_datapath.spec_runner import SpecRun

pytestmark = pytest.mark.fixture

SPEC = "git_prs_merged"

ALICE = "alice@example.com"
BOB = "bob@example.com"
CAROL = "carol@example.com"
DAVE = "dave@example.com"


def test_a_merged_request_counts_on_its_merge_day_and_an_unmerged_one_never_does(
    spec: SpecRun,
) -> None:
    """Three of alice's five requests merged; the open one, the closed-but-never-merged one
    and the re-synced duplicate stay out. dave's commit rides a request he did not author,
    so the merge is none of his."""
    r = spec.call(
        {
            "url": "/v1/metric-results",
            "method": "POST",
            "body": {
                "entity": {"type": "person", "ids": [ALICE, DAVE]},
                "period": {"from": "2026-10-01", "to": "2026-10-02"},
                "metrics": [
                    {
                        "metric_key": "git.prs_merged",
                        "views": [
                            {"view": "period"},
                            {"view": "timeseries", "bucket": "day"},
                            {"view": "breakdown", "dimensions": ["source"]},
                        ],
                    }
                ],
            },
        }
    )
    assert r.status == 200

    r.row("git.prs_merged", "period", entity_id=ALICE).equals(value=3)
    r.row("git.prs_merged", "timeseries", entity_id=ALICE).contains(
        points={"bucket_start": "2026-10-01", "value": 3}
    )
    r.row(
        "git.prs_merged",
        "breakdown",
        entity_id=ALICE,
        dimensions={"key": "source", "value": "github"},
    ).equals(value=3)
    r.row("git.prs_merged", "period", entity_id=DAVE).equals(value=None)


def test_the_branch_scope_split_says_where_each_request_was_aimed(spec: SpecRun) -> None:
    """Two of alice's merges aimed at the default branch and one at a release branch, and
    the two halves partition her period total."""
    r = spec.call(
        {
            "url": "/v1/metric-results",
            "method": "POST",
            "body": {
                "entity": {"type": "person", "ids": [ALICE]},
                "period": {"from": "2026-10-01", "to": "2026-10-02"},
                "metrics": [
                    {
                        "metric_key": "git.prs_merged",
                        "views": [{"view": "breakdown", "dimensions": ["branch_scope"]}],
                    }
                ],
            },
        }
    )
    assert r.status == 200

    r.row(
        "git.prs_merged",
        "breakdown",
        entity_id=ALICE,
        dimensions={"key": "branch_scope", "value": "default"},
    ).equals(value=2)
    r.row(
        "git.prs_merged",
        "breakdown",
        entity_id=ALICE,
        dimensions={"key": "branch_scope", "value": "non_default"},
    ).equals(value=1)


def test_a_gitlab_merge_request_counts_only_in_the_merged_state(spec: SpecRun) -> None:
    """bob's merged request counts; his closed-without-merging and open ones do not."""
    r = spec.call(
        {
            "url": "/v1/metric-results",
            "method": "POST",
            "body": {
                "entity": {"type": "person", "ids": [BOB]},
                "period": {"from": "2026-10-01", "to": "2026-10-02"},
                "metrics": [
                    {
                        "metric_key": "git.prs_merged",
                        "views": [
                            {"view": "period"},
                            {"view": "timeseries", "bucket": "day"},
                            {"view": "breakdown", "dimensions": ["source"]},
                        ],
                    }
                ],
            },
        }
    )
    assert r.status == 200

    r.row("git.prs_merged", "period", entity_id=BOB).equals(value=1)
    r.row("git.prs_merged", "timeseries", entity_id=BOB).contains(
        points={"bucket_start": "2026-10-01", "value": 1}
    )
    r.row(
        "git.prs_merged",
        "breakdown",
        entity_id=BOB,
        dimensions={"key": "source", "value": "gitlab"},
    ).equals(value=1)


def test_a_bitbucket_request_is_dated_by_its_terminal_merge_event_and_a_declined_one_is_dropped(
    spec: SpecRun,
) -> None:
    """carol's one merged request reaches her through her account binding alone, and its
    bucket is the merge event's day rather than the request's own later update."""
    r = spec.call(
        {
            "url": "/v1/metric-results",
            "method": "POST",
            "body": {
                "entity": {"type": "person", "ids": [CAROL]},
                "period": {"from": "2026-10-01", "to": "2026-10-02"},
                "metrics": [
                    {
                        "metric_key": "git.prs_merged",
                        "views": [
                            {"view": "period"},
                            {"view": "timeseries", "bucket": "day"},
                            {"view": "breakdown", "dimensions": ["source"]},
                        ],
                    }
                ],
            },
        }
    )
    assert r.status == 200

    r.row("git.prs_merged", "period", entity_id=CAROL).equals(value=1)
    r.row("git.prs_merged", "timeseries", entity_id=CAROL).contains(
        points={"bucket_start": "2026-10-01", "value": 1}
    )
    r.row(
        "git.prs_merged",
        "breakdown",
        entity_id=CAROL,
        dimensions={"key": "source", "value": "bitbucket_cloud"},
    ).equals(value=1)


def test_the_window_the_requests_were_created_in_holds_no_merges(spec: SpecRun) -> None:
    """Every request was opened inside this window and merged after it, so nobody has a value."""
    r = spec.call(
        {
            "url": "/v1/metric-results",
            "method": "POST",
            "body": {
                "entity": {"type": "person", "ids": [ALICE, BOB, CAROL]},
                "period": {"from": "2026-09-20", "to": "2026-09-30"},
                "metrics": [{"metric_key": "git.prs_merged", "views": [{"view": "period"}]}],
            },
        }
    )
    assert r.status == 200

    for person in (ALICE, BOB, CAROL):
        r.row("git.prs_merged", "period", entity_id=person).equals(value=None)
