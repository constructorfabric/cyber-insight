# Quality vectors: authoring guidance

Use the five vectors to help an author discover and clarify product quality
expectations. This guide produces improvement suggestions, not a new PRD or
FEATURE readiness gate. Missing tags, a missing vector row, or an unavailable
rollup do not create a validation failure. Existing canonical validation and
agreed product requirements retain their meaning.

## Meanings and useful questions

The order below is a stable display convention. The product's users, risks and
business consequences determine priority; the order does not rank obligations.

| Vector | Product meaning | Useful authoring question |
|---|---|---|
| Efficiency | Total cost of ownership: compute, storage, licensing, platform overhead and user/operator effort across delivery and operation. | Which resource or human effort makes this capability costly to provide or use? |
| Reliability | Dependable, correct and consistent operation over time, including availability, fault tolerance and recovery. | What must remain correct or recoverable when data changes or a dependency fails? |
| Performance | Latency, throughput and scalability under stated operating conditions. | Which operation must complete within a budget, or sustain a rate, at what scale? |
| Security | Protection against unauthorized access, disclosure, misuse or loss, including isolation and auditability. | Which actor may access which information, and what must remain protected? |
| Versatility | Breadth and adaptability across supported use cases, sources, protocols, integrations and deployment options. | Which supported scenarios must work without bespoke changes? |

These are broad definitions. Test coverage can inform confidence in evidence;
it is not itself a measure of product reliability. Resource examples do not
imply that storage or operator time is negligible. A shorter build pipeline is
relevant to Efficiency when tied to a product delivery cost or requirement.

## Help improve a PRD

Start with the module's real quality concerns and their consequences. Consider
all five lenses, then suggest only requirements that matter for the requested
scope. Avoid inventing obligations merely to fill categories.

For each useful suggestion, identify the requirement, explain what is unclear,
and propose wording or a measurement approach. Look for an observable property,
a target or invariant, scope, relevant conditions, and a plausible verification
method. A numeric metric, boolean invariant, supported range or interaction
constraint can each express a checkable expectation. Verification detail belongs
in DESIGN and FEATURE; a PRD need not prescribe a test harness.

| Draft wording | Suggested improvement |
|---|---|
| Lookup must be fast. | Identify the lookup operation, latency percentile, measurement boundary and representative workload. Ask for an agreed target or a baseline measurement; do not invent a budget. |
| Support all sources. | Reference a maintained supported-source set and clarify multiple-instance behavior. |
| No cross-tenant disclosure. | Clarify responses, nested relationships and diagnostic surfaces, including identifier collisions. |
| Operation should be economical. | Identify the relevant compute, storage or operator-effort cost and the unit of useful work. |

Keep one authoritative FR/NFR definition and ID. A local NFR can carry
`**Vector**`, `**Threshold**` and `**Rationale**`; the threshold may be an absolute
invariant rather than a number. Preserve agreed targets and clearly label
proposed targets and open decisions with an owner or source of input. Missing
conditions invite clarification, not a fabricated metric or an exclusion.

Reference an unchanged upstream obligation with `**Inherits**` and its NFR ID.
Record the responsible role and intended shared verification in `**Verification**`;
evidence can remain pending during authoring. An individual default NFR may be
excluded with a reason while other requirements in its vector remain applicable.
A wholly inapplicable vector is a separate, explicit scope decision.

An optional vector summary should reference these IDs and explain business
consequences without introducing a second definition or target. Tags permit
grouping when useful; the rollup counts declarations, not adequacy or results.

## Carry the obligation into DESIGN and FEATURE

Use DESIGN's existing NFR Allocation table to explain the responsible component,
design response and verification approach. For shared obligations, identify the
contribution scope, responsible FEATURE or scenarios, and role responsible for
assessing the complete requirement. A FEATURE's successful local checks may be
only part of that evidence. For performance, state the measurement boundary and
any derived component budget; retain end-to-end verification of the original
target. Percentiles from separate components do not simply add up.

FEATURE section 1.2 references applicable FR/NFR IDs. Flows, algorithms and DoDs
describe its contribution. Preserve canonical section 6 Acceptance Criteria as
the feature's readable completion checklist. Insight's section 7 Testing maps
the feature's claims to vector-attributed scenarios and exact executable tests.

One requirement can have several scenarios; one scenario can reference several
requirements. Its vector names the claim being verified. Supporting assertions
can address another requirement without proving that requirement in full. Keep
the suite's one-primary-vector convention. For independent claims in different
vectors, use assertion-focused tests that share setup rather than adding several
native vector markers to one test. Reuse a test across features only when its
assertions establish each linked scenario's claim and scope.

Scenario numbers are stable local anchors. Editorial edits retain their number.
When the target, scope or expected behavior changes, reassess linked tests and
clear the implementation checkbox until the complete revised claim is mapped.
Splits and merges retain the old number's disposition and point to successors.
Evidence identifies the revision containing both specification and tests, plus
fixtures, conditions, command and result.

## Use the examples

The registered kit examples demonstrate canonical artifact structure and CDSL.
Use them when adding quality expectations to an artifact; they do not establish
product test coverage.
