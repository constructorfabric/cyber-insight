# Custom Skill Extensions

Add your project-specific skill instructions here.
These are loaded alongside the generated skills in `{cf-studio-path}/.gen/SKILL.md`.

## Quality vectors in spec artifacts

Use the [quality-vector authoring guide](kits/sdlc/guides/quality-vectors.md) when
writing or reviewing PRD quality expectations, DESIGN allocation or FEATURE
verification. It owns the definitions, improvement prompts, inheritance and
shared-evidence guidance. Vector order is a display convention; product risks
determine priority. Advisory suggestions do not add a readiness gate.

Use `quality-vector-tests` in `.claude/skills/` to author FEATURE section 7 Testing
and map exact tests back to feature-owned scenarios. It owns that format and the
suite mapping. Preserve canonical Acceptance Criteria and FR/NFR IDs. Stand API/UI
test collection continues to require one native vector marker per test.

Existing artifacts adopt this extension only within the requested scope. Agreed
requirements define expectations; record implementation disagreements for review.
The optional rollup counts declarations and does not certify coverage or results.
