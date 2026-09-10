---
status: accepted
date: 2026-09-09
---

# ADR-0005: MCP Clients Authorize in a Browser, and Person-Scoped Tokens Come Later


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

**ID**: `cpt-insightspec-v3-adr-browser-authorization-for-mcp`

## Context and Problem Statement

Authoring happens over MCP: a client calls `put_metric`, `put_widget` and
`put_dashboard` with an access token carrying the `mcp:author` scope. The
clients are developer tools on a laptop — a terminal agent, an editor — and
something has to give them that token.

## Decision Drivers

* A person's authority to author has to be the authority they already have on
  the stand, not a second credential list.
* A grant must be revocable and must expire, because the people holding one
  change.
* The first release has to work for a person at a keyboard.

## Considered Options

* An OAuth 2.1 authorization code obtained in a browser.
* A long-lived person-scoped API token issued by the platform.
* One shared static token per instance, like the ingest token.

## Decision Outcome

Browser authorization ships first: the client discovers the resource from the
protected-resource metadata, the person signs in, and the gateway issues a
scoped token. Person-scoped tokens are planned beside it rather than instead of
it, for clients that cannot open a browser.

### Consequences

* A person re-authorizes when the grant expires; nothing renews silently.
* A headless client — CI, a scheduled job — has no path to authoring yet, and
  waits for the person-scoped token.
* The gateway stays the only issuer, so revoking a person's access to the stand
  revokes their authoring.

### Confirmation

[tests/mcp.sh](../../../../src/backend/services/insight-v3-core/tests/mcp.sh)
drives the challenge and the metadata document without a credential; the
authoring half of that script runs only when a browser-issued token is
exported, which is the shape of the decision.

## Pros and Cons of the Options

* **Browser authorization** — the person's own identity, scoped and expiring;
  needs a browser, so no headless client.
* **Person-scoped token** — works headless; a long-lived secret to store and
  rotate, and none exists yet.
* **Static per-instance token** — trivial to ship; every author is the same
  author, and nothing can be revoked for one person.

## Traceability

- **PRD**: [PRD.md](../PRD.md)
- **DESIGN**: [DESIGN.md](../DESIGN.md)
