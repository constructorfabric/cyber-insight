# ADR-0017: Tenant-Wide Visibility as a Configured Policy, Not as Issued Grants

<!-- toc -->

- [Context and Problem Statement](#context-and-problem-statement)
- [Decision Drivers](#decision-drivers)
- [Considered Options](#considered-options)
- [Decision Outcome](#decision-outcome)
  - [Consequences](#consequences)
  - [Confirmation](#confirmation)
- [Pros and Cons of the Options](#pros-and-cons-of-the-options)
  - [A configured policy parameter bound into the one derivation (chosen)](#a-configured-policy-parameter-bound-into-the-one-derivation-chosen)
  - [Issue every person a tenant-wide visibility grant at seed time](#issue-every-person-a-tenant-wide-visibility-grant-at-seed-time)
  - [Treat an empty org chart as implying tenant-wide visibility](#treat-an-empty-org-chart-as-implying-tenant-wide-visibility)
- [More Information](#more-information)
- [Traceability](#traceability)

<!-- /toc -->

**ID**: `cpt-insightspec-adr-0017-visibility-policy-configuration`

**Status:** Accepted

## Context and Problem Statement

The visible set is derived from the reporting line: a caller sees themselves,
the people their active grants name, and the org-chart descendants of both.
That derivation assumes the install has a reporting line to descend. Not every
one does — an organisation whose only identity source is a chat platform or a
code host has a roster with no supervisor edges at all, and under the
reporting-line rule every caller in it sees exactly one person: themselves.
Every roster is empty, every picker is empty, and every metric that names
another person is filtered away.

There are two shapes of answer. Either the install issues each person a
tenant-wide visibility grant, which makes the data say something the operator
never decided about each individual; or the derivation itself takes a second
mode. The choice matters more than it first appears, because grants are
append-only records with an author and a reason, and issuing tens of thousands
of them to work around a missing reporting line is not something a later
decision can cleanly undo.

## Decision Drivers

- An install with no reporting lines must be fully functional, not degraded.
- Whatever is chosen must be reversible. An operator switching a stand from one
  organisational model to the other must not lose the grants they made
  deliberately.
- One derivation must keep serving every consumer — the batch filter, the
  profile gate, the roster, the picker and the org-chart reads — or they will
  disagree.
- The grant tables are audit evidence. Filling them with machine-issued rows
  destroys their value as a record of deliberate exceptions.
- A misread setting must not silently pick the more permissive branch.

## Considered Options

- A configured policy parameter bound into the one derivation (chosen)
- Issue every person a tenant-wide visibility grant at seed time
- Treat an empty org chart as implying tenant-wide visibility

## Decision Outcome

Chosen option: **a configured policy parameter bound into the one derivation**,
because it is the only option that is reversible and that leaves the grant
tables meaning what they say.

The install carries one setting with two values. Under `org_chart` — the
default — the visible set is the caller, their active grants, and the
org-chart descendants of both. Under `flat` it is every person in the tenant.
The value is bound as a parameter into the same expressions every consumer
already uses, so the batch filter, the profile gate, the roster, the picker and
both org-chart reads cannot diverge under either policy.

Three properties follow deliberately:

- **Nothing is written.** Switching the policy creates no row and destroys
  none. Grants keep their meaning under either value, and switching back
  restores the previous behaviour exactly.
- **Roles stay out of the predicate.** Holding the admin role confers no
  visibility under either policy. Administering identity and seeing people
  remain separate powers, as ADR-0012 and ADR-0015 established.
- **An unreadable value refuses to load.** A setting that decides who may see
  whom must stop the service rather than resolve to whichever branch a type
  default happens to be. Anything that is not one of the two names — a
  different case, a hyphen instead of an underscore, an empty string — fails
  configuration deserialization.

The caller's own view reports the policy, so a consumer seeing an empty
reporting tree can tell "this install has no reporting lines" from "this person
has no reports" without inferring it.

### Consequences

- **Positive:** an install with no reporting lines works with no data
  workaround and no irreversible write.
- **Positive:** the grant tables continue to record only deliberate exceptions,
  so they remain usable as audit evidence.
- **Positive:** one derivation still backs every consumer; the policy cannot
  make two surfaces disagree.
- **Negative:** the flat policy is all-or-nothing per install. An organisation
  wanting most people flat and a few restricted has no expression for it, and
  would need the reporting-line policy plus grants.
- **Negative:** a per-install setting is a thing to get wrong. Setting `flat`
  on an install that does have reporting lines silently widens everyone's
  visibility, and nothing in the data records that it happened.
- **Negative:** a reader of the grant tables alone cannot tell what a caller
  can see; they must also know the policy.

### Confirmation

The live test suite builds each case against both policies from one fixture, so
a change that makes a surface consult the policy differently fails. Unit tests
pin the deserialization: the default when the field is absent, each accepted
name, and refusal for every near-miss spelling. The reported policy on the
caller's own view is covered by the same suite.

## Pros and Cons of the Options

### A configured policy parameter bound into the one derivation (chosen)

- **Pro:** reversible; nothing is written.
- **Pro:** one derivation continues to serve every consumer.
- **Pro:** the intent is legible in one place rather than inferred from tens of
  thousands of rows.
- **Con:** all-or-nothing per install.
- **Con:** the effective permission now depends on configuration as well as
  data.

### Issue every person a tenant-wide visibility grant at seed time

- **Pro:** no change to the derivation at all; the existing wildcard grant
  already means what is wanted.
- **Con:** not reversible in any clean sense. The rows are append-only records
  with an author and a reason, and the author would be a machine.
- **Con:** destroys the grant table's value as an audit record of deliberate
  exceptions.
- **Con:** the seed would have to keep issuing them for every new person,
  making the workaround permanent.

### Treat an empty org chart as implying tenant-wide visibility

- **Pro:** no setting to get wrong; the install configures itself.
- **Con:** fails open on exactly the wrong input. A stand whose org chart is
  empty because the rebuild failed, or because a connector stopped emitting
  supervisors, would silently show everyone everyone.
- **Con:** the transition is invisible: the first successful rebuild would
  silently narrow every caller's visibility with no record of why.

## More Information

The two policies are not a permission hierarchy — `flat` is not "more
permission" granted to anyone, it is a different statement about what the
organisation's data means. That is why it is configuration rather than a grant:
a grant answers "may this person see that person", and this setting answers
"does this organisation model visibility on a reporting line at all".

## Traceability

- Setting: `services/identity-resolution/src/config.rs`
- Derivation: `subchart_repo` visible-set expressions
- Reported at: `api::me`
- Tests: `config` unit tests, `api::http_live_tests` (both policies from one
  fixture)
- Related: ADR-0012 (admin-only reads on the grant tables), ADR-0015
  (self-scoped visible-set read), ADR-0010 (org-chart cache)

This decision addresses:

- `cpt-insightspec-fr-identity-visible-persons-policy` — the policy this
  requirement describes.
- `cpt-insightspec-fr-identity-visibility-grants` — what the policy does not
  touch.
- `cpt-insightspec-fr-identity-me` — reporting the policy to a consumer.
- `cpt-insightspec-nfr-identity-source-versatility` — an install with no
  reporting-line source stays functional.
- `cpt-insightspec-principle-identity-single-visibility` — the principle this
  decision preserves.
