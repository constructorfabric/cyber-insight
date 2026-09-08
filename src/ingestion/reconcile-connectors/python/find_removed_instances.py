#!/usr/bin/env python3
"""Sources belonging to an instance the install no longer configures.

CLI:
  find_removed_instances.py <plan_tsv> <tenant> [connector]

Stdin:  the `sources/list` payload.
Stdout: TSV `airbyte_source_id<TAB>connector<TAB>instance` per source to remove.
Exit:   0 always; 2 on bad arg count; 1 on a payload that is not a source listing.

This is the sibling case of the cascade: a connector configured twice loses one
Secret, so one instance must go while the other keeps running. The cascade
cannot do it — it fires only when a connector has no Secret at all — and the
orphan GC cannot either, since the connector is still very much known.

Three refusals, each of them load-bearing:

* A connector with NO planned instance is left alone. Its removal belongs to the
  cascade, which also takes the definition and the schedules with it; taking its
  sources here would race that and report the same removal twice.
* A source whose name does not carry both the connector and the tenant is left
  alone. The instance it belongs to cannot be read out of it, and deleting a
  source whose owner is unknown is how a healthy connector loses its data.
* The connector is resolved by LONGEST-prefix match against the planned names,
  never by the first that fits. `claude-team` is a prefix of
  `claude-team-invoices`, so the short one would otherwise claim the long one's
  sources and delete every one of them.
"""

import sys
from pathlib import Path

from airbyte_sources import decode, owner_of

PLAN_SOURCE_ID = 8
PLAN_SECRET_NAME = 9


def _planned(plan_path: str) -> tuple[dict[str, set[str]], set[str]]:
    """Instances per connector, and the connectors the plan has a row for."""
    instances: dict[str, set[str]] = {}
    known: set[str] = set()
    for line in Path(plan_path).read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        fields = line.split("\t")
        if not fields[0]:
            continue
        known.add(fields[0])
        if len(fields) <= PLAN_SECRET_NAME or not fields[PLAN_SECRET_NAME]:
            continue
        instances.setdefault(fields[0], set()).add(fields[PLAN_SOURCE_ID])
    return instances, known


def instance_of(source_name: str, connector: str, tenant: str) -> str:
    head, tail = f"{connector}-", f"-{tenant}"
    if not source_name.startswith(head) or not source_name.endswith(tail):
        return ""
    return source_name[len(head) : len(source_name) - len(tail)]


def main() -> int:
    if not 3 <= len(sys.argv) <= 4:
        sys.stderr.write("find_removed_instances: expected <plan_tsv> <tenant> [connector]\n")
        return 2
    plan_path, tenant = sys.argv[1], sys.argv[2]
    only = sys.argv[3] if len(sys.argv) == 4 else ""

    instances, _known = _planned(plan_path)
    sources = decode(sys.stdin, "find_removed_instances")
    if sources is None:
        return 1

    # Only connectors the plan still configures: a connector with no instance
    # left is the cascade's to remove, not this pass's.
    configured = set(instances)
    if only:
        configured &= {only}

    for source in sources:
        connector = owner_of(source.name, configured)
        if connector is None:
            continue
        instance = instance_of(source.name, connector, tenant)
        if not instance or instance in instances[connector]:
            continue
        print("\t".join([source.source_id, connector, instance]))
    return 0


if __name__ == "__main__":
    sys.exit(main())
