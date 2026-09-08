"""A source record nobody can read is left alone rather than acted on.

Both readers of the `sources/list` payload emit TSV their caller turns straight
into deletions, and both callers suppress a non-zero exit. So a record that is
not two strings has two ways to do damage: `"sourceId": null` reaching the shell
as the literal `None` — an id the caller then deletes and whose CronWorkflow it
removes — and a name that is not a string ending the reader mid-listing, which
the caller reads as "the listing held nothing".

Run: pytest src/ingestion/reconcile-connectors/tests
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SELECTOR = ROOT / "python" / "select_connector_sources.py"
FINDER = ROOT / "python" / "find_removed_instances.py"

CONNECTOR = "example-tracker"
TENANT = "example-tenant"
GOOD_NAME = f"{CONNECTOR}-{CONNECTOR}-main-{TENANT}"


def select(sources: list, tmp_path: Path) -> subprocess.CompletedProcess[str]:
    known = tmp_path / "known.json"
    known.write_text(json.dumps([CONNECTOR]), encoding="utf-8")
    return subprocess.run(
        [sys.executable, str(SELECTOR), CONNECTOR, TENANT, str(known)],
        input=json.dumps(sources),
        capture_output=True,
        text=True,
        check=False,
    )


def find(sources: list, tmp_path: Path) -> subprocess.CompletedProcess[str]:
    plan = tmp_path / "plan.tsv"
    plan.write_text(
        "\t".join(
            [CONNECTOR, "dir", "1", "nocode", "", "", "", "bronze", "main", "secret", "hash"]
        )
        + "\n",
        encoding="utf-8",
    )
    return subprocess.run(
        [sys.executable, str(FINDER), str(plan), TENANT],
        input=json.dumps(sources),
        capture_output=True,
        text=True,
        check=False,
    )


class TestARecordWithNoUsableId:
    def test_the_selector_emits_nothing_for_it(self, tmp_path: Path) -> None:
        result = select([{"name": GOOD_NAME, "sourceId": None}], tmp_path)

        assert result.returncode == 0, result.stderr
        assert result.stdout == "", "a null id was emitted as a value to delete"
        assert "None" not in result.stdout

    def test_the_finder_emits_nothing_for_it(self, tmp_path: Path) -> None:
        removed = f"{CONNECTOR}-{CONNECTOR}-gone-{TENANT}"
        result = find([{"name": removed, "sourceId": None}], tmp_path)

        assert result.returncode == 0, result.stderr
        assert result.stdout == ""


class TestARecordThatIsNotTwoStrings:
    def test_a_non_string_name_does_not_end_the_listing(self, tmp_path: Path) -> None:
        """The readable records after it still have to be answered for: the
        caller cannot tell a crash from an empty listing, and an empty listing
        means "nothing to remove"."""
        result = select(
            [{"name": 7, "sourceId": "src-bad"}, {"name": GOOD_NAME, "sourceId": "src-good"}],
            tmp_path,
        )

        assert result.returncode == 0, result.stderr
        assert result.stdout.splitlines() == [f"src-good\t{CONNECTOR}-main"]
        assert "unreadable" in result.stderr

    def test_a_record_that_is_not_an_object_is_skipped(self, tmp_path: Path) -> None:
        result = select(["not-a-source", {"name": GOOD_NAME, "sourceId": "src-good"}], tmp_path)

        assert result.returncode == 0, result.stderr
        assert result.stdout.splitlines() == [f"src-good\t{CONNECTOR}-main"]


class TestAPayloadThatIsNotAListing:
    def test_the_selector_refuses_it(self, tmp_path: Path) -> None:
        """Not a listing at all is different from a listing with a bad record in
        it: nothing in it can be trusted, so nothing is emitted and the exit code
        says so."""
        result = select({"sources": []}, tmp_path)  # type: ignore[arg-type]

        assert result.returncode == 1
        assert result.stdout == ""
