---
status: draft
date: 2026-09-07
---

# Decomposition: Insight v3 Core

<!-- toc -->

- [1. Overview](#1-overview)
- [2. Entries](#2-entries)
  - [2.1 Data Ingestion - HIGH](#21-data-ingestion---high)
  - [2.2 Semantic Layer - HIGH](#22-semantic-layer---high)
  - [2.3 Widgets - HIGH](#23-widgets---high)
  - [2.4 Dashboards - HIGH](#24-dashboards---high)
  - [2.5 Alerts - MEDIUM](#25-alerts---medium)
  - [2.6 AI - HIGH](#26-ai---high)
  - [2.7 Data Access - HIGH](#27-data-access---high)
  - [2.8 Definition Management - HIGH](#28-definition-management---high)
  - [2.9 Authoring over MCP - HIGH](#29-authoring-over-mcp---high)
  - [2.10 Access Control - HIGH](#210-access-control---high)
  - [2.11 People in Metrics - MEDIUM](#211-people-in-metrics---medium)
- [3. Feature Dependencies](#3-feature-dependencies)

<!-- /toc -->

## 1. Overview

Eleven features: data in, data out, metrics over it, widgets, dashboards, alerts, and a chat over
all of it — plus what every definition kind shares, an agent-facing way to author them, who is
allowed to, and naming people in them.

The platform-usage requirements in [PRD §5.7](./PRD.md#57-platform-usage) are not decomposed yet.

## 2. Entries

**Overall implementation status:**

- [ ] `p1` - **ID**: `cpt-insightspec-v3-status-overall`

### 2.1 Data Ingestion - HIGH

- [x] `p1` - **ID**: `cpt-insightspec-v3-feature-data-ingestion`

- **Purpose**: Get data in: a table per ingest stream, then rows into it.

- **Depends On**: None

- **Requirements Covered**:

  - [x] `p1` - `cpt-insightspec-v3-fr-accept-data`
  - [x] `p1` - `cpt-insightspec-v3-fr-write-connectors`
  - [ ] `p2` - `cpt-insightspec-v3-fr-first-class-connectors`

- **Data**:

  - `cpt-insightspec-v3-dbtable-raw-data`

### 2.2 Semantic Layer - HIGH

- [x] `p1` - **ID**: `cpt-insightspec-v3-feature-semantic-layer`

- **Purpose**: Create metrics based on ingested data.

- **Depends On**: 2.1

- **Requirements Covered**:

  - [x] `p1` - `cpt-insightspec-v3-fr-create-metrics`
  - [ ] `p2` - `cpt-insightspec-v3-fr-metrics-for-stand`

### 2.3 Widgets - HIGH

- [x] `p1` - **ID**: `cpt-insightspec-v3-feature-widgets`

- **Purpose**: Create widgets in realtime, per user.

- **Depends On**: 2.2

- **Requirements Covered**:

  - [x] `p1` - `cpt-insightspec-v3-fr-create-widgets`

- **Out of scope**:
  - Sharing widgets, which comes later

### 2.4 Dashboards - HIGH

- [x] `p1` - **ID**: `cpt-insightspec-v3-feature-dashboards`

- **Purpose**: Create dashboards in realtime, per user.

- **Depends On**: 2.3

- **Requirements Covered**:

  - [x] `p1` - `cpt-insightspec-v3-fr-create-dashboards`

- **Out of scope**:
  - Sharing dashboards, which comes later

### 2.5 Alerts - MEDIUM

- [ ] `p2` - **ID**: `cpt-insightspec-v3-feature-alerts`

- **Purpose**: Create alerts in realtime, delivered to Zulip.

- **Depends On**: 2.2

- **Requirements Covered**:

  - [ ] `p2` - `cpt-insightspec-v3-fr-create-alerts`

- **Out of scope**:
  - Destinations other than Zulip

### 2.6 AI - HIGH

- [ ] `p1` - **ID**: `cpt-insightspec-v3-feature-ai`

- **Purpose**: A chat over the data, reading context the team supplies of its own — what their
  metrics mean, which tables to prefer, the words they use for things.

- **Status**: answering and creating are built. Reader-supplied context is not; today the model
  sees only the stand's tables, their sampled fields and what is already stored.

- **Depends On**: 2.2, 2.3, 2.4, 2.5

- **Requirements Covered**:

  - [x] `p1` - `cpt-insightspec-v3-fr-ai-answer`
  - [x] `p1` - `cpt-insightspec-v3-fr-ai-create`

### 2.7 Data Access - HIGH

- [ ] `p1` - **ID**: `cpt-insightspec-v3-feature-data-access`

- **Purpose**: Get data out. Name is provisional.

- **Status**: rows come out only through a metric run — no raw read, no download.

- **Depends On**: 2.1, 2.2

- **Requirements Covered**:

  - [ ] `p1` - `cpt-insightspec-v3-fr-read-data`
  - [ ] `p2` - `cpt-insightspec-v3-fr-download-report`

### 2.8 Definition Management - HIGH

- [x] `p1` - **ID**: `cpt-insightspec-v3-feature-definition-management`

- **Purpose**: What a metric, a widget, a dashboard and later an alert all share: one store keyed
  by kind and name, editing and renaming and deleting, and a page at a time with a total and a
  search when there are hundreds.

  It is one entry rather than a line in each of 2.2 to 2.5 because the rules run between kinds: a
  metric cannot be deleted while a widget draws it, and renaming it rewrites the widgets that name
  it.

- **Depends On**: 2.2, 2.3, 2.4

- **Requirements Covered**: TBD — no PRD requirement names editing or browsing yet.

### 2.9 Authoring over MCP - HIGH

- [x] `p1` - **ID**: `cpt-insightspec-v3-feature-mcp-authoring`

- **Purpose**: Let an agent do what an author does — read the table catalogue, write a metric, run
  it, draw it, arrange a board — through MCP rather than the browser.

- **Depends On**: 2.2, 2.3, 2.4, 2.8, 2.10

- **Requirements Covered**: TBD — the PRD describes the chat, not an agent-facing surface.

### 2.10 Access Control - HIGH

- [x] `p1` - **ID**: `cpt-insightspec-v3-feature-access-control`

- **Purpose**: Decide who may do what: a token for ingest, an administrator for anything that
  creates a table, a person's own authority for authoring, and a read-only warehouse user for
  every query the assistant runs.

- **Depends On**: 2.1

- **Requirements Covered**: TBD — [PRD §5.3](./PRD.md#53-access-control) is not written yet.

### 2.11 People in Metrics - MEDIUM

- [x] `p2` - **ID**: `cpt-insightspec-v3-feature-people-in-metrics`

- **Purpose**: Show a person by the name they are known by rather than the address a source system
  recorded, by resolving a column against the identity mirror.

- **Depends On**: 2.2

- **Requirements Covered**: TBD — [PRD §5.2](./PRD.md#52-identity-resolution) is not written yet.

- **Out of scope**:
  - Resolving identities. This reads what identity-resolution publishes.

## 3. Feature Dependencies

```text
2.10 Access Control
 |
2.1 Data Ingestion
 |
 +-- 2.7 Data Access
 +-- 2.2 Semantic Layer
      |
      +-- 2.11 People in Metrics
      +-- 2.5 Alerts
      +-- 2.3 Widgets --- 2.4 Dashboards
                           |
                           +-- 2.8 Definition Management
                           +-- 2.6 AI (over 2.2-2.5)
                           +-- 2.9 Authoring over MCP (over 2.2-2.4, 2.8)
```
