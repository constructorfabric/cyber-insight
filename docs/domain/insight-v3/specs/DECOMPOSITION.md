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
  - [2.8 Authoring over MCP - HIGH](#28-authoring-over-mcp---high)
- [3. Feature Dependencies](#3-feature-dependencies)

<!-- /toc -->

## 1. Overview

Eight features, each with the elements it is built from and the state of each one.

The platform-usage requirements in [PRD §5.7](./PRD.md#57-platform-usage) are not decomposed yet.

## 2. Entries

**Overall implementation status:**

- [ ] `p1` - **ID**: `cpt-insightspec-v3-status-overall`

### 2.1 Data Ingestion - HIGH

- [ ] `p1` - **ID**: `cpt-insightspec-v3-feature-data-ingestion`

- **Purpose**: Get data in.

- **Depends On**: None

- **Elements**:

  - [x] `POST /v1/raw-data` — a JSON payload, named by the stream it belongs to
  - [x] `PUT /v1/tables/{table}` — the stream's own table, fixed schema
  - [x] the table catalogue, each table's fields sampled from its rows
  - [ ] a connector shipped with the product

- **Data**:

  - `cpt-insightspec-v3-dbtable-raw-data`

- **Requirements Covered**:

  - [x] `p1` - `cpt-insightspec-v3-fr-accept-data`
  - [x] `p1` - `cpt-insightspec-v3-fr-write-connectors`
  - [ ] `p2` - `cpt-insightspec-v3-fr-first-class-connectors`

### 2.2 Semantic Layer - HIGH

- [ ] `p1` - **ID**: `cpt-insightspec-v3-feature-semantic-layer`

- **Purpose**: Create metrics based on ingested data.

- **Depends On**: 2.1

- **Elements**:

  - [x] `PUT`/`GET /v1/metrics/{name}`, and `POST /v1/metrics/{name}/run`
  - [x] fields, filters, grouping, ordering and a row limit, compiled to SQL
  - [x] a field dividing two of the query's own fields, read as a percentage
  - [x] a condition per aggregate, for a rate whose halves share one column
  - [x] a field naming a person, shown by the name identity knows them by
  - [x] refuses a table this stand does not have, and any unsafe identifier
  - [x] rename and delete, a delete refused while a widget draws the metric
  - [x] the catalogue: a page at a time with the total, search over name and body
  - [ ] metrics shipped for the stand

- **Requirements Covered**:

  - [x] `p1` - `cpt-insightspec-v3-fr-create-metrics`
  - [ ] `p2` - `cpt-insightspec-v3-fr-metrics-for-stand`

### 2.3 Widgets - HIGH

- [ ] `p1` - **ID**: `cpt-insightspec-v3-feature-widgets`

- **Purpose**: Create widgets in realtime, per user.

- **Depends On**: 2.2

- **Elements**:

  - [x] `PUT`/`GET /v1/widgets/{name}`
  - [x] table, line, bar, area, stat and pie
  - [x] refuses a widget naming a column its metric does not produce
  - [x] rename and delete, a delete refused while a dashboard holds the widget
  - [x] the catalogue: a page at a time with the total, search over name and body

- **Requirements Covered**:

  - [x] `p1` - `cpt-insightspec-v3-fr-create-widgets`

- **Out of scope**:
  - Sharing widgets, which comes later

### 2.4 Dashboards - HIGH

- [ ] `p1` - **ID**: `cpt-insightspec-v3-feature-dashboards`

- **Purpose**: Create dashboards in realtime, per user.

- **Depends On**: 2.3

- **Elements**:

  - [x] `PUT`/`GET /v1/dashboards/{name}`
  - [x] an ordered list of items: widgets, headings, free text
  - [x] reordering, which keeps the title and refuses an unknown widget
  - [x] the board itself, at `/portal/custom/{name}`
  - [x] rename and delete
  - [x] the catalogue: a page at a time with the total, search over name and body

- **Requirements Covered**:

  - [x] `p1` - `cpt-insightspec-v3-fr-create-dashboards`

- **Out of scope**:
  - Sharing dashboards, which comes later

### 2.5 Alerts - MEDIUM

- [ ] `p2` - **ID**: `cpt-insightspec-v3-feature-alerts`

- **Purpose**: Create alerts in realtime, delivered to Zulip.

- **Depends On**: 2.2

- **Elements**:

  - [ ] an alert over a metric, with the condition that fires it
  - [ ] something that evaluates it
  - [ ] delivery to Zulip
  - [ ] rename and delete, and a catalogue like the others

- **Requirements Covered**:

  - [ ] `p2` - `cpt-insightspec-v3-fr-create-alerts`

- **Out of scope**:
  - Destinations other than Zulip

### 2.6 AI - HIGH

- [ ] `p1` - **ID**: `cpt-insightspec-v3-feature-ai`

- **Purpose**: A chat over the data.

- **Depends On**: 2.2, 2.3, 2.4, 2.5

- **Elements**:

  - [x] `POST /v1/chat` — answers a question, or writes definitions
  - [x] the stand's tables and their sampled fields, in the prompt
  - [x] a repair round when the model's JSON or its query is refused
  - [x] the chat panel beside a board
  - [ ] context a team writes for itself: what its metrics mean, which tables to prefer

- **Requirements Covered**:

  - [x] `p1` - `cpt-insightspec-v3-fr-ai-answer`
  - [x] `p1` - `cpt-insightspec-v3-fr-ai-create`

### 2.7 Data Access - HIGH

- [ ] `p1` - **ID**: `cpt-insightspec-v3-feature-data-access`

- **Purpose**: Get data out, and decide who may.

- **Depends On**: 2.1, 2.2

- **Elements**:

  - [x] a token on every ingest write
  - [x] an administrator, checked against identity, for creating a table
  - [x] a read-only warehouse user for every query the assistant runs
  - [ ] read raw rows over the API
  - [ ] the same rows downloaded as a report
  - [ ] which data a caller may read, decided by their role

- **Requirements Covered**:

  - [ ] `p1` - `cpt-insightspec-v3-fr-read-data`
  - [ ] `p2` - `cpt-insightspec-v3-fr-download-report`

### 2.8 Authoring over MCP - HIGH

- [ ] `p1` - **ID**: `cpt-insightspec-v3-feature-mcp-authoring`

- **Purpose**: Let an agent author what a person authors.

- **Depends On**: 2.2, 2.3, 2.4, 2.7

- **Elements**:

  - [x] an MCP server over streamable HTTP, behind the gateway
  - [x] the challenge, the protected-resource metadata and the `mcp:author` scope
  - [x] ten tools: the table catalogue, a metric written and run, a widget, a board and its
        order, and the catalogue read, searched and deleted from
  - [ ] a credential for a client that cannot open a browser

- **Requirements Covered**: TBD — the PRD describes the chat, not an agent-facing surface.

## 3. Feature Dependencies

```text
2.1 Data Ingestion
 |
 +-- 2.7 Data Access
 +-- 2.2 Semantic Layer
      |
      +-- 2.5 Alerts
      +-- 2.3 Widgets --- 2.4 Dashboards
                           |
                           +-- 2.6 AI (over 2.2-2.5)
                           +-- 2.8 Authoring over MCP (over 2.2-2.4, 2.7)
```
