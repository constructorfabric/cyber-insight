# Identity Resolution Service

The product's authority on who a person is, served by the Rust
`identity-resolution` service (`src/backend/services/identity-resolution/`).
It resolves profiles over the append-only `persons` journal, decides whose data
a caller may see, holds the role and visibility grants the product's
authorization reads, exposes the operator correction surface, resolves logins
for the authenticator, and runs the scheduled jobs that rebuild the person
projection and publish it to the analytics warehouse.

It is **not** read-only: operator corrections, grants and the login bootstrap
all write, and the two batch subcommands rewrite the projection wholesale.

| Spec | Path |
|---|---|
| PRD | [specs/PRD.md](specs/PRD.md) |
| DESIGN | [specs/DESIGN.md](specs/DESIGN.md) |
| ADRs | [specs/ADR/](specs/ADR/) |
| Generated contract | [../openapi.json](../openapi.json) |

The identity **domain** — what counts as evidence, how a correction folds into
the journal, how conflicts are classified — is specified separately, in
[`docs/domain/identity-resolution`](../../../../domain/identity-resolution). This
set covers the service that exposes it.

## Deployment

| Path | Command |
|---|---|
| Dev (Docker Compose, default) | `./dev-compose.sh up` runs the service alongside MariaDB. Build its image with `./dev-compose.sh build identity-resolution`. |
| Dev (Kubernetes via gitops) | `cd deploy/gitops && make deploy ENV=local` installs the umbrella chart, which includes the service when `identityResolution.deploy=true`. |
| Production / staging | Standard umbrella install; set `identityResolution.deploy=true` and `identityResolution.image.tag=<release>` in the values overlay. |
| Standalone (no umbrella) | `helm install identity-resolution ./src/backend/services/identity-resolution/helm` with a pre-created `insight-identity-resolution-config` Secret. |

The umbrella emits the `insight-identity-resolution-config` Secret
automatically when `identityResolution.deploy=true`. Its keys use the
**underscored** gear segment — `APP__gears__identity_resolution__config__*`,
not the hyphenated YAML gear name — carrying the database URL derived from the
auto-generated credentials, the warehouse coordinates for the projection
rebuild, and the optional default tenant and bootstrap admin. The full field
list is in [DESIGN §4.1](specs/DESIGN.md#41-configuration-surface).

## API surface

The generated contract is the authority; the inventory with each route's gate
is [DESIGN §3.3](specs/DESIGN.md#33-api-contracts). In outline:

| Group | Routes | Gate |
|---|---|---|
| Profiles | `POST /v1/profiles`, `POST /v1/profiles/batch` | identified |
| People and visibility | `GET /v1/me`, `GET /v1/people`, `GET /v1/people/{person_id}`, `GET`/`POST /v1/visible-persons` | identified (tenant-wide roster needs admin) |
| Org chart | `GET /v1/subchart`, `GET /v1/subchart/{person_id}` | identified |
| Operator corrections | `/v1/persons`, `/v1/resolution/*` | admin |
| Grants | `/v1/roles`, `/v1/person-roles`, `/v1/visibility` | admin |
| Job journals | `/v1/persons-seed*`, `/v1/persons-sync*` | admin |
| Login bootstrap | `/internal/persons/*` — five routes, one question each | service principal, absent from the contract |

The caller, their tenant and their type come from the signed gateway token;
there is no request-time tenant fallback. `GET /health` and `GET /healthz` are
the host gear's, not this service's — they report process health, **not**
database reachability (see DESIGN §3.3).

## Local run

```sh
cargo run -p identity-resolution -- -c <config.yaml> migrate   # schema first
cargo run -p identity-resolution -- -c <config.yaml>           # server
```

`seed`, `sync` and `openapi` are the other subcommands; the first two are the
batch jobs and have no HTTP trigger.

## Tests

```sh
cd src/backend && cargo test -p identity-resolution
```

The live suites — the repository-level ones and the HTTP-level one that drives
the real route table — run when `INTEGRATION_TESTS_MARIADB_URL` is set and skip
cleanly otherwise. They are never marked ignored, because the identity CI job
runs `cargo test` without `--include-ignored`.
