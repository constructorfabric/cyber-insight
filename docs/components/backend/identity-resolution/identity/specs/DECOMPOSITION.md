---
status: draft
version: "1.1"
date: 2026-09-08
---

# Decomposition: Identity Service Quality Adoption

**Revision 1.1:** Trace upstream requirements through the feature to vector tests;
keep acceptance conditions separate from scenario tracking.

<!-- toc -->

- [1. Overview](#1-overview)
- [2. Entries](#2-entries)
  - [2.1 Profile Resolution — HIGH](#21-profile-resolution--high)
- [3. Feature Dependencies](#3-feature-dependencies)

<!-- /toc -->

## 1. Overview

This manifest registers the profile-resolution slice for explicit adoption of the
quality-vector PRD and FEATURE format. It is a partial decomposition of the
existing service, not a claim that its other capabilities are absent or accepted.
The service [PRD](PRD.md) and [DESIGN](DESIGN.md) remain upstream references;
contract disagreements and measurement prerequisites are recorded in the FEATURE.

## 2. Entries

**Overall implementation status:**

- [ ] `p1` - **ID**: `cpt-identity-svc-status-overall`

### 2.1 [Profile Resolution](feature-profile-resolution/FEATURE.md) — HIGH

- [ ] `p1` - **ID**: `cpt-identity-svc-feature-profile-resolution`

- **Purpose**: Establish a reviewable quality acceptance baseline for a current
  profile lookup, including inherited domain obligations and unverified targets.
- **Depends On**: None within this adoption manifest; migrated storage, seeded
  observations and verified caller context are external prerequisites.
- **Scope**:
  - Single profile resolution, projection and refusal semantics.
  - Local and inherited NFR traceability through feature-owned vector scenarios and test links.
  - Explicit contract, visibility, source-fixture and measurement decisions.
- **Out of scope**:
  - Batch and roster APIs, corrections, seed/migration lifecycle, roles and login bootstrap.
  - Runtime changes, new executable tests, or a declaration that existing NFRs pass.
- **Requirements Covered**:
  - `cpt-insightspec-fr-identity-profile-resolve`
  - `cpt-insightspec-fr-identity-profile-ambiguous-422`
  - `cpt-insightspec-fr-identity-profile-ids-list`
  - `cpt-insightspec-fr-identity-profile-org-tree`
  - `cpt-insightspec-fr-identity-profile-validation`
  - `cpt-insightspec-fr-identity-lookup-hydrate`
  - `cpt-insightspec-fr-identity-lookup-404`
  - `cpt-insightspec-fr-identity-lookup-parent`
  - `cpt-insightspec-fr-identity-lookup-subordinates`
  - `cpt-insightspec-fr-identity-routing-name-split`
  - [ ] `p1` - `cpt-insightspec-nfr-identity-latency`
  - [ ] `p1` - `cpt-insightspec-nfr-identity-memory`
  - [ ] `p1` - `cpt-insightspec-nfr-identity-logging-pii`
  - [ ] `p1` - `cpt-insightspec-nfr-identity-uuid-roundtrip`
  - [ ] `p1` - `cpt-identity-svc-nfr-profile-source-coverage`
  - [ ] `p1` - `cpt-ir-nfr-tenant-isolation`
  - [ ] `p1` - `cpt-ir-nfr-alias-lookup-latency`
- **Design Principles Covered**:
  - [ ] `p1` - `cpt-insightspec-principle-identity-observation-log`
  - [ ] `p1` - `cpt-insightspec-principle-identity-centralised-sql`
  - [ ] `p1` - `cpt-insightspec-principle-identity-pii-boundary`
- **Design Constraints Covered**:
  - [ ] `p1` - `cpt-insightspec-constraint-identity-binary16-uuid`
  - [ ] `p1` - `cpt-insightspec-constraint-identity-mysql-backend`
- **Domain Model Entities**: `ProfileResponse`, `PersonResponse`, identity observations.
- **Design Components**:
  - [ ] `p1` - `cpt-insightspec-component-identity-api`
  - [ ] `p1` - `cpt-insightspec-component-identity-domain`
  - [ ] `p1` - `cpt-insightspec-component-identity-infra`
- **API**: `POST /v1/profiles`.
- **Sequences**:
  - [ ] `p1` - `cpt-insightspec-seq-identity-lookup-happy`
- **Data**:
  - [ ] `p1` - `cpt-insightspec-dbtable-identity-persons`
  - [ ] `p1` - `cpt-insightspec-dbtable-identity-org-chart`

## 3. Feature Dependencies

Only one feature is registered in this adoption manifest, so there are no
intra-manifest dependencies. Producer behaviour and domain-wide evidence retain
their own scope. The feature cannot be accepted until its named review decisions,
deferred scenarios, and measurement prerequisites are resolved.
