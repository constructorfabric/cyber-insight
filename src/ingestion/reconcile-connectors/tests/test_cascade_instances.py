"""What the cascade deletes, and what it refuses to attribute to itself.

The cascade runs for a connector with no Secret at all, so both questions it
asks are answered from source names alone: which sources are this connector's,
and which instance each one is. Both have a wrong answer that deletes live data
— a sibling connector's sources under a shared name prefix, and a schedule named
after an instance that was guessed rather than read.

Run: pytest src/ingestion/reconcile-connectors/tests
"""

from __future__ import annotations

import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

CONNECTOR = "claude-team"
TENANT = "example-tenant"

#: Everything the cascade reaches for, recorded rather than performed.
STUBS = """
log_line()          { printf '%s\\n' "$*" >&2; }
log_event()         { :; }
reconcile__log()    { printf '%s\\n' "$*" >&2; }
reconcile_compute_tenant() { printf 'example-tenant'; }
ab_workspace_id()     { printf 'workspace-1'; }
ab_list_connections() { printf '[]'; }
ab_delete_source()    { printf 'DELETE-SOURCE %s\\n' "$1" >> "$CALLS"; }
disc_load_descriptors() {
  printf '%s\\tdir\\t1\\tnocode\\t\\t\\t\\tbronze\\n' claude-team claude-team-invoices gitlab
}
argo_delete_cronworkflow() {
  printf 'DELETE-CRONWORKFLOW %s %s\\n' "$1" "$3" >> "$CALLS"
}
argo_delete_superseded_cronworkflows() {
  printf 'DELETE-SUPERSEDED %s\\n' "$1" >> "$CALLS"
}
"""


def cascade(sources: list[dict], tmp_path: Path) -> tuple[int, list[str], str]:
    calls = tmp_path / "calls"
    script = f"""
    set -uo pipefail
    export INSIGHT_NAMESPACE=insight
    export CONNECTORS_DIR="{ROOT}/../connectors"
    export AIRBYTE_URL=http://127.0.0.1:1
    export INSIGHT_TENANT_ID={TENANT}
    export CALLS="{calls}"
    source "{ROOT}/lib/reconcile.sh"
    {STUBS}
    ab_list_sources() {{ printf '%s' {json.dumps(json.dumps(sources))}; }}
    reconcile_cascade_delete "{CONNECTOR}"
    """
    result = subprocess.run(
        ["bash", "-c", script], capture_output=True, text=True, check=False
    )
    recorded = calls.read_text(encoding="utf-8").splitlines() if calls.exists() else []
    return result.returncode, recorded, result.stderr


def source(name: str, airbyte_id: str) -> dict:
    return {"name": name, "sourceId": airbyte_id}


class TestASourceThatNamesNoInstance:
    def test_its_source_goes_and_no_instance_schedule_is_touched(
        self, tmp_path: Path
    ) -> None:
        code, calls, _ = cascade(
            [source(f"{CONNECTOR}-legacy", "src-legacy")], tmp_path
        )

        assert code == 0
        assert "DELETE-SOURCE src-legacy" in calls
        assert not any(line.startswith("DELETE-CRONWORKFLOW") for line in calls), (
            "an instance was invented for a source that named none"
        )
        assert f"DELETE-SUPERSEDED {CONNECTOR}" in calls, (
            "the shapes that name no instance are still the cascade's to remove"
        )

    def test_the_log_says_what_was_left_behind(self, tmp_path: Path) -> None:
        _, _, stderr = cascade(
            [source(f"{CONNECTOR}-legacy", "src-legacy")], tmp_path
        )

        assert "names no instance" in stderr


class TestASourceThatNamesOne:
    def test_both_the_source_and_its_schedule_go(self, tmp_path: Path) -> None:
        """The other half: reading the instance out of the name is what the
        cascade does, and it must keep doing it."""
        code, calls, stderr = cascade(
            [source(f"{CONNECTOR}-{CONNECTOR}-main-{TENANT}", "src-main")], tmp_path
        )

        assert code == 0, stderr
        assert "DELETE-SOURCE src-main" in calls
        assert f"DELETE-CRONWORKFLOW {CONNECTOR} {CONNECTOR}-main" in calls


class TestASiblingConnectorSharingTheNamePrefix:
    def test_its_sources_are_not_this_connectors_to_delete(
        self, tmp_path: Path
    ) -> None:
        """`claude-team` prefixes `claude-team-invoices`, and a source of the
        longer one begins with the shorter one's name plus a separator. Removing
        the shorter one's Secret must not take the longer one's data with it —
        the two are separate installations of separate connectors."""
        code, calls, stderr = cascade(
            [
                source(f"{CONNECTOR}-{CONNECTOR}-main-{TENANT}", "src-main"),
                source(
                    f"{CONNECTOR}-invoices-{CONNECTOR}-invoices-main-{TENANT}", "src-inv"
                ),
            ],
            tmp_path,
        )

        assert code == 0, stderr
        assert "DELETE-SOURCE src-main" in calls
        assert "DELETE-SOURCE src-inv" not in calls, (
            "the cascade deleted a source belonging to another connector"
        )
        assert not any("invoices" in line for line in calls if "CRONWORKFLOW" in line)
