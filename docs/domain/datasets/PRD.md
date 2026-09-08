---
version: 1.1
status: proposed
date: 2026-09-08
---

# PRD — Datasets

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
  - [5.1 Dataset definitions](#51-dataset-definitions)
  - [5.2 Registry and discovery](#52-registry-and-discovery)
  - [5.3 Initial datasets](#53-initial-datasets)
  - [5.4 Tenant-defined datasets (planned)](#54-tenant-defined-datasets-planned)
  - [5.5 Access policies (planned)](#55-access-policies-planned)
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

A dataset defines queryable data at a documented row grain. It specifies dimensions for
grouping and filtering, numeric fields for aggregation, and supported time fields. Chart
builders discover these capabilities without inspecting warehouse schemas.

This PRD covers shipped datasets and planned extensions for tenant-defined datasets and
access policies. [DESIGN.md](DESIGN.md) describes the implementation and its limits. Query
execution belongs to the [query engine](https://github.com/constructorfabric/insight/blob/d912c610da33259475d817fc009d34de35b8fe10/docs/domain/query-engine/PRD.md).

### 1.2 Background / Problem Statement

Pre-built metrics encode their source data, aggregation, and supported dimensions separately.
They do not provide a reusable description of the underlying data for chart builders.

Warehouse tables alone do not supply that contract. Consumers need an explicit row grain,
queryable fields, display labels, and consistent deduplication and attribution rules.

Tenant-defined datasets would extend this catalog with custom classifications, derived data,
and imported tables.

### 1.3 Goals (Business Outcomes)

- Build queries from discoverable dataset metadata.
- Define and verify each dataset's row grain.
- Add shipped datasets without changing query-engine code.
- Apply the same validation requirements to shipped and tenant-defined datasets.
- Declare access policies once per dataset for enforcement on every query.

### 1.4 Glossary

| Term | Meaning |
|---|---|
| Dataset | Queryable data with a declared grain, dimensions, measurables, and time fields |
| Grain | What one row represents |
| Row identity | Fields that uniquely identify a row at the declared grain |
| Dimension | Field available for grouping and filtering |
| Breakdown | Query results grouped by one or more dimensions |
| Measurable | Numeric field available for aggregation; distinct from a pre-built metric |
| Declaration | Dataset metadata stored as a file or record |
| Shipped dataset | Dataset maintained and released with the product |
| Tenant-defined dataset | Dataset created within a tenant at runtime |
| Registry | Catalog of available dataset declarations |
| Access policy | Dataset, row, or column visibility rules |

## 2. Actors

### 2.1 Human Actors

#### Dashboard author

**ID**: `cpt-insightspec-ds-actor-author`

**Role**: builds charts and queries.
**Needs**: discoverable fields and documented dataset semantics.

#### Instance administrator

**ID**: `cpt-insightspec-ds-actor-admin`

**Role**: accesses initial discovery; approves tenant-defined datasets in the planned extension.
**Needs**: dataset provenance, authorship, and access policies for review.

#### Product engineer

**ID**: `cpt-insightspec-ds-actor-engineer`

**Role**: maintains shipped datasets.
**Needs**: self-contained definitions and validation before release.

### 2.2 System Actors

#### Query engine

**ID**: `cpt-insightspec-ds-actor-engine`

**Role**: consumes declarations to plan and execute queries.

#### Constructor page

**ID**: `cpt-insightspec-ds-actor-constructor`

**Role**: uses discovery metadata to present dataset fields and construct queries.

#### Warehouse

**ID**: `cpt-insightspec-ds-actor-warehouse`

**Role**: stores the prepared data backing datasets.

## 3. Operational Concept & Environment

### 3.1 Module-Specific Environment Constraints

- Datasets use prepared pipeline data; declarations do not change data collection.
- Shipped definitions change through product releases, not runtime edits.

## 4. Scope

### 4.1 In Scope

**Initial implementation**: shipped declarations, row identity and validation, a registry,
admin-only discovery, and the commits and file changes datasets.

**Planned extensions**: field descriptions, units, value suggestions, tenant-defined datasets,
approval and versioning, and declared access policies. These remain product requirements;
they are not implemented by the initial release.

### 4.2 Out of Scope

- Query execution, aggregation, and chart rendering.
- Data collection and pipeline implementation.
- Access-policy enforcement, owned by the query engine.
- Selection of subsequent shipped datasets.

## 5. Functional Requirements

### 5.1 Dataset definitions

#### Dataset definition

- [x] `p1` - **ID**: `cpt-insightspec-ds-fr-single-home`

A dataset **MUST** have one definition that groups its data preparation, grain,
queryable fields, and documentation.

**Rationale**: Reviewing these together reduces inconsistent changes.

**Actors**: `cpt-insightspec-ds-actor-engineer`

#### Row grain

- [x] `p1` - **ID**: `cpt-insightspec-ds-fr-grain`

Every dataset **MUST** declare its row identity and verify its uniqueness against the
prepared data.

**Rationale**: Duplicate rows can inflate counts and sums.

**Actors**: `cpt-insightspec-ds-actor-engineer`

#### Queryable fields

- [x] `p1` - **ID**: `cpt-insightspec-ds-fr-fields`

A dataset **MUST** expose all stable dimensions for grouping and filtering, identify their
display-label fields, and declare its measurables and time fields.

**Rationale**: A missing dimension prevents queries that the underlying data could support.
The criteria for including a field as a stable dimension remain open in Section 11.

**Actors**: `cpt-insightspec-ds-actor-author`

#### Missing dimension values

- [x] `p1` - **ID**: `cpt-insightspec-ds-fr-absent-values`

A dataset **MUST** declare a visible, filterable representation for missing dimension
values. A declaration without a required representation **MUST** be rejected.

**Rationale**: Missing values must remain identifiable in grouped results. The current
null-handling contract is documented in DESIGN; broader missing-value semantics remain open.

**Actors**: `cpt-insightspec-ds-actor-author`

#### Consistent data preparation

- [x] `p1` - **ID**: `cpt-insightspec-ds-fr-correctness-baked`

Dataset preparation **MUST** apply deduplication, attribution, and superseded-record
exclusion rules before queries use the data.

**Rationale**: Queries over the same dataset must use the same counting and attribution rules.

**Actors**: `cpt-insightspec-ds-actor-engineer`

### 5.2 Registry and discovery

#### Dataset registry

- [x] `p1` - **ID**: `cpt-insightspec-ds-fr-registry`

All dataset consumers **MUST** use one registry of available declarations. The registry
**MUST** support shipped and, in the planned extension, tenant-defined datasets.

**Rationale**: Discovery and query execution must agree on available datasets.

**Actors**: `cpt-insightspec-ds-actor-engine`

#### Declaration validation

- [x] `p1` - **ID**: `cpt-insightspec-ds-fr-validated`

Before exposing a dataset, the product **MUST** validate its referenced fields and their
types, rejecting invalid declarations with an error identifying the dataset and field.
Shipped declarations **MUST** also be validated before release.

**Rationale**: Invalid definitions should fail before users depend on them. DESIGN states the
current validation source and its limits.

**Actors**: `cpt-insightspec-ds-actor-engineer`, `cpt-insightspec-ds-actor-admin`

#### Dataset discovery

- [x] `p1` - **ID**: `cpt-insightspec-ds-fr-discovery`

The product **MUST** describe available datasets, dimensions, labels, measurables, and time
fields without exposing storage details. Initial discovery **MUST** be restricted to instance
administrators; broader visibility depends on the planned access policies.

**Rationale**: Consumers need query capabilities without warehouse access.

**Actors**: `cpt-insightspec-ds-actor-constructor`

#### Field metadata and value suggestions

- [ ] `p2` - **ID**: `cpt-insightspec-ds-fr-field-metadata`

Each exposed field **MUST** have a display name and short description; numeric fields
**MUST** include units. The product **MUST** suggest values present in a field.

**Rationale**: Shared metadata keeps field presentation consistent across clients.

**Actors**: `cpt-insightspec-ds-actor-constructor`

### 5.3 Initial datasets

#### Commits and file changes

- [x] `p1` - **ID**: `cpt-insightspec-ds-fr-first-two`

The product **MUST** provide commits and file changes datasets. Commits represent authored
commits; file changes represent changes to paths within commits. Both **MUST** expose author,
repository, project, source, branch scope, event time, and line counts.

**Rationale**: These datasets establish the contract at two related grains. DESIGN specifies
their exact row identities and preparation rules.

**Actors**: `cpt-insightspec-ds-actor-author`

### 5.4 Tenant-defined datasets (planned)

#### Tenant-defined datasets

- [ ] `p3` - **ID**: `cpt-insightspec-ds-fr-own-datasets`

Authorized authors **MUST** be able to create datasets at runtime as derived datasets with
filters, labels, and custom categories; reusable saved queries; or imported tables and lists.
An assistant may create these on the author's behalf.

**Rationale**: Tenant-specific data and classifications extend shipped datasets.

**Actors**: `cpt-insightspec-ds-actor-author`, `cpt-insightspec-ds-actor-admin`

#### Tenant-defined dataset validation

- [ ] `p3` - **ID**: `cpt-insightspec-ds-fr-own-validated`

Tenant-defined datasets **MUST** use the same validation rules and errors as shipped
datasets, remain tenant-scoped, and be revalidated after underlying schema changes.

**Rationale**: Dataset origin must not weaken validation or tenant isolation.

**Actors**: `cpt-insightspec-ds-actor-author`

#### Approval before sharing

- [ ] `p3` - **ID**: `cpt-insightspec-ds-fr-approval`

A newly created tenant-defined dataset **MUST** be available immediately to its author and
**MUST NOT** be available to others until an administrator approves it and sets its access
policies.

**Rationale**: Authors can explore privately before publishing shared data.

**Actors**: `cpt-insightspec-ds-actor-admin`

#### Dataset version history

- [ ] `p3` - **ID**: `cpt-insightspec-ds-fr-versioning`

Editing a tenant-defined dataset **MUST** create a new version and preserve earlier
versions. The product **MUST** record each version's author and approver.

**Rationale**: Version history supports investigation of changed query results.

**Actors**: `cpt-insightspec-ds-actor-admin`

### 5.5 Access policies (planned)

#### Declared access policies

- [ ] `p2` - **ID**: `cpt-insightspec-ds-fr-access-rules`

A dataset **MUST** support declarations for dataset access, row visibility, and column
visibility. It **MUST** identify fields containing personal data.

**Rationale**: The query engine needs dataset-level policies to enforce consistent access.

**Actors**: `cpt-insightspec-ds-actor-admin`, `cpt-insightspec-ds-actor-engine`

## 6. Non-Functional Requirements

### 6.1 NFR Inclusions

#### Dataset extensibility

- [x] `p1` - **ID**: `cpt-insightspec-ds-nfr-additive`

Adding a shipped dataset **MUST NOT** require query-engine changes.

**Threshold**: zero changes to query-engine code when adding a supported dataset definition.

**Rationale**: dataset additions should not require engine development.

#### Validation before use

- [x] `p1` - **ID**: `cpt-insightspec-ds-nfr-fail-early`

Invalid shipped declarations **MUST** block release. Invalid runtime declarations **MUST**
be rejected on save.

**Threshold**: all declaration mismatches detectable by the validator are rejected before use.

**Rationale**: query authors cannot repair dataset definitions. The initial implementation
validates shipped declarations; runtime validation is planned.

#### Discovery without warehouse queries

- [x] `p1` - **ID**: `cpt-insightspec-ds-nfr-cheap-discovery`

Dataset discovery **MUST NOT** query the warehouse.

**Threshold**: zero warehouse queries per discovery request.

**Rationale**: loading dataset metadata must not add warehouse query load.

### 6.2 NFR Exclusions

- **Warehouse writes**: declarations describe prepared data; this module does not write it.
- **Authentication**: supplied by the hosting service; dataset permissions remain in scope.
- **Availability and recovery**: inherit the hosting service's requirements.
- **Accessibility and translation**: apply to consuming interfaces; this module has no UI.

## 7. Public Library Interfaces

### 7.1 Public API Surface

#### Dataset discovery

- [x] `p1` - **ID**: `cpt-insightspec-ds-interface-discovery`

**Type**: read-only dataset metadata.

**Stability**: experimental while administrator-only.

**Description**: available datasets and their queryable fields.

**Breaking Change Policy**: existing fields retain their meaning; metadata additions are
additive. DESIGN defines the interface contract.

### 7.2 External Integration Contracts

#### Prepared data

- [x] `p1` - **ID**: `cpt-insightspec-ds-contract-prepared-data`

**Direction**: required from the data pipeline.

**Protocol/Format**: one prepared relation per dataset at its declared grain, with
counting and attribution rules applied.

**Compatibility**: referenced fields must exist with compatible types; detected mismatches
invalidate the declaration.

## 8. Use Cases

#### Discover dataset fields

- [x] `p1` - **ID**: `cpt-insightspec-ds-usecase-discover`

**Actor**: `cpt-insightspec-ds-actor-author`

**Preconditions**: the author has discovery permission; initially this requires administrator access.

**Main Flow**:

1. The author selects an available dataset.
2. The client presents its dimensions, measurables, and time fields.
3. The author builds a query using those fields.

**Postconditions**: the query uses discoverable metadata without warehouse inspection.

**Alternative Flows**: if no datasets are available, the client reports that state.

#### Add a shipped dataset

- [x] `p1` - **ID**: `cpt-insightspec-ds-usecase-ship-one`

**Actor**: `cpt-insightspec-ds-actor-engineer`

**Preconditions**: prepared data exists or is supplied with the dataset.

**Main Flow**:

1. The engineer defines the dataset's grain, fields, preparation, and documentation.
2. Validation checks the declaration and row uniqueness.
3. The dataset becomes discoverable in a product release.

**Postconditions**: a dataset is available without query-engine changes.

**Alternative Flows**: invalid definitions block release with a validation error.

#### Create and share a tenant-defined dataset (planned)

- [ ] `p3` - **ID**: `cpt-insightspec-ds-usecase-own-dataset`

**Actor**: `cpt-insightspec-ds-actor-author`

**Preconditions**: the author may create datasets within the tenant.

**Main Flow**:

1. The author derives and saves a dataset.
2. Validation succeeds; the dataset becomes available to its author.
3. An administrator sets access policies and approves sharing.

**Postconditions**: authorized users can query the approved dataset.

**Alternative Flows**: invalid definitions are rejected; unapproved datasets remain private.

## 9. Acceptance Criteria

- [x] Commits and file changes datasets declare their row identities.
- [x] Automated checks verify row uniqueness against prepared data.
- [x] Discovery describes queryable fields without exposing storage details.
- [x] Invalid field references fail shipped-declaration validation before release.
- [ ] Runtime declarations receive equivalent validation.
- [ ] Tenant-defined datasets remain author-only until approved for sharing.

These markers distinguish delivered requirements from planned ones. They are not evidence
that every deployment or end-to-end workflow has been validated.

## 10. Dependencies

| Dependency | Purpose | Criticality |
|---|---|---|
| Data pipeline | Prepared relations and grain checks | p1 |
| Schema catalog | Field and type information for validation | p1 |
| Query engine | Query planning and execution from declarations | p1 |
| Identity service | Administrator checks; future approval and access policies | p1 |

## 11. Assumptions

- Prepared relations can be queried within the query engine's limits.
- Shared dataset definitions can serve multiple query and chart configurations.
- Derived datasets are expected to cover common customization needs; this is unverified.

**Open questions**

| Question | Owner | Resolve by |
|---|---|---|
| What qualifies a dimension as stable and eligible for exposure? | product + backend | before extending field selection rules |
| Does missing-value handling cover empty strings or source sentinels as well as nulls? | product + backend | before extending missing-value semantics |
| Which datasets follow commits and file changes? | product | after initial chart workflows |
| What must administrators review before approving a dataset? | product | before tenant-defined datasets |
| Where are team-to-resource mappings defined for access policies? | product + backend | before access-policy implementation |

## 12. Risks

| Risk | Impact | Mitigation |
|---|---|---|
| Incorrect grain | Inflated counts or sums | Declare row identity and test uniqueness |
| Invalid declarations | Failed queries | Shared validation before release or save |
| Duplicate tenant-defined datasets | Unclear dataset selection | Approval and purpose documentation |
| Schema drift | Invalid references or changed results | Revalidation; explicit snapshot limits in DESIGN |

**Revision 1.1**: clarified terminology, implementation scope, and validation limits; recorded
unresolved field-selection and missing-value semantics.
