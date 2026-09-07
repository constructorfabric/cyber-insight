# The data-path suite

End-to-end tests for one question: **given these rows in bronze, does the API
serve this number?** Each test seeds a fixture into the warehouse, builds the
models above it, calls `/v1/metric-results` (or the identity API), and asserts
what comes back.

It is the opposite of `tests/stand`, which reads an already-seeded stand and
writes nothing. This suite owns its instance: it seeds, it clears between
specs, and it must never be pointed at a stand another suite is using. The
shared library it imports (`insight_datapath`) lives in `../lib`; both are one
uv project (`tests/pyproject.toml`).

## Layout

| Path | Contents |
|---|---|
| `conftest.py` | The instance a run reads and writes, and the caller it asks as. |
| `metrics/` | One directory per metric class — `ai`, `ci`, `collab`, `git`, `tasks`, `wiki`. A spec is a `*.test.yaml` fixture plus the module beside it. |
| `metrics/conftest.py` | The `spec` fixture, the assertion ledger, and the module-to-fixture contract. |
| `metrics/templates/` | Shared fixture fragments the specs pull in by `$ref`. |
| `metrics/schemas/` | One schema per bronze table, checked against every fixture row before it is seeded. |
| `identity/` | Person resolution, account binding and the journal — no metric responses. |
| `meta/` | The library's own tests: the loader, the expectation rules, the ownership guard, the reset probe. |

## Running it locally

Everything goes through `./dev-compose.sh test-stand`, from the repository
root. Give the stand an instance name so it does not collide with a stand you
already have up:

```bash
./dev-compose.sh test-stand minimal --instance=datapath        # bring up just what the data path needs
./dev-compose.sh test-stand test --instance=datapath --tree=tests/datapath/metrics/git
./dev-compose.sh test-stand test --instance=datapath --tree=tests/datapath/meta
./dev-compose.sh test-stand down --instance=datapath
```

`--tree=` takes a **directory**, never a file. To run one module, pass its
directory and select with `-k`:

```bash
./dev-compose.sh test-stand test --instance=datapath \
  --tree=tests/datapath/metrics/git -k default_branch_lines_added
```

The `meta` tree needs a stand but seeds no fixture data, and runs in well
under a minute — run it first when you change anything in `../lib`.

## The rules

Five rules govern a test here. The last column is the one that matters: a rule
a machine enforces cannot rot, and a rule only convention enforces will.

| Rule | Why | Enforced by | Breaking it looks like |
|---|---|---|---|
| One metric per key, one view per kind, one row per selector | A duplicate row is a defect, not a pick-the-first | `metric_expect.py` — the three selectors | `find {...} matched 2 rows (expected exactly 1)` |
| A selected row has its view's required fields asserted before the case ends | A peer test checking one field of seven is green and proves almost nothing | the completeness check in `metric_expect.py` | `... row leaves ['n', 'p25', ...] unasserted` |
| Coverage is booked when a test asserts, never when it selects | Reading a view without checking anything in it must earn nothing | the recorder callback on each assertion method | the gate drops below its metric count |
| Every fixture file is claimed by exactly one module | An unclaimed fixture is silently inert — it never runs and nobody notices | `meta/test_spec_ownership.py` | that guard fails, naming the orphan |
| Row methods for one row's fields; plain pytest for everything else | — | **convention only** | nothing fails; a reviewer catches it or nobody does |

### How a test reaches its fixture

A module declares the fixture it owns with a module-level constant:

```python
SPEC = "git_default_branch_lines_added"
```

The `spec` fixture reads that constant and loads `<name>.test.yaml` from the
directory the module sits in. There is no discovery by filename and no
implicit pairing — **a fixture nothing declares is never loaded**, which is
why the ownership guard exists. It checks three things: every fixture is
claimed, no fixture is claimed twice, and every claim names a fixture that
exists beside its module.

Fixtures compose through `$ref`, resolved relative to the fixture's own
location:

```yaml
bronze_bamboohr.employees:
  - $ref: ../templates/people.yaml#/templates/erin
```

### The assertion rules

Six rules, all in `metric_expect.py`:

1. **Exactly one match, everywhere.** Selecting a metric, a view or a row
   raises unless exactly one thing matched. Zero and two are both errors.
2. **Numbers compare within rel 1e-9 / abs 1e-6**, never by `==`, because the
   warehouse computes ratios. `bool` is excluded from the numeric path.
   `approx()` applies the same tolerance to an assertion written outside a row
   method.
3. **A selector is a subset match.** It checks only the keys it names, and
   against a *list* it succeeds when any element matches — which is how a
   dimension selector picks one tuple out of a row's dimension list.
4. **Touch a row, owe its view's required fields.** A period row owes `value`;
   a peer row owes all seven of its statistics; a timeseries row owes `points`;
   a breakdown row owes `value`; a rollup row owes `value` and its contributing
   entity count; a histogram row owes `bins`. Checked at the end of the case.
5. **Coverage is booked on assertion, not selection.** Every assertion method
   books its view. Selecting a row books nothing. Asking for a whole view's
   entries is the one exception — returning that list *is* the assertion
   surface, so it books on access.
6. **Row methods for one row's fields, plain pytest for everything else** — the
   HTTP status, the length of a list, a rule over every entry, a point inside a
   series, and anything that is not a metric response at all.

Rule 4 has a deliberate escape hatch. Selecting **every** row of a view rather
than one returns rows that are not held to the completeness check, for a rule
over the whole list:

```python
resolution = float(one(r.rows("tasks.resolution_time", "period"), entity_id=CAROL)["value"])
assert 23.5 < resolution < 24.5
```

Reach for it when a range or a shape is the claim. Do not reach for it to
avoid asserting fields you should be asserting.

### Where plain pytest belongs

Most assertions in this suite are ordinary `assert` statements, and that is by
design — the row methods exist for the one thing that needs bookkeeping. Plain
pytest carries the HTTP status, the length of a served list, ranges and
inequalities, rules over every entry, points inside a series, and the whole of
the `identity` and `meta` trees, which never see a metric response.

The pure helpers — the subset matcher, the one-and-only-one selector, the
tolerance wrapper — are free functions with no state, so a plain assert can use
them too:

```python
assert float(one(active, bucket_start="2026-01-05")["value"]) == approx(1.0)
```

## The coverage gate

Each run writes an assertion ledger — which metric and view each test asserted
— to `.artifacts/metric_assertions.json`. The gate merges every ledger against
the builtin metric registry the suite read from analytics, and fails unless
every builtin metric has every supported view asserted somewhere:

```bash
python3 tests/lib/insight_datapath/metric_coverage.py \
  --universe-file coverage-inputs/metric_definitions.json \
  --ledger coverage-inputs/metric_assertions-*.json
```

Because it takes several ledgers, the shards need not each be complete — only
their union.

## In CI

`.github/workflows/e2e-bronze-to-api.yml` runs the suite as five shards —
`ai`, `git`, `tasks`, `rest`, `identity` — each on its own stand instance.
`rest` takes every metric class the named shards do not, discovered from the
tree, so a new metric class can never silently fall out of the run. Each shard
uploads its ledger; the coverage gate is a separate job that downloads all of
them.

On a pull request the umbrella check reports a deferral and the shards do not
run; they run in the merge queue and on manual dispatch. To exercise a branch
before merging, dispatch it:

```bash
gh workflow run e2e-bronze-to-api.yml --ref <your-branch>
```

## Writing a new spec

1. Add `metrics/<class>/<name>.test.yaml` — the bronze rows, plus a
   description that says what the fixture proves and how the numbers arise.
   Keep every value synthetic.
2. Add `metrics/<class>/test_<name>.py` beside it with `SPEC = "<name>"`, and
   write one test per claim. Name the test after the rule it proves, not the
   mechanics.
3. Run the `meta` tree — the ownership guard tells you immediately if the two
   are not paired.
4. Run the class tree and confirm the numbers.

A number in an assertion should be traceable to the fixture that produced it.
Prefer a figure the fixture makes inevitable — a sum of rows you seeded — over
one you copied from a passing response, which pins whatever the code did rather
than what it should do.
