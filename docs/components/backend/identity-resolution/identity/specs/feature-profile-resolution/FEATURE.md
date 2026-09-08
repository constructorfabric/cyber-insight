---
status: draft
version: "1.0"
date: 2026-09-08
---

# Feature: Profile Resolution


<!-- toc -->

- [1. Feature Context](#1-feature-context)
  - [1.1 Overview](#11-overview)
  - [1.2 Purpose](#12-purpose)
  - [1.3 Actors](#13-actors)
  - [1.4 References](#14-references)
  - [1.5 Review decisions](#15-review-decisions)
- [2. Actor Flows (CDSL)](#2-actor-flows-cdsl)
  - [Resolve a profile](#resolve-a-profile)
  - [Diagnose a failed lookup](#diagnose-a-failed-lookup)
- [3. Processes / Business Logic (CDSL)](#3-processes--business-logic-cdsl)
  - [Assemble current attributes and edges](#assemble-current-attributes-and-edges)
- [4. States (CDSL)](#4-states-cdsl)
  - [No new lifecycle](#no-new-lifecycle)
- [5. Definitions of Done](#5-definitions-of-done)
  - [Profile selection and projection](#profile-selection-and-projection)
  - [Safe profile diagnostics](#safe-profile-diagnostics)
  - [Quality acceptance evidence](#quality-acceptance-evidence)
- [6. Acceptance Criteria](#6-acceptance-criteria)
- [7. Testing](#7-testing)

<!-- /toc -->

- [ ] `p1` - **ID**: `cpt-identity-svc-featstatus-profile-resolution`

## 1. Feature Context

- [ ] `p1` - `cpt-identity-svc-feature-profile-resolution`

### 1.1 Overview

Resolve a caller's identity lookup to one current person profile, with attributes,
source aliases, and the configured organisation tree. This is a retrospective
specification of an existing read capability, with proposed quality acceptance
and outstanding contract decisions; unchecked items do not imply that the
entire capability is unimplemented.

The scope is the existing `POST /v1/profiles` operation. Batch profile lookup,
roster browsing, operator corrections, seeding, migration execution, role
administration, and login bootstrap are separate capabilities.

### 1.2 Purpose

Make the service's quality obligations traceable from the PRD through acceptance
criteria to named scenarios. The PRD's business invariants define expected
behaviour; the committed API contract defines the supported wire surface.
Disagreements are recorded below rather than resolved by copying implementation
behaviour into the oracle.

**Requirements**:
`cpt-insightspec-fr-identity-profile-resolve`,
`cpt-insightspec-fr-identity-profile-ambiguous-422`,
`cpt-insightspec-fr-identity-profile-ids-list`,
`cpt-insightspec-fr-identity-profile-org-tree`,
`cpt-insightspec-fr-identity-profile-validation`,
`cpt-insightspec-fr-identity-lookup-hydrate`,
`cpt-insightspec-fr-identity-lookup-parent`,
`cpt-insightspec-fr-identity-lookup-subordinates`,
`cpt-insightspec-fr-identity-routing-name-split`,
`cpt-insightspec-nfr-identity-latency`,
`cpt-insightspec-nfr-identity-memory`,
`cpt-insightspec-nfr-identity-logging-pii`,
`cpt-insightspec-nfr-identity-uuid-roundtrip`,
`cpt-identity-svc-nfr-profile-source-coverage`,
`cpt-ir-nfr-tenant-isolation`,
`cpt-ir-nfr-alias-lookup-latency`.

**Principles**: `cpt-insightspec-principle-identity-observation-log`,
`cpt-insightspec-principle-identity-centralised-sql`,
`cpt-insightspec-principle-identity-pii-boundary`.

### 1.3 Actors

| Actor | Role in Feature |
|-------|-----------------|
| `cpt-insightspec-actor-api-gateway` | Forward an authenticated caller's lookup and verified tenant context. |
| `cpt-insightspec-actor-platform-sre` | Diagnose lookup failures without exposing lookup values or credentials. |
| `cpt-insightspec-actor-mariadb` | Supply persisted identity observations and organisation edges. |

### 1.4 References

- **PRD**: [Identity Service PRD](../PRD.md), especially sections 1.3, 5.2 and 6.
- **Design**: [Identity Service DESIGN](../DESIGN.md), including UUID storage and profile assembly.
- **Manifest**: [DECOMPOSITION](../DECOMPOSITION.md#21-profile-resolution--high).
- **Wire contract**: [committed OpenAPI](../../../openapi.json), operation `identity_resolution.profiles.resolve` and schemas `ResolveProfileRequest`, `ProfileResponse`, `Problem`.
- **Inherited requirements**: [Identity Resolution PRD](../../../../../../domain/identity-resolution/specs/PRD.md#6-non-functional-requirements).
- **Dependencies**: migrated identity storage, seeded observations and organisation edges, and a caller with a verified tenant. This FEATURE does not verify those producers' entire lifecycle.

### 1.5 Review decisions

These decisions must be resolved before treating this draft as an approved
acceptance baseline. Role owners below are proposed responsibilities for review.

| Decision | Finding and proposed disposition | Owner / resolution point |
|----------|----------------------------------|--------------------------|
| D-1: Ambiguity contract | PRD section 5.2 and ADR-0009 require 422 and structured ambiguity members; the committed contract exposes 409 and `Problem`. AC-4 covers refusal without selecting an arbitrary person; AC-15 defers the exact wire oracle. Reconcile PRD, DESIGN, ADR and contract together before enabling that gate. | Identity service API owner, before approval of this FEATURE. |
| D-2: Legacy latency | The local NFR names the old GET lookup, which is absent from the current route table. Do not transfer its 50 ms p95 target or unreferenced 200 ms fallback to POST. The inherited profile target remains applicable independently. | Identity service maintainer and product owner, before approval of a replacement or retirement of AC-14. |
| D-3: Measurement conditions | Memory specifies 50 000 observation rows, 100 RPS and 24 hours; domain latency specifies p99 < 50 ms at 1000 requests/s without a fixture. No identity load/soak lane was found in `tests/`, `scripts/` or `.github/`. Define one synthetic fixture, hardware, tree shape, request distribution, warm-up and latency duration; reuse it at each requirement's own load. | Identity service maintainer and QA, before running scenarios 12–13 or claiming either NFR. |
| D-4: Source coverage | The new PRD NFR formalises the existing multi-source goal. Inventory supported identity-emitting sources and pin representative fixtures, including two instances of a source. A unit fixture alone does not prove every connector's output. | Identity service and connector maintainers with QA, before claiming AC-11. |
| D-5: Canonical ID and visibility | OpenAPI includes `person_id` lookup, absent from PRD section 5.2. Code inspection also exposes a same-tenant visibility filter before ambiguity selection; tenant isolation alone does not specify that policy. AC-16 follows the published lookup mode; AC-17 is a proposed visibility criterion pending the product/API owner's approval. | Product/API owner, reconcile the upstream requirements before FEATURE approval. |

The wider service PRD/DESIGN audit remains open; this profile slice does not
certify other endpoints or inherited domain-wide mirror behaviour.

## 2. Actor Flows (CDSL)

### Resolve a profile

- [ ] `p1` - **ID**: `cpt-identity-svc-flow-profile-resolution-read`

**Actor**: `cpt-insightspec-actor-api-gateway`

**Success Scenarios**: A valid email, source-instance ID, or canonical person ID
identifies one permitted person; the caller receives the current profile.

**Error Scenarios**: Invalid input, missing caller/tenant, no permitted match,
multiple permitted matches, or storage failure produce an explicit refusal.

**Steps**:

1. [ ] - `p1` - API: POST /v1/profiles with the lookup and verified caller context - `inst-request-profile`
2. [ ] - `p1` - **IF** caller or tenant cannot be verified, **RETURN** the contract's authentication or tenant error without a profile - `inst-reject-unverified-context`
3. [ ] - `p1` - Validate the lookup shape against the profile contract - `inst-validate-profile-request`
4. [ ] - `p1` - DB: Resolve candidates within the verified tenant, applying the source instance when required - `inst-resolve-tenant-candidates`
5. [ ] - `p1` - Apply the caller-visibility policy proposed in AC-17 before reporting candidates; adoption awaits D-5 - `inst-filter-profile-candidates`
6. [ ] - `p1` - **IF** no permitted candidate exists, **RETURN** the contract's not-found result - `inst-return-profile-not-found`
7. [ ] - `p1` - **IF** several permitted persons match, **RETURN** an ambiguity refusal; exact wire representation awaits D-1 - `inst-return-profile-ambiguity`
8. [ ] - `p1` - Assemble the current profile using `cpt-identity-svc-algo-profile-resolution-assemble` - `inst-assemble-resolved-profile`
9. [ ] - `p1` - **RETURN** one profile using the committed response schema - `inst-return-resolved-profile`

### Diagnose a failed lookup

- [ ] `p1` - **ID**: `cpt-identity-svc-flow-profile-resolution-diagnose`

**Actor**: `cpt-insightspec-actor-platform-sre`

**Success Scenarios**: Request metadata allows a failure to be correlated without
disclosing the requested email or store credentials.

**Error Scenarios**: A failing database read must not appear to be a successful
empty profile. Exact dependency-failure expectations beyond the published error
contract require a separate availability decision.

**Steps**:

1. [ ] - `p1` - Correlate the failed lookup using structured request and failure metadata - `inst-correlate-profile-failure`
2. [ ] - `p1` - Emit only the logging fields and sanitised error context allowed by DESIGN section 4.2 - `inst-redact-profile-diagnostics`
3. [ ] - `p1` - **RETURN** a contract error without database credentials - `inst-return-safe-profile-error`

## 3. Processes / Business Logic (CDSL)

### Assemble current attributes and edges

- [ ] `p1` - **ID**: `cpt-identity-svc-algo-profile-resolution-assemble`

**Input**: One resolved person, verified tenant, current observations and
configured organisation-tree source.

**Output**: Profile attributes, current source aliases, parent and bounded
subordinate tree. Persisted UUIDs retain their canonical bytes.

**Steps**:

1. [ ] - `p1` - DB: Read the person's latest observations per source instance and attribute within the verified tenant - `inst-read-current-observations`
2. [ ] - `p1` - Select each attribute by the PRD's latest-value rule across sources; apply the documented display-name fallback - `inst-select-current-attributes`
3. [ ] - `p1` - Project the current native-ID binding for each source instance, preserving instance identity - `inst-project-current-aliases`
4. [ ] - `p1` - Read the parent from the configured organisation-edge source rather than stale parent observations - `inst-read-current-parent`
5. [ ] - `p1` - Walk subordinates on that source, stopping at visited persons and the configured depth, and skipping children without observations - `inst-bound-profile-tree`
6. [ ] - `p1` - **RETURN** the assembled profile with the contract's absent-field and empty-array rules - `inst-return-profile-projection`

## 4. States (CDSL)

### No new lifecycle

Not applicable: profile resolution is a read operation and introduces no
persisted lifecycle. Observation and organisation-edge state belongs to the
producer specifications.

## 5. Definitions of Done

### Profile selection and projection

- [ ] `p1` - **ID**: `cpt-identity-svc-dod-profile-resolution-correctness`

The system **MUST** satisfy the approved selection, projection and refusal
criteria without mutating identity state.

**Implements**:
- `cpt-identity-svc-flow-profile-resolution-read`
- `cpt-identity-svc-algo-profile-resolution-assemble`

**Constraints**: `cpt-insightspec-constraint-identity-binary16-uuid`,
`cpt-insightspec-constraint-identity-mysql-backend`.

**Touches**:
- API: `POST /v1/profiles`
- DB: identity observations and organisation edges, read only
- Entities: `ProfileResponse`, `PersonResponse`

### Safe profile diagnostics

- [ ] `p1` - **ID**: `cpt-identity-svc-dod-profile-resolution-diagnostics`

The system **MUST** allow correlation of successful and failed lookups without
leaking raw email lookup values or database credentials.

**Implements**:
- `cpt-identity-svc-flow-profile-resolution-diagnose`

**Constraints**: `cpt-insightspec-principle-identity-pii-boundary`.

**Touches**:
- API: profile request logging and error responses

### Quality acceptance evidence

- [ ] `p1` - **ID**: `cpt-identity-svc-dod-profile-resolution-evidence`

The feature **MUST** link implemented scenarios to their tests and attach passing
evidence identifying revision, fixtures and run conditions. D-1 through D-5 and
deferred criteria must be resolved before declaring the feature accepted.
Inherited domain obligations remain shared gates.

**Implements**:
- `cpt-identity-svc-flow-profile-resolution-read`
- `cpt-identity-svc-flow-profile-resolution-diagnose`

**Touches**:
- Tests: the suites and manual procedures in section 7

## 6. Acceptance Criteria

This is the proposed criterion set for this new FEATURE, not a claim of product
approval. Existing invariant and NFR references are preserved; D-1 through D-5
identify decisions that prevent acceptance.

- [ ] AC-1. A valid email or source-instance native ID resolves the expected single person with current attributes and tenant identity (`cpt-insightspec-fr-identity-profile-resolve`).
- [ ] AC-2. Newer attribute observations replace older values in the response without restarting the service; explicit names override the documented display-name split fallback (`cpt-insightspec-fr-identity-lookup-hydrate`, `cpt-insightspec-fr-identity-routing-name-split`).
- [ ] AC-3. A valid lookup with no permitted match returns 404 with the published problem schema, including an empty seeded identity store (`cpt-insightspec-fr-identity-lookup-404`).
- [ ] AC-4. A lookup matching several permitted persons refuses to select an arbitrary person (`cpt-insightspec-fr-identity-profile-ambiguous-422`); the exact wire assertion is AC-15.
- [ ] AC-5. The response contains exactly the current native-ID aliases, one per source instance, with no superseded aliases (`cpt-insightspec-fr-identity-profile-ids-list`).
- [ ] AC-6. Parent and subordinate fields follow the configured edge source; stale parent observations, cycles, missing child observations and depth limits do not create extra or unbounded nodes (`cpt-insightspec-fr-identity-profile-org-tree`, `cpt-insightspec-fr-identity-lookup-parent`, `cpt-insightspec-fr-identity-lookup-subordinates`).
- [ ] AC-7. Invalid lookup bodies are rejected with 400 and the published problem schema (`cpt-insightspec-fr-identity-profile-validation`); historical error-URN differences remain part of D-1's contract reconciliation.
- [ ] AC-8. Profiles, nested identities and error responses disclose zero identity data from another tenant across supported lookup modes (inherited `cpt-ir-nfr-tenant-isolation`).
- [ ] AC-9. Tenant, source-instance, person and author UUIDs round-trip byte-for-byte through persistence and reads (`cpt-insightspec-nfr-identity-uuid-roundtrip`).
- [ ] AC-10. Success, validation failure and storage failure retain correlatable structured logs with zero raw email lookup values, connection strings or database credentials (`cpt-insightspec-nfr-identity-logging-pii`).
- [ ] AC-11. Every source in the approved identity-emitting fixture manifest resolves expected people and aliases, including distinct instances with the same native ID (`cpt-identity-svc-nfr-profile-source-coverage`).
- [ ] AC-12. RSS remains ≤ 384 MiB over the specified 24-hour, 100-RPS, 50 000-observation-row soak, after D-3 fixes the remaining conditions (`cpt-insightspec-nfr-identity-memory`).
- [ ] AC-13. Existing-binding profile lookups satisfy p99 < 50 ms at sustained 1000 requests/s, with D-3's approved measurement conditions (inherited `cpt-ir-nfr-alias-lookup-latency`).
- [ ] AC-14. **Deferred:** retain the local legacy lookup's p95 ≤ 50 ms obligation until its operation mapping and scale conditions are resolved (`cpt-insightspec-nfr-identity-latency`). Owner: Identity service maintainer and product owner; D-2 must close before acceptance.
- [ ] AC-15. **Deferred:** ambiguity and validation errors match one reconciled wire oracle, including status and permitted diagnostic fields (`cpt-insightspec-fr-identity-profile-ambiguous-422`, `cpt-insightspec-fr-identity-profile-validation`). Owner: Identity service API owner; conflicting PRD/ADR/OpenAPI expectations require D-1's decision before acceptance.
- [ ] AC-16. Canonical person-ID lookup returns the same profile as another supported lookup for that person and can address a person without email (published `ResolveProfileRequest` contract; upstream alignment pending D-5).
- [ ] AC-17. **Proposed, deferred:** a same-tenant caller cannot discover hidden profiles through success, nested fields or ambiguity diagnostics. Owner: product/API owner; D-5 must define visibility and mixed visible/hidden candidate outcomes before a scenario is accepted.

## 7. Testing

This retrospective profile-resolution plan protects correct identity attribution
first, using synthetic fixtures through the Rust, identity data-path and API
suites. All scenarios are unchecked pending exact test mapping; existing nearby
tests are starting points, not evidence that these complete oracles are implemented
or passing.

Planned scenarios cover 14/17 criteria: AC-1 → 1 · AC-2 → 2 · AC-3 → 3 ·
AC-4 → 4 · AC-5 → 14 · AC-6 → 5 · AC-7 → 6 · AC-8 → 9,10 · AC-9 → 7 ·
AC-10 → 8,11 · AC-11 → 15 · AC-12 → 12 · AC-13 → 13 · AC-16 → 16.
AC-14, AC-15 and AC-17 are deferred with owners and reasons in section 6; they
remain in the denominator. D-3 and D-4 also block execution or acceptance of
their planned scenarios.

- [ ] 1. **Expected person** — Reliability · identity-e2e · AC-1 — seed a synthetic person's observations, resolve by email and by source-instance native ID → both return that person's expected UUID, tenant and current attributes.
- [ ] 2. **Current attributes** — Reliability · identity-e2e · AC-2 — append newer cross-source attributes and vary explicit names versus display-name-only input, then read without restart → newest fields win and fallback applies only where explicit names are absent.
- [ ] 3. **No matching person** — Reliability · identity-e2e · AC-3 — query an empty synthetic identity store and an unknown value in a populated one as a valid caller → each returns 404 and the published problem schema.
- [ ] 4. **Ambiguity refusal** — Reliability · identity-e2e · AC-4 — give one lookup two permitted candidate persons → resolution refuses without choosing either profile; exact status and diagnostic fields remain deferred under AC-15.
- [ ] 5. **Bounded organisation tree** — Reliability · identity-e2e · AC-6 — seed competing edge sources, stale parent observations, a cycle, a missing child and a tree beyond the configured depth → only the selected source's valid parent and bounded, non-repeated descendants appear.
- [ ] 6. **Invalid lookup shapes** — Reliability · stand-api · AC-7 — send malformed JSON, empty values, unsupported lookup types, native IDs without either source coordinate, and email lookups with source coordinates → every invalid request returns 400 with the published problem schema.
- [ ] 7. **Identifier preservation** — Reliability · identity-e2e · AC-9 — persist distinct synthetic tenant, source, person and author UUIDs whose byte order would expose truncation or reversal, then read storage and profile identities → all four fields retain their exact original bytes.
- [ ] 8. **Correlatable diagnostics** — Reliability · rust-unit · AC-10 — capture logging for successful, invalid and failing profile requests → each retains the structured service, request and error metadata required by DESIGN section 4.2.
- [ ] 9. **Tenant collision isolation** — Security · identity-e2e · AC-8 — seed overlapping emails, native IDs and source coordinates in two synthetic tenants and query each supported mode → returned profiles and nested identities belong only to the verified tenant.
- [ ] 10. **Tenant error isolation** — Security · identity-e2e · AC-8 — add another tenant's competing candidates, then query successful, missing and ambiguous lookups → status selection and identity-bearing diagnostics reveal no candidate from the other tenant.
- [ ] 11. **Private lookup logs** — Security · rust-unit · AC-10 — capture success, invalid-body and database-failure logs using distinctive synthetic email and credential canaries → none of the raw lookup values, connection strings or credentials appears.
- [ ] 12. **Bounded memory** — Efficiency · manual · AC-12 — after D-3 approval, sustain the specified hot/cold lookup mix at 100 RPS for 24 hours over 50 000 synthetic observation rows → service RSS never exceeds 384 MiB; no load/soak lane is wired, so procedure and evidence remain required.
- [ ] 13. **Profile lookup latency** — Performance · manual · AC-13 — after D-3 approval, run existing-binding profile lookups at sustained 1000 requests/s on the approved synthetic fixture and hardware → p99 stays below 50 ms, with errors reported separately rather than discarded; the load harness and run duration remain prerequisites.
- [ ] 14. **Current source aliases** — Versatility · identity-e2e · AC-5 — seed several source instances and supersede a native-ID observation → the resolved profile lists exactly the current alias of each instance, without the older alias.
- [ ] 15. **Every identity source** — Versatility · identity-e2e · AC-11 — feed each approved manifest source through its identity data path, including two instances sharing a native ID → expected people and alias sets resolve without instance collisions; missing source fixtures remain open coverage gaps.
- [ ] 16. **Canonical identity lookup** — Versatility · stand-api · AC-16 — resolve the same synthetic person by canonical ID and email, then resolve a seeded person without email by canonical ID → shared lookups return equal profiles and the email-less person's profile remains addressable.

**Evidence ownership:** Identity service maintainer and QA own scenario-to-test
links and revision-stamped results; connector maintainers own the source fixture
inventory. Candidate suites already exist at
[identity data-path](../../../../../../../tests/datapath/identity/),
[profile API tests](../../../../../../../tests/stand/api/identity/test_profiles.py),
and [Rust logging tests](../../../../../../../src/backend/services/identity-resolution/src/api/log_leak_tests.rs).
Their current assertions must be checked against each full scenario before
checking any box. Rust checks cannot substitute for database round trips, and
a skipped persona or absent source fixture is not passing evidence.
