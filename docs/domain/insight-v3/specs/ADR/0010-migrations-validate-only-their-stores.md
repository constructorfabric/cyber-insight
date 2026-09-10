---
status: accepted
date: 2026-09-09
---

# ADR-0010: A Migration Validates the Stores It Writes, and Nothing Else


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

**ID**: `cpt-insightspec-v3-adr-migrations-validate-only-their-stores`

## Context and Problem Statement

The `migrate` subcommand and the serving process read one gear config.
Validating that whole config before migrating made the command demand an
identity URL, a chat key and an MCP origin — none of which a migration reads —
and each missing setting surfaced only after the last one was supplied.

## Decision Drivers

* A stand must be able to bring its schema up before it can serve anything.
* A failure should name a setting the command actually uses.
* The serving path must keep validating everything it needs, at boot.

## Considered Options

* One validation for both paths.
* Make the serving settings optional everywhere.
* A second, narrower validation for the migration path.

## Decision Outcome

`stores_from_app_config` validates the warehouse and the definition store, and
returns those two. The serving path keeps its full validation.

### Consequences

* Two validation entry points over one config, which have to stay in step as
  settings are added.
* A serving misconfiguration is caught at boot rather than at migrate time,
  which is where it belongs.
* CI migrates with no identity service and no chat key.

### Confirmation

The migrate step in CI runs with neither an identity URL nor a chat key set.

## Pros and Cons of the Options

* **One validation** — nothing to keep in step; a migration fails for want of a
  service it never calls.
* **Optional everywhere** — both paths pass; a serving process starts with
  settings it needs missing, and fails on a reader's first request.
* **A narrower second validation** — each command asks for what it uses; two
  places to add a setting to.

## Traceability

- **PRD**: [PRD.md](../PRD.md)
- **DESIGN**: [DESIGN.md](../DESIGN.md)
