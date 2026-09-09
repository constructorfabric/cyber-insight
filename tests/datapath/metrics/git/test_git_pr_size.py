"""Typical diff size per pull request, served per person over a window.

Lines added plus lines removed, one value per request, dated by the day the request
was OPENED and taken whatever state the request reached. A request whose line counts
were never collected contributes nothing; a request whose counts were collected and
are zero contributes a real zero. Bitbucket reports one row per changed file, and only
the parent request's last-update stamp says which rows belong to the diff it has now.
"""

from __future__ import annotations

import pytest
from insight_datapath.spec_runner import SpecRun

pytestmark = pytest.mark.fixture

SPEC = "git_pr_size"

ALICE = "alice@example.com"
BOB = "bob@example.com"
CAROL = "carol@example.com"
HEIDI = "heidi@example.com"

SOURCE_GITHUB = {"key": "source", "value": "github"}


def test_the_median_takes_every_request_whatever_it_became(spec: SpecRun) -> None:
    """[12, 24 open, 36 closed-unmerged, 48, 100] medians to 36, not the mean 44.

    The 2026-12-01 bucket holds the even set [12, 24, 36, 48] and serves the upper
    middle 36 rather than the average 30 — quantileExact takes an index, never a pair.
    """
    r = spec.call(
        {
            "url": "/v1/metric-results",
            "method": "POST",
            "body": {
                "entity": {"type": "person", "ids": [ALICE]},
                "period": {"from": "2026-12-01", "to": "2026-12-02"},
                "metrics": [
                    {
                        "metric_key": "git.pr_size",
                        "views": [
                            {"view": "period"},
                            {"view": "timeseries", "bucket": "day"},
                            {"view": "breakdown", "dimensions": ["source"]},
                            {"view": "histogram"},
                        ],
                    }
                ],
            },
        }
    )
    assert r.status == 200

    r.row("git.pr_size", "period", entity_id=ALICE).equals(value=36)
    series = r.row("git.pr_size", "timeseries", entity_id=ALICE)
    series.contains(points={"bucket_start": "2026-12-01", "value": 36})
    series.contains(points={"bucket_start": "2026-12-02", "value": 100})
    r.row("git.pr_size", "breakdown", entity_id=ALICE, dimensions=SOURCE_GITHUB).equals(value=36)
    r.row("git.pr_size", "histogram", entity_id=ALICE).nonempty("bins")


def test_a_collected_zero_is_a_value_not_an_absence(spec: SpecRun) -> None:
    """carol's [0, 10, 30] medians to 10; dropping the observed zero would serve 30.

    Her zero-line request had its counts collected — the source answered, and the
    answer was that no lines changed. That is an observation, and the class columns
    are nullable so that a request whose counts were never collected can say so
    instead.
    """
    r = spec.call(
        {
            "url": "/v1/metric-results",
            "method": "POST",
            "body": {
                "entity": {"type": "person", "ids": [CAROL]},
                "period": {"from": "2026-12-01", "to": "2026-12-01"},
                "metrics": [{"metric_key": "git.pr_size", "views": [{"view": "period"}]}],
            },
        }
    )
    assert r.status == 200
    r.row("git.pr_size", "period", entity_id=CAROL).equals(value=10)


def test_gitlab_reports_the_request_but_no_size(spec: SpecRun) -> None:
    """GitLab has no diff-stat stream for a merge request, so bob gets no size.

    His created count is asserted beside it: without that, a null size would equally
    describe a request that never reached the metric at all.
    """
    r = spec.call(
        {
            "url": "/v1/metric-results",
            "method": "POST",
            "body": {
                "entity": {"type": "person", "ids": [BOB]},
                "period": {"from": "2026-12-01", "to": "2026-12-01"},
                "metrics": [
                    {"metric_key": "git.pr_size", "views": [{"view": "period"}]},
                    {"metric_key": "git.prs_created", "views": [{"view": "period"}]},
                ],
            },
        }
    )
    assert r.status == 200
    r.row("git.pr_size", "period", entity_id=BOB).equals(value=None)
    r.row("git.prs_created", "period", entity_id=BOB).equals(value=1)


def test_bitbucket_takes_the_current_file_rows_and_not_a_stale_one(spec: SpecRun) -> None:
    """801 sums its three current files to 40; 802's diff is 30 lines, not 100.

    802 holds a 70-line row for a file that left the diff in a rebase, written under
    an earlier update stamp. Size is the newest stamp's rows taken whole — resolving
    each file to its own newest row would keep the dropped file, which has no newer
    row to displace it.
    """
    r = spec.call(
        {
            "url": "/v1/metric-results",
            "method": "POST",
            "body": {
                "entity": {"type": "person", "ids": [HEIDI]},
                "period": {"from": "2026-12-11", "to": "2026-12-13"},
                "metrics": [
                    {
                        "metric_key": "git.pr_size",
                        "views": [
                            {"view": "period"},
                            {"view": "timeseries", "bucket": "day"},
                        ],
                    }
                ],
            },
        }
    )
    assert r.status == 200

    series = r.row("git.pr_size", "timeseries", entity_id=HEIDI)
    series.contains(points={"bucket_start": "2026-12-11", "value": 40})
    series.contains(points={"bucket_start": "2026-12-12", "value": 30})
    r.row("git.pr_size", "period", entity_id=HEIDI).equals(value=40)


def test_a_bitbucket_request_with_no_diffstat_contributes_nothing(spec: SpecRun) -> None:
    """803's size was never collected, so 2026-12-13 carries no size — but a request.

    Bitbucket names the author on the request itself, so 803 reaches the metric on its
    account binding and its created count proves it. The absent size is therefore the
    missing diffstat and nothing else.
    """
    r = spec.call(
        {
            "url": "/v1/metric-results",
            "method": "POST",
            "body": {
                "entity": {"type": "person", "ids": [HEIDI]},
                "period": {"from": "2026-12-13", "to": "2026-12-13"},
                "metrics": [
                    {"metric_key": "git.pr_size", "views": [{"view": "period"}]},
                    {"metric_key": "git.prs_created", "views": [{"view": "period"}]},
                ],
            },
        }
    )
    assert r.status == 200
    r.row("git.pr_size", "period", entity_id=HEIDI).equals(value=None)
    r.row("git.prs_created", "period", entity_id=HEIDI).equals(value=1)


def test_the_window_holding_every_close_holds_no_creation(spec: SpecRun) -> None:
    """Every one of alice's requests closes or merges inside 12-04…12-07, and the
    window serves nothing: the value sits on the day the request was opened."""
    r = spec.call(
        {
            "url": "/v1/metric-results",
            "method": "POST",
            "body": {
                "entity": {"type": "person", "ids": [ALICE]},
                "period": {"from": "2026-12-04", "to": "2026-12-07"},
                "metrics": [{"metric_key": "git.pr_size", "views": [{"view": "period"}]}],
            },
        }
    )
    assert r.status == 200
    r.row("git.pr_size", "period", entity_id=ALICE).equals(value=None)


def test_empty_window(spec: SpecRun) -> None:
    """A window with no requests serves an honest null, not a zero."""
    r = spec.call(
        {
            "url": "/v1/metric-results",
            "method": "POST",
            "body": {
                "entity": {"type": "person", "ids": [ALICE]},
                "period": {"from": "2025-01-01", "to": "2025-01-31"},
                "metrics": [{"metric_key": "git.pr_size", "views": [{"view": "period"}]}],
            },
        }
    )
    assert r.status == 200
    r.row("git.pr_size", "period", entity_id=ALICE).equals(value=None)
