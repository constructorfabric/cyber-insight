# The data-path suite

End-to-end tests for one question: **given these rows in bronze, does the API
serve this number?** Each test seeds a fixture, builds the models above it,
calls the API and asserts what comes back.

The opposite of `tests/stand`, which reads an already-seeded stand and writes
nothing. This suite owns its instance — it seeds and clears between specs, so
never point it at a stand another suite is using. Its library, `insight_datapath`,
lives in `../lib`; both are one uv project (`tests/pyproject.toml`).

| Path | Contents |
|---|---|
| `metrics/` | One directory per metric class. A spec is a `*.test.yaml` fixture plus the module beside it. |
| `metrics/templates/` | Fixture fragments the specs pull in by `$ref`. |
| `metrics/schemas/` | One schema per bronze table, checked against every row before it is seeded. |
| `identity/` | Person resolution, account binding, the journal — no metric responses. |
| `meta/` | The library's own tests, including the ownership guard. |

## Running it

Give the stand an instance name so it does not collide with one you already
have up:

```bash
./dev-compose.sh test-stand minimal --instance=datapath
./dev-compose.sh test-stand test --instance=datapath --tree=tests/datapath/metrics/git
./dev-compose.sh test-stand down --instance=datapath
```

`--tree=` takes a **directory**, never a file — narrow with `-k`:

```bash
./dev-compose.sh test-stand test --instance=datapath \
  --tree=tests/datapath/metrics/git -k default_branch_lines_added
```

`meta` seeds no fixture data and runs in under a minute. Run it first when you
change anything in `../lib`.

## The rules

The last column is the one that matters: a rule a machine enforces cannot rot,
and a rule only convention enforces will.

| Rule | Enforced by | Breaking it looks like |
|---|---|---|
| One metric per key, one view per kind, one row per selector | the three selectors in `metric_expect.py` | `matched 2 rows (expected exactly 1)` |
| A selected row has its view's required fields asserted | the completeness check | `row leaves ['n', 'p25', ...] unasserted` |
| Coverage is booked when a test asserts, never when it selects | the recorder on each assertion method | the gate drops below its metric count |
| Every fixture is claimed by exactly one module | `meta/test_spec_ownership.py` | that guard fails, naming the orphan |
| Row methods for a row's fields, plain pytest for the rest | **convention only** | nothing fails |

A duplicate row is a defect, not a pick-the-first. A peer test checking one
field of seven is green and proves almost nothing. Reading a view without
checking anything in it must earn no coverage. A fixture no module claims is
never loaded, and nobody notices.

### Assertion rules

1. **Exactly one match, everywhere.** Zero and two are both errors.
2. **Numbers compare within rel 1e-9 / abs 1e-6**, never by `==`, because the
   warehouse computes ratios. `bool` is excluded. `approx()` applies the same
   tolerance outside a row method.
3. **A selector is a subset match** — named keys only, and against a *list* any
   matching element wins, which is how a dimension selector picks one tuple out
   of a row's dimension list. Numbers in a selector carry rule 2's tolerance.
4. **Touch a row, owe its view's required fields.** A period row owes `value`;
   a peer row all seven statistics; a timeseries row `points`; a breakdown row
   `value`; a rollup row `value` and its contributing entity count; a histogram
   row `bins`. Checked at the end of the case.
5. **Coverage is booked on assertion, not selection.** Asking for a whole
   view's entries is the exception — that list *is* the assertion surface.
6. **Row methods for one row's fields, plain pytest for everything else** — the
   status, list lengths, ranges, rules over every entry, points inside a
   series, and the `identity` and `meta` trees, which see no metric response.
   Most assertions here are ordinary `assert` statements, by design.

Rule 4 has an escape hatch: selecting *every* row of a view returns rows that
are not held to the completeness check, for a rule over the whole list.

```python
resolution = float(one(r.rows("tasks.resolution_time", "period"), entity_id=CAROL)["value"])
assert 23.5 < resolution < 24.5
```

Use it when a range or a shape is the claim, not to skip fields you should be
asserting.

## Writing a spec

A module declares the fixture it owns, and the loader reads `<name>.test.yaml`
from the module's own directory:

```python
SPEC = "git_default_branch_lines_added"
```

There is no discovery by filename. Fixtures compose through `$ref`, resolved
relative to the fixture's own location:

```yaml
- $ref: ../templates/people.yaml#/templates/erin
```

Name a test after the rule it proves, not the mechanics. A number in an
assertion should be traceable to the fixture that produced it — prefer a figure
the fixture makes inevitable over one copied from a passing response, which
pins whatever the code did rather than what it should do. Run `meta` to confirm
the pairing, then the class tree to confirm the numbers.

## The gate and CI

Each run writes an assertion ledger to `.artifacts/metric_assertions.json`. The
gate merges every ledger against the builtin metric registry and fails unless
each metric has every supported view asserted somewhere:

```bash
python3 tests/lib/insight_datapath/metric_coverage.py \
  --universe-file coverage-inputs/metric_definitions.json \
  --ledger coverage-inputs/metric_assertions-*.json
```

It takes several ledgers, so shards need not each be complete — only their
union. CI runs five: `ai`, `git`, `tasks`, `rest`, `identity`, where `rest`
takes every class the others do not, discovered from the tree. The shards run
in the merge queue, not on a pull request; dispatch a branch with
`gh workflow run e2e-bronze-to-api.yml --ref <branch>`.
