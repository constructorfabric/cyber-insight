#!/usr/bin/env python3
"""Which Airbyte sources belong to one connector, and which instance each is.

CLI:
  select_connector_sources.py <connector> <tenant>

Stdin:  the `sources/list` payload (a JSON array of source objects).
Stdout: TSV `airbyte_source_id<TAB>instance_source_id` per matching source.
Exit:   0 always; 2 on bad arg count, 1 on a payload that is not JSON.

Sources are named `{connector}-{source_id}-{tenant}` by the reconcile loop, so
the instance's own id is what is left once the two known ends are removed. A
source whose name does not carry both ends is emitted with an empty instance id
rather than dropped: the caller still has to delete it, and guessing an id for
it would name an instance that never existed.

INVARIANT: the connector end is matched as `{connector}-`, never as a bare
prefix. Connector slugs share prefixes (`claude-team` and `claude-team-invoices`),
and a bare prefix match would hand one connector's sources to the other.
"""

import json
import sys
from typing import Any


def instance_of(name: str, connector: str, tenant: str) -> str:
    """The instance id inside a source name, or empty when it carries none."""
    head = f"{connector}-"
    tail = f"-{tenant}"
    if not name.startswith(head) or not name.endswith(tail):
        return ""
    return name[len(head) : len(name) - len(tail)]


def belongs_to(name: str, connector: str) -> bool:
    return name == connector or name.startswith(f"{connector}-")


def main() -> int:
    if len(sys.argv) != 3:
        sys.stderr.write("select_connector_sources: expected <connector> <tenant>\n")
        return 2
    connector, tenant = sys.argv[1], sys.argv[2]
    try:
        sources: list[dict[str, Any]] = json.load(sys.stdin)
    except json.JSONDecodeError as exc:
        sys.stderr.write(f"select_connector_sources: bad JSON on stdin: {exc}\n")
        return 1

    for source in sources:
        name = source.get("name", "") or ""
        if not belongs_to(name, connector):
            continue
        print(f"{source.get('sourceId', '')}\t{instance_of(name, connector, tenant)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
