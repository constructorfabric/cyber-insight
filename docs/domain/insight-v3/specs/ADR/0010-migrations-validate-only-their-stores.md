---
status: accepted
date: 2026-09-09
---

# ADR-0010: A Migration Validates the Stores It Writes


<!-- toc -->

- [Context and Problem Statement](#context-and-problem-statement)
- [Decision Drivers](#decision-drivers)
- [Considered Options](#considered-options)
- [Decision Outcome](#decision-outcome)
  - [Consequences](#consequences)
  - [Confirmation](#confirmation)
- [Traceability](#traceability)

<!-- /toc -->

**ID**: `cpt-insightspec-v3-adr-migrations-validate-only-their-stores`

## Context and Problem Statement

`migrate` and the serving process read one config. Validating all of it made the
command demand an identity URL, a chat key and an MCP origin, one refusal at a
time.

## Decision Drivers

* Schema must come up on a stand that serves nothing.
* A failure should name a setting the command reads.

## Considered Options

* One validation for both paths — nothing to keep in step; a migration fails
  for want of a service it never calls.
* Make the serving settings optional everywhere — both paths pass; a server
  starts missing what it needs.
* A narrower validation for the migration path — each command asks for what it
  uses; two places to add a setting to. **Chosen.**

## Decision Outcome

`stores_from_app_config` validates the warehouse and the definition store and
returns those two. Serving keeps its full validation.

### Consequences

* Two validation paths over one config, to keep in step.
* A serving misconfiguration surfaces at boot, where it belongs.

### Confirmation

CI migrates with neither an identity URL nor a chat key set.

## Traceability

- **PRD**: [PRD.md](../PRD.md)
- **DESIGN**: [DESIGN.md](../DESIGN.md)
