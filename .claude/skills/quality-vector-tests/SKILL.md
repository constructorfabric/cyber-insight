---
name: quality-vector-tests
description: >-
  Write or revise vector-attributed tests for an Insight feature. Trace PRD FR/NFRs
  through the canonical FEATURE requirement references to scenarios and executable
  tests. Use when adding a Testing section or reviewing its vectors, test links,
  inherited obligations or coverage gaps; keep Acceptance Criteria in the kit's form.
---

# Quality-vector tests for Insight features

Attach tests to the feature they verify. The trace is:

`PRD FR/NFR → DESIGN allocation → FEATURE Requirements + feature ID → scenario → test`

Read the [quality-vector authoring guide](../../../.cf-studio/config/kits/sdlc/guides/quality-vectors.md)
for definitions, advisory PRD improvements, shared obligations and scenario changes.
It is the shared product model; this skill owns the Testing format and suite mapping.

The canonical SDLC kit already supplies the feature ID, the `Requirements` field
in section 1.2, flows, DoDs and a plain Acceptance Criteria checklist. Preserve
those. `## 7. Testing` is Insight's small extension to that structure; it does
not introduce AC IDs, AC coverage ratios, a new test artifact kind, or new code
marker syntax.

## 1. Resolve the owning feature and requirements

Read the registered FEATURE, its DECOMPOSITION entry, PRD and DESIGN. In section
1.2, retain the FR/NFR IDs that the feature implements, including inherited NFRs.
Confirm that each inherited ID resolves to the upstream definition and that its
target and verification owner remain applicable.

Read DESIGN's NFR allocation to establish this feature's contribution and who
assesses the complete obligation. Suggest missing allocation details within scope;
identify measurement boundaries and any component budgets without inventing targets.

Read Acceptance Criteria as the feature's observable completion conditions.
Improve unclear criteria when that is within the user's requested scope, but do
not number or rewrite them just to attach tests. An existing author's criterion
IDs may remain for compatibility; they are not this skill's traceability key.

Each scenario belongs to the FEATURE identified once by `**Feature**` in Testing.
Its `**Requirements**` field cites the relevant upstream FR/NFR subset already
listed in section 1.2. This makes a precise path through the feature without
inserting an AC layer. An applicable NFR needs a scenario, shared-test link, or
an explicit deferred entry naming the owner and reason.

An issue is a planning surface. If a FEATURE exists, keep Testing there and link
the issue to it when issue editing is authorized. If no FEATURE exists, use the
issue URL as temporary ownership and its existing requirement references; state
that the PRD → FEATURE → test chain is incomplete. Do not invent a registered
feature ID or an AC taxonomy to make the report look complete.

## 2. Ground the scope and expected outcomes

Agreed requirements define expected outcomes. Code inspection locates the real
boundaries, suitable suites, existing tests and specification disagreements; it
must not silently redefine the oracle to match current behaviour. Read
`scope-feature-tests` for deeper scope analysis.

For existing code, distinguish a retrospective test plan from new capability.
For a port or migration, lead with parity on the same synthetic inputs. Tag
differences `exact`, `known-diff(direction)`, or `merge` where appropriate.
New specs may precede code; say so.

Keep unresolved product decisions in feature context, with owners and resolution
points. A test with no agreed upstream requirement is deferred and identifies
that missing link. Missing infrastructure is also a gap, never a reason to mark
an applicable vector n/a. Do not manufacture targets, fixtures, supported-source
counts, or degraded behaviour.

## 3. Choose one vector and one suite per scenario

Use [vector-mapping.md](references/vector-mapping.md) for common Insight probes
within the guide's broader product definitions. The scenario's primary vector
identifies its verification claim. Preserve the scope and target of the upstream
obligations being verified; supporting assertions do not prove another obligation
in full merely because its ID is referenced.

Choose the cheapest existing suite capable of falsifying the claim:

| Target component | Suite tags, cheapest adequate first |
|---|---|
| Frontend | `fe-unit` (vitest `unit` project) → `fe-component` (vitest `storybook` project) → `stand-ui` (`tests/stand/ui`) |
| Serving / analytics | `rust-unit` (inline `#[cfg(test)]`) → `metric-spec` (`tests/datapath/metrics/<class>`) → `stand-api` (`tests/stand/api`) |
| Authentication | `rust-unit` → `auth-rig` (`src/backend/services/authenticator/tests`) → `stand-api` / `stand-ui` |
| Identity | `rust-unit` → `identity-e2e` (`tests/datapath/identity`) → `stand-api` |
| Ingestion | `connector-tests` (`src/ingestion/connectors/*/*/tests`) → `dbt-tests` (`src/ingestion/dbt/tests`) → `metric-spec` |
| Cross-cutting | `ci-static` (scans and gates in `.github/`) or `manual` |

Verify tools before naming them: inspect the relevant test directory, package
scripts or CI workflows. `scripts/counts.sh` takes repo-wide denominators
and reports MOVED rather than a misleading zero when a source has shifted; take a
count from it rather than quoting one. A browser scenario must need browser-observable
behaviour. Performance and Efficiency may use `manual` when no suitable load or
soak lane exists, but must identify the missing procedure and conditions.
Measure them on the same approved reference fixture at each NFR's own load.

## 4. Write Testing

Use this shape in a FEATURE; an issue uses `## Testing` instead:

```markdown
## 7. Testing

**Feature**: `cpt-{system}-feature-{slug}`

{Brief scope, primary risk, fixtures and test boundaries. Reference upstream
targets instead of defining new ones here.}

- [ ] 1. **Scenario name** — Security · identity-e2e — perform an action → observe the required outcome.
  **Requirements**: `cpt-{system}-nfr-{slug}`.
  **Test**: Not implemented.
- **deferred** 2. **Scenario name** — Performance · manual — perform the defined measurement → satisfy the upstream target.
  **Requirements**: `cpt-{system}-nfr-{other-slug}`.
  **Blocked by**: {owner, reason and resolution point}.

**Versatility** — n/a: {why no local or inherited obligation applies to this feature}.
```

The canonical AC checklist stays in section 6. Do not expand it into a copy of
the test plan or add a scenario-to-AC map. Test ownership comes from the
feature ID; scenario numbers are stable local anchors for test references, not
another requirement namespace.

- One numbered checkbox tracks one scenario, with exactly one vector, one suite,
  and an executable do → expect. Subordinate metadata uses plain continuation
  lines, never additional checkboxes.
- Keep numbers stable once published. Append new numbers; retain a dropped
  number with its disposition. Do not renumber tests to match changed ACs.
- Editorial changes retain their number. Changes to a target, scope or expected
  behavior require reassessing linked tests; clear the implementation checkbox
  until the complete revised claim is mapped. Record successor numbers for a
  split or merge, and keep passing evidence tied to the specification/test revision.
- `*(main gate)*` after a scenario name is optional for the parity gate.
- `**Requirements**` names exact upstream IDs. A shared test can satisfy several
  features when its assertions prove each claim and scope. A scenario may reference
  several requirements and a requirement may have several scenarios. For independent
  claims in different vectors, use assertion-focused tests sharing setup; retain
  one primary native vector marker per test.
- `**Test**` links to the exact file and function or case when implemented.
  Otherwise distinguish `Not implemented` from `Not yet mapped to the complete
  scenario`; a directory is not implementation evidence.
- Replace the checkbox with `**deferred**` when a scenario cannot yet be defined
  or executed. Preserve its number, vector, requirement reference and owner.
  If the requirement itself is missing, say so and link the decision that must
  establish it; do not cite an unrelated NFR.
- Consider all five vectors and suggest missing scenarios for applicable obligations.
  Explain exclusions when useful; missing categories alone do not add a requirement
  or readiness gate. An inherited obligation still needs its applicable evidence.
- Group by risk; keep prose understandable without reading implementation code.
  Keep feature and requirement IDs because they are the traceability links.

See [example-feature.md](references/example-feature.md) for a worked synthetic
PRD → DESIGN → FEATURE → executable test example. The older port, migration and lean examples are
historical scope studies, not the current output contract.

## 5. Link executable tests back to the feature

When implementing or mapping a scenario:

1. Verify that the assertions prove the complete scenario claim. Its primary vector
   should describe that claim; a supporting requirement may have another vector.
2. Put the FEATURE path, its existing `cpt-…-feature-…` ID and stable scenario
   number in test metadata, a docstring, or an appropriate test reference.
   Keep the native vector marker where the suite supports one: stand API/UI tests
   require exactly one pytest vector marker under `tests/pyproject.toml`.
   For YAML metric specs, use their existing description field.
3. Link the scenario's `**Test**` field back to that exact test and check its box.
   A checked box means implemented, not currently passing.
4. Record passing evidence separately: tested revision, fixture/version, run
   command, conditions and result. Skipped tests, missing personas and absent
   source fixtures are not passing evidence.

Use the canonical `flow/algo/state/dod` markers when FULL code traceability is
required by the kit. Do not invent `@cpt-test` or `@cpt-feature` markers: those
are not supported code marker kinds. FEATURE/test links complement, rather than
replace, the kit's implementation trace.

For an existing test, link only the behaviour its assertions actually prove.
Do not check a broad scenario merely because a nearby test passes.

## 6. Write back and validate

Keep the target the user named and the requested scope. Existing authorization
to update a draft or PR covers those edits. If a product decision is unresolved,
record it for review instead of silently deciding it while formatting tests.

Re-read the file or issue immediately before editing to preserve concurrent work.
For GitHub bodies, use a file outside the product repository and `--body-file`;
re-fetch after editing. When both an issue and FEATURE exist, the long-lived
FEATURE owns the test content.

For a FEATURE:

```sh
cfs toc docs/<path>/FEATURE.md
cfs validate --artifact docs/<path>/FEATURE.md
cfs validate-toc docs/<path>/FEATURE.md
.claude/skills/quality-vector-tests/scripts/rollup.sh docs/<path>/FEATURE.md
```

Use the guide for advisory improvement suggestions after canonical validation.
Review the chain in both directions: PRD IDs resolve; DESIGN allocates responsibility;
the FEATURE declares its requirements; scenarios reference the right subset;
implemented test links resolve and tests cite their owner. Explain missing or
deferred evidence without certifying an unmet requirement. This review adds no
QV pass/fail gate. CFS validates canonical artifact references; the optional
rollup counts declarations and does not validate metadata or prove coverage.

The rollup retains older criterion-tagged lines for compatibility and reports
deferred scenarios separately. It ignores fenced examples and comments.
Do not use a ratio of ACs to scenarios as a quality or traceability gate.

It prints one line per vector: a count of scenarios or tagged NFRs, `inherited (n)`
and `deferred (n)` alongside that count, `n/a (declared)` where an exclusion names
the vector, and `MISSING` where the vector is neither tagged nor excluded. `MISSING`
records an absence for the author to consider, not a failure — which vectors a
feature or module needs is a product judgement, not a completeness rule. The script
does exit non-zero on shapes it cannot read: an unreadable path, a `**Vector**` value
outside the five, an NFR entry with neither an ID nor `**Inherits**`, and a vector
both tagged and excluded. Those are contradictions in the artifact, not verdicts on
its quality.
