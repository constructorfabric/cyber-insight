---
version: 1.0
status: proposed
date: 2026-09-08
---

# PRD — Data Sets

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
  - [5.1 Defining a data set](#51-defining-a-data-set)
  - [5.2 Offering data sets](#52-offering-data-sets)
  - [5.3 The first data sets](#53-the-first-data-sets)
  - [5.4 Data sets people add](#54-data-sets-people-add)
  - [5.5 Who may see what](#55-who-may-see-what)
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

A data set is one kind of thing people can ask about — commits, file changes, later
deployments or tickets. It names what can be broken down, what can be counted, and over
which dates, in words a person recognises. Everything the product can answer, and everything
a chart can offer, comes from this list.

Built by [DESIGN.md](DESIGN.md). Asking questions of these data sets is the
[query engine](../query-engine/PRD.md).

### 1.2 Background / Problem Statement

The product's numbers are pre-built metrics. Each one hard-codes which table it reads, what
it counts and how it may be split. Nothing describes the underlying data itself, so there is
no list a person or a page can choose from, and every new question means a developer reading
warehouse tables to work out where the answer lives.

The tables themselves are not a substitute. They carry columns nobody should have to reason
about, they need rules applied to avoid double counting, and they say nothing about which
column is a name for which other column. A person cannot tell from a table what one row
means.

Customers also have facts we do not: which repositories belong to which product line, which
people belong to which cost centre. Today those cannot enter the product at all.

### 1.3 Goals (Business Outcomes)

- Anyone building a chart can see what is available to ask about, in readable words, without
  reading a table or asking a developer.
- One row of a data set means one thing, and that meaning is written down where the data set
  is defined.
- Adding a data set is a self-contained piece of work: define it, describe it, ship it — no
  change to the code that answers questions.
- People and their assistants add data sets of their own, and what they add is checked the
  same way as what we ship.
- The rules about who may see which rows are attached to the data set, so every question
  inherits them.

### 1.4 Glossary

| Term | Meaning |
|---|---|
| **Data set** | one kind of thing to ask about, with everything needed to ask: what one row is, what it can be split by, what can be counted, which dates apply |
| **Grain** | what one row of a data set represents, exactly: one commit, one file in one commit |
| **Breakdown** | a column an answer can be split by, with a readable name |
| **Measure** | a number that can be counted or added up |
| **Declaration** | the file or record that states all of the above |
| **Shipped data set** | one the product provides and maintains |
| **Own data set** | one a person or their assistant adds to their own installation |
| **Registry** | the single list of every data set available right now, shipped and added |
| **Access rule** | a condition attached to a data set that decides who may ask about it and which rows count |

## 2. Actors

### 2.1 Human Actors

#### Dashboard author

**ID**: `cpt-insightspec-ds-actor-author`

**Role**: builds charts and questions for other people.
**Needs**: to see what exists and what each thing means before building anything.

#### Instance administrator

**ID**: `cpt-insightspec-ds-actor-admin`

**Role**: runs the installation; approves data sets people add before others rely on them.
**Needs**: to see what was added, by whom, and over what data, before approving.

#### Product engineer

**ID**: `cpt-insightspec-ds-actor-engineer`

**Role**: adds and maintains the data sets the product ships.
**Needs**: to add one in a single place, with checks that catch a mistake before release.

### 2.2 System Actors

#### Query engine

**ID**: `cpt-insightspec-ds-actor-engine`

**Role**: the only reader of the registry; turns a question about a data set into an answer.

#### Constructor page

**ID**: `cpt-insightspec-ds-actor-constructor`

**Role**: shows people what is available and builds a question from it.

#### Warehouse

**ID**: `cpt-insightspec-ds-actor-warehouse`

**Role**: holds the prepared tables behind the data sets.

## 3. Operational Concept & Environment

### 3.1 Module-Specific Environment Constraints

- A data set stands on prepared data that the pipeline already builds; defining one never
  changes how data is collected.
- What the product ships is fixed in a release: a shipped data set changes by shipping a new
  version, never by editing it in a running installation.

## 4. Scope

### 4.1 In Scope

- What a data set is: its grain, its breakdowns and their readable names, its measures, its
  dates, and how its rows must be read to avoid double counting.
- Where a data set is defined: one place, next to the thing that builds its data.
- The registry that holds every available data set and hands it to whoever asks.
- Describing the available data sets to people and pages.
- Checking a data set against the data that actually exists, before it is offered.
- The first two: commits and file changes.
- Data sets people add themselves, and the approval step before others use them.
- Access rules as something a data set declares.

### 4.2 Out of Scope

- Asking questions: filters, breakdowns, counting, charts — the [query
  engine](../query-engine/PRD.md).
- Collecting data and building the tables underneath.
- Enforcing access rules at answer time; that is the engine applying what is declared here.
- Deciding which data sets to build next.

## 5. Functional Requirements

### 5.1 Defining a data set

#### One definition, in one place

- [x] `p1` - **ID**: `cpt-insightspec-ds-fr-single-home`

A data set **MUST** be defined in one place that holds everything about it: how its data is
built, what one row means, what can be asked of it, and its own page of notes.

**Rationale**: split across the codebase, the parts drift and nobody can review a data set
as one thing.

**Actors**: `cpt-insightspec-ds-actor-engineer`

#### Say what one row is

- [x] `p1` - **ID**: `cpt-insightspec-ds-fr-grain`

Every data set **MUST** state what makes one row unique, and that statement **MUST** be
enforced by an automatic check against the built data.

**Rationale**: every number is a count or a sum of rows; if one row is not one thing, every
number is wrong.

**Actors**: `cpt-insightspec-ds-actor-engineer`

#### Offer every stable column

- [x] `p1` - **ID**: `cpt-insightspec-ds-fr-fields`

A data set **MUST** offer, as things to break down or filter by, every column whose value is
stable enough to mean something, and **MUST** name which other column carries the readable
label for each. It **MUST** state which columns hold numbers that can be counted or added up,
and which hold the dates a question can range over.

**Rationale**: an axis that exists in the data but not in the list is a question nobody can
ask; a value without its label is a chart nobody can read.

**Actors**: `cpt-insightspec-ds-actor-author`

#### Name the rows with nothing there

- [x] `p1` - **ID**: `cpt-insightspec-ds-fr-absent-values`

Where a column may hold nothing, the data set **MUST** give those rows a name people can see
and filter by, and the product **MUST** refuse a data set that leaves them unnamed.

**Rationale**: rows that vanish from a breakdown make totals that do not add up.

**Actors**: `cpt-insightspec-ds-actor-author`

#### Carry the counting rules

- [x] `p1` - **ID**: `cpt-insightspec-ds-fr-correctness-baked`

A data set **MUST** carry the rules that make its numbers right — the same work counted once
however many sources reported it, the right person credited, superseded records excluded —
applied once where its data is built, not by whoever asks.

**Rationale**: rules that live in the asker's head produce a different number for every
asker.

**Actors**: `cpt-insightspec-ds-actor-engineer`

### 5.2 Offering data sets

#### Keep one list

- [x] `p1` - **ID**: `cpt-insightspec-ds-fr-registry`

The product **MUST** keep a single list of the data sets available right now, holding both
the ones it ships and the ones people added, and everything that reads data sets **MUST**
read that list.

**Rationale**: two lists become two answers to "what can I ask about".

**Actors**: `cpt-insightspec-ds-actor-engine`

#### Check against the data that exists

- [x] `p1` - **ID**: `cpt-insightspec-ds-fr-validated`

Before a data set is offered, the product **MUST** check every column it names against the
data that actually exists, and refuse it — saying which part is wrong — if anything is
missing or of the wrong kind. For shipped data sets this check **MUST** also run before
release.

**Rationale**: a promise the data cannot keep is an error in front of a person instead of a
failed build.

**Actors**: `cpt-insightspec-ds-actor-engineer`, `cpt-insightspec-ds-actor-admin`

#### Describe what is available

- [x] `p1` - **ID**: `cpt-insightspec-ds-fr-discovery`

The product **MUST** describe every available data set — its breakdowns with their labels,
its measures, its dates — to people and pages that may see it, and **MUST NOT** reveal where
or how the data is stored.

**Rationale**: a page builds a question from this description; storage details help nobody
and expose the warehouse.

**Actors**: `cpt-insightspec-ds-actor-constructor`

#### Readable names and value suggestions

- [ ] `p2` - **ID**: `cpt-insightspec-ds-fr-field-metadata`

Each column a data set offers **MUST** carry a readable name, a short description and, for
numbers, a unit; and the product **MUST** be able to suggest the values a column actually
holds.

**Rationale**: wording is the product's job, not each page's, and nobody should type a
repository name from memory.

**Actors**: `cpt-insightspec-ds-actor-constructor`

### 5.3 The first data sets

#### Commits and file changes

- [x] `p1` - **ID**: `cpt-insightspec-ds-fr-first-two`

The product **MUST** ship a commits data set — one row per commit somebody wrote — and a file
changes data set — one row per file a commit touched — each with the person, the repository,
the project, the source, the branch scope, the dates and the lines involved.

**Rationale**: they answer most questions asked of engineering data today and prove the shape
works for two different grains.

**Actors**: `cpt-insightspec-ds-actor-author`

### 5.4 Data sets people add

#### Add your own

- [ ] `p3` - **ID**: `cpt-insightspec-ds-fr-own-datasets`

The product **MUST** let someone add a data set at runtime in three forms: a narrowed or
relabelled version of an existing one, with their own categories added; a saved question
other people can break down further; and their own table or uploaded list. An assistant may
write any of these on someone's behalf.

**Rationale**: the questions worth asking outgrow any list we ship.

**Actors**: `cpt-insightspec-ds-actor-author`, `cpt-insightspec-ds-actor-admin`

#### Same checks, whoever wrote it

- [ ] `p3` - **ID**: `cpt-insightspec-ds-fr-own-validated`

A data set someone adds **MUST** be checked exactly like a shipped one, refused in the same
words, kept inside the organisation that added it, and re-checked whenever the data beneath
it changes shape.

**Rationale**: an added data set is used for the same decisions as a shipped one.

**Actors**: `cpt-insightspec-ds-actor-author`

#### Yours immediately, shared after approval

- [ ] `p3` - **ID**: `cpt-insightspec-ds-fr-approval`

A newly added data set **MUST** be usable straight away by whoever added it, and **MUST NOT**
be available to anyone else until an administrator approves it and its access rules are set.

**Rationale**: exploring should be instant; sharing a number other people act on should not
be.

**Actors**: `cpt-insightspec-ds-actor-admin`

#### Keep the versions

- [ ] `p3` - **ID**: `cpt-insightspec-ds-fr-versioning`

Editing an added data set **MUST** create a new version rather than overwrite the old one,
and the product **MUST** record who wrote and who approved each version.

**Rationale**: a chart that changed needs an answer to "what changed, and who agreed to it".

**Actors**: `cpt-insightspec-ds-actor-admin`

### 5.5 Who may see what

#### Declare the access rules

- [ ] `p2` - **ID**: `cpt-insightspec-ds-fr-access-rules`

A data set **MUST** be able to declare who may ask about it at all, which rows count towards
a given person's answers, and which columns they may see — including the fact that some of
its columns describe named people.

**Rationale**: visibility is a property of the data, not of each question asked about it.

**Actors**: `cpt-insightspec-ds-actor-admin`, `cpt-insightspec-ds-actor-engine`

## 6. Non-Functional Requirements

### 6.1 NFR Inclusions

#### Adding one is a contained change

- [x] `p1` - **ID**: `cpt-insightspec-ds-nfr-additive`

Adding a shipped data set **MUST** require no change to the code that answers questions.

**Threshold**: a new data set is a new folder plus its declaration; nothing in the query
engine is edited.

**Rationale**: if every data set costs engine work, the list stops growing.

#### Broken definitions never reach people

- [x] `p1` - **ID**: `cpt-insightspec-ds-nfr-fail-early`

A shipped data set that does not match the data **MUST** stop the build; one added at runtime
**MUST** be refused when it is saved.

**Threshold**: no path where a mismatch first appears as a failed question.

**Rationale**: the person asking cannot fix a broken definition.

#### Descriptions cost nothing to read

- [x] `p1` - **ID**: `cpt-insightspec-ds-nfr-cheap-discovery`

Describing the available data sets **MUST NOT** touch the warehouse.

**Threshold**: descriptions served from the list already in memory.

**Rationale**: a page asks on every load; the warehouse is for answering questions.

### 6.2 NFR Exclusions

- **Storing anything**: shipped data sets ship with the product; added ones live in the
  product's own store. This part writes nothing to the warehouse.
- **Signing in**: handled before any request reaches this.
- **Uptime and recovery**: inherited from the service the registry lives in.
- **Accessibility and translation**: no interface of its own; the pages that read it follow
  the product's standards.

## 7. Public Library Interfaces

### 7.1 Public API Surface

#### The description of what is available

- [x] `p1` - **ID**: `cpt-insightspec-ds-interface-discovery`

**Type**: published read-only description of every available data set

**Stability**: still changing while administrator-only

**Description**: the list a page or a person uses to find out what can be asked about.

**Breaking Change Policy**: a field already described keeps its meaning; new detail is added,
never substituted.

### 7.2 External Integration Contracts

#### The prepared data underneath

- [x] `p1` - **ID**: `cpt-insightspec-ds-contract-prepared-data`

**Direction**: required from the data pipeline

**Protocol/Format**: one prepared table per data set, at the stated grain, with the counting
rules already applied

**Compatibility**: a column a data set names must exist; if it stops existing, the check
fails rather than the answer being wrong.

## 8. Use Cases

#### Find out what can be asked about

- [x] `p1` - **ID**: `cpt-insightspec-ds-usecase-discover`

**Actor**: `cpt-insightspec-ds-actor-author`

**Preconditions**:
- At least one data set is available to this person.

**Main Flow**:
1. The author opens the page that builds questions.
2. The product lists the available data sets and, for the chosen one, what can be broken
   down, what can be counted, and which dates apply.
3. The author builds a question from that list alone.

**Postconditions**:
- A question was built without anyone reading a table.

**Alternative Flows**:
- **Nothing is available to this person**: the page says so rather than showing an empty
  list of fields.

#### Add a data set to the product

- [x] `p1` - **ID**: `cpt-insightspec-ds-usecase-ship-one`

**Actor**: `cpt-insightspec-ds-actor-engineer`

**Preconditions**:
- The data behind it exists or is built as part of the same change.

**Main Flow**:
1. The engineer creates the data set's folder: how its data is built, what one row is, what
   can be asked, and a page of notes.
2. The checks confirm every named column exists and that one row is one thing.
3. The data set ships and appears in the list.

**Postconditions**:
- A new data set is available and nothing in the query engine changed.

**Alternative Flows**:
- **A named column does not exist**: the build fails, naming the column.

#### Add one of your own

- [ ] `p3` - **ID**: `cpt-insightspec-ds-usecase-own-dataset`

**Actor**: `cpt-insightspec-ds-actor-author`

**Preconditions**:
- The person may add data sets in their organisation.

**Main Flow**:
1. The author narrows an existing data set, adds their own categories, and saves it.
2. The product checks it as it would a shipped one and makes it available to the author.
3. An administrator reviews it, sets its access rules, and approves it.

**Postconditions**:
- Other people can build charts on it.

**Alternative Flows**:
- **It does not match the data**: refused when saved, naming what is wrong.
- **Not approved**: it stays visible to its author alone.

## 9. Acceptance Criteria

- [x] The commits and file changes data sets are available, and each states what one row is.
- [x] Each one's grain is enforced by a check against the built data.
- [x] The description of a data set names its breakdowns, labels, measures and dates, and no
  storage detail.
- [x] A data set naming a column the data does not have fails before release.
- [ ] A data set added at runtime is refused on the same grounds a shipped one would be.
- [ ] An added data set is invisible to everyone but its author until approved.

## 10. Dependencies

| Dependency | Description | Criticality |
|------------|-------------|-------------|
| Data pipeline | builds the prepared table behind each data set | p1 |
| Record of what the warehouse holds | what declarations are checked against before release | p1 |
| Query engine | the reader of the registry; turns data sets into answers | p1 |
| Identity service | who is asking, for approval and access rules | p2 |

## 11. Assumptions

- Most questions are answered by a handful of well-shaped data sets rather than many narrow
  ones.
- A data set's prepared table is small enough to be queried directly within the engine's
  limits.
- People adding their own will mostly narrow and relabel what exists rather than bring
  entirely new data.

**Open questions**

| Question | Owner | Resolve by |
|---|---|---|
| Which data sets come after commits and file changes, in what order? | product | after the first charts are built |
| How does an administrator review an added data set — what do they see? | product | when own data sets start |
| Where do team-to-resource facts live, for the access rules that need them? | product + backend | before the first access rule |

## 12. Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| A data set's grain is wrong | every number built on it is wrong, quietly | the grain is stated and checked automatically against the built data |
| The list grows faster than the checks | broken definitions reach people | one validation path for shipped and added alike, run before release and on save |
| People add near-duplicate data sets | nobody knows which to use | approval before sharing; each one carries a page saying what it is for |
| A prepared table changes shape underneath | questions fail or answer differently | declarations are re-checked against the data; a mismatch is refused, not guessed |
