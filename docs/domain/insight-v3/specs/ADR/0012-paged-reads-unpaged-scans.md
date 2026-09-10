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
- [Pros and Cons of the Options](#pros-and-cons-of-the-options)
- [Traceability](#traceability)

<!-- /toc -->

**ID**: `cpt-insightspec-v3-adr-paged-reads-unpaged-scans`

## Context and Problem Statement

The catalogues of metrics, widgets and dashboards outgrew a screen, so reading
them needs paging and a total. The same store answers a different question at
delete time: which definitions depend on this one.

## Decision Drivers

* A reader needs a page, a total and a search over the whole catalogue.
* A dependency check that missed a row outside the current page would allow a
  deletion that breaks a board.

## Considered Options

* Page every read, and let the dependency scan walk the pages.
* Page the reads, and keep an unpaged read for the scan.

## Decision Outcome

The read endpoints take a limit and an offset and answer with a total; the
definitions trait keeps an unpaged `list` that only the dependency scan calls.

### Consequences

* Two read paths over one table, one of which has no user-facing caller.
* The scan's cost grows with the catalogue; it runs on delete, not on render.

### Confirmation

The definitions tests cover the paged answer with its total, and deleting a
definition another one draws is refused by name.

## Pros and Cons of the Options

* **Page everything** — one read path; a scan that forgets to walk every page
  silently permits a bad delete.
* **Unpaged scan** — the check sees the whole catalogue by construction; the
  trait carries a method the API never exposes.

## Traceability

- **PRD**: [PRD.md](../PRD.md)
- **DESIGN**: [DESIGN.md](../DESIGN.md)
