---
status: accepted
date: 2026-09-09
---

# ADR-0009: A Dashboard Is an Ordered List of Items, Not a List of Widget Names


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

**ID**: `cpt-insightspec-v3-adr-a-dashboard-is-an-item-list`

## Context and Problem Statement

A dashboard body held `widgets`: the names to draw, in order. Nothing could sit
between two widgets, so a board could not be split into sections or labelled,
and the assistant had no way to rearrange one.

## Decision Drivers

* The assistant must be able to reorder a board and title the groups on it.
* Boards already stored have to keep rendering.
* A body the model writes must be refused when it is not understood, rather
  than half-read.

## Considered Options

* A second field beside `widgets` holding the labels and their positions.
* A layout grid with coordinates per item.
* One ordered `items` list whose entries are a widget, a heading or a text
  block.

## Decision Outcome

`items` is the shape written from now on. A reader still accepts a body that
carries only `widgets`, and every item kind rejects unknown fields.

### Consequences

* Two shapes to read, one to write, until nothing stores the old one.
* Rearranging is a write of the whole list, so two authors editing one board at
  once lose one of the two orders.
* Anything the renderer does not know is refused at authoring time instead of
  rendering as a gap.

### Confirmation

The dashboard unit tests read both shapes and assert that arranging a board
keeps its title and refuses a name no widget has.

## Pros and Cons of the Options

* **A second field** — leaves the old shape untouched; two fields must agree
  about position, and the model gets that wrong.
* **A layout grid** — expresses any arrangement; the assistant has to reason
  about geometry to add one heading.
* **An item list** — the reading order is the list order, and headings are just
  items; every board has to be read through two shapes for a while.

## Traceability

- **PRD**: [PRD.md](../PRD.md)
- **DESIGN**: [DESIGN.md](../DESIGN.md)
