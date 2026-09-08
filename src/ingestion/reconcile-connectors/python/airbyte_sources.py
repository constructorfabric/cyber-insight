#!/usr/bin/env python3
"""The `sources/list` payload: which sources are readable, and whose they are.

Both readers splice what they emit into a TSV their caller turns into deletions,
so a record that is not two strings is not one to act on: `"sourceId": null`
reaches the shell as the literal `None` and is deleted as if it were an id, and
a name that is not a string ends the reader mid-listing while the caller — which
suppresses the failure — carries on as though the listing held nothing.

An unreadable record is skipped and named on stderr rather than ending the run.
These callers delete what the listing describes, so a record nobody can read is
one to leave alone, not a reason to stop reading the rest of them.
"""

from __future__ import annotations

import json
import sys
from dataclasses import dataclass
from typing import Any, TextIO


@dataclass(frozen=True)
class SourceRecord:
    """One Airbyte source, reduced to the two fields either reader uses."""

    source_id: str
    name: str


def decode(stream: TextIO, subject: str) -> list[SourceRecord] | None:
    """Every readable source in the payload, or None when it is not a listing."""
    try:
        payload: Any = json.load(stream)
    except json.JSONDecodeError as exc:
        sys.stderr.write(f"{subject}: bad JSON on stdin: {exc}\n")
        return None
    if not isinstance(payload, list):
        sys.stderr.write(f"{subject}: the payload is not a list of sources\n")
        return None

    records: list[SourceRecord] = []
    for position, item in enumerate(payload):
        record = _record(item)
        if record is None:
            sys.stderr.write(f"{subject}: source {position} is unreadable — skipping\n")
            continue
        records.append(record)
    return records


def owner_of(source_name: str, connectors: set[str]) -> str | None:
    """Which of these connectors a source name belongs to, or None.

    INVARIANT: the LONGEST name that fits, never the first. Connector slugs
    prefix one another — `claude-team` prefixes `claude-team-invoices` — and a
    source of the longer one begins with the shorter one's name plus a
    separator, so a first-fit match hands one connector every source of the
    other and deletes them under its name.
    """
    candidates = [
        name
        for name in connectors
        if source_name == name or source_name.startswith(f"{name}-")
    ]
    return max(candidates, key=len) if candidates else None


def _record(item: Any) -> SourceRecord | None:
    if not isinstance(item, dict):
        return None
    source_id, name = item.get("sourceId"), item.get("name")
    if not isinstance(source_id, str) or not source_id:
        return None
    if not isinstance(name, str):
        return None
    return SourceRecord(source_id, name)
