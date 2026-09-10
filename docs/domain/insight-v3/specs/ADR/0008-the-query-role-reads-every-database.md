---
status: accepted
date: 2026-09-09
---

# ADR-0008: The Assistant's Warehouse Role Reads Every Database


<!-- toc -->

- [Context and Problem Statement](#context-and-problem-statement)
- [Decision Drivers](#decision-drivers)
- [Considered Options](#considered-options)
- [Decision Outcome](#decision-outcome)
  - [Consequences](#consequences)
  - [Confirmation](#confirmation)
- [Pros and Cons of the Options](#pros-and-cons-of-the-options)
- [Traceability](#traceability)

<!-- /toc -->

**ID**: `cpt-insightspec-v3-adr-the-query-role-reads-every-database`

## Context and Problem Statement

A metric compiles to a `SELECT` and runs as a read-only warehouse principal,
separate from the credential the service writes with. A stand gains a database
per connector it turns on, and a metric may be written over any of them.

## Decision Drivers

* A metric over a newly connected source must run without anyone editing a
  grant first.
* The path the assistant queries through must not be able to write.
* Nothing in this service hardcodes a database name.

## Considered Options

* Grant `SELECT` on the databases a stand has today, and extend the grant per
  connector.
* Grant `SELECT` on everything.

## Decision Outcome

`GRANT SELECT ON *.*` to the read-only user. Every metric on the stand can be
answered; nothing on that connection can write.

### Consequences

* The role can read every database on the instance, including those belonging
  to other services.
* Whatever the assistant can be talked into reading, it can only read — data
  the instance already holds, through a credential that cannot change it.
* Adding a connector needs no grant work, which is the point.

### Confirmation

The ledger-grant tests assert the reader cannot write, and that a caller with
no role at all is refused.

## Pros and Cons of the Options

* **Per-database grants** — the role sees only what it was meant to; every new
  connector is a migration nobody remembers to write, and the failure looks
  like a broken metric.
* **Read everything** — every metric works the day its data lands; the blast
  radius of a bad query is every table on the instance, bounded to reads.

## Traceability

- **PRD**: [PRD.md](../PRD.md)
- **DESIGN**: [DESIGN.md](../DESIGN.md)
