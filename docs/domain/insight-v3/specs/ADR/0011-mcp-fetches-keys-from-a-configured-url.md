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
- [Pros and Cons of the Options](#pros-and-cons-of-the-options)
- [Traceability](#traceability)

<!-- /toc -->

**ID**: `cpt-insightspec-v3-adr-mcp-fetches-keys-from-a-configured-url`

## Context and Problem Statement

An MCP client discovers this resource from `mcp.public_url` — the audience its
token carries. The server verifies that token against the issuer's JWKS. On a
local stand the public URL is `localhost`, which inside the container names the
container and not the gateway, so the key fetch fails and every authorized call
is refused.

## Decision Drivers

* The client's view of this resource must stay one URL: the audience and the
  metadata it discovers.
* The server has to reach the keys from wherever it runs.
* A deployment whose public origin routes internally should need no new
  setting.

## Considered Options

* Derive the key URL from the public URL only.
* Configure an internal base URL and derive every endpoint from it.
* Configure the key URL alone, blank meaning "derive it from the public URL".

## Decision Outcome

`mcp.jwks_url` names where the keys are fetched. Blank derives it from the
public URL, which is what a deployment with a routable origin wants.

### Consequences

* A local stand sets one environment variable pointing at the gateway.
* The public URL remains the single client-facing identity; nothing else is
  duplicated.
* An operator can point key fetching at something the advertised issuer does
  not serve, and the mistake shows as refused tokens.

### Confirmation

Compose sets it to the gateway's JWKS endpoint, and the MCP test asserts the
challenge still names the public resource and its scope.

## Pros and Cons of the Options

* **Derive only** — nothing to configure; the server cannot start verifying on
  any stand whose public origin is not routable from inside.
* **An internal base URL** — one setting covers future endpoints; it is a
  second description of the issuer, and the two can disagree.
* **The key URL alone** — names exactly the one call that has to route
  differently; a stand that needs it must know to set it.

## Traceability

- **PRD**: [PRD.md](../PRD.md)
- **DESIGN**: [DESIGN.md](../DESIGN.md)
