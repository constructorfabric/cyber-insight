---
version: 1.2
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
  - [5.1 Asking a question](#51-asking-a-question)
  - [5.2 Being told what went wrong](#52-being-told-what-went-wrong)
  - [5.3 Questions we cannot answer yet](#53-questions-we-cannot-answer-yet)
  - [5.4 Who is allowed to ask](#54-who-is-allowed-to-ask)
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

Let a person ask their own question of the data the product already collects — narrow it,
break it down, count or add something up, watch it over time — and get a table they can turn
into a chart. Nobody has to build that question into the product first.

What can be asked about is the [data sets](../datasets/PRD.md); this document is how a
question about one gets asked and answered. Built by [DESIGN.md](DESIGN.md). What happens to
today's charts is in [MIGRATION.md](MIGRATION.md).

### 1.2 Background / Problem Statement

Every chart in the product is a pre-built metric. Someone decided in advance what it counts,
what you can break it down by, and how far back it looks. A question nobody anticipated —
"how much of our change volume goes into test files, by file type, on GitHub only" — needs
new code and a release. The person with the question has to ask an engineer and wait.

A page that lets people build their own charts cannot exist either, because there is nothing
for it to offer: no list of what can be broken down, filtered or counted, no readable names,
no idea how far back it may look.

There is a SQL console, but only administrators who know the warehouse can use it. It cannot
be opened to everyone: raw SQL cannot be made to respect who is allowed to see whose data,
and it gives no protection against the mistakes that quietly produce a wrong number.

### 1.3 Goals (Business Outcomes)

- A new question takes minutes, not a release. Today: one release cycle from question to
  chart. Target: the same working session.
- Answers are right without the asker knowing anything about the warehouse — no double
  counting, no invented zeroes, no data from another organisation, days that start and end
  at the same moment for everyone.
- When a question cannot be answered, the page says which part is wrong and what would work
  instead.
- A saved question either still means what it meant, or it stops and says so. It never
  quietly starts answering something else.
- Once we can enforce who may see whose data, the same page opens to leads, not just
  administrators.
- A question can be asked of any data set the product offers, including ones added after the
  engine shipped, without the engine learning anything about them first.

### 1.4 Glossary

| Term | Meaning |
|---|---|
| **Data set** | one kind of thing you can ask about; defined and described by the [data sets](../datasets/PRD.md) |
| **Breakdown** | splitting an answer into rows: per person, per repository, per week |
| **Measure** | something you can count or add up: commits, lines added |
| **Filter** | a condition that keeps only the rows you mean |
| **Time window** | the from–to dates every question must state |
| **Time step** | how wide each bar or point is: a day, a week, a month |
| **Rejection** | an answer the system refuses to give, saying which part of the question is wrong |
| **Access rule** | who may ask about a data set, and which rows their answers may include |
| **Constructor** | the page where an administrator builds a question and sees the result |

## 2. Actors

### 2.1 Human Actors

#### Instance administrator

**ID**: `cpt-insightspec-qe-actor-admin`

**Role**: runs one installation of the product; today the only person allowed to ask.
**Needs**: answer a new question the same day, and understand a rejection without asking an
engineer.

#### Team lead

**ID**: `cpt-insightspec-qe-actor-lead`

**Role**: looks at numbers about the people they are responsible for, and only those people.
**Needs**: their limits applied automatically, and charts that keep working over time.

#### Dashboard author

**ID**: `cpt-insightspec-qe-actor-author`

**Role**: builds charts and saved questions for other people to read.
**Needs**: to see what can be asked — the available breakdowns, measures and limits.

### 2.2 System Actors

#### Constructor page

**ID**: `cpt-insightspec-qe-actor-constructor`

**Role**: shows what can be asked, turns a filled-in form into a question, draws the answer.

#### Identity service

**ID**: `cpt-insightspec-qe-actor-identity`

**Role**: says who is asking, what they are allowed to do, and whose data they may see.

#### Warehouse

**ID**: `cpt-insightspec-qe-actor-warehouse`

**Role**: stores the prepared data and answers each question.

## 3. Operational Concept & Environment

### 3.1 Module-Specific Environment Constraints

- Reads only prepared, ready-to-query data. It never touches raw collected data and never
  changes anything.
- A data set can only be offered if it really exists in the warehouse; a mismatch is caught
  when the product is built, not when someone asks.

## 4. Scope

### 4.1 In Scope

- Asking a question: filters, breakdowns including over time, counts and totals with their
  own conditions, sorting, a row limit, a required time window.
- Telling a caller the bounds a question must stay inside, reported alongside what each data
  set says about itself.
- Getting a clear answer: a table with readable names beside the values, and a note when the
  answer was cut short.
- Clear rejections that point at the part of the question to fix.
- An administrator-only page for building questions.
- A named list of what comes next, as requirements: or/not conditions, rolling time windows,
  your own categories, unique counts, percentages of a total, running and moving figures,
  subtotals, paging, saved questions, and enforcing access rules.

### 4.2 Out of Scope

- What can be asked about: how a data set is defined, described and added — the
  [data sets](../datasets/PRD.md).
- Letting people write SQL; the administrator SQL console stays as it is.
- How a chart looks: chart types, layouts, formatting.
- Step-by-step journey analysis over individual event streams.
- Alerts and notifications about an answer.
- Switching the existing charts over before this can do what they do
  ([MIGRATION.md](MIGRATION.md)).

## 5. Functional Requirements

### 5.1 Asking a question

#### Ask a data set directly

- [x] `p1` - **ID**: `cpt-insightspec-qe-fr-ask`

The system **MUST** answer a question that names a data set, a time window and at least one
thing to count, plus optional filters, breakdowns, sorting and a row limit — with nothing
built in advance.

**Rationale**: the whole point; a question should not need a release.

**Actors**: `cpt-insightspec-qe-actor-admin`, `cpt-insightspec-qe-actor-constructor`

#### Break down by anything, including time

- [x] `p1` - **ID**: `cpt-insightspec-qe-fr-group`

The system **MUST** allow a breakdown by any offered column and by day, week or month, and
**MUST** return the readable name beside the value it stands for.

**Rationale**: a chart needs a stable value to key on and a name a person can read.

**Actors**: `cpt-insightspec-qe-actor-author`

#### Narrow to the rows you mean

- [x] `p1` - **ID**: `cpt-insightspec-qe-fr-filter`

The system **MUST** filter by exact value, a list of values, greater or less than, a range,
"has a value", and text patterns (simple wildcards or a regular expression). It **MUST**
reject a greater-or-less-than test on text and a text pattern on a number.

**Rationale**: the asker decides what "test files" means. A comparison on the wrong kind of
column gives a wrong answer that looks right.

**Actors**: `cpt-insightspec-qe-actor-author`

#### Count and total, with conditions

- [x] `p1` - **ID**: `cpt-insightspec-qe-fr-fold`

The system **MUST** count rows and compute totals, averages, smallest and largest, each
optionally over just the rows a condition selects. When nothing matched, the cell **MUST** be
blank rather than a zero — except a count, where zero is the true answer.

**Rationale**: one question can answer several columns at once, and a made-up zero is a wrong
number.

**Actors**: `cpt-insightspec-qe-actor-author`

#### Keep every answer within safe limits

- [x] `p1` - **ID**: `cpt-insightspec-qe-fr-bounds`

The system **MUST** require a time window of at most 731 days (two years), limit how many
rows an answer holds, how large it may be and how many questions run at once, and **MUST**
say when an answer was cut at the row limit.

**Rationale**: everyone shares one warehouse, and a chart that was cut short without saying
so is misleading.

**Actors**: `cpt-insightspec-qe-actor-admin`

### 5.2 Being told what went wrong

#### Point at every problem at once

- [x] `p1` - **ID**: `cpt-insightspec-qe-fr-refusal`

The system **MUST** check the whole question before rejecting it, and report every problem
with the part of the question it concerns, a code a page can act on, and — where there is a
list of valid choices — that list.

**Rationale**: a page can highlight the wrong boxes; a person reads one clear message.

**Actors**: `cpt-insightspec-qe-actor-author`

### 5.3 Questions we cannot answer yet

#### And, or, not

- [ ] `p2` - **ID**: `cpt-insightspec-qe-fr-boolean`

The system **MUST** allow conditions combined with "any of", "all of" and "not", nested as
deeply as needed.

**Rationale**: "GitHub or GitLab, but not bots" cannot be said with "and" alone.

**Actors**: `cpt-insightspec-qe-actor-author`

#### Rolling time windows

- [ ] `p2` - **ID**: `cpt-insightspec-qe-fr-relative-window`

The system **MUST** accept windows stated relative to today — the last twelve weeks, this
month, the previous quarter, the year so far — and report the actual dates it used.

**Rationale**: a saved chart with fixed dates is out of date tomorrow.

**Actors**: `cpt-insightspec-qe-actor-author`

#### Your own categories, and parts of a date

- [ ] `p2` - **ID**: `cpt-insightspec-qe-fr-derived`

The system **MUST** allow a breakdown by categories the asker defines with their own rules,
and by parts of a date such as day of the week or hour.

**Rationale**: everyone's definition of "test code" or "core service" differs.

**Actors**: `cpt-insightspec-qe-actor-author`

#### The rest of the usual chart maths

- [ ] `p2` - **ID**: `cpt-insightspec-qe-fr-richer-folds`

The system **MUST** offer unique counts (exact and estimated), medians and percentiles,
spread, arithmetic between computed columns, running and moving figures, percentage of a
total, a top-N with everything else grouped as "other", empty periods shown as gaps or
zeroes, subtotals and grand totals, and measures counted once per period such as headcount.

**Rationale**: this is what today's charts already do and what anyone building a chart
expects.

**Actors**: `cpt-insightspec-qe-actor-author`

#### Calendars and comparisons

- [ ] `p3` - **ID**: `cpt-insightspec-qe-fr-time-intelligence`

The system **MUST** work in a chosen time zone, with a chosen first day of the week and start
of the financial year, and **MUST** compare a period against the same stretch of the period
before it.

**Rationale**: "versus last quarter" comes up in every review.

**Actors**: `cpt-insightspec-qe-actor-author`

#### See more, and look behind a number

- [ ] `p3` - **ID**: `cpt-insightspec-qe-fr-rows`

The system **MUST** let a reader page past the row limit, and **MUST** show the individual
records behind any single figure in an answer.

**Rationale**: a number nobody can open is a number nobody trusts.

**Actors**: `cpt-insightspec-qe-actor-lead`

#### Save a question and share it

- [ ] `p3` - **ID**: `cpt-insightspec-qe-fr-saved`

The system **MUST** save a question under a name, re-check it every time it is opened, let a
reader change the dates, sorting and row limit and add their own filters, and refuse changes
that would make it a different question.

**Rationale**: a dashboard is a set of saved questions; they must break loudly rather than
drift.

**Actors**: `cpt-insightspec-qe-actor-author`, `cpt-insightspec-qe-actor-lead`

#### Combine sources, and bring your own reference data

- [ ] `p3` - **ID**: `cpt-insightspec-qe-fr-cross-dataset`

The system **MUST** answer one question spanning several data sets that share a breakdown,
without inflating any number, and **MUST** let a customer's own reference list — repository
to product line, person to cost centre — be used as a breakdown everywhere it fits.

**Rationale**: real questions cross sources, and product lines and cost centres are facts
only the customer has.

**Actors**: `cpt-insightspec-qe-actor-author`

#### Funnels and return rates

- [ ] `p3` - **ID**: `cpt-insightspec-qe-fr-funnels`

The system **MUST** count how many people, deals or tickets reached each step of a sequence
within a period, and how many came back after a first action.

**Rationale**: deal stages, ticket transitions and the life of a pull request are all
funnels.

**Actors**: `cpt-insightspec-qe-actor-author`

### 5.4 Who is allowed to ask

#### Administrators only, for now

- [x] `p1` - **ID**: `cpt-insightspec-qe-fr-admin-gate`

The system **MUST** refuse both questions and the list of what can be asked to anyone who is
not an administrator of the installation, until access rules exist for the data sets that
describe people.

**Rationale**: these data sets show what named people did.

**Actors**: `cpt-insightspec-qe-actor-admin`, `cpt-insightspec-qe-actor-lead`

#### Access rules

- [ ] `p2` - **ID**: `cpt-insightspec-qe-fr-access-policies`

For each data set the system **MUST** enforce who may ask about it at all, which rows may
count towards their answers — based on facts about the asker, such as the people they manage
or the repositories their teams own — and which columns they may see. This **MUST** apply to
every question, no matter how it is broken down.

**Rationale**: a count per repository is still a count of people's work; what a chart is
grouped by must not change who is visible in it.

**Actors**: `cpt-insightspec-qe-actor-lead`, `cpt-insightspec-qe-actor-identity`

## 6. Non-Functional Requirements

### 6.1 NFR Inclusions

#### Right answers without expert knowledge

- [x] `p1` - **ID**: `cpt-insightspec-qe-nfr-correctness`

Every answer **MUST** cover only the asker's own organisation, count nothing twice, leave a
cell blank when nothing matched, and treat a day as the same stretch of time for everyone.
There **MUST** be no way for a question to switch any of this off.

**Threshold**: no exceptions; checked by tests that compare answers which must agree.

**Rationale**: this is why the product answers questions itself instead of handing out SQL.

#### Predictable cost

- [x] `p1` - **ID**: `cpt-insightspec-qe-nfr-bounded`

A question **MUST** stay within stated limits, and be rejected rather than run when it would
exceed them.

**Threshold**: time window at most 731 days; at most 10 000 rows; at most 16 MiB per answer;
8 questions at once per server, with the rest asked to retry shortly.

**Rationale**: the warehouse is shared with everything else the product does.

#### Fast enough to explore

- [ ] `p2` - **ID**: `cpt-insightspec-qe-nfr-latency`

An ordinary question **SHOULD** come back within two seconds for 95 out of 100 asks on a
reference installation.

**Threshold**: 2 seconds at the 95th percentile, up to four breakdowns, one year of data.

**Rationale**: building a chart is a back-and-forth; waiting breaks it.

#### A promise that does not shift

- [x] `p1` - **ID**: `cpt-insightspec-qe-nfr-contract`

What can be asked and what comes back **MUST** be published in a form other software can
check against, and anything unrecognised in a question **MUST** be rejected rather than
ignored.

**Threshold**: the published description accepts exactly what the product accepts; any
divergence stops the build.

**Rationale**: a page built against the description must not disagree with the product.

#### People's data stays handled as people's data

- [x] `p1` - **ID**: `cpt-insightspec-qe-nfr-person-data`

Columns that identify a person **MUST** be shown only to those the access rules allow, and
**MUST NOT** be copied or stored anywhere by this part of the product. Keeping and deleting
that data stays where it already happens.

**Threshold**: nothing stored here; person columns marked as such where data sets are
described.

**Rationale**: no second copy of people's activity with a life of its own.

### 6.2 NFR Exclusions

- **Working offline**: it reads live data; there is nothing to work from offline.
- **Storing anything reliably**: it stores nothing until saved questions arrive.
- **Signing in**: handled before a question reaches it.
- **Uptime and recovery targets**: inherited from the service it lives in; it holds no data
  of its own to recover.
- **Accessibility, translation, phones**: the page is administrator-only today and follows
  the product's usual standards; revisited when leads get access.
- **Certifications**: none beyond the product's own.
- **Monitoring**: speed and failures are recorded with the rest of the service; nothing
  special is needed here.

## 7. Public Library Interfaces

### 7.1 Public API Surface

#### The question and answer format

- [x] `p1` - **ID**: `cpt-insightspec-qe-interface-query`

**Type**: the published request and answer format of the analytics API

**Stability**: still changing while administrator-only; settles when access rules arrive

**Description**: the one way to ask a data set a question, and to find out what can be asked.

**Breaking Change Policy**: anything already accepted keeps its meaning; new options are
added, never substituted.

### 7.2 External Integration Contracts

#### Who is asking

- [ ] `p2` - **ID**: `cpt-insightspec-qe-contract-caller-context`

**Direction**: required from the identity service

**Protocol/Format**: the asker's permissions, the people they may see, the teams they belong
to, resolved for each question

**Compatibility**: if a fact an access rule needs is missing, the question is refused — never
answered more widely.

## 8. Use Cases

#### Build a chart nobody planned for

- [x] `p1` - **ID**: `cpt-insightspec-qe-usecase-build-chart`

**Actor**: `cpt-insightspec-qe-actor-admin`

**Preconditions**:
- The person is an administrator; data sets are available.

**Main Flow**:
1. The page lists the data sets and, for the chosen one, what can be broken down, filtered
   and counted.
2. The administrator picks dates, breakdowns, what to count, any filters, and asks.
3. The product either points at what is wrong, or returns a table with readable names and a
   note if it was cut short.
4. The page draws the table and a chart from that answer alone.

**Postconditions**:
- A chart exists that nobody built into the product.

**Alternative Flows**:
- **Something is wrong**: each problem is shown next to the box it belongs to; nothing is
  answered.
- **Too many rows**: the answer is cut and says so.

#### Decide for yourself what counts as test code

- [x] `p1` - **ID**: `cpt-insightspec-qe-usecase-own-category`

**Actor**: `cpt-insightspec-qe-actor-author`

**Preconditions**:
- File changes can be filtered by path.

**Main Flow**:
1. The author adds a second column counting only the changes whose path matches their own
   pattern.
2. The product checks the pattern makes sense and counts only those rows.

**Postconditions**:
- One table shows the matching lines beside the total, per file type, using the author's own
  definition.

**Alternative Flows**:
- **The pattern is not valid**: the answer is refused, pointing at the pattern.

#### A lead sees only their own team

- [ ] `p2` - **ID**: `cpt-insightspec-qe-usecase-lead-visibility`

**Actor**: `cpt-insightspec-qe-actor-lead`

**Preconditions**:
- The data set has an access rule tying it to the people the asker may see.

**Main Flow**:
1. The lead opens a saved chart broken down by repository.
2. The product applies the rule before counting anything, so only their people are included.

**Postconditions**:
- The same saved chart is correct for two different leads, and different for each.

**Alternative Flows**:
- **The product cannot tell who the lead may see**: the question is refused, not widened.

## 9. Acceptance Criteria

- [x] An administrator answers a question about each available data set from the page, with
  nothing built in advance.
- [x] Answers that must agree do agree on a test installation: the weeks add up to the whole
  period; filtering to one value matches what the breakdown showed for it; a pattern matches
  what the exact values match.
- [x] Someone who is not an administrator is refused, both the questions and the list.
- [x] Every refusal names the part of the question that is wrong.
- [ ] A saved question is refused after the data behind it changes shape, instead of
  answering something different.
- [ ] A lead's answer includes only people they may see, however the chart is broken down.

## 10. Dependencies

| Dependency | Description | Criticality |
|------------|-------------|-------------|
| Prepared data sets | the ready-to-query tables, built with the counting rules already applied | p1 |
| Record of what the warehouse holds | checked when the product is built, so a data set cannot promise a column that is not there | p1 |
| Identity service | says who is an administrator today; who may see whom, later | p1 |
| Published API description | what the page and the automated tests are built from | p1 |

## 11. Assumptions

- The product ships the first data sets; people add their own later, and both kinds are
  checked and treated the same way.
- Administrators may see everything in their own organisation; the restriction is for
  everyone else.
- The test installation contains data for every data set, so the checks mean something.
- Cost depends on the dates and row limit rather than on how large the customer is; those
  limits are the lever if an installation outgrows them.

**Open questions**

| Question | Owner | Resolve by |
|---|---|---|
| Where do we record which teams own which repositories? | product + backend | before the first access rule |
| What protects small groups from being singled out once leads can ask? | product | before the page opens to leads |
| Which data sets come after commits and file changes, in what order? | product | after the constructor page |
| Who may add a data set, and what does approval look like when an assistant wrote it? | product | when own data sets start |
| Is "show me the records behind this number" part of the same request or its own? | backend | when that work starts |

## 12. Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| What people want to ask outpaces what we support | they go back to asking engineers, or to SQL | the list in DESIGN is the backlog, in priority order; the SQL console stays for administrators |
| Access rules take longer than expected | charts stay administrator-only | access rules are the first thing after the basics; the restriction is deliberate, not a default |
| A very broad question is slow on a large customer | exploring becomes painful | limits on dates, rows, size and how many run at once; the "cut short" note tells the asker to narrow it |
| The data behind a saved question changes shape | wrong answers or errors | data sets are checked when the product is built; saved questions are re-checked each time they are opened |
| Someone's own data set is wrong, slow or too revealing | bad numbers spread, or data leaks past its audience | the same checks and limits as a shipped data set; usable by its author alone until an administrator approves it and its access rules are set |
