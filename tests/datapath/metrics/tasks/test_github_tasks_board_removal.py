"""Taking a card off a board ends the status span it was last seen in.

A status span runs until the next status event, and the last one runs until
`now()` — so an issue whose final word is "In progress" accrues development
time every day, forever, on a board it no longer sits on. The removal is a
status event naming no column, which closes the span where it belongs: In
progress from the 6th to the 9th is 72 hours, and stays 72 whenever this runs.
Without the closing row the same fixture reports the hours between the 6th and
today, so a wrong answer here is both wrong and unstable.
"""

from __future__ import annotations

import pytest
from insight_datapath.spec_runner import SpecRun

pytestmark = pytest.mark.fixture

SPEC = "github_tasks_board_removal"

CAROL = "carol@example.com"


def test_removing_a_card_ends_the_in_progress_span(spec: SpecRun) -> None:
    """Three days in progress, closed by the removal rather than by today."""
    r = spec.call(
        {
            "url": "/v1/metric-results",
            "method": "POST",
            "body": {
                "entity": {"type": "person", "ids": [CAROL]},
                "period": {"from": "2026-03-01", "to": "2026-03-31"},
                "metrics": [
                    {"metric_key": "tasks.dev_time", "views": [{"view": "period"}]},
                ],
            },
        }
    )
    assert r.status == 200

    r.row("tasks.dev_time", "period", entity_id=CAROL).equals(value=72)
