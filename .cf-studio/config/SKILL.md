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
- **FEATURE** — `## 7. Testing` carries one checkbox scenario per line, each with exactly one
  vector, one suite tag, and the acceptance criterion it covers, written as do → expect.
  Author it with the `quality-vector-tests` skill in `.claude/skills/`, which owns the format,
  the vector mapping and the suite table. A vector with nothing to check says `n/a` with a
  reason.
