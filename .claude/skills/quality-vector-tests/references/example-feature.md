# Worked trace — PRD to feature to test

This synthetic excerpt demonstrates ownership and links, not a complete feature
or evidence of a passing test. Paths and IDs are illustrative.

The PRD defines a quality obligation once:

```markdown
#### Private lookup diagnostics

- [ ] `p1` - **ID**: `cpt-example-nfr-private-lookups`

**Vector**: Security

Diagnostic logs **MUST NOT** disclose raw lookup email values.

**Threshold**: Zero raw email lookup values in successful and rejected request logs.

**Rationale**: Diagnostic access must not expose the identity being requested.
```

The FEATURE keeps its canonical requirement references and acceptance checklist,
then attaches tests to the feature:

```markdown
## 1. Feature Context

- [ ] `p1` - `cpt-example-feature-profile-lookup`

### 1.2 Purpose

**Requirements**: `cpt-example-nfr-private-lookups`.

## 6. Acceptance Criteria

- [ ] Lookup diagnostics remain useful without disclosing raw lookup values.

## 7. Testing

**Feature**: `cpt-example-feature-profile-lookup`

- [x] 1. **Private lookup logs** — Security · rust-unit — capture successful and rejected lookup logs with a synthetic email canary → the raw canary appears nowhere.
  **Requirements**: `cpt-example-nfr-private-lookups`.
  **Test**: [private_lookup_logs](../../tests/profile_logs.rs#L12).

Implementation is mapped; passing evidence still needs a tested revision,
fixture, command and result.
```

The test's existing metadata or test reference points back to the FEATURE path,
`cpt-example-feature-profile-lookup`, and scenario 1. A pytest test uses the
suite's native `@pytest.mark.security`; a Rust test records Security with its
feature reference. Neither introduces a new CFS code marker kind.

A reviewer can now navigate from the PRD obligation into the feature's declared
scope and from its Security scenario to the exact assertion, then back from the
test to its owning feature. Acceptance Criteria remain readable completion
conditions, not traceability identifiers.
