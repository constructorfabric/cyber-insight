---
status: accepted
date: 2026-09-10
---

# ADR-0006: The Assistant Answers From the Model, or Says It Cannot


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

**ID**: `cpt-insightspec-v3-adr-the-assistant-answers-from-the-model`

## Context and Problem Statement

`POST /v1/chat` turns a question into either an answer over the warehouse or a
set of definitions to store. It needs an Anthropic key. Not every stand has
one, and the first release shipped a `chat_mode: canned` setting whose backend
replied with a fixed proposal built from the message's first word.

## Decision Drivers

* A reply the reader is expected to act on has to come from the model.
* A stand whose key was never set should say so, to the reader and to whoever
  deployed it.
* The rest of the service — ingest, definitions, MCP, the catalogue — must
  serve without a key, including in CI.

## Considered Options

* Keep canned mode for stands without a key.
* One live backend, and refuse to boot without a key.
* One live backend, and refuse the request without a key.

## Decision Outcome

There is one backend. A blank key gives `ChatError::NoKey` and the endpoint
answers that the assistant is not configured on this instance; the service
still starts and everything else still works.

### Consequences

* There is no offline reply to demonstrate the chat panel with; a test scripts
  the proposal it wants instead.
* A stand that wants the assistant configures a key; one that does not loses
  exactly one endpoint.
* Nothing invents metric names out of a stray word any more.

### Confirmation

A config test proves a blank key still validates, and an API test proves the
endpoint refuses rather than answering.

## Pros and Cons of the Options

* **Canned mode** — any stand can answer; the answer is fiction that reads as
  real, and the missing key stays invisible.
* **Refuse to boot** — the misconfiguration is loud; a stand that never wanted
  the assistant cannot run the service at all, CI included.
* **Refuse the request** — the misconfiguration surfaces where it matters and
  costs nothing elsewhere; a reader only learns of it by asking.

## Traceability

- **PRD**: [PRD.md](../PRD.md)
- **DESIGN**: [DESIGN.md](../DESIGN.md)
