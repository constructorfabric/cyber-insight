---
status: draft
version: "1.1"
date: 2026-09-08
---

# Feature: Profile Resolution

**Revision 1.1:** Attach vector tests directly to this feature and its PRD
requirements. Restore the canonical acceptance checklist; retain open decisions
as deferred scenarios rather than an AC coverage scheme.


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
  - [Scenarios](#scenarios)
  - [Test boundaries and evidence](#test-boundaries-and-evidence)

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

Make the service's quality obligations traceable from the PRD through
the feature to named scenarios and exact tests. The PRD's business invariants define expected
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
`cpt-insightspec-fr-identity-lookup-404`,
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
| D-1: Ambiguity contract | PRD section 5.2 and ADR-0009 require 422 and structured ambiguity members; the committed contract exposes 409 and `Problem`. Scenario 4 covers refusal without selecting an arbitrary person; scenario 18 defers the exact wire oracle. Reconcile PRD, DESIGN, ADR and contract together before enabling that gate. | Identity service API owner, before approval of this FEATURE. |
| D-2: Legacy latency | The local NFR names the old GET lookup, which is absent from the current route table. Do not transfer its 50 ms p95 target or unreferenced 200 ms fallback to POST. The inherited profile target remains applicable independently. | Identity service maintainer and product owner, before approval of a replacement or retirement of scenario 17. |
| D-3: Measurement conditions | Memory specifies 50 000 observation rows, 100 RPS and 24 hours; domain latency specifies p99 < 50 ms at 1000 requests/s without a fixture. No identity load/soak lane was found in `tests/`, `scripts/` or `.github/`. Define one synthetic fixture, hardware, tree shape, request distribution, warm-up and latency duration; reuse it at each requirement's own load. | Identity service maintainer and QA, before running scenarios 12–13 or claiming either NFR. |
| D-4: Source coverage | The new PRD NFR formalises the existing multi-source goal. Inventory supported identity-emitting sources and pin representative fixtures, including two instances of a source. A unit fixture alone does not prove every connector's output. | Identity service and connector maintainers with QA, before claiming scenario 15. |
| D-5: Canonical ID and visibility | OpenAPI includes `person_id` lookup, absent from PRD section 5.2. Code inspection also exposes a same-tenant visibility filter before ambiguity selection; tenant isolation alone does not specify that policy. Scenario 16's canonical-ID requirement link and scenario 19's visibility requirement are deferred until the product/API owner reconciles the upstream scope. | Product/API owner, reconcile the upstream requirements before FEATURE approval. |

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
5. [ ] - `p1` - Apply the caller-visibility policy pending D-5 before reporting candidates; adoption awaits D-5 - `inst-filter-profile-candidates`
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
requirements without mutating identity state.

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

The feature **MUST** link its PRD requirements to vector-attributed scenarios and exact tests and attach passing
evidence identifying revision, fixtures and run conditions. D-1 through D-5 and
deferred scenarios must be resolved before declaring the feature accepted.
Tests must cite this FEATURE path, its feature ID and stable scenario number;
native test vector markers must match. Inherited domain obligations remain shared gates.

**Implements**:
- `cpt-identity-svc-flow-profile-resolution-read`
- `cpt-identity-svc-flow-profile-resolution-diagnose`

**Touches**:
- Tests: the suites and manual procedures in section 7

## 6. Acceptance Criteria

- [ ] Supported lookups return the expected current person profile, attributes and source aliases within the permitted scope.
- [ ] Missing, invalid and ambiguous lookups produce the agreed refusal without selecting an arbitrary person.
- [ ] Profile and organisation-tree projection preserve identity, source-instance boundaries and the configured traversal limits.
- [ ] No profile, nested identity, diagnostic response or service log exposes information prohibited by the agreed tenant, visibility and privacy requirements.
- [ ] The feature satisfies its applicable local and inherited quality requirements under approved fixture and measurement conditions.

The checklist states feature completion conditions. D-1 through D-5 must be
resolved before acceptance; test cases, requirement links and deferred work live
in Testing.

## 7. Testing

**Feature**: `cpt-identity-svc-feature-profile-resolution`

These tests belong to the profile-resolution feature and verify the PRD FR/NFRs
declared in section 1.2. Use synthetic fixtures through the Rust, identity
data-path and API suites. Each test records this FEATURE path, the feature ID,
its stable scenario number and one vector; the scenario links back to the exact
test when mapped. All implementation mappings and passing evidence remain open.

### Scenarios

- [ ] 1. **Expected person** — Reliability · identity-e2e — seed a synthetic person's observations, resolve by email and by source-instance native ID → both return that person's expected UUID, tenant and current attributes.
  **Requirements**: `cpt-insightspec-fr-identity-profile-resolve`.
  **Test**: Not yet mapped to this complete scenario.

- [ ] 2. **Current attributes** — Reliability · identity-e2e — append newer cross-source attributes and vary explicit names versus display-name-only input, then read without restart → newest fields win and fallback applies only where explicit names are absent.
  **Requirements**: `cpt-insightspec-fr-identity-lookup-hydrate`, `cpt-insightspec-fr-identity-routing-name-split`.
  **Test**: Not yet mapped to this complete scenario.

- [ ] 3. **No matching person** — Reliability · identity-e2e — query an empty synthetic identity store and an unknown value in a populated one as a valid caller → each returns 404 and the published problem schema.
  **Requirements**: `cpt-insightspec-fr-identity-lookup-404`.
  **Test**: Not yet mapped to this complete scenario.

- [ ] 4. **Ambiguity refusal** — Reliability · identity-e2e — give one lookup two permitted candidate persons → resolution refuses without choosing either profile; exact status and diagnostic fields remain deferred under scenario 18.
  **Requirements**: `cpt-insightspec-fr-identity-profile-ambiguous-422`.
  **Test**: Not yet mapped to this complete scenario.

- [ ] 5. **Bounded organisation tree** — Reliability · identity-e2e — seed competing edge sources, stale parent observations, a cycle, a missing child and a tree beyond the configured depth → only the selected source's valid parent and bounded, non-repeated descendants appear.
  **Requirements**: `cpt-insightspec-fr-identity-profile-org-tree`, `cpt-insightspec-fr-identity-lookup-parent`, `cpt-insightspec-fr-identity-lookup-subordinates`.
  **Test**: Not yet mapped to this complete scenario.

- [ ] 6. **Invalid lookup shapes** — Reliability · stand-api — send malformed JSON, empty values, unsupported lookup types, native IDs without either source coordinate, and email lookups with source coordinates → every invalid request returns 400 with the published problem schema.
  **Requirements**: `cpt-insightspec-fr-identity-profile-validation`.
  **Test**: Not yet mapped to this complete scenario.

- [ ] 7. **Identifier preservation** — Reliability · identity-e2e — persist distinct synthetic tenant, source, person and author UUIDs whose byte order would expose truncation or reversal, then read storage and profile identities → all four fields retain their exact original bytes.
  **Requirements**: `cpt-insightspec-nfr-identity-uuid-roundtrip`.
  **Test**: Not yet mapped to this complete scenario.

- [ ] 8. **Safe diagnostic fields** — Security · rust-unit — capture logging for successful, invalid and failing profile requests → only allow-listed request fields and sanitised error context appear, with the required correlation fields retained.
  **Requirements**: `cpt-insightspec-nfr-identity-logging-pii`.
  **Test**: Not yet mapped to this complete scenario.

- [ ] 9. **Tenant collision isolation** — Security · identity-e2e — seed overlapping emails, native IDs and source coordinates in two synthetic tenants and query each supported mode → returned profiles and nested identities belong only to the verified tenant.
  **Requirements**: `cpt-ir-nfr-tenant-isolation`.
  **Test**: Not yet mapped to this complete scenario.

- [ ] 10. **Tenant error isolation** — Security · identity-e2e — add another tenant's competing candidates, then query successful, missing and ambiguous lookups → status selection and identity-bearing diagnostics reveal no candidate from the other tenant.
  **Requirements**: `cpt-ir-nfr-tenant-isolation`.
  **Test**: Not yet mapped to this complete scenario.

- [ ] 11. **Private lookup logs** — Security · rust-unit — capture success, invalid-body and database-failure logs using distinctive synthetic email and credential canaries → none of the raw lookup values, connection strings or credentials appears.
  **Requirements**: `cpt-insightspec-nfr-identity-logging-pii`.
  **Test**: Not yet mapped to this complete scenario.

- **deferred** 12. **Bounded memory** — Efficiency · manual — after D-3 approval, sustain the specified hot/cold lookup mix at 100 RPS for 24 hours over 50 000 synthetic observation rows → service RSS never exceeds 384 MiB; no load/soak lane is wired, so procedure and evidence remain required.
  **Requirements**: `cpt-insightspec-nfr-identity-memory`.
  **Test**: Not yet mapped to this complete scenario.
  **Blocked by**: D-3 — Identity service maintainer and QA must approve the fixture and soak procedure before execution.

- **deferred** 13. **Profile lookup latency** — Performance · manual — after D-3 approval, run existing-binding profile lookups at sustained 1000 requests/s on the approved synthetic fixture and hardware → p99 stays below 50 ms, with errors reported separately rather than discarded; the load harness and run duration remain prerequisites.
  **Requirements**: `cpt-ir-nfr-alias-lookup-latency`.
  **Test**: Not yet mapped to this complete scenario.
  **Blocked by**: D-3 — Identity service maintainer and QA must approve the shared fixture, hardware, run duration and load harness before execution.

- [ ] 14. **Current source aliases** — Versatility · identity-e2e — seed several source instances and supersede a native-ID observation → the resolved profile lists exactly the current alias of each instance, without the older alias.
  **Requirements**: `cpt-insightspec-fr-identity-profile-ids-list`.
  **Test**: Not yet mapped to this complete scenario.

- **deferred** 15. **Every identity source** — Versatility · identity-e2e — feed each approved manifest source through its identity data path, including two instances sharing a native ID → expected people and alias sets resolve without instance collisions; missing source fixtures remain open coverage gaps.
  **Requirements**: `cpt-identity-svc-nfr-profile-source-coverage`.
  **Test**: Not yet mapped to this complete scenario.
  **Blocked by**: D-4 — Identity service and connector maintainers with QA must approve the source fixture manifest before coverage can be claimed.

- **deferred** 16. **Canonical identity lookup** — Versatility · stand-api — resolve the same synthetic person by canonical ID and email, then resolve a seeded person without email by canonical ID → shared lookups return equal profiles and the email-less person's profile remains addressable.
  **Requirements**: `cpt-insightspec-fr-identity-profile-resolve`.
  **Test**: Not yet mapped to this complete scenario.
  **Blocked by**: D-5 — Product/API owner must reconcile the published canonical-ID mode with the PRD profile requirement before this test's requirement link is accepted.

- **deferred** 17. **Legacy lookup latency** — Performance · manual — measure the approved successor operation under the agreed scale conditions → satisfy the retained local latency obligation.
  **Requirements**: `cpt-insightspec-nfr-identity-latency`.
  **Blocked by**: D-2 — Identity service maintainer and product owner must replace or retire the legacy operation mapping and fix the scale conditions before a runnable test is specified.

- **deferred** 18. **Error wire contract** — Reliability · stand-api — exercise ambiguous and invalid profile lookups → status and permitted diagnostic fields match the reconciled contract.
  **Requirements**: `cpt-insightspec-fr-identity-profile-ambiguous-422`, `cpt-insightspec-fr-identity-profile-validation`.
  **Blocked by**: D-1 — Identity service API owner must reconcile PRD, DESIGN, ADR and OpenAPI before the exact oracle is accepted.

- **deferred** 19. **Caller visibility** — Security · identity-e2e — query visible and hidden same-tenant candidates through each supported lookup → success, nested profiles and ambiguity diagnostics respect the agreed caller scope.
  **Requirements**: Missing upstream visibility requirement; tenant isolation alone does not specify same-tenant visibility.
  **Blocked by**: D-5 — Product/API owner must establish and reference the visibility requirement in the PRD and this feature's section 1.2 before this test can be specified.

### Test boundaries and evidence

Identity data-path tests use persisted synthetic observations and edges; Rust
tests cover logging and pure assembly logic. API tests exercise the published
wire surface with synthetic callers. A unit fixture cannot prove connector
output or database UUID persistence. Memory and latency share the D-3 fixture
at their distinct upstream loads; neither has an approved runnable procedure yet.

Identity service maintainer and QA own requirement-to-scenario mappings, exact
test links and revision-stamped results; connector maintainers own the source
fixture inventory. Existing candidates include
[identity data-path](../../../../../../../tests/datapath/identity/),
[profile API tests](../../../../../../../tests/stand/api/identity/test_profiles.py),
and [Rust logging tests](../../../../../../../src/backend/services/identity-resolution/src/api/log_leak_tests.rs).
These are discovery links, not implemented-test evidence. Check a scenario only
when its full outcome is asserted by an exact linked test that cites this
feature and the matching vector. Passing evidence separately records revision,
fixture, command and result; skips and missing fixtures do not pass.
