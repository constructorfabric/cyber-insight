"""Removing one instance while its siblings keep running.

The multi-instance case neither existing removal path covers: the cascade fires
only when a connector has NO Secret, and the orphan GC only when the connector
itself is unknown. Between them sits the ordinary thing an operator does —
delete the second Secret — and its source and schedule have to go with it while
the first instance is left untouched.

Every case here is about a refusal as much as a removal: this pass deletes live
Airbyte sources, so what it declines to touch is the part worth pinning.

Run: pytest src/ingestion/reconcile-connectors/tests
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FINDER = ROOT / "python" / "find_removed_instances.py"

TENANT = "example-tenant"


def plan_row(connector: str, source_id: str = "", secret: str = "") -> str:
    namespace = "bronze_" + connector.replace("-", "_")
    return "\t".join(
        [connector, "dir", "1", "nocode", "", "", "", namespace, source_id, secret, "hash"]
    )


def source(name: str, airbyte_id: str) -> dict:
    return {"name": name, "sourceId": airbyte_id}


def find(
    tmp_path: Path,
    plan: list[str],
    sources: list[dict],
    connector: str | None = None,
) -> list[tuple[str, str, str]]:
    plan_file = tmp_path / "plan.tsv"
    plan_file.write_text("\n".join(plan) + "\n", encoding="utf-8")
    argv = [sys.executable, str(FINDER), str(plan_file), TENANT]
    if connector is not None:
        argv.append(connector)
    result = subprocess.run(
        argv, input=json.dumps(sources), capture_output=True, text=True, check=False
    )
    assert result.returncode == 0, result.stderr
    return [tuple(line.split("\t")) for line in result.stdout.splitlines()]


class TestTheSiblingCase:
    def test_the_instance_whose_secret_is_gone_is_removed(self, tmp_path: Path) -> None:
        removed = find(
            tmp_path,
            [plan_row("claude-team", "claude-team-main", "secret-main")],
            [
                source(f"claude-team-claude-team-main-{TENANT}", "src-main"),
                source(f"claude-team-claude-team-second-{TENANT}", "src-second"),
            ],
        )

        assert removed == [("src-second", "claude-team", "claude-team-second")]

    def test_the_surviving_instance_is_left_alone(self, tmp_path: Path) -> None:
        """The whole point of the pass: deleting the second Secret must not cost
        the first instance its source."""
        removed = find(
            tmp_path,
            [
                plan_row("claude-team", "claude-team-main", "secret-main"),
                plan_row("claude-team", "claude-team-second", "secret-second"),
            ],
            [
                source(f"claude-team-claude-team-main-{TENANT}", "src-main"),
                source(f"claude-team-claude-team-second-{TENANT}", "src-second"),
            ],
        )

        assert removed == []


class TestWhatThisPassRefusesToTouch:
    def test_a_connector_with_no_instance_left_belongs_to_the_cascade(
        self, tmp_path: Path
    ) -> None:
        """The cascade takes its definition and schedules too. Taking the
        sources here would race it and report one removal twice."""
        removed = find(
            tmp_path,
            [plan_row("claude-team")],
            [source(f"claude-team-claude-team-main-{TENANT}", "src-main")],
        )

        assert removed == []

    def test_a_source_that_does_not_carry_this_tenant_is_left_alone(
        self, tmp_path: Path
    ) -> None:
        """Its instance cannot be read out of the name, and deleting a source
        whose owner is unknown is how a healthy connector loses its data."""
        removed = find(
            tmp_path,
            [plan_row("claude-team", "claude-team-main", "secret-main")],
            [source("claude-team-claude-team-second-someone-else", "src-elsewhere")],
        )

        assert removed == []

    def test_a_longer_connector_keeps_its_own_sources(self, tmp_path: Path) -> None:
        """`claude-team` is a prefix of `claude-team-invoices`. Matched as a
        bare prefix, the short one would claim every source of the long one and
        delete all of them."""
        removed = find(
            tmp_path,
            [
                plan_row("claude-team", "claude-team-main", "secret-a"),
                plan_row("claude-team-invoices", "claude-team-invoices-main", "secret-b"),
            ],
            [
                source(f"claude-team-claude-team-main-{TENANT}", "src-team"),
                source(f"claude-team-invoices-claude-team-invoices-main-{TENANT}", "src-invoices"),
            ],
        )

        assert removed == []

    def test_a_source_of_a_connector_outside_the_filter_is_left_alone(
        self, tmp_path: Path
    ) -> None:
        """`--connector` narrows what a tick reconciles, so it has to narrow
        what the tick removes by the same amount."""
        removed = find(
            tmp_path,
            [
                plan_row("claude-team", "claude-team-main", "secret-a"),
                plan_row("zulip", "zulip-main", "secret-b"),
            ],
            [
                source(f"claude-team-claude-team-gone-{TENANT}", "src-gone"),
                source(f"zulip-zulip-gone-{TENANT}", "src-zulip-gone"),
            ],
            connector="zulip",
        )

        assert removed == [("src-zulip-gone", "zulip", "zulip-gone")]
