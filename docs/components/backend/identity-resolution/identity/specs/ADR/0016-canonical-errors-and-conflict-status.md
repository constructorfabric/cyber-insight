# ADR-0016: Canonical Error Envelope, and Conflict as the Status for a Violated Invariant

<!-- toc -->

- [Context and Problem Statement](#context-and-problem-statement)
- [Decision Drivers](#decision-drivers)
- [Considered Options](#considered-options)
- [Decision Outcome](#decision-outcome)
  - [Consequences](#consequences)
  - [Confirmation](#confirmation)
- [Pros and Cons of the Options](#pros-and-cons-of-the-options)
  - [Adopt the framework's canonical envelope and map a violated invariant onto conflict (chosen)](#adopt-the-frameworks-canonical-envelope-and-map-a-violated-invariant-onto-conflict-chosen)
  - [Keep the specified 422 and bypass the framework for these refusals](#keep-the-specified-422-and-bypass-the-framework-for-these-refusals)
  - [Leave the documents as they were and treat the gap as known drift](#leave-the-documents-as-they-were-and-treat-the-gap-as-known-drift)
- [More Information](#more-information)
- [Traceability](#traceability)

<!-- /toc -->

**ID**: `cpt-insightspec-adr-0016-canonical-errors-and-conflict-status`

**Status:** Accepted — supersedes the wire mapping in ADR-0009 and ADR-0013

## Context and Problem Statement

Three earlier decisions in this set describe refusals in terms the service does
not implement. ADR-0009 specifies an ambiguous profile as `422 Unprocessable
Entity` with a problem body of type `urn:insight:error:ambiguous_profile`
carrying structured members — the offending lookup and the list of matched
persons. ADR-0013 specifies the role in-use guard as `422
urn:insight:error:role_in_use`. The PRD and DESIGN that grew around them
described an RFC 7807 envelope with a `urn:insight:error:*` type vocabulary
throughout.

None of that shipped. When the service was built on the shared host framework
it inherited that framework's canonical error model: an RFC 9457
`application/problem+json` envelope with a fixed status vocabulary, typed
per-resource namespaces, and structured detail carried in a `context` object
rather than in members invented per endpoint. That vocabulary has no
unprocessable-entity member. The refusals landed as `409 Conflict` with a
machine-readable reason, and the type strings are the framework's resource
namespaces, not `urn:insight:error:*`.

Consumers branch on these. The front end distinguishes an ambiguous profile
from a missing one; the analytics service distinguishes a refused guard from a
transport failure. So the documented-versus-implemented gap is not cosmetic:
anyone writing a consumer from the specification would branch on a status that
never arrives. This ADR records what was actually decided, and retires the
claims that contradict it.

## Decision Drivers

- A specification that names a status no consumer will ever receive is worse
  than one that says nothing, because it is actionable and wrong.
- The error model is the host framework's, not this service's. Deviating from
  it would mean bypassing the framework's own response mapping in every
  handler.
- The distinction consumers actually need is *which invariant was violated*,
  not which of two 4xx numbers carries it.
- One mapping across every guard in the service, so a consumer learns the rule
  once.

## Considered Options

- Adopt the framework's canonical envelope and map a violated invariant onto conflict (chosen)
- Keep the specified 422 and bypass the framework for these refusals
- Leave the documents as they were and treat the gap as known drift

## Decision Outcome

Chosen option: **adopt the framework's canonical envelope and map a violated
data invariant onto conflict**, because the alternative is a bespoke response
path in the one part of the service where consistency matters most.

Concretely:

- Every refusal is the canonical problem envelope: `type`, `title`, `status`,
  `detail`, `context`, and a trace identifier, served as
  `application/problem+json`. The `type` is a framework resource namespace
  identifying the surface that refused — one per resource: profile, person
  search, correction, role, person role, visibility, subchart, access, and the
  two operation journals.
- A violated **data invariant** is `409 Conflict`, built through the
  framework's `aborted` constructor. Three guards use it, and they are
  deliberately alike: an ambiguous profile, a role that is still assigned, and
  the revoke that would remove a tenant's last admin.
- The machine-readable discriminator is the reason string on the envelope, not
  the status. The matched person identifiers for an ambiguous profile are
  carried in the human-readable detail, because the canonical envelope has no
  place for a caller-defined structured payload.
- A malformed request is an invalid-argument refusal naming the offending
  field; a missing prerequisite — an unresolved tenant, an unconfigured roster
  — is a precondition violation naming the prerequisite. Both surface as
  `400`.
- Identification and authorization stay distinct: an unidentified caller is
  `401`, an identified caller without the required role is `403`.

The status vocabulary is the framework's and is not extended here. If an
unprocessable-entity member is added to it later, revisiting this mapping is a
new decision, not an automatic consequence.

### Consequences

- **Positive:** one envelope and one status rule across every surface;
  consumers learn it once.
- **Positive:** the specification now describes what a consumer receives, so a
  client written from it works.
- **Negative:** the structured ambiguity payload ADR-0009 promised does not
  exist. An operator reads the matched identifiers out of the detail text, and
  a tool that wants them must parse it.
- **Negative:** conflict is a less precise signal than unprocessable entity
  for "the request was well-formed but the stored data is wrong". Consumers
  must read the reason to tell the three guards apart.
- **Negative:** ADR-0009 and ADR-0013 are now partly historical. Each keeps its
  decision — a structured lookup body with a single-result invariant; a hard
  delete behind an in-use guard — and loses only its wire mapping.

### Confirmation

The live HTTP suite drives the real route table and asserts the status of each
guard, so a change of mapping fails the build rather than surfacing in a
consumer. The generated contract lists conflict among the responses of every
route that carries a guard, and its drift gate keeps it in step with the route
table. Reviewing a new guard for the same mapping is a standing review item.

## Pros and Cons of the Options

### Adopt the framework's canonical envelope and map a violated invariant onto conflict (chosen)

- **Pro:** no bespoke response path; the framework's mapping is the only one.
- **Pro:** the type identifies the refusing resource, which is more useful to
  a consumer than a hand-maintained URN vocabulary.
- **Con:** loses the structured ambiguity payload.
- **Con:** conflates "your request conflicts with current state" and "the
  stored data violates an invariant" under one status.

### Keep the specified 422 and bypass the framework for these refusals

- **Pro:** preserves ADR-0009 and ADR-0013 verbatim, including the structured
  payload.
- **Con:** three handlers would construct responses outside the framework's
  mapping, and every future guard would have to choose which path to take.
- **Con:** the generated contract is produced from the framework's metadata, so
  the bypassed responses would be absent from it — the drift gate would not
  catch a change to them.

### Leave the documents as they were and treat the gap as known drift

- **Pro:** no document churn.
- **Con:** the gap is exactly the kind that produces a broken consumer: it is
  specific, plausible and wrong.
- **Con:** a known-drift note does not tell a reader what the truth is.

## More Information

The three guards that share the conflict mapping are the service's only
data-invariant refusals, and each exists because the alternative is silent
damage: choosing one of several matched persons attributes one human's work to
another; deleting an assigned role leaves permission rows pointing at nothing;
removing the last admin leaves a tenant with no in-product way back. Grouping
them under one status is deliberate — they are the same kind of answer.

## Traceability

- Error namespaces: `services/identity-resolution/src/api/error.rs`
- Guards: `api::handlers::resolve_profile`, `api::roles`, `api::person_roles`
- Contract: `docs/components/backend/identity-resolution/openapi.json`
- Tests: `api::http_live_tests`, `tests/stand/api/identity`
- Related: ADR-0009 (single-result invariant — its 422 mapping superseded
  here), ADR-0013 (role in-use guard — same), ADR-0014 (last-admin
  protection)

This decision addresses:

- `cpt-insightspec-fr-identity-profile-ambiguous` — the status and shape the
  refusal takes.
- `cpt-insightspec-fr-identity-roles-in-use-guard` — same mapping.
- `cpt-insightspec-fr-identity-person-roles-last-admin` — same mapping.
- `cpt-insightspec-fr-identity-lookup-400-tenant` — precondition violations
  and their status.
- `cpt-insightspec-constraint-identity-canonical-errors` — the constraint this
  decision establishes.
