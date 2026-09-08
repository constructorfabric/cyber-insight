#!/usr/bin/env python3
"""The `sources/list` payload: which sources are readable, and whose they are.

Both readers splice what they emit into a TSV their caller turns into deletions,
so a record that is not three strings is not one to act on: `"sourceId": null`
reaches the shell as the literal `None` and is deleted as if it were an id, and
a name that is not a string ends the reader mid-listing while the caller — which
suppresses the failure — carries on as though the listing held nothing.

An unreadable record is skipped and named on stderr rather than ending the run.
These callers delete what the listing describes, so a record nobody can read is
one to leave alone, not a reason to stop reading the rest of them.

INVARIANT: ownership comes from the source's `sourceDefinitionId`, never from
the shape of its name where anything else can answer. A source is named
`{connector}-{source_id}-{tenant}` and a source id is arbitrary, so one
connector's source can spell another connector's name exactly:
`claude-team` with the source id `invoices-main` under tenant `default` is
`claude-team-invoices-main-default`, which reads as `claude-team-invoices`'s.
Every caller here is on the way to a deletion, so the name is a fallback and
the fallback refuses whatever it cannot answer alone.
"""

from __future__ import annotations

import json
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any, TextIO


@dataclass(frozen=True)
class SourceRecord:
    """One Airbyte source, reduced to the three fields the readers use."""

    source_id: str
    name: str
    #: The definition the source was created against. Empty only where the
    #: listing carried none, which is where ownership falls back to the name.
    definition_id: str


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


def load_definitions(path: str, subject: str) -> dict[str, str] | None:
    """`sourceDefinitionId` → connector name, for the definitions we manage.

    Only `custom` definitions: those are the ones this loop publishes under a
    connector's own name (ADR-0009). A marketplace definition that happens to
    carry the same name belongs to whoever installed it, not to a connector
    here.
    """
    try:
        listed = json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        sys.stderr.write(f"{subject}: cannot read the definition listing: {exc}\n")
        return None
    if not isinstance(listed, list):
        sys.stderr.write(f"{subject}: the definition listing is not a list\n")
        return None

    owners: dict[str, str] = {}
    for item in listed:
        if not isinstance(item, dict) or item.get("custom") is not True:
            continue
        definition_id, name = item.get("sourceDefinitionId"), item.get("name")
        if isinstance(definition_id, str) and definition_id and isinstance(name, str) and name:
            owners[definition_id] = name
    return owners


def owner_of(
    record: SourceRecord,
    definitions: dict[str, str],
    connectors: set[str],
) -> str | None:
    """Which connector a source belongs to, or None when nothing establishes it.

    The definition answers first and alone: Airbyte created the source against
    it, so it is a recorded fact rather than a reading of a name.

    The name is asked only when the definition cannot answer — a definition
    already deleted, or a listing that carried none — and then only when exactly
    one connector could own the name. Several is not a tie to break: the
    counterexample above is precisely a name two connectors can both claim, and
    picking either of them by any rule deletes one connector's data under the
    other's name.
    """
    named = definitions.get(record.definition_id)
    if named is not None:
        return named
    candidates = [
        name
        for name in connectors
        if record.name == name or record.name.startswith(f"{name}-")
    ]
    return candidates[0] if len(candidates) == 1 else None


def _record(item: Any) -> SourceRecord | None:
    if not isinstance(item, dict):
        return None
    source_id, name = item.get("sourceId"), item.get("name")
    if not isinstance(source_id, str) or not source_id:
        return None
    if not isinstance(name, str):
        return None
    definition_id = item.get("sourceDefinitionId")
    return SourceRecord(
        source_id, name, definition_id if isinstance(definition_id, str) else ""
    )
