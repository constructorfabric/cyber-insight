# ADR-0003: Latest-Per-Source Lookup Semantics

**ID**: `cpt-insightspec-adr-0003-latest-per-source-semantics`



<!-- toc -->

- [Context and Problem Statement](#context-and-problem-statement)
- [Decision Drivers](#decision-drivers)
- [Considered Options](#considered-options)
- [Decision Outcome](#decision-outcome)
  - [Consequences](#consequences)
  - [Confirmation](#confirmation)
- [Pros and Cons of the Options](#pros-and-cons-of-the-options)
  - [Latest-per-source (chosen)](#latest-per-source-chosen)
  - [Latest-per-(tenant, person, attribute)](#latest-per-tenant-person-attribute)
- [More Information](#more-information)
- [Traceability](#traceability)

<!-- /toc -->

**Status:** Accepted

## Context and Problem Statement

`persons` is append-only — the same `(tenant, person, source_type,
source_id, value_type)` may have many rows over time as the source
publishes new values. The service must decide which row "represents"
the current value of an attribute. The question is whether to collapse
across sources first and then pick the most recent row, or to pick the
most recent row per source first and collapse across sources second.

## Decision Drivers

- Source-level conflicts (BambooHR vs Cursor disagree on
  `display_name`) must remain visible to the assembler so they can be
  resolved deterministically.
- The lookup SQL must reuse the same projection the seed and the
  `account_person_map` rebuild already use, to keep the identity
  domain SQL consistent.
- Email rebinding (an account's upstream email changes) must invalidate
  the old email — silently resolving stale emails is a data-leak risk.

## Considered Options

- Latest-per-(tenant, person, attribute) — collapse across sources
  first, then pick the most recent row.
- Latest-per-source per (tenant, person, source_type, source_id,
  value_type) — pick the most recent row per source, then collapse
  across sources at the assembler level.

## Decision Outcome

Use latest-per-source. For email lookup we additionally require the
latest row per `(value_type='email', value_id=…)` partition to map to
the queried email — otherwise the lookup misses.

### Consequences

- The lookup query is a CTE with `ROW_NUMBER() OVER PARTITION BY`. The
  index `idx_value_id (insight_tenant_id, value_type, value_id)`
  covers the email lookup; the partition columns sit on
  `idx_tenant_person`.
- Conflict resolution is documented as "max created_at across
  sources"; Phase 2 may revisit this with an explicit source-priority
  table.
- Email rebinding makes the old email cease to resolve: the latest row
  per `(source_type, source_id, 'email', value_id=old)` no longer
  reflects the current binding, so the lookup returns 404. This is
  the agreed behaviour.

### Confirmation

Confirmed by integration tests that seed multi-source observations for
the same `person_id` and assert the assembler picks the latest value
per `value_type` (`PersonsEndpointTests.LatestPerSourceWins`).
Negative test confirms an obsoleted email no longer resolves to the
person it used to bind.

## Pros and Cons of the Options

### Latest-per-source (chosen)

- Good, because source-level conflicts remain visible to the assembler
  for deterministic resolution.
- Good, because the same projection is reused by the seed and
  `account_person_map` rebuild — one SQL shape across the domain.
- Good, because the existing `idx_value_id` and `idx_tenant_person`
  indexes cover the access pattern.
- Bad, because the lookup query is a CTE with a window function —
  slightly more complex than a `GROUP BY` collapse.

### Latest-per-(tenant, person, attribute)

- Good, because the SQL is simpler — `MAX(created_at)` and a join.
- Bad, because conflict information is lost before the assembler sees
  it; future source-priority logic would need to re-fetch rows.
- Bad, because it diverges from the seed pipeline's projection —
  identity-domain SQL would have two shapes.

## More Information

- `docs/domain/identity-resolution/specs/DESIGN.md` §"Table: persons"
  for the `ROW_NUMBER()` projection the seed and the reader share.

## Traceability

- [`cpt-insightspec-fr-identity-lookup-resolve-by-email`](../PRD.md#address-matching-uses-the-current-value-per-source)
- [`cpt-insightspec-fr-identity-lookup-hydrate`](../PRD.md#compose-attributes-from-every-source)
- [`cpt-insightspec-principle-identity-centralised-sql`](../DESIGN.md#centralised-sql)
