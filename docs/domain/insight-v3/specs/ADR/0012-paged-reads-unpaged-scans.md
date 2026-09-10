---
status: accepted
date: 2026-09-09
---

# ADR-0012: List Endpoints Page; the Dependency Scan Does Not


<!-- toc -->

- [Context and Problem Statement](#context-and-problem-statement)
- [Decision Drivers](#decision-drivers)
- [Considered Options](#considered-options)
- [Decision Outcome](#decision-outcome)
  - [Consequences](#consequences)
  - [Confirmation](#confirmation)
- [Traceability](#traceability)

<!-- /toc -->

**ID**: `cpt-insightspec-v3-adr-paged-reads-unpaged-scans`

## Context and Problem Statement

The catalogues outgrew a screen. The same store answers a different question at
delete time: what depends on this definition.

## Decision Drivers

* A reader needs a page, a total and a search.
* A dependency check that misses a row permits a delete that breaks a board.

## Considered Options

* Page every read and walk the pages in the scan — one read path; a scan that
  forgets a page permits a bad delete silently.
* Page the reads and keep an unpaged read for the scan — the check sees
  everything by construction; the trait carries a method the API never
  exposes. **Chosen.**

## Decision Outcome

Reads take a limit and an offset and answer with a total. The definitions trait
keeps an unpaged `list`, called only by the dependency scan.

### Consequences

* Two read paths, one with no user-facing caller.
* The scan's cost grows with the catalogue; it runs on delete, not on render.

### Confirmation

The definitions tests cover the paged answer and its total; deleting a
definition another draws is refused by name.

## Traceability

- **PRD**: [PRD.md](../PRD.md)
- **DESIGN**: [DESIGN.md](../DESIGN.md)
