# PRD — Identity Resolution Service

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
  - [5.1 Caller identity and authorization](#51-caller-identity-and-authorization)
  - [5.2 Profile resolution](#52-profile-resolution)
  - [5.3 The people roster](#53-the-people-roster)
  - [5.4 Visibility](#54-visibility)
  - [5.5 Organisation chart](#55-organisation-chart)
  - [5.6 Roles and assignments](#56-roles-and-assignments)
  - [5.7 Login resolution](#57-login-resolution)
  - [5.8 Operator corrections](#58-operator-corrections)
  - [5.9 Scheduled projection and publication](#59-scheduled-projection-and-publication)
  - [5.10 Schema lifecycle](#510-schema-lifecycle)
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

The identity-resolution service is the product's authority on **who a person
is**. It answers three different questions for three different callers, over
one append-only journal of identity observations:

- **Who is this?** — the product resolves a profile (attributes, org tree,
  every source-native account the person holds) for the front end and for
  analytics enrichment.
- **Whose data may this caller see?** — every product surface that shows one
  person's numbers to another person asks this service for the caller's
  visible set, so the roster, the picker, the org chart and the metrics
  runtime cannot disagree about it.
- **Who just signed in?** — at login the authenticator has an identity
  provider principal and needs the person it belongs to, before any tenant or
  caller context exists.

Alongside those reads it owns the writes that keep the answers correct: the
operator correction surface over account-to-person bindings, the role and
visibility grant tables the product's authorization reads, and the scheduled
jobs that rebuild the person projection from connector evidence and publish it
to the analytics warehouse.

### 1.2 Background / Problem Statement

Identity-bearing data reaches Insight from every connector — an HR directory,
a chat platform, a code host, a task tracker, an AI tool. Each names the same
human differently: an employee id here, a login there, an address in a third
place. Every cross-source metric the product computes depends on those names
collapsing onto one person, and every access decision depends on that person
being the one the caller is entitled to see.

An append-only observation journal (`persons`) unifies the sources behind one
schema, but a journal alone answers nothing synchronously: it has no notion of
a current value, no org tree, no permission model, and no way for a login to
find its person. This service is the synchronous layer over that journal. It
must see every source the pipeline writes, reflect a correction without a
restart, refuse to cross a tenant boundary by construction, and hold the one
place where "may this caller see this person" is decided.

The service also carries the consequence of being asked those questions at the
wrong moment. A first install has an empty journal; a login can arrive for
someone no connector has yet described; two sources can claim the same
address. None of those may crash, and none may be resolved by guessing.

### 1.3 Goals (Business Outcomes)

- **One person across sources.** A human observed by any supported connector
  resolves to a single person, so cross-source metrics attribute to them
  rather than fragmenting.
- **One visibility answer.** Every surface that shows a person's data derives
  its permission from the same rule in this service, so a name and the numbers
  behind it cannot answer to different permissions.
- **Corrections take effect immediately.** An operator's binding, merge,
  detach or exclusion changes what the product answers without a redeploy or a
  restart, and remains attributable afterwards.
- **A person can always sign in once the organisation lists them.** Login
  resolution succeeds for anyone the configured roster states, including
  members the directory publishes without an address.
- **Nothing crosses a tenant.** No configuration mistake and no data state
  produces an answer assembled from another tenant's rows.

### 1.4 Glossary

| Term | Definition |
|------|------------|
| Observation | One `(value type, value)` datapoint emitted by one source for one person at one instant. Never updated; superseded by a later observation on the same partition. |
| Journal | The append-only store of observations. Every current value the service reports is derived from it, never stored as a mutable field. |
| Current value | The latest observation per person, per source instance, per value type. "Latest per source" is the projection; the assembler then picks a winner across sources. |
| Binding | An observation that ties a source-native account to a person. Written by the seed, by the login bootstrap, or by an operator correction. |
| Account | A source-native identity: a connector type, a connector instance, and an id within it. The unit an operator binds, detaches or excludes. |
| Roster | The one source configured as the authority on who exists. Only it may cause a person to be minted for an account carrying no address. |
| Visible set | The set of persons a caller may see, derived per request from the caller, their grants and the org chart, under the configured visibility policy. |
| Visibility grant | An explicit, time-bounded record that one person may see another (or the whole tenant), independent of the reporting line. |
| Role assignment | A time-bounded grant of a named role to a person in a tenant. The `admin` role gates the operator surfaces. |
| Org chart | The materialised parent-to-child edge cache with validity intervals, rebuilt from the journal, filtered to one configured source. |
| Operation journal | The record of one batch job run or one operator correction: who ran it, what was asked, what changed, and why it failed. |
| Service principal | A caller whose token identifies another service rather than a person. The only caller admitted to the internal login-bootstrap routes. |

## 2. Actors

### 2.1 Human Actors

#### Identity operator

**ID**: `cpt-insightspec-actor-identity-operator`

**Role**: Holds the `admin` role in a tenant. Reviews accounts the automatic
pipeline could not decide, binds them to people, merges duplicates, detaches
wrong bindings, excludes accounts that are not people, and manages role and
visibility grants.

**Needs**: To see every person and account in the tenant regardless of the
reporting line; to have a correction take effect at once and survive the next
automatic run; to be refused rather than guessed for when the evidence is
contested; to read back what each of their decisions did.

#### Product user

**ID**: `cpt-insightspec-actor-product-user`

**Role**: A signed-in person using Insight. Never calls this service directly —
every read is on their behalf from the front end or the analytics service —
but the answer is scoped to them, so they are the subject of every visibility
decision.

**Needs**: To see themselves, the people they are entitled to see, and nobody
else; to reach a colleague's profile from any surface with the same result;
to be told what they may do without probing for a refusal.

#### Platform operator

**ID**: `cpt-insightspec-actor-platform-sre`

**Role**: Installs and runs Insight on a cluster. Configures the roster source,
the visibility policy and the org-chart source, seeds the first admin, and
diagnoses a stand where the projection is stale, a login is refused, or a job
failed.

**Needs**: A schema that migrates itself before the service serves; a job that
refuses a destructive run rather than completing it; a journal that explains a
failure without access to the logs; logs that name a failure without carrying
a person's address.

### 2.2 System Actors

#### Gateway

**ID**: `cpt-insightspec-actor-api-gateway`

**Role**: The product's edge. Terminates the browser session, mints the signed
token every caller of this service presents, and forwards front-end requests
under the identity mount. The caller identity, their tenant and their roles
reach this service only as claims of that token.

#### Authenticator

**ID**: `cpt-insightspec-actor-authenticator`

**Role**: Drives the login. Presents an identity-provider principal to this
service to find the person it belongs to, provisions one when the roster lists
someone the journal has no binding for, reads the person's active roles to mint
into the session token, and resolves an address for the administrative view-as
feature.

#### Analytics service

**ID**: `cpt-insightspec-actor-analytics`

**Role**: Enriches metric responses with person attributes and forwards a
cleared request's person set here to learn which of them the caller may see.
Its access decisions are this service's answers.

#### Connector pipeline

**ID**: `cpt-insightspec-actor-seed-pipeline`

**Role**: Produces the identity evidence the projection is built from. Each
connector emits identity observations into the warehouse; the service's own
scheduled job reads them and folds them into the journal. The pipeline does not
write the journal directly.

#### Metrics warehouse

**ID**: `cpt-insightspec-actor-metrics-warehouse`

**Role**: Consumes a published snapshot of the journal so warehouse transforms
can attribute activity to a person without calling the service per row. The
service is the sole writer of that snapshot.

#### Identity database

**ID**: `cpt-insightspec-actor-mariadb`

**Role**: Stores the journal, the org chart, the person projection, the role
and visibility grants, and the operation journal. The service owns this schema
and migrates it.

#### Scheduler

**ID**: `cpt-insightspec-actor-scheduler`

**Role**: Runs the projection rebuild on a schedule and on demand. Runs the job
as a process, not as an API call — there is no authenticated trigger for it.

## 3. Operational Concept & Environment

### 3.1 Module-Specific Environment Constraints

- **The schema is migrated before the service serves.** Migration runs as a
  separate process step against the same database; a failed migration must
  prevent the service from serving rather than surface as a runtime error.
- **No in-process cache of the journal.** Every answer is derived per request,
  so a correction or a completed job is visible immediately and there is no
  cache to invalidate. The resource budget assumes this.
- **A signed caller token is mandatory.** There is no unauthenticated mode and
  no configured fallback identity for a request. A request whose token carries
  no tenant is refused rather than served against a default.
- **Batch work runs outside the request path.** The projection rebuild and the
  snapshot publish are processes with their own lifetimes, exit codes and
  concurrency control. They must be safe to run concurrently with serving and
  with each other.
- **The organisation shape is configuration, not data.** Which source supplies
  the reporting line, which source is the roster, and whether visibility
  follows the reporting line at all are per-install settings. An install with
  no reporting lines is a supported deployment, not a degraded one.

## 4. Scope

### 4.1 In Scope

- Resolving a person from an address, a source-native account id, or the
  canonical person key, and assembling their current attributes, org tree and
  account list.
- Deciding, for one caller, which persons they may see — as a batch filter, as
  a paged roster, and as the gate on every other read.
- Serving the canonical people roster and the org chart, including a
  point-in-time view of the reporting line.
- The role catalogue, role assignments and visibility grants that the product's
  authorization is read from, and the caller's own view of them.
- The operator correction surface over account-to-person bindings, and the
  review queue of accounts awaiting a decision.
- Login-time resolution and provisioning for the authenticator, over routes
  reachable only by a service principal.
- The scheduled projection rebuild, the snapshot publish, their concurrency
  control and input guards, and the journal that records every run.
- The schema this service owns, its migrations and the first-admin bootstrap.

### 4.2 Out of Scope

- **How identity evidence is produced.** Connector extraction and the warehouse
  models that emit identity observations belong to the ingestion pipeline.
- **The matching semantics themselves.** What counts as evidence, how a
  correction folds into the journal and how conflicts are classified are
  specified by the identity-resolution domain artifacts; this document
  specifies the service's obligations when exposing them —
  [domain PRD](../../../../../domain/identity-resolution/specs/PRD.md).
- **Being an identity provider.** The service never authenticates anyone. It
  maps an already-authenticated principal onto a person.
- **Session handling.** Sessions, tokens and their lifetimes belong to the
  authenticator and the gateway.
- **Automatic fuzzy matching.** Nothing in this service links two identities on
  a similarity judgement; an undecidable case reaches an operator.
- **Erasure of a person on request.** The journal is append-only; a data-subject
  erasure path is a separate, unbuilt capability.

## 5. Functional Requirements

> **Testing strategy**: requirements are verified by automated tests in the
> service's own suite — unit tests over the pure decision logic, and live tests
> that drive the real route table against a real database. Where the observable
> behaviour is the wire contract, the generated interface document is the
> checked artifact and a drift gate enforces it.

### 5.1 Caller identity and authorization

#### Every request answers to the caller's token

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-caller-identified`

**Vector**: Security

The system **MUST** derive the caller, their tenant and their type from the
verified token presented with the request, and **MUST** refuse any request
whose token identifies no person or no tenant. No request parameter, header or
configuration setting may name the caller instead.

**Rationale**: The caller identity is the input to every visibility decision
this service makes. Accepting it from anywhere the caller controls would make
every other access rule decorative.

**Actors**: `cpt-insightspec-actor-api-gateway`, `cpt-insightspec-actor-product-user`

#### An unresolved tenant is refused, never defaulted

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-lookup-400-tenant`

**Vector**: Security

The system **MUST** refuse a request whose token carries no resolvable tenant,
and **MUST NOT** fall back to a configured tenant to serve it. The refusal
**MUST** name the missing tenant as the cause so a misconfigured install is
diagnosable.

**Rationale**: Defaulting a tenant on a multi-tenant install serves one
customer's data to another. A configured tenant exists for the batch jobs and
the first-admin bootstrap, which have no caller; it is not a request-time
fallback.

**Actors**: `cpt-insightspec-actor-platform-sre`

#### Operator surfaces require an active admin role

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-admin-gate`

**Vector**: Security

Every operator surface — the correction verbs, the account and person searches
behind them, the role, role-assignment and visibility grant management, the
tenant-wide roster, and the job journals — **MUST** require the caller to hold
an active `admin` assignment in their own tenant. The check **MUST** read the
current assignment state per request, so a grant or revocation takes effect
without a restart, and **MUST** distinguish "not identified" from "identified
but not permitted".

**Rationale**: These surfaces read and rewrite who everyone is. Gating them on
a live grant rather than on a token claim alone means a revocation is
immediate, and separating the two refusals lets the front end tell a signed-out
user from an unprivileged one.

**Actors**: `cpt-insightspec-actor-identity-operator`

#### Internal routes admit only service principals

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-service-principal-gate`

**Vector**: Security

The login-bootstrap routes **MUST** be reachable only by a caller whose token
identifies a service rather than a person, **MUST NOT** appear in the published
interface document, and **MUST** refuse any other caller. Because they run
before a caller identity exists, they are the only routes exempt from the
visibility gate, and that exemption **MUST NOT** be reachable from any
person-authenticated route.

**Rationale**: These routes answer without the protections every other route
applies. Confining them to a service principal, and keeping them off the public
contract, is what stops that latitude from becoming a way for a signed-in
person to read the whole directory.

**Actors**: `cpt-insightspec-actor-authenticator`

#### The caller can read their own identity and permissions

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-me`

The system **MUST** let any identified caller read who the token says they are,
which roles they actively hold, and which visibility policy the install runs.
An empty role list **MUST** be a successful answer rather than a refusal.

**Rationale**: A consumer that must probe for a refusal to learn what it may
show produces a worse experience and a noisier audit trail than one that asks
once. The policy is reported for the same reason — an empty org tree under a
flat install is not a missing reporting line.

**Actors**: `cpt-insightspec-actor-product-user`

### 5.2 Profile resolution

#### Resolve a profile by address, account id or person key

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-profile-resolve`

**Vector**: Versatility

The system **MUST** resolve a single profile from any of three keys: an address
matched across every source in the tenant; a source-native account id scoped to
one named connector instance; or the canonical person key itself. All three
**MUST** produce the same profile for the same person, and the person key
**MUST** be usable for a person the journal holds no address for.

**Rationale**: Callers hold different keys. The front end routes on the person
key, an operator workflow starts from a connector account, and an enrichment
path starts from an address. One contract covering all three keeps a single
assembly path, so the answers cannot diverge by caller.

**Actors**: `cpt-insightspec-actor-api-gateway`, `cpt-insightspec-actor-analytics`

#### Address matching uses the current value per source

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-lookup-resolve-by-email`

The system **MUST** match an address against the current observation for each
source instance, so a superseded address stops resolving to its former owner.
Matching **MUST** be case-insensitive without requiring the caller to normalise
the input.

**Rationale**: An address that moved between people must resolve to whoever
holds it now; requiring every caller to normalise the case makes correctness
depend on each caller getting it right.

**Actors**: `cpt-insightspec-actor-api-gateway`

#### Compose attributes from every source

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-lookup-hydrate`

**Vector**: Versatility

The system **MUST** compose the person's attributes from the current
observation of every source that describes them, preferring no source by
default and picking the most recently observed value per attribute.

**Rationale**: One source is authoritative for some attributes and silent on
others. A profile assembled from a single preferred source would be
systematically incomplete for every organisation whose directory does not
carry everything.

**Actors**: `cpt-insightspec-actor-api-gateway`

#### Reject a malformed lookup before querying

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-profile-validation`

The system **MUST** refuse a lookup whose key and scope do not agree — a
source-scoped key without its connector instance, or an unscoped key carrying
one — naming the offending field, before any data is read.

**Rationale**: The cross-field rules are the difference between "this account
in this connector" and "this value anywhere in the tenant". Deciding them in
one place, ahead of the query, keeps a malformed request from being answered
under the wrong scope.

**Actors**: `cpt-insightspec-actor-api-gateway`

#### Filter candidates by visibility before deciding the outcome

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-profile-visibility-gate`

**Vector**: Security

The system **MUST** reduce the matched candidates to those the caller may see
**before** deciding whether the lookup found nothing, found one person, or is
ambiguous. A candidate the caller cannot see **MUST NOT** change the outcome,
appear in a refusal, or make an otherwise unique match read as ambiguous.

**Rationale**: Applying visibility after the decision leaks the existence of
people the caller may not see through the shape of the answer, and lets an
invisible duplicate deny the caller a lookup they are entitled to.

**Actors**: `cpt-insightspec-actor-product-user`

#### Refuse an ambiguous lookup rather than choosing

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-profile-ambiguous`

**Vector**: Reliability

When a lookup matches more than one visible person, the system **MUST** refuse
it as a violated invariant and name the matched persons, rather than returning
one of them.

**Rationale**: The invariant is one current person per address in a tenant and
per account within a connector instance. Silently choosing would attribute one
human's data to another and hide the data defect that caused it; naming the
matches lets an operator repair it without re-querying.

**Actors**: `cpt-insightspec-actor-identity-operator`

#### An unmatched lookup is an answer, not a failure

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-lookup-unmatched`

The system **MUST** distinguish "no such person here" from a service failure,
and **MUST** answer the former identically whether the person does not exist or
the caller may not see them.

**Rationale**: An empty journal is the normal state of a fresh install, so
callers must be able to treat "not found" as routine. Answering the invisible
case the same way is what stops the refusal from disclosing that the person
exists.

**Actors**: `cpt-insightspec-actor-api-gateway`, `cpt-insightspec-actor-product-user`

#### Report every account the person holds

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-profile-ids-list`

**Vector**: Versatility

A resolved profile **MUST** carry every current source-native account binding
the person holds, one per connector instance.

**Rationale**: Consumers that would otherwise make one follow-up lookup per
source get the whole picture in the answer they already asked for.

**Actors**: `cpt-insightspec-actor-api-gateway`

#### Report the supervisor from the org chart

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-lookup-parent`

A resolved profile **MUST** carry the person's supervisor as recorded by the
configured org-chart source, hydrated from that supervisor's own observations.
Superseded supervisor attributes carried on the person's own observations
**MUST NOT** be reported — the org chart is the source of truth for the
reporting line.

**Rationale**: Reading the edge from one materialised, source-scoped cache
makes the supervisor and the subtree consistent by construction, and keeps a
stale attribute on the person's own record from contradicting it.

**Actors**: `cpt-insightspec-actor-api-gateway`

#### Report the reporting subtree

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-lookup-subordinates`

**Vector**: Reliability

A resolved profile **MUST** carry the recursive subtree below the person on the
configured org-chart source. The walk **MUST** terminate on a repeated person,
**MUST** skip a person the journal does not describe rather than emitting an
empty placeholder, and **MUST** stop at a configured depth. Expanding the
subtree **MUST** be switchable off without affecting the rest of the profile.

**Rationale**: A cache rebuilt from imperfect evidence can contain a cycle, and
an unbounded recursion over one is an outage. The switch exists because the
expansion is the most expensive part of the answer and not every install needs
it.

**Actors**: `cpt-insightspec-actor-api-gateway`

#### Derive missing name parts from the display name

- [x] `p2` - **ID**: `cpt-insightspec-fr-identity-routing-name-split`

The system **MUST** derive first and last name from the display name when
neither is separately observed, handling both the "family name first" and
"given name first" conventions.

**Rationale**: Several directories publish only a display name. Deriving the
parts keeps the profile complete without a connector backfill, and keeping the
rule in one place stops each consumer inventing its own.

**Actors**: `cpt-insightspec-actor-api-gateway`

#### Resolve many people in one request

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-profile-batch`

**Vector**: Performance

The system **MUST** resolve a set of person keys to profiles in a single
request, returning only those the caller may see, and **MUST** bound the number
of keys one request may carry.

**Rationale**: A metric response names many people at once; resolving them one
request at a time turns one screen into hundreds of round trips. The bound is
what keeps a single request from becoming the outage the round trips were.

**Actors**: `cpt-insightspec-actor-analytics`

### 5.3 The people roster

#### Serve the canonical roster

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-people-roster`

The system **MUST** serve the current roster of people in the caller's tenant,
each with the name parts, address, account handle and attributes their sources
state, and their supervisor. The roster **MUST** default to what the caller may
see; reading the whole tenant **MUST** require the admin role.

**Rationale**: Two different questions — "who can I work with" and "who exists"
— must not answer through the same unguarded surface, and defaulting to the
narrower one means a caller cannot enumerate the organisation by omitting a
parameter.

**Actors**: `cpt-insightspec-actor-product-user`, `cpt-insightspec-actor-identity-operator`

#### Narrow the roster by search terms

- [x] `p2` - **ID**: `cpt-insightspec-fr-identity-people-search`

**Vector**: Versatility

The system **MUST** narrow a listing by search terms matched against the
person's current observed values, requiring every term to match. A term that is
a person key **MUST** name that person directly, so a person the journal holds
no values for is still reachable.

**Rationale**: Search must agree with resolution — a value that stopped being
current must stop matching its former owner. The person key is the one
identifier an operator can copy from a screen, and the only handle on a person
with no other values.

**Actors**: `cpt-insightspec-actor-identity-operator`

#### Page every listing safely

- [x] `p2` - **ID**: `cpt-insightspec-fr-identity-listing-paging`

**Vector**: Reliability

Every listing **MUST** be paged with a bounded page size and an opaque
continuation token. A token **MUST** be refused when it was issued for a
different listing, a different tenant or a different query, rather than
resuming at a position that query never ordered.

**Rationale**: An unbounded listing is an outage on a large tenant. A token
accepted by the wrong query silently skips or repeats people, which is worse
than a refusal because nothing surfaces it.

**Actors**: `cpt-insightspec-actor-product-user`

### 5.4 Visibility

#### Answer which of these people the caller may see

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-visible-persons-batch`

**Vector**: Security

The system **MUST** answer, for a bounded set of person keys supplied by the
caller, which of them that caller may see. The answer **MUST** be a subset of
what was asked, so it cannot be used to discover who exists.

**Rationale**: The metrics runtime holds the people a request would report on
and needs one authoritative filter rather than its own copy of the rule.
Echoing only the input is what keeps that filter from doubling as a directory.

**Actors**: `cpt-insightspec-actor-analytics`

#### The visible set follows the configured policy

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-visible-persons-policy`

**Vector**: Security

The visible set **MUST** be derived under one configured policy for the whole
install: either the caller, their explicit grants and the org-chart descendants
of both; or every person in the tenant. The same policy **MUST** apply to the
batch filter, the roster, the profile gate and the org-chart reads, and roles
**MUST NOT** confer visibility. Changing the policy **MUST NOT** write or
destroy any grant.

**Rationale**: An organisation whose roster carries no reporting lines would
otherwise see nothing, and issuing everyone a wildcard grant to compensate
would be an irreversible rewrite of the permission data. Sharing one derivation
across every consumer is what makes them agree; keeping roles out of it means
an operator role does not silently widen what its holder can read.

**Actors**: `cpt-insightspec-actor-platform-sre`, `cpt-insightspec-actor-product-user`

#### Enumerate the caller's visible people

- [x] `p2` - **ID**: `cpt-insightspec-fr-identity-visible-persons-roster`

The system **MUST** let a caller enumerate the people in their own visible set,
a page at a time, using the same label rule and ordering as the operator
listing.

**Rationale**: A picker that ordered or labelled people differently from the
roster beside it reads as two different datasets. Deriving it by restriction
from the same listing is what keeps them one.

**Actors**: `cpt-insightspec-actor-product-user`

#### Manage visibility grants

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-visibility-grants`

**Vector**: Security

The system **MUST** let an operator grant one person visibility of another, or
of the whole tenant, and revoke it. Grants **MUST** be time-bounded and retain
their history, and **MUST** record who made the change and why.

**Rationale**: Visibility beyond the reporting line is an exception, and an
exception without an author, a time and a reason cannot be audited or safely
removed later.

**Actors**: `cpt-insightspec-actor-identity-operator`

### 5.5 Organisation chart

#### Serve a subtree the caller may see

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-subchart-read`

**Vector**: Security

The system **MUST** serve the reporting subtree rooted at a named person, and
**MUST** answer a root the caller may not see identically to a root that does
not exist. Descendants **MUST NOT** be filtered individually.

**Rationale**: A distinct refusal for "exists but hidden" discloses the person.
Per-node filtering is unnecessary because the visible set is closed under
descent — once the root is visible, so is everything below it — and adding it
would only invite the two rules to drift apart.

**Actors**: `cpt-insightspec-actor-product-user`

#### Serve every root the caller may see

- [x] `p2` - **ID**: `cpt-insightspec-fr-identity-subchart-forest`

The system **MUST** serve the set of top-level people in the caller's visible
set as a forest, and **MUST** answer an empty forest rather than a refusal when
the caller sees no one.

**Rationale**: A caller who has not been placed in the org chart is a normal
state on a partially seeded install; answering it as an error would make an
ordinary screen look broken.

**Actors**: `cpt-insightspec-actor-product-user`

#### Read the reporting line as it stood

- [x] `p2` - **ID**: `cpt-insightspec-fr-identity-subchart-point-in-time`

**Vector**: Versatility

The system **MUST** support reading the org chart as of a past instant, and
**MUST** refuse an instant in the future.

**Rationale**: A metric over a past period is only interpretable against the
organisation as it was then. A future instant is always a caller mistake and
answering it would silently return the present.

**Actors**: `cpt-insightspec-actor-analytics`

#### Every traversal is bounded

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-subchart-bounded`

**Vector**: Reliability

Every org-chart traversal **MUST** be depth-bounded by a server-side limit that
the caller can lower but not raise, and that applies when the caller specifies
no depth.

**Rationale**: The depth limit is the only thing standing between a caller and
a whole large tenant in one response, and between cyclic cache data and a
database recursion error. A caller-supplied bound alone secures neither.

**Actors**: `cpt-insightspec-actor-platform-sre`

#### Maintain the parent-and-child edge cache

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-org-chart-table`

The system **MUST** maintain a per-source cache of direct reporting edges with
validity intervals, holding at most one current edge per person per source
instance, and retaining superseded edges as history.

**Rationale**: Reporting lines are per-source facts — a manager in the HR
directory is not the same relation as an administrator in a chat platform — and
a materialised cache turns every tree read into an index walk instead of a
recomputation over the journal.

**Actors**: `cpt-insightspec-actor-mariadb`

#### Rebuild edges deterministically from the journal

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-org-chart-rebuild`

**Vector**: Reliability

The rebuild **MUST** derive edges from the journal so that the same journal
always produces the same cache. It **MUST** prefer a resolved supervisor
reference over one that has to be matched by address; **MUST** skip an edge
whose supervisor matches nobody rather than inventing a placeholder person;
**MUST** skip self-references; **MUST** close an edge while the person is
inactive and open a new one when they return, rather than reopening the closed
one; **MUST** treat a person with no activity status as always active; and
**MUST** report, without failing, any mutual pair of edges it produced.

**Rationale**: Determinism is what makes the cache safe to rebuild rather than
patch. Inventing a placeholder for an unmatched supervisor would fabricate
people; reopening a closed edge would erase the fact that someone left and
returned; and a mutual pair is a data defect an operator must see, not a reason
to abandon a whole run.

**Actors**: `cpt-insightspec-actor-seed-pipeline`, `cpt-insightspec-actor-mariadb`

#### Read current edges by person

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-org-chart-read`

The system **MUST** read the current supervisor of a person and the current
direct reports of a person, tenant-scoped, preserving which source instance
each edge came from.

**Rationale**: The profile projection and the tree traversals share these two
reads, so one abstraction keeps them consistent, and preserving the source
means a multi-source install can tell the two organisational shapes apart.

**Actors**: `cpt-insightspec-actor-api-gateway`, `cpt-insightspec-actor-mariadb`

### 5.6 Roles and assignments

#### Maintain the role catalogue

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-roles-catalogue`

The system **MUST** maintain the catalogue of named roles, tenant-independent,
and **MUST** provide the roles the product's own authorization depends on
without an operator having to create them.

**Rationale**: The role that gates this service's operator surfaces cannot
itself be something an operator must create first — a fresh install would have
no way to reach the surface that creates it.

**Actors**: `cpt-insightspec-actor-identity-operator`

#### Refuse to delete a role in use

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-roles-in-use-guard`

**Vector**: Reliability

Deleting a role **MUST** be refused while any active assignment of it exists,
and the check and the deletion **MUST NOT** be separable by a concurrent grant.

**Rationale**: A deleted role with live assignments leaves permission rows
pointing at nothing, which reads as either "no permission" or "unknown
permission" depending on the consumer. Checking and deleting atomically is what
stops a grant landing in the gap.

**Actors**: `cpt-insightspec-actor-identity-operator`

#### Grant and revoke roles with history

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-person-roles-grant`

**Vector**: Security

The system **MUST** let an operator grant a role to a person in a tenant and
revoke it, keeping every assignment time-bounded with its author and reason,
and **MUST** keep revoked assignments as history rather than deleting them.

**Rationale**: Permission changes are the events an audit asks about first.
Deleting them on revocation destroys exactly the record that matters.

**Actors**: `cpt-insightspec-actor-identity-operator`

#### Refuse to revoke the last admin

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-person-roles-last-admin`

**Vector**: Reliability

The system **MUST** refuse to revoke the last active admin assignment in a
tenant.

**Rationale**: There is no in-product path back from a tenant with no admin —
the surface that grants the role is itself admin-gated — so recovery requires
direct database access.

**Actors**: `cpt-insightspec-actor-identity-operator`

#### Report a person's active roles to the authenticator

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-active-roles-for-token`

**Vector**: Security

The system **MUST** report a named person's active role names in a tenant to a
service principal, so the session token can carry them, and **MUST** treat an
empty list as a valid answer.

**Rationale**: The session token is minted once per login and refresh; without
this read it would either carry no roles or the authenticator would need its
own copy of the assignment rules.

**Actors**: `cpt-insightspec-actor-authenticator`

### 5.7 Login resolution

#### Resolve a login by directory principal

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-login-by-external-id`

**Vector**: Security

The system **MUST** resolve an identity-provider principal — its source type
and its native user id — to a person for the authenticator, and **MUST NOT**
fall back to matching an address when no binding exists.

**Rationale**: The directory id is the identifier the provider guarantees;
an address is not, and it can be reassigned. A fallback would mean a login for
an unbound principal silently entering as whoever last held that address.

**Actors**: `cpt-insightspec-actor-authenticator`

#### Resolve a login by roster address where configured

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-login-by-roster-email`

**Vector**: Security

Where the install's provider has no directory connector, the system **MUST**
resolve a login by address, confined to the configured roster source, scoped to
the caller's tenant, and matched only against a person still holding a live
account under that source. It **MUST** refuse outright when no roster is
configured, when no tenant is known, or when no such person exists, and
**MUST** record when the address was claimed by more than one person.

**Rationale**: An address does not carry the cross-tenant uniqueness a
directory id does, so this path spends the tenant already known rather than
searching every install. Confining it to one source and to live accounts is
what keeps a login from matching a stale record; refusing rather than widening
is what keeps a misconfiguration from admitting everyone.

**Actors**: `cpt-insightspec-actor-authenticator`

#### Provision a person the roster already lists

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-login-provision`

**Vector**: Security

When an authenticated principal has no binding, the system **MUST** mint a
person for them **only** if a connector has already observed that account, and
**MUST** bind it under that observation's own connector instance. It **MUST**
refuse for an account the source reports as closed, for an account carrying an
address that the automatic resolution would link anyway, and for an asserted
tenant that is not the one the journal is keyed by. Two concurrent logins
**MUST NOT** produce two people, and an account an operator has already
excluded **MUST NOT** be revived.

**Rationale**: Without this, a directory member published without an address is
refused at login until an operator binds them by hand. Minting only for an
already-observed account is what keeps the roster the authority on who exists,
rather than letting anyone who reaches the provider become a person. Reusing
the observed connector instance is what lets the next scheduled run adopt the
person instead of minting a second.

**Actors**: `cpt-insightspec-actor-authenticator`, `cpt-insightspec-actor-identity-operator`

#### Resolve an address for administrative view-as

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-login-override`

**Vector**: Security

The system **MUST** provide the authenticator a resolution by address across
any source and any tenant, reachable only by a service principal, and used only
for the administrative view-as feature. It **MUST** be a distinct route from
every login-resolution path.

**Rationale**: This lookup has deliberately more latitude than any login may
have. Keeping it on its own route — rather than as a mode of a shared one — is
what makes it impossible for an absent field on a login to reach it.

**Actors**: `cpt-insightspec-actor-authenticator`

### 5.8 Operator corrections

#### Expose the correction verbs

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-corrections-surface`

The system **MUST** expose the operator corrections defined by the identity
domain — binding an account to a person, merging people, detaching an account,
and excluding an account that is not a person — including a bulk form of
binding with a bounded item count and a per-item outcome. Each **MUST** append
to the journal under the calling operator and **MUST NOT** update or delete
what is already recorded.

**Rationale**: The semantics belong to the domain; the service's obligation is
that they are reachable, attributable and non-destructive. Per-item outcomes on
a bulk call are what let an operator import a prepared table without a single
bad row failing the rest.

**Actors**: `cpt-insightspec-actor-identity-operator`

#### Journal every correction

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-corrections-journal`

**Vector**: Security

Every correction **MUST** be recorded with its author, its request, its
outcome, and its time, and the trail for one account or one person **MUST** be
readable back.

**Rationale**: A correction changes who data is attributed to. Being able to
reconstruct who decided what, and when, is the difference between an auditable
system and one that must be trusted.

**Actors**: `cpt-insightspec-actor-identity-operator`

#### Surface what needs a decision

- [x] `p2` - **ID**: `cpt-insightspec-fr-identity-review-queue`

The system **MUST** surface the accounts awaiting an operator decision, with
the evidence behind each, and **MUST** distinguish an account minted from the
roster from one whose evidence is contested.

**Rationale**: An operator cannot review what they cannot enumerate, and the
two cases need different judgements — one is a confirmation, the other is a
conflict.

**Actors**: `cpt-insightspec-actor-identity-operator`

### 5.9 Scheduled projection and publication

#### Rebuild the projection on a schedule

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-seed-run`

**Vector**: Reliability

The system **MUST** rebuild the person projection — bindings, the people
roster and the org chart — from connector evidence, on a schedule and on
demand, as a process rather than an authenticated request. Its completion
**MUST** include publishing the refreshed journal to the metrics warehouse.

**Rationale**: Without a scheduled rebuild the organisation freezes at the last
manual run while the underlying connectors keep changing. Keeping it off the
API means no caller can trigger a full rebuild, and folding the publish into
completion means the warehouse cannot be left behind by a successful run.

**Actors**: `cpt-insightspec-actor-scheduler`, `cpt-insightspec-actor-seed-pipeline`

#### Refuse a destructive run

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-seed-guards`

**Vector**: Reliability

The rebuild **MUST** refuse to run when its input is empty, and when the
journal already holds people under a tenant other than the one it would write.
The publish **MUST** refuse to replace a populated snapshot with an empty one.
Each refusal **MUST** be overridable explicitly and **MUST** be recorded as a
failed run with its reason.

**Rationale**: An empty read almost always means a misconfigured or wiped
dependency, not that nobody exists — and acting on it either erases the
warehouse mirror or mints a parallel set of people under the wrong tenant,
neither of which an append-only journal can undo. An explicit override exists
because a deliberate wipe is a real operation.

**Actors**: `cpt-insightspec-actor-platform-sre`

#### Serialize runs and reclaim abandoned ones

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-seed-serialization`

**Vector**: Reliability

Concurrent rebuilds **MUST** be serialized, including across separate
deployments sharing one database; a run that cannot acquire its turn **MUST**
report that distinctly rather than queueing behind it; and a run abandoned
mid-flight **MUST** be reclaimable by the next one rather than blocking it.

**Rationale**: Connectors finishing together trigger their rebuild steps at the
same time, and two rebuilds over the same journal race. A distinct outcome for
"busy" lets the scheduler treat it as normal; reclaiming abandoned runs is what
keeps one crashed process from stopping every later one.

**Actors**: `cpt-insightspec-actor-scheduler`

#### Publish the journal to the warehouse

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-publish-persons-snapshot`

**Vector**: Reliability

The system **MUST** publish the journal to the metrics warehouse as a whole
snapshot replaced atomically, so a reader never sees a partial state, and
**MUST** provide a way to republish it when it has fallen behind.

**Rationale**: The warehouse resolves activity to a person for every metric
built; a partially visible snapshot would attribute a slice of the
organisation's work to nobody. The manual republish is the repair path when a
publish was missed.

**Actors**: `cpt-insightspec-actor-metrics-warehouse`

#### Read what each run did

- [x] `p2` - **ID**: `cpt-insightspec-fr-identity-operations-journal-read`

The system **MUST** let an operator read the history of rebuild and publish
runs — status, what was asked, what changed, and the failure reason — without
access to the process logs.

**Rationale**: These jobs run unattended, and the person diagnosing a stale
organisation is usually not the person with log access to the cluster.

**Actors**: `cpt-insightspec-actor-platform-sre`, `cpt-insightspec-actor-identity-operator`

### 5.10 Schema lifecycle

#### Own and migrate the schema before serving

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-migrations-startup`

**Vector**: Reliability

The service **MUST** own its database schema and apply its own migrations
before it serves traffic; a failed migration **MUST** prevent serving. Each
migration **MUST** be recorded so a repeated run is a no-op, and **MUST** be
individually re-runnable without error.

**Rationale**: Serving against an unmigrated schema turns a deployment mistake
into wrong answers rather than a failed rollout. Recording and idempotence
together mean a restart, a retry and a crash mid-migration all converge on the
same state.

**Actors**: `cpt-insightspec-actor-mariadb`, `cpt-insightspec-actor-platform-sre`

#### Record every state transition

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-schema-relax-uniqueness`

**Vector**: Reliability

The journal **MUST** record a value returning to a previous value at a later
time as a separate event, while a re-run of the rebuild over the same evidence
**MUST NOT** duplicate rows.

**Rationale**: Deduplicating on the value alone conflates "the same fact
re-observed on a re-run", which should collapse, with "this became true again",
which is a distinct event — and losing the latter erases every departure and
return from the history.

**Actors**: `cpt-insightspec-actor-mariadb`, `cpt-insightspec-actor-seed-pipeline`

#### Compare identifier values case-insensitively

- [x] `p1` - **ID**: `cpt-insightspec-fr-identity-schema-case-insensitive-value-id`

**Vector**: Reliability

Identifier-shaped values **MUST** compare case-insensitively at the storage
layer, so no caller has to normalise a value to get the right answer.

**Rationale**: A case-sensitive comparison makes a lookup for an address fail
against the same address stored with different capitalisation. Fixing it once
in storage removes a contract every caller would otherwise have to honour
individually.

**Actors**: `cpt-insightspec-actor-api-gateway`, `cpt-insightspec-actor-mariadb`

#### Seed the first administrator

- [x] `p2` - **ID**: `cpt-insightspec-fr-identity-bootstrap-admin`

The system **MUST** be able to grant the admin role to a configured person in
the configured tenant at migration time, idempotently, and **MUST** skip with a
warning rather than fail when the configuration is incomplete.

**Rationale**: A fresh install has nobody who can reach the admin-gated surface
that grants the admin role. Failing the migration over an optional convenience
would make an unrelated deployment fail.

**Actors**: `cpt-insightspec-actor-platform-sre`

## 6. Non-Functional Requirements

This service sits in front of every screen the product shows and inside every
access decision it makes. Its quality concerns follow from that position:
**Security** dominates, because a wrong answer here discloses one person's data
to another or admits the wrong person at login; **Reliability** follows,
because the projection is rebuilt rather than edited and a bad run is not
undoable; **Performance** matters because a profile read is on the critical
path of every screen; **Efficiency** matters because the service holds no cache
and pays for that at every request; **Versatility** matters because the set of
sources an install runs is not known in advance.

### 6.1 NFR Inclusions

#### No answer crosses a tenant

- [x] `p1` - **ID**: `cpt-insightspec-nfr-identity-tenant-isolation`

**Vector**: Security

Every person-authenticated answer **MUST** be assembled only from the caller's
own tenant, including nested projections such as the supervisor, the subtree
and the account list.

**Threshold**: An invariant, not a rate: zero person-authenticated responses
contain data from another tenant, under any configuration. The two service-only
login lookups that deliberately search across tenants are the whole of the
exception, and each is confined to a single response field.

**Rationale**: Cross-tenant disclosure is the failure that ends a customer
relationship, and it is reachable through the least obvious paths — a nested
supervisor, an ambiguity list — not the obvious ones.

#### The caller cannot enlarge their visible set

- [x] `p1` - **ID**: `cpt-insightspec-nfr-identity-visibility-integrity`

**Vector**: Security

No request parameter, no listing, and no refusal message **MUST** reveal a
person outside the caller's visible set, and no role **MUST** widen it.

**Threshold**: An invariant: for a fixed caller and policy, the union of person
identifiers appearing in every response body and every refusal across the whole
authenticated surface is a subset of that caller's visible set.

**Rationale**: The visible set is only as good as its weakest surface. A
refusal that names a person, or a batch answer that reports one that was not
asked about, defeats the rule everywhere else.

#### No credential and no address in a log line

- [x] `p1` - **ID**: `cpt-insightspec-nfr-identity-logging-pii`

**Vector**: Security

Logs **MUST** be structured and **MUST NOT** carry a person's address or any
credential, including in configuration dumps, connection targets and error
payloads.

**Threshold**: An invariant: no log line emitted by the service on any code
path contains a personal address or a credential, verified by tests that seed
recognisable values and scan the captured output.

**Rationale**: Log aggregation crosses trust boundaries that the service's own
answers do not, so anything reaching it is effectively disclosed more widely
than the API ever discloses it.

#### Every response and traversal is bounded

- [x] `p1` - **ID**: `cpt-insightspec-nfr-identity-bounded-responses`

**Vector**: Reliability

Every listing, batch request and recursive expansion **MUST** have a
server-side bound that the caller can lower but not raise.

**Threshold**: An invariant: no authenticated request can produce an unbounded
response or an unbounded traversal — page sizes, batch item counts and org-tree
depth all clamp to a server maximum, including when the caller supplies nothing
or supplies a nonsense value.

**Rationale**: The org chart and the roster grow with the customer, so any
surface without a ceiling is an outage that arrives on the largest and most
important install first.

#### A batch job is safe to re-run and safe to interrupt

- [x] `p1` - **ID**: `cpt-insightspec-nfr-identity-job-idempotence`

**Vector**: Reliability

Rebuilding the projection or publishing the snapshot **MUST** be safe to repeat
and safe to abandon: a repeat over unchanged evidence changes nothing, and an
interrupted run leaves no state that blocks the next one.

**Threshold**: An invariant: a second run over unchanged evidence produces no
new observations and no changed edges, and a run terminated at any point leaves
the schema, the lock and the operation journal in a state the next run
completes from.

**Rationale**: These jobs run unattended on a schedule against an append-only
store. A non-idempotent repeat corrupts the history it cannot then undo, and a
run that leaves a lock behind stops every later one silently.

#### Profile resolution latency

- [x] `p1` - **ID**: `cpt-insightspec-nfr-identity-latency`

**Vector**: Performance

Resolving one profile **MUST** complete within an agreed budget under a stated
organisation size, measured at the gateway-to-service hop.

**Threshold**: **Open decision.** The measurement boundary is fixed — the
gateway-to-service hop for a single profile resolution with the subtree
expansion enabled — but no target has been agreed. Two properties must be
stated together with any number: the organisation size it holds at, and whether
it covers the subtree expansion, whose cost is proportional to the subtree, not
constant. Owner: the platform performance baseline; until it lands, this
obligation is measured and reported, not enforced.

**Rationale**: A profile read is on the critical path of every screen, so it
constrains the whole product's responsiveness. Recording it as an open decision
rather than an invented number keeps a fabricated budget from being cited as
agreed.

#### Steady-state footprint without a cache

- [x] `p1` - **ID**: `cpt-insightspec-nfr-identity-memory`

**Vector**: Efficiency

The service **MUST** operate within its deployed memory allocation while
holding no cache of the journal.

**Threshold**: Resident memory stays within the memory limit the shipped chart
configures for the service, at steady state, under the install's normal read
mix. The limit is the agreed figure; any specification-level number must be
reconciled with it rather than stated independently.

**Rationale**: Deriving every answer per request is a deliberate trade of
memory for freshness, and the deployed limit is what makes that trade
verifiable instead of theoretical.

#### Identifier round-trip fidelity

- [x] `p1` - **ID**: `cpt-insightspec-nfr-identity-uuid-roundtrip`

**Vector**: Reliability

Every identifier written and read back **MUST** be byte-identical, with no
reliance on a driver's textual fallback representation.

**Threshold**: An invariant: an identifier written by one path and read by
another compares equal, for every identifier column in the schema.

**Rationale**: The compact binary storage form silently truncates a textual
identifier rather than rejecting it, so the failure surfaces as a person who
cannot be found rather than as an error at the write.

#### Support any configured source set

- [x] `p2` - **ID**: `cpt-insightspec-nfr-identity-source-versatility`

**Vector**: Versatility

Adding a connector that emits identity observations **MUST NOT** require a
change to this service, and an install that configures no reporting-line
source, no roster source, or a single source **MUST** remain fully functional.

**Threshold**: An invariant: the service's behaviour is a function of the
observations present and the configured policy, with no per-connector branch in
its code. Every combination of "reporting source configured or not" and "roster
configured or not" is a supported install.

**Rationale**: The connector set differs per customer and grows continuously.
A service that needs a change per source makes every new connector a release of
the identity service too.

### 6.2 NFR Exclusions

- **Service-level availability target**: not applicable here — the service is
  stateless beyond its connection pool, so availability is a property of the
  deployment and the database, and is specified at the platform level rather
  than per service.
- **Recovery objectives (data loss and downtime tolerance)**: not applicable
  here — the service stores no state that is not derivable from the journal and
  the connector evidence, so recovery objectives belong to the database and the
  ingestion pipeline that feed it.
- **Write throughput target**: not applicable — the interactive surface's
  writes are operator corrections and grants, which are human-paced; the bulk
  write path is the scheduled rebuild, whose obligation is idempotence and
  refusal to run destructively, not a rate.
- **Interaction-capability obligations (usability, accessibility,
  internationalization, device support)**: not applicable — the service has no
  user interface; these obligations are held by the front end that consumes it.
- **Safety obligations**: not applicable — the service has no physical
  actuation and no path to physical harm.

## 7. Public Library Interfaces

The wire contract is generated from the implementation and committed at
[`openapi.json`](../../openapi.json); a build gate fails on drift, so it is the
authority on shapes, parameters and status codes. This section names the
interface surfaces, their stability and their compatibility policy; the endpoint
inventory and the error model are in
[DESIGN §3.3](DESIGN.md#33-api-contracts).

### 7.1 Public API Surface

#### Profile resolution

- [x] `p1` - **ID**: `cpt-insightspec-interface-identity-profile-resolve`

**Type**: HTTP/REST, JSON request body.

**Stability**: stable.

**Description**: Resolves one person from an address, a source-scoped account
id or the canonical person key, and the batch form that resolves many person
keys at once. Returns the person's current attributes, their org tree and every
account they hold, filtered by the caller's visible set.

**Breaking Change Policy**: additive response fields and additional accepted
lookup keys are non-breaking; removing a field or narrowing an accepted key
requires a major version.

#### People and visibility

- [x] `p1` - **ID**: `cpt-insightspec-interface-identity-people`

**Type**: HTTP/REST.

**Stability**: stable.

**Description**: The canonical people roster and single-person read, the
caller's own visible roster, the batch visible-set filter, and the caller's
self-description. Paged surfaces issue an opaque continuation token that is
valid only for the listing and query that issued it.

**Breaking Change Policy**: additive fields are non-breaking; the continuation
token is opaque and its encoding may change at any time, so no consumer may
construct or parse one.

#### Organisation chart

- [x] `p1` - **ID**: `cpt-insightspec-interface-identity-subchart`

**Type**: HTTP/REST.

**Stability**: stable.

**Description**: The reporting subtree rooted at a person the caller may see,
and the forest of every root they may see, both optionally as of a past instant
and both bounded by a server-side depth limit.

**Breaking Change Policy**: additive node fields are non-breaking; the depth
ceiling is an operational setting and may change without a version bump.

#### Administration

- [x] `p1` - **ID**: `cpt-insightspec-interface-identity-admin`

**Type**: HTTP/REST.

**Stability**: stable.

**Description**: The admin-gated surfaces — the role catalogue, role
assignments, visibility grants, the operator person and account searches, the
correction verbs, the review queue, and the rebuild and publish journals.

**Breaking Change Policy**: additive fields and new verbs are non-breaking;
changing which refusals a guard produces is breaking, because consumers branch
on them.

#### Internal login resolution

- [x] `p1` - **ID**: `cpt-insightspec-interface-identity-internal-login`

**Type**: HTTP/REST, service principals only.

**Stability**: internal — deliberately absent from the published contract, and
changeable in lockstep with the authenticator.

**Description**: One route per login question: resolve by directory principal,
resolve by roster address, provision a person the roster already lists, read a
person's active roles, and the administrative view-as address resolution. Each
answers exactly one question, so no absent field can route a login onto a path
the install did not configure.

**Breaking Change Policy**: not a public contract; changes ship together with
the authenticator. The response shape is depended on verbatim and is pinned by
tests on both sides.

#### Health and readiness

- [x] `p1` - **ID**: `cpt-insightspec-interface-identity-health`

**Type**: HTTP/REST.

**Stability**: stable.

**Description**: The liveness and readiness endpoints the deployment's probes
are wired to. They are provided by the service host, not implemented by this
service, and consequently report process health rather than database
reachability — see [DESIGN §3.3](DESIGN.md#33-api-contracts) for what that
implies for a stand whose database is unreachable.

**Breaking Change Policy**: no payload contract; never breaking.

### 7.2 External Integration Contracts

#### Host and gear configuration

- [x] `p1` - **ID**: `cpt-insightspec-contract-identity-env-config`

**Direction**: required from the operator.

**Protocol/Format**: the service host's layered configuration — a YAML section
per gear, overridable per field by environment variables. The full field list,
defaults and the override spelling are in
[DESIGN §4.1](DESIGN.md#41-configuration-surface).

**Compatibility**: fields are added compatibly; removing or renaming one is a
breaking change to the deployment contract and requires a chart major version.

#### Deployment secret

- [x] `p2` - **ID**: `cpt-insightspec-contract-identity-config-secret`

**Direction**: provided by the deployment chart, consumed by the service.

**Protocol/Format**: a Kubernetes Secret carrying the configuration overrides
that must not be committed — the database connection, the warehouse
coordinates and their credentials, and the optional bootstrap settings.

**Compatibility**: additive keys are non-breaking; the service must start with
only the required keys present.

#### Published person snapshot

- [x] `p1` - **ID**: `cpt-insightspec-contract-identity-persons-snapshot`

**Direction**: provided by this service, consumed by the warehouse transforms.

**Protocol/Format**: a whole-table snapshot of the journal in the analytics
warehouse, replaced atomically, carrying its own publication watermark.

**Compatibility**: the service is the sole writer; consumers must tolerate the
snapshot being replaced beneath them and must not write to it.

## 8. Use Cases

#### A screen shows a colleague's profile

- [x] `p1` - **ID**: `cpt-insightspec-usecase-identity-lookup-email`

**Actor**: `cpt-insightspec-actor-product-user`

**Preconditions**:

- The projection has been built at least once for the tenant.
- The caller holds a valid session and their token names their tenant.

**Main Flow**:

1. The caller opens a person's page in the product.
2. The front end asks this service to resolve that person by their key.
3. The service establishes the caller and tenant from the token.
4. The service narrows the match to what the caller may see.
5. The service composes the person's current attributes across sources, their
   supervisor and their subtree, and every account they hold.
6. The front end renders the profile.

**Postconditions**:

- The caller has seen only a person within their visible set.

**Alternative Flows**:

- **Nobody matches, or the caller may not see them**: the service answers
  "not found" identically in both cases, and the front end shows the same
  empty state.
- **Several visible people match**: the service refuses and names them; the
  front end reports a data problem rather than showing an arbitrary person.

#### An operator resolves an unattributed account

- [x] `p1` - **ID**: `cpt-insightspec-usecase-identity-operator-correction`

**Actor**: `cpt-insightspec-actor-identity-operator`

**Preconditions**:

- The operator holds an active admin assignment in the tenant.
- A connector has observed an account the automatic pipeline did not bind.

**Main Flow**:

1. The operator lists the accounts awaiting a decision and reads the evidence
   behind one.
2. The operator searches the tenant's people for the person it belongs to.
3. The operator binds the account to that person.
4. The service appends the binding under the operator's authorship, journals
   the call, and publishes the refreshed journal to the warehouse.
5. Subsequent product reads attribute the account's activity to that person.

**Postconditions**:

- The account is bound, the decision is attributable, and the next scheduled
  rebuild preserves it.

**Alternative Flows**:

- **The account is not a person**: the operator excludes it; it stops reaching
  the queue and never resolves to a person at login.
- **The operator picked the wrong person**: the operator detaches the binding;
  the history retains both decisions.

#### Someone signs in for the first time

- [x] `p1` - **ID**: `cpt-insightspec-usecase-identity-login-bootstrap`

**Actor**: `cpt-insightspec-actor-authenticator`

**Preconditions**:

- The install's identity provider has authenticated the principal.
- A connector has observed the account the principal corresponds to.

**Main Flow**:

1. The authenticator asks this service for the person behind the principal,
   using whichever resolution the install configured.
2. No binding exists, so the authenticator asks the service to provision one.
3. The service confirms a connector has already observed the account, and that
   the account is neither closed nor one that automatic resolution would link.
4. The service mints a person, binds the account under the observed connector
   instance, and marks it for operator confirmation.
5. The authenticator reads the person's active roles and mints the session.

**Postconditions**:

- The person can use the product, and their account is queued for an operator
  to confirm.

**Alternative Flows**:

- **No connector has observed the account**: the service refuses; the
  authenticator denies the login rather than creating an unknown person.
- **An operator has already excluded the account**: the service answers that it
  resolves to no person, and records the refusal.
- **Two logins race**: one mints; the other reads what is in force. One person
  results.

#### An operator diagnoses a stale organisation

- [x] `p2` - **ID**: `cpt-insightspec-usecase-identity-diagnose-stale-projection`

**Actor**: `cpt-insightspec-actor-platform-sre`

**Preconditions**:

- The operator holds an active admin assignment in the tenant.

**Main Flow**:

1. The operator reads the history of rebuild runs.
2. The most recent run is recorded as failed, with the guard that refused it
   and the reason.
3. The operator corrects the underlying cause — the warehouse coordinates, the
   configured tenant, or the connector that produced no evidence.
4. The operator triggers a run out of band and confirms it completed.

**Postconditions**:

- The projection and the published snapshot are current, and the journal
  records both the failure and the recovery.

## 9. Acceptance Criteria

- [ ] A caller resolves the same person by address, by source-scoped account id
      and by person key, and receives the same profile.
- [ ] A caller cannot resolve, list, filter or traverse to a person outside
      their visible set, and the refusal for a hidden person is
      indistinguishable from the refusal for one that does not exist.
- [ ] A lookup matching several visible people is refused and names them;
      making one of them invisible to the caller turns the same lookup into a
      successful single match.
- [ ] Granting and revoking the admin role changes what the operator surfaces
      allow on the next request, with no restart.
- [ ] Revoking the last active admin assignment in a tenant is refused, and
      deleting a role with an active assignment is refused.
- [ ] Under the tenant-wide visibility policy, a caller with no reporting line
      and no grants sees the whole tenant; switching back leaves every grant
      unchanged.
- [ ] A login for a principal the roster lists but no connector has observed is
      refused; one for an observed account provisions exactly one person, and
      the next scheduled rebuild adopts that person rather than minting another.
- [ ] A rebuild over unchanged evidence writes no new observations and changes
      no edges; a rebuild against an empty input, and a publish that would empty
      a populated snapshot, are both refused and recorded as failed runs.
- [ ] The committed interface document regenerates from the implementation with
      no difference.
- [ ] `cfs validate --artifact` reports no error for the PRD, the DESIGN and
      every ADR in this specification set, and `cfs validate-toc` passes for
      each.

## 10. Dependencies

| Dependency | Description | Criticality |
|------------|-------------|-------------|
| Identity database | Stores the journal, the org chart, the people projection, the grants and the operation journal. The service owns and migrates this schema. | p1 |
| Analytics warehouse | Supplies the connector identity evidence the rebuild reads, and receives the published person snapshot. | p1 |
| Gateway and authenticator | Establish and sign the caller identity every request is answered against. Without them the service admits nobody. | p1 |
| Connector pipeline | Produces the identity evidence. Without it the journal is empty and every lookup legitimately finds nothing. | p1 |
| Scheduler | Runs the rebuild. Without it the projection freezes at the last manual run. | p1 |
| Service host framework | Provides the routing, the token verification, the configuration layering, the health endpoints and the log pipeline. | p1 |

## 11. Assumptions

- One database per install; the service neither shards nor writes to more than
  one.
- Tenants are identified by an opaque fixed-width identifier; a change to that
  representation is a schema and specification revision, not a configuration
  change.
- Every person-authenticated caller reaches the service through the gateway.
  Direct callers are not a supported deployment.
- At most one source is configured as the roster. Naming a second, or naming a
  source spanning several connector instances, is unsupported and produces
  duplicate people the service cannot rejoin.
- The connector evidence names the same human consistently enough for the
  address match to be correct where it applies; where it is not, the operator
  correction surface is the intended remedy rather than a better automatic
  match.

## 12. Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| The readiness endpoint does not reflect database reachability | A stand shows healthy pods while every request fails, and the failure is diagnosed as a client problem. | Documented in DESIGN §3.3 as a known gap; a readiness check that exercises the connection pool is the fix. |
| The roster setting is enabled and later withdrawn | The journal is append-only, so the minted people and their queue items remain; on an install resolving logins by address, clearing it denies every login rather than only disabling minting. | The refusal is explicit and logged; the setting's dual effect is documented at the configuration surface. |
| A specification number and a deployed setting disagree | An obligation is cited against a figure the install never ran, and the discrepancy is discovered during an incident. | Thresholds name the deployed setting as the agreed figure; an unagreed target is recorded as an open decision rather than a number. |
| The visibility rule is re-implemented by a consumer | Two surfaces disagree about who may see whom, and the more permissive one wins. | One derivation serves the batch filter, the roster, the profile gate and the tree reads; consumers are given the filter rather than the rule. |
| The evidence source changes what it emits | New attributes are silently ignored, or an existing one stops arriving and its value silently ages. | The projection is derived, not stored, so a missing attribute reads as absent rather than stale; the rebuild journal records what each run wrote. |
| Operator corrections and an automatic rebuild disagree | A correction is undone by the next scheduled run and reappears in the queue. | Corrections are recorded as observations in the same journal the rebuild reads, so a later run preserves rather than overwrites them. |
