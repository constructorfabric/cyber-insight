---
version: 1.3
status: proposed
date: 2026-09-08
---

# PRD — Query Engine

<!-- toc -->

- [1. Overview](#1-overview)
  - [1.1 Purpose](#11-purpose)
  - [1.2 Background / Problem Statement](#12-background--problem-statement)
  - [1.3 Goals (Business Outcomes)](#13-goals-business-outcomes)
  - [1.4 Glossary](#14-glossary)
- [2. Actors](#2-actors)
  - [2.1 Human Actors](#21-human-actors)
  - [2.2 System Actors](#22-system-actors)
- [3. Operational Concept & Environment](#3-operational-concept--environment)
  - [3.1 Module-Specific Environment Constraints](#31-module-specific-environment-constraints)
- [4. Scope](#4-scope)
  - [4.1 In Scope](#41-in-scope)
  - [4.2 Out of Scope](#42-out-of-scope)
- [5. Functional Requirements](#5-functional-requirements)
  - [5.1 Query execution](#51-query-execution)
  - [5.2 Validation](#52-validation)
  - [5.3 Planned capabilities](#53-planned-capabilities)
  - [5.4 Authorization](#54-authorization)
- [6. Non-Functional Requirements](#6-non-functional-requirements)
  - [6.1 NFR Inclusions](#61-nfr-inclusions)
  - [6.2 NFR Exclusions](#62-nfr-exclusions)
- [7. Public Library Interfaces](#7-public-library-interfaces)
  - [7.1 Public API Surface](#71-public-api-surface)
  - [7.2 External Integration Contracts](#72-external-integration-contracts)
- [8. Use Cases](#8-use-cases)
- [9. Acceptance Criteria](#9-acceptance-criteria)
- [10. Dependencies](#10-dependencies)
- [11. Assumptions](#11-assumptions)
- [12. Risks](#12-risks)

<!-- /toc -->

## 1. Overview

### 1.1 Purpose

The query engine executes structured queries over declared datasets and returns typed tables
for charts and analysis. Callers select filters, grouping, aggregates, ordering, and a time
window without defining a new server-side metric.

[Datasets](../datasets/PRD.md) define the available data and fields. [DESIGN.md](DESIGN.md)
defines query semantics and implementation. [MIGRATION.md](MIGRATION.md) covers the transition
from existing metric APIs.

### 1.2 Background / Problem Statement

Pre-built metrics couple each supported calculation to server code. Queries outside those
combinations require development and a release.

A chart builder needs a discoverable query contract: available fields, supported operations,
limits, and structured errors. An administrator SQL console does not provide that contract
or automatically apply the dataset semantics required by chart consumers.

### 1.3 Goals (Business Outcomes)

- Build a supported query and chart within one working session, without a release.
- Apply tenant isolation, dataset read rules, null semantics, and time boundaries consistently.
- Return actionable validation errors identifying invalid inputs.
- Preserve saved-query meaning or reject incompatible definitions.
- Extend access to team leads after dataset access policies are enforced.
- Query newly declared datasets without engine changes.

### 1.4 Glossary

| Term | Meaning |
|---|---|
| Dataset | Declared queryable data with a row grain and field metadata |
| Dimension | Declared field available for grouping and filtering |
| Measurable | Declared numeric field available for aggregation |
| Aggregate | Computation over rows, such as count, sum, or average |
| Filter | Predicate selecting rows |
| Grouping | Partitioning results by dimensions or time buckets |
| Time window | Required inclusive start and end dates |
| Time grain | Bucket size: initially day, week, or month |
| Violation | Validation error with a field path, reason code, and explanation |
| Access policy | Dataset, row, or column visibility rule |
| Constructor | Client for composing queries and rendering results |

## 2. Actors

### 2.1 Human Actors

#### Instance administrator

**ID**: `cpt-insightspec-qe-actor-admin`

**Role**: executes queries under the initial administrator-only access model.
**Needs**: query composition and actionable errors without warehouse expertise.

#### Team lead

**ID**: `cpt-insightspec-qe-actor-lead`

**Role**: planned query consumer restricted to authorized people and resources.
**Needs**: visibility policies applied independently of grouping.

#### Dashboard author

**ID**: `cpt-insightspec-qe-actor-author`

**Role**: builds charts and, in the planned extension, saved queries.
**Needs**: discoverable fields, operations, and limits.

### 2.2 System Actors

#### Constructor page

**ID**: `cpt-insightspec-qe-actor-constructor`

**Role**: composes requests, renders results, and displays validation errors.

#### Identity service

**ID**: `cpt-insightspec-qe-actor-identity`

**Role**: supplies administrator checks and, later, caller attributes for access policies.

#### Warehouse

**ID**: `cpt-insightspec-qe-actor-warehouse`

**Role**: stores prepared datasets and executes compiled queries.

## 3. Operational Concept & Environment

### 3.1 Module-Specific Environment Constraints

- Queries read prepared datasets and do not modify data.
- Dataset declarations must pass validation before use. The dataset design documents the
  limits of snapshot-based validation.

## 4. Scope

### 4.1 In Scope

**Initial implementation**: single-dataset queries with filters, dimension and time grouping,
conditional aggregates, ordering, row limits, a required time window, structured errors,
typed results, and truncation reporting. Discovery includes query limits.

**Planned product scope**: a constructor UI, Boolean filter groups, relative windows, derived
dimensions, richer aggregates, expressions, windows, totals, pagination, saved queries,
cross-dataset analysis, and access-policy enforcement. DESIGN records each capability's status.

### 4.2 Out of Scope

- Dataset definition and registration, owned by the datasets module.
- User-authored SQL; the administrator SQL console remains separate.
- Chart layout and formatting.
- Event-path analysis and sessionization beyond the planned funnel and retention operations.
- Alerts and notifications.
- Replacing existing chart APIs before capability and migration requirements are met.

## 5. Functional Requirements

### 5.1 Query execution

#### Structured queries

- [x] `p1` - **ID**: `cpt-insightspec-qe-fr-ask`

The system **MUST** execute a query naming a dataset, time window, and at least one
aggregate, with optional filters, grouping, ordering, and row limit.

**Rationale**: Supported queries must not require pre-built metrics.

**Actors**: `cpt-insightspec-qe-actor-admin`, `cpt-insightspec-qe-actor-constructor`

#### Dimension and time grouping

- [x] `p1` - **ID**: `cpt-insightspec-qe-fr-group`

The system **MUST** group by declared dimensions and by day, week, or month. For dimensions
with a declared label, results **MUST** include the label beside the stable value.

**Rationale**: Clients need stable identifiers and readable labels.

**Actors**: `cpt-insightspec-qe-actor-author`

#### Row filters

- [x] `p1` - **ID**: `cpt-insightspec-qe-fr-filter`

The system **MUST** support equality, membership, ordered comparisons, inclusive ranges,
non-null checks, wildcard patterns, and regular expressions. It **MUST** reject operations
incompatible with the target field, including ordered text comparisons and numeric patterns.

**Rationale**: Field-aware validation prevents misleading comparisons.

**Actors**: `cpt-insightspec-qe-actor-author`

#### Conditional aggregates

- [x] `p1` - **ID**: `cpt-insightspec-qe-fr-fold`

The system **MUST** support count, sum, average, minimum, and maximum, each with an optional
filter. An aggregate over no observations **MUST** return null, except count, which returns
zero.

**Rationale**: Null distinguishes missing observations from measured zero.

**Actors**: `cpt-insightspec-qe-actor-author`

#### Query limits

- [x] `p1` - **ID**: `cpt-insightspec-qe-fr-bounds`

The system **MUST** require a time window of at most 731 days and bound result rows,
response size, and concurrent execution. Results **MUST** indicate row-limit truncation.

**Rationale**: Resource limits protect shared capacity; truncation must remain visible.

**Actors**: `cpt-insightspec-qe-actor-admin`

### 5.2 Validation

#### Validation errors

- [x] `p1` - **ID**: `cpt-insightspec-qe-fr-refusal`

The system **MUST** validate requests before execution and report violations with field
paths, machine-readable reasons, and valid choices where available. It **MUST** report
independent semantic violations together when the request can be resolved.

**Rationale**: Clients can identify invalid inputs without repeated trial-and-error requests.

**Actors**: `cpt-insightspec-qe-actor-author`

### 5.3 Planned capabilities

#### Boolean filter groups

- [ ] `p2` - **ID**: `cpt-insightspec-qe-fr-boolean`

The system **MUST** support nested AND, OR, and NOT conditions.

**Rationale**: Compound selection requires more than the initial implicit AND. Nesting limits
remain a design parameter.

**Actors**: `cpt-insightspec-qe-actor-author`

#### Relative time windows

- [ ] `p2` - **ID**: `cpt-insightspec-qe-fr-relative-window`

The system **MUST** support relative and calendar windows, including recent weeks, the
current month, the previous quarter, and year-to-date. Results **MUST** report resolved dates.

**Rationale**: Saved charts need windows that advance over time.

**Actors**: `cpt-insightspec-qe-actor-author`

#### Derived dimensions

- [ ] `p2` - **ID**: `cpt-insightspec-qe-fr-derived`

The system **MUST** group by caller-defined categories and date parts such as weekday or hour.

**Rationale**: Authors need classifications beyond shipped dimensions.

**Actors**: `cpt-insightspec-qe-actor-author`

#### Advanced aggregation

- [ ] `p2` - **ID**: `cpt-insightspec-qe-fr-richer-folds`

The system **MUST** support exact and approximate distinct counts, medians, percentiles,
dispersion, computed-column arithmetic, running and moving calculations, share of total,
top-N with an Other group, empty-period fill, subtotals, grand totals, and per-period
aggregation for quantities such as headcount.

**Rationale**: These operations expand chart coverage beyond basic aggregates.

**Actors**: `cpt-insightspec-qe-actor-author`

#### Calendars and period comparison

- [ ] `p3` - **ID**: `cpt-insightspec-qe-fr-time-intelligence`

The system **MUST** support a chosen timezone, week start, and fiscal-year start, and
compare equivalent portions of successive periods.

**Rationale**: Calendar settings determine comparable reporting periods.

**Actors**: `cpt-insightspec-qe-actor-author`

#### Pagination and drilldown

- [ ] `p3` - **ID**: `cpt-insightspec-qe-fr-rows`

The system **MUST** support pagination beyond the result row limit and retrieval of records
contributing to a result cell.

**Rationale**: Users need complete result traversal and supporting records.

**Actors**: `cpt-insightspec-qe-actor-lead`

#### Saved queries

- [ ] `p3` - **ID**: `cpt-insightspec-qe-fr-saved`

The system **MUST** save named queries and revalidate them on each use. Callers may change
time windows, ordering, and limits, and add filters; changes to the query definition
**MUST** be rejected. DESIGN specifies the proposed override contract.

**Rationale**: Revalidation and restricted overrides preserve query meaning.

**Actors**: `cpt-insightspec-qe-actor-author`, `cpt-insightspec-qe-actor-lead`

#### Cross-dataset queries and enrichment

- [ ] `p3` - **ID**: `cpt-insightspec-qe-fr-cross-dataset`

The system **MUST** combine datasets sharing dimensions without aggregate inflation and
support tenant reference data as additional dimensions wherever the keys are compatible.

**Rationale**: Shared dimensions enable analysis across sources and tenant classifications.

**Actors**: `cpt-insightspec-qe-actor-author`

#### Funnels and retention

- [ ] `p3` - **ID**: `cpt-insightspec-qe-fr-funnels`

The system **MUST** count entities reaching ordered steps within a period and entities
returning after an initial event.

**Rationale**: Sequence and return-rate analysis require operations beyond independent counts.

**Actors**: `cpt-insightspec-qe-actor-author`

### 5.4 Authorization

#### Initial administrator restriction

- [x] `p1` - **ID**: `cpt-insightspec-qe-fr-admin-gate`

The system **MUST** reject query execution and discovery for non-administrators until
access policies protect datasets containing personal data.

**Rationale**: The initial engine has no row- or column-level access-policy enforcement.

**Actors**: `cpt-insightspec-qe-actor-admin`, `cpt-insightspec-qe-actor-lead`

#### Dataset access policies

- [ ] `p2` - **ID**: `cpt-insightspec-qe-fr-access-policies`

The system **MUST** enforce dataset access, row visibility based on caller attributes, and
column visibility for every query, regardless of its grouping.

**Rationale**: Grouping by a resource must not bypass restrictions on the underlying people.

**Actors**: `cpt-insightspec-qe-actor-lead`, `cpt-insightspec-qe-actor-identity`

## 6. Non-Functional Requirements

### 6.1 NFR Inclusions

#### Consistent result semantics

- [x] `p1` - **ID**: `cpt-insightspec-qe-nfr-correctness`

Queries **MUST** enforce tenant isolation, dataset read rules, null-preserving aggregates
with the count exception, and consistent UTC day boundaries. Callers **MUST NOT** override
these rules.

**Threshold**: equivalence checks for regrouping and filtering must agree within their
applicable aggregate semantics.

**Rationale**: query configuration must not bypass shared data rules.

#### Bounded execution

- [x] `p1` - **ID**: `cpt-insightspec-qe-nfr-bounded`

The system **MUST** reject requests exceeding declared input limits and stop execution when
runtime limits are reached.

**Threshold**: at most 731 days, 10,000 result rows, 16 MiB per answer, and eight concurrent
scans per process; excess demand receives a retryable rejection.

**Rationale**: time and row limits alone do not bound query cost or result size.

#### Interactive latency

- [ ] `p2` - **ID**: `cpt-insightspec-qe-nfr-latency`

Queries with up to four grouping axes over one year **SHOULD** complete within two seconds
at the 95th percentile on a reference environment.

**Threshold**: p95 ≤ 2 seconds. Reference data volume and workload remain to be defined;
this is a target, not a measured guarantee.

**Rationale**: query composition requires responsive feedback.

#### Published contract

- [x] `p1` - **ID**: `cpt-insightspec-qe-nfr-contract`

Request and response formats **MUST** have a machine-readable contract. Unknown request
members **MUST** be rejected rather than ignored.

**Threshold**: contract drift checks fail on divergence between published and generated schemas.

**Rationale**: clients need a consistent request and response structure. Semantic constraints
remain subject to runtime validation.

#### Personal-data handling

- [x] `p1` - **ID**: `cpt-insightspec-qe-nfr-person-data`

Personal fields **MUST** be restricted to authorized callers. The engine **MUST NOT** persist
query result data; retention and deletion remain with the source systems.

**Threshold**: no engine-owned result store. Dataset personal-field classification and
fine-grained policies are planned; the initial implementation uses the administrator gate.

**Rationale**: query execution must not create an independently retained copy of activity data.

### 6.2 NFR Exclusions

- **Offline operation**: queries require warehouse access.
- **Durable engine storage**: absent until saved queries are implemented.
- **Authentication, availability, and recovery**: inherited from the hosting service.
- **Accessibility, localization, and device support**: owned by consuming clients under product standards.
- **Additional certifications**: no module-specific requirements.
- **Monitoring**: uses service query-latency and error metrics.

## 7. Public Library Interfaces

### 7.1 Public API Surface

#### Query contract

- [x] `p1` - **ID**: `cpt-insightspec-qe-interface-query`

**Type**: published analytics request and response format.

**Stability**: experimental while administrator-only.

**Description**: structured queries and typed tabular results; discovery reports query limits.

**Breaking Change Policy**: accepted operations retain their meaning; extensions are additive.

### 7.2 External Integration Contracts

#### Caller context (planned)

- [ ] `p2` - **ID**: `cpt-insightspec-qe-contract-caller-context`

**Direction**: required from identity.

**Protocol/Format**: caller permissions, visible people, and team membership resolved per query.

**Compatibility**: missing attributes required by a policy cause rejection, not broader access.

## 8. Use Cases

#### Build a query-backed chart

- [x] `p1` - **ID**: `cpt-insightspec-qe-usecase-build-chart`

**Actor**: `cpt-insightspec-qe-actor-admin`

**Preconditions**: administrator access and an available dataset.

**Main Flow**:

1. The client presents dataset fields and query limits.
2. The administrator selects a time window, grouping, aggregates, and filters.
3. The engine returns a typed result or validation errors.
4. The client renders the result and its truncation status.

**Postconditions**: a chart uses a query without a pre-built metric.

**Alternative Flows**: invalid inputs are identified; truncated results are labelled.

#### Conditional aggregation

- [x] `p1` - **ID**: `cpt-insightspec-qe-usecase-own-category`

**Actor**: `cpt-insightspec-qe-actor-author`

**Preconditions**: administrator access and a dataset with a path dimension and line measurable.

**Main Flow**:

1. The author requests total lines and lines matching a path pattern, grouped by file type.
2. The engine validates the pattern and computes both aggregates.

**Postconditions**: results compare a caller-defined category with the total.

**Alternative Flows**: invalid patterns produce a field violation.

#### Team-scoped query (planned)

- [ ] `p2` - **ID**: `cpt-insightspec-qe-usecase-lead-visibility`

**Actor**: `cpt-insightspec-qe-actor-lead`

**Preconditions**: an enforced dataset policy identifies the caller's visible people.

**Main Flow**:

1. The lead opens a saved query grouped by repository.
2. The engine restricts rows to authorized people before aggregation.

**Postconditions**: results respect the caller's visibility regardless of grouping.

**Alternative Flows**: missing visibility context causes rejection.

## 9. Acceptance Criteria

- [x] Administrators can query declared datasets without pre-built metrics.
- [x] Automated reconciliation cases cover equivalent grouping and filtering operations.
- [x] Non-administrators are denied query execution and discovery.
- [x] Semantic validation errors identify invalid request fields.
- [ ] Saved queries reject incompatible dataset changes.
- [ ] Team-lead queries enforce row visibility independently of grouping.

The initial delivery provides the API. The chart-builder workflow also requires the planned
constructor client; completed API requirements do not establish end-to-end UI delivery.

## 10. Dependencies

| Dependency | Purpose | Criticality |
|---|---|---|
| Dataset registry | Validated declarations and prepared-data semantics | p1 |
| Schema catalog | Snapshot metadata used by dataset validation | p1 |
| Identity service | Administrator checks; planned visibility context | p1 |
| Published API contract | Client integration and schema drift checks | p1 |

## 11. Assumptions

- Shipped datasets provide prepared data; tenant-defined datasets are a later extension.
- Administrators may query their session's tenant under the initial access model.
- Reconciliation fixtures cover each supported dataset and the semantics being compared.
- Query cost depends on data volume, selectivity, and grouping cardinality as well as dates
  and result limits. Limits alone do not establish latency guarantees.

**Open questions**

| Question | Owner | Resolve by |
|---|---|---|
| Where are team-to-resource mappings defined? | product + backend | before access policies |
| What replaces minimum-peer suppression for broader access? | product | before team-lead access |
| What reference volume and workload define the latency target? | backend | before latency acceptance |
| What nesting limit applies to Boolean groups? | backend | before Boolean filters |
| Which datasets follow commits and file changes? | product | after constructor workflows |
| Who may create datasets, and what must approvers review? | product | before tenant-defined datasets |
| Is drilldown a query mode or a separate operation? | backend | before drilldown |

## 12. Risks

| Risk | Impact | Mitigation |
|---|---|---|
| Unsupported query operations | Continued dependence on pre-built metrics or SQL | Explicit capability roadmap; retain existing APIs |
| Delayed access policies | Team leads cannot use the engine | Keep administrator restriction until enforcement is available |
| Broad or high-cardinality queries | Slow execution or resource exhaustion | Input, byte, timeout, and concurrency limits |
| Dataset schema changes | Invalid saved queries | Planned revalidation; document snapshot limits |
| Invalid tenant-defined datasets | Incorrect results or unintended access | Shared validation and limits; planned approval and policies |

**Revision 1.3**: clarified query terminology, initial delivery, planned capabilities,
validation boundaries, and unresolved requirements.
