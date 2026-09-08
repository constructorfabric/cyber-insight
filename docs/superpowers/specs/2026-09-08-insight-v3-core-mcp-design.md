# An MCP server on insight-v3-core

**Goal:** an MCP client — Claude Code, or any other — authors metrics, widgets and
dashboards in `insight-v3-core`, runs a metric to see its rows, and discovers what
tables exist, over the same definitions the portal reads.

**Why:** [PRD.md](../../domain/insight-v3/specs/PRD.md) asks for the "ability to use
your own AI (LLM, Claude) via CLI/MCP" alongside easy creation of metrics, widgets and
dashboards. The definition endpoints already exist and are declarative JSON; what is
missing is a transport an MCP client can speak and a credential it can hold.

**Scope:** authoring, reading and running. The ingest surface (`/v1/tables/{table}`,
`/v1/raw-data`) and the chat endpoint (`/v1/chat`) are deliberately absent — ingest is
gated by a separate token, and an MCP client is already a model, so a second one in the
loop buys nothing.

**Relationship to the analytics MCP server:** unchanged and untouched. It keeps
`query_sql` on `/mcp` for raw warehouse exploration. This is a second server on
`/mcp/v3`, and a client that wants both configures both.

---

## 1. Placement: a listener of its own

The MCP server runs on its own axum listener inside the `insight-v3-core` process, on
its own port, sharing the gear's `AppState`. It is not mounted on the gear's REST
router.

This is forced, not chosen. The gears api-gateway applies its authn middleware as a
single global layer and resolves the requirement per path from the OpenAPI operation
specs; a path with no matching spec falls back to `require_auth_by_default`, which
defaults to true. A service nested onto the gear router therefore inherits a demand for
the gear's own ES256 `internal-services` bearer, and an MCP access token — issued by the
authenticator for a different audience — is rejected before rmcp sees the request. The
`route_policies` config cannot relax this; it only adds required scopes.

A separate listener also keeps the two auth models physically apart: the REST routes
trust the gateway's session exchange, the MCP listener verifies a bearer itself.

**Port:** `mcp.bind_addr`, defaulting to `0.0.0.0:8087`. The gear's REST listener stays
on `8086`.

**Lifecycle:** the listener is spawned during gear `init` and shuts down on the
toolkit's cancellation token, so an in-flight streamable-http session ends with the
process rather than outliving it.

**Disabled by default:** `mcp.enabled` is false unless configured. A deployment that has
not set a public URL has no way to verify a token, and must not open the port.

## 2. Authorization: the token is the proof of admin

Verification mirrors the analytics MCP server: JWKS fetched from the gateway's
`.well-known/jwks.json`, issuer pinned to the gateway public URL, audience pinned to
this server's resource URL, cooldown on JWKS refresh, and a challenge response carrying
`WWW-Authenticate` with the protected-resource metadata URL.

Two values are new:

| | analytics `/mcp` | v3-core `/mcp/v3` |
|---|---|---|
| resource | `{public}/mcp` | `{public}/mcp/v3` |
| scope | `mcp:query` | `mcp:author` |

**The admin decision comes from the token, not from the identity service.** The REST
handlers call `require_admin`, which forwards the caller's `Authorization` to identity
`GET /v1/me` and reads the roles off the answer. That cannot work for an MCP caller: the
gateway forwards an MCP bearer through unchanged, and identity expects the internal
ES256 `internal-services` JWT that the gateway mints from a session cookie. The
round-trip would fail, and `require_admin` correctly treats a role check it cannot make
as a refusal rather than a permit — so every MCP write would be refused.

Deciding from the token is sound because the authenticator issues an MCP grant only to
an active administrator and re-verifies that role on every refresh. A validly issued,
unexpired, correctly scoped MCP access token therefore *is* evidence of an
administrator, and no second opinion is available to ask.

The REST path keeps `require_admin` exactly as it is. Nothing about the portal's
authorization changes.

## 3. The authenticator learns a set of resources

Today the OAuth endpoints accept exactly one resource and one scope, and reject anything
else with `invalid_target` or `invalid_scope`. Both become sets, and the pairing between
them is enforced: a grant for `/mcp` may carry only `mcp:query`, a grant for `/mcp/v3`
only `mcp:author`. A client cannot obtain authoring scope against the read-only server
or the reverse.

Protected-resource metadata is served per resource, so each server advertises its own
document. Authorization-server metadata advertises the union of supported scopes.

Grants stay per-resource: a client wanting both servers authorizes twice, once per
resource. This follows from binding a token's audience to a single resource, and is not
a defect to design around.

Nothing about grant lifetime, refresh rotation, storage or revocation changes.

## 4. The tools

Eight, each a thin skeleton over an existing domain call.

| Tool | Effect |
|---|---|
| `list_definitions(kind)` | names of every stored metric, widget or dashboard |
| `get_definition(kind, name)` | one definition's stored body |
| `put_metric(name, body)` | create or replace a metric |
| `put_widget(name, body)` | create or replace a widget |
| `put_dashboard(name, body)` | create or replace a dashboard |
| `delete_definition(kind, name)` | remove one, subject to the existing referent checks |
| `run_metric(name)` | compile and run a stored metric, returning its rows |
| `list_tables()` | every visible database and table, with columns and layer |

The three `put_*` tools stay separate rather than collapsing into one
`put_definition(kind, body)`. A metric body and a widget body have little in common, and
a single tool would have to take an untyped object — losing the per-kind JSON schema that
tells a client what shape to send. Three tools with three schemas is the difference
between a client writing a valid definition first time and guessing.

Validation is reused, never restated. A widget is checked against its metric's produced
columns by the existing check, and a delete consults the existing referent lookup, so a
definition written over MCP is subject to exactly the constraints a definition written
through the portal is.

`list_tables` serves the existing catalogue — database, table, layer, columns — which is
already cached, so a client exploring schemas does not re-read `system.columns` per
question.

**Errors:** a refusal returns the domain error's public message as an MCP tool error, not
a transport failure. A client should be able to read "widget names a column its metric
does not produce" and fix its next call.

## 5. Gateway and deployment

The gateway gains a route for `/mcp/v3` pointing at the new port, in bearer-passthrough
mode and gated on MCP being enabled, in both the Helm values and the compose route file.

One gateway change is not additive: the protected-resource metadata URL used to build a
`401` challenge is currently a single configured value. It becomes per-route, so a
challenge from `/mcp/v3` advertises the `/mcp/v3` metadata document rather than the
`/mcp` one. A client that follows the challenge to discover where to authorize would
otherwise be sent to the wrong server.

Chart values gain the enable flag, the public URL and the port for the new server,
mirroring how the analytics MCP server is configured.

## 6. Testing

**Unit, in the service:** the verifier refuses a token with the wrong audience, the wrong
issuer, the wrong scope, or an expired claim, and accepts a well-formed one. Each tool
has a happy path and its characteristic refusal. The widget-column check and the delete
referent check are exercised through the MCP path, not only the REST one, so a future
edit cannot leave MCP writes unvalidated.

**Unit, in the authenticator:** a resource outside the set is refused; a scope outside
the set is refused; a scope valid for the other resource is refused for this one; each
resource's metadata document names its own resource and scope.

**End to end:** a shell test in the service's existing style — authorize, put a metric,
put a widget that draws it, put a dashboard holding that widget, run the metric, read the
rows, delete in dependency order. Run against a live local stack, not written and left
unrun.

All fixtures are synthetic.

## 7. Out of scope

Per-user or per-tenant definitions — the definition tables are keyed by name alone, and
that does not change here. Ingest over MCP. Chat over MCP. Alerts, which the PRD lists
but which have no endpoint yet. Retiring the analytics MCP server.

## 8. Known defect, not addressed here

The two deployment paths disagree about how `insight-v3-core`'s REST routes are
authenticated: the Helm route uses instance-token passthrough, the compose route uses the
default session exchange, and the prefixes differ between them. Instance-token
passthrough clears the cookie and forwards `Authorization` unchanged, so the session
cookie the definition routes are documented to rely on does not arrive, and
`require_admin` has nothing to forward. This predates this work and is left alone; the
MCP listener does not depend on either path.
