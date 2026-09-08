# Custom Skill Extensions

Add your project-specific skill instructions here.
These are loaded alongside the generated skills in `{cf-studio-path}/.gen/SKILL.md`.

## Quality vectors in spec artifacts

Insight evaluates quality along five vectors — **Efficiency, Reliability, Performance,
Security, Versatility** — in that order, which is a priority ranking, not an alphabetical one.
They are the same five the stand suites enforce: every api/ui test carries exactly one vector
marker, declared in `tests/pyproject.toml` and checked at collection.

- **PRD** — every NFR in `## 6. Non-Functional Requirements` carries exactly one vector. The
  vector rides the NFR's own `cpt-{system}-nfr-{slug}` id, which already traces into DESIGN
  §1.2. Do NOT add a separate five-row summary table: the five-vector view is produced by
  grouping the NFRs, never authored alongside them, so it cannot drift from them.
  Section 6.1 also records inherited obligations with `**Vector**`, `**Inherits**` (the upstream
  NFR ID), and `**Verification**` (shared evidence and owner). Only genuinely inapplicable
  obligations belong in section 6.2; inheriting a target unchanged is not an exclusion.
- **FEATURE** — `## 7. Testing` carries one checkbox scenario per line, each with exactly one
  vector and one suite tag, written as do → expect. The owning feature ID is declared once;
  scenarios cite PRD FR/NFR IDs from the canonical section 1.2 Requirements field and link to
  exact tests when implemented. Tests cite the feature ID, path and stable scenario number.
  Keep section 6 Acceptance Criteria in the canonical kit form, without imposed AC IDs or ratios.
  Author it with the `quality-vector-tests` skill in `.claude/skills/`, which owns the format,
  the vector mapping and the suite table. A vector with nothing to check says `n/a` with a
  reason.

For new artifacts and Testing sections added through this workflow, apply the quality-vector
checks in the PRD and FEATURE checklists after CFS validation. The rollup reports declarations,
not test results; verify targets, rationale, upstream references, feature-to-test links and evidence
semantically. Existing artifacts are not required to adopt these additions until explicitly
migrated. Agreed requirements determine scenario expectations; code inspection discovers risks
and disagreements to resolve in the specification, never silently changes the expected outcome.
