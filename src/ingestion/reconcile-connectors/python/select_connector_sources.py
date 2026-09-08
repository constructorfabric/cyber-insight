#!/usr/bin/env python3
"""Which Airbyte sources belong to one connector, and which instance each is.

CLI:
  select_connector_sources.py <connector> <tenant> <known_connectors_file>

Args:   `known_connectors_file` holds `extract_descriptor_names.py` output — the
        JSON array of every connector this build ships.
Stdin:  the `sources/list` payload (a JSON array of source objects).
Stdout: TSV `airbyte_source_id<TAB>instance_source_id` per matching source.
Exit:   0 always; 2 on bad arg count, 1 on a payload that is not a source
        listing or an unreadable connector list.

Sources are named `{connector}-{source_id}-{tenant}` by the reconcile loop, so
the instance's own id is what is left once the two known ends are removed. A
source whose name does not carry both ends is emitted with an empty instance id
rather than dropped: the caller still has to delete it, and guessing an id for
it would name an instance that never existed.

INVARIANT: ownership is decided against every connector this build ships, by
longest match. `{connector}-` alone is not ownership — a source of
`claude-team-invoices` begins with `claude-team-` too, and this pass runs on the
way to deleting what it selects.
"""

import json
import sys
from pathlib import Path

from airbyte_sources import decode, owner_of


def instance_of(name: str, connector: str, tenant: str) -> str:
    """The instance id inside a source name, or empty when it carries none."""
    head = f"{connector}-"
    tail = f"-{tenant}"
    if not name.startswith(head) or not name.endswith(tail):
        return ""
    return name[len(head) : len(name) - len(tail)]


def main() -> int:
    if len(sys.argv) != 4:
        sys.stderr.write(
            "select_connector_sources: expected <connector> <tenant> <known_connectors_file>\n"
        )
        return 2
    connector, tenant, known_path = sys.argv[1], sys.argv[2], sys.argv[3]
    try:
        listed = json.loads(Path(known_path).read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        sys.stderr.write(f"select_connector_sources: cannot read the connector list: {exc}\n")
        return 1
    if not isinstance(listed, list):
        sys.stderr.write("select_connector_sources: the connector list is not a list\n")
        return 1
    known = {name for name in listed if isinstance(name, str) and name}
    # The connector being cascaded is one of them whether or not the caller's
    # listing named it, or nothing it owns would ever be selected.
    known.add(connector)

    sources = decode(sys.stdin, "select_connector_sources")
    if sources is None:
        return 1

    for source in sources:
        if owner_of(source.name, known) != connector:
            continue
        print(f"{source.source_id}\t{instance_of(source.name, connector, tenant)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
