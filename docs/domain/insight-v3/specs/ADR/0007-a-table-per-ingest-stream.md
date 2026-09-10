---
status: accepted
date: 2026-09-08
---

# ADR-0007: One Physical Table per Ingest Stream, All With the Same Fixed Schema


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

**ID**: `cpt-insightspec-v3-adr-a-table-per-ingest-stream`

## Context and Problem Statement

Raw data arrives as a JSON payload named by its stream. The migration creates
`raw_data`; `PUT /v1/tables/{table}` creates further tables with the identical
column set, and an insert names the stream it belongs to.

## Decision Drivers

* A stream's rows have to be readable on their own, cheaply, without every
  query filtering the whole ingest history.
* The schema is ours, not the caller's: a caller may ask for a table, never for
  its columns.
* A table name reaches DDL, so it must be constrained rather than escaped.

## Considered Options

* One `raw_data` table for everything, told apart by a `table_name` column.
* A table per stream, with a schema the caller describes.
* A table per stream, all with the same fixed schema.

## Decision Outcome

A table per stream, every one with `id`, `table_name`, `raw_data` and
`received_at`, ordered the same way. Creation is admin-only and the name must
match `^[A-Za-z0-9_]{1,128}$`. `raw_data` is simply the stream the migration
brings up.

### Consequences

* The table count grows with the number of streams a stand ingests.
* The catalogue tells our tables from every other service's in the same
  database by the sorting key the fixed schema gives them.
* `table_name` stays on every row even though the table already implies it: it
  is the provenance a reader sees in the payload.
* Creation is a separate request from ingest, so a connector's first write
  needs an administrator once.

### Confirmation

`tables::tests::table_creation_uses_the_fixed_raw_data_schema` pins the DDL,
and the k3s functional test creates a stream's table, writes to it, and counts
the row in that table rather than in `raw_data`.

## Pros and Cons of the Options

* **One table** — nothing to create, one place to look; every read scans or
  filters all streams, and one noisy stream sets the cost for the rest.
* **Caller-described schema** — fits the source exactly; makes the caller the
  owner of our storage and the DDL a request body.
* **Fixed schema per stream** — reads stay per stream and the shape stays ours;
  one more request before the first write, and more tables to list.

## Traceability

- **PRD**: [PRD.md](../PRD.md)
- **DESIGN**: [DESIGN.md](../DESIGN.md)
