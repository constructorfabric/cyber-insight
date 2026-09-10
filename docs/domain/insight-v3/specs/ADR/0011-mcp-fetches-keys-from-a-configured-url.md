---
status: accepted
date: 2026-09-09
---

# ADR-0011: The MCP Server Takes a Separate URL for the Signing Keys


<!-- toc -->

- [Context and Problem Statement](#context-and-problem-statement)
- [Decision Drivers](#decision-drivers)
- [Considered Options](#considered-options)
- [Decision Outcome](#decision-outcome)
  - [Consequences](#consequences)
  - [Confirmation](#confirmation)
- [Traceability](#traceability)

<!-- /toc -->

**ID**: `cpt-insightspec-v3-adr-mcp-fetches-keys-from-a-configured-url`

## Context and Problem Statement

A client discovers this resource from `mcp.public_url`. The server verifies
tokens against the issuer's JWKS — and on a stand that URL is `localhost`, which
inside the container is the container, not the gateway.

## Decision Drivers

* One client-facing URL: the audience and the metadata.
* The server has to reach the keys from where it runs.

## Considered Options

* Derive the key URL from the public URL — nothing to configure; cannot verify
  on a stand whose origin is not routable from inside.
* Configure an internal base URL and derive endpoints from it — covers future
  endpoints; a second description of the issuer, free to disagree.
* Configure the key URL alone, blank deriving it — names the one call that
  routes differently; a stand that needs it must know to set it. **Chosen.**

## Decision Outcome

`mcp.jwks_url` names where keys are fetched. Blank derives it from the public
URL.

### Consequences

* A local stand sets one variable pointing at the gateway.
* Pointing it at the wrong issuer shows up as refused tokens.

### Confirmation

Compose sets it to the gateway's JWKS; the MCP test asserts the challenge still
names the public resource and its scope.

## Traceability

- **PRD**: [PRD.md](../PRD.md)
- **DESIGN**: [DESIGN.md](../DESIGN.md)
