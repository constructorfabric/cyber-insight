# ADR-0018: Mint a Login Identity Only From an Observed Roster Account, Over Single-Question Routes

<!-- toc -->

- [Context and Problem Statement](#context-and-problem-statement)
- [Decision Drivers](#decision-drivers)
- [Considered Options](#considered-options)
- [Decision Outcome](#decision-outcome)
  - [Consequences](#consequences)
  - [Confirmation](#confirmation)
- [Pros and Cons of the Options](#pros-and-cons-of-the-options)
  - [Mint from an already-observed roster account, over one route per question (chosen)](#mint-from-an-already-observed-roster-account-over-one-route-per-question-chosen)
  - [One resolve route that falls back from id to address](#one-resolve-route-that-falls-back-from-id-to-address)
  - [Refuse, and require an operator to bind every such person by hand](#refuse-and-require-an-operator-to-bind-every-such-person-by-hand)
- [More Information](#more-information)
- [Traceability](#traceability)

<!-- /toc -->

**ID**: `cpt-insightspec-adr-0018-roster-minted-login-identity`

**Status:** Accepted

## Context and Problem Statement

A person can only use the product once the journal holds a binding from their
identity-provider principal to a person. Normally the scheduled rebuild writes
that binding: it links a provider account to a person by address. Two
situations defeat it.

The first is a member the directory publishes with no address. The rebuild
skips the account, so the binding never exists and the person is refused at
every sign-in until an operator binds them by hand — which they cannot do
until someone notices the refusal.

The second is an install whose identity provider has no directory connector at
all. Nothing ever writes an account row for that provider, so a login
resolution keyed on the provider's native user id can match nobody, and every
sign-in on the install fails.

Both push toward the same tempting shortcut: when the id lookup finds nothing,
fall back to matching the address the token carries. That shortcut is the thing
to avoid. An address is not an identifier a provider guarantees — it is
reassignable, and it is stated by every source, not only the trusted one. A
silent fallback would mean a login for an unbound principal entering as
whoever last held that address, and it would be reached by the *absence* of a
field rather than by a decision anyone made.

## Decision Drivers

- Nobody may become a person merely by authenticating; the organisation's
  roster decides who exists.
- No absent or empty field may change which resolution path a login takes.
- A person the roster lists must be able to sign in without an operator acting
  first.
- Whatever is minted must be adoptable by the next scheduled rebuild rather
  than duplicated by it.
- An operator decision already recorded — in particular an exclusion — must
  survive a login attempt.
- Two concurrent first logins must produce one person.

## Considered Options

- Mint from an already-observed roster account, over one route per question (chosen)
- One resolve route that falls back from id to address
- Refuse, and require an operator to bind every such person by hand

## Decision Outcome

Chosen option: **mint from an already-observed roster account, over one route
per question.**

**One setting names the roster.** A single configured source is trusted to say
who exists. Only its accounts may cause a person to be minted without an
address to match on. Naming a second source is not supported, and neither is
naming a source that spans several connector instances: one addressless human
listed twice becomes two persons that nothing can rejoin, and the journal is
append-only, so that cannot be undone.

**Minting requires prior observation.** Provisioning succeeds only for an
account a connector has already seen, and binds it under *that observation's
own connector instance*. Both halves are load-bearing. The first makes this
"the provider authenticated somebody the organisation already lists", never
"anyone who reaches the provider becomes a person". The second is what lets the
next rebuild — which recognises an account by the whole triple of type,
instance and id — adopt this person instead of minting a second one beside it.

**Three refusals, decided before the write.** An account the source reports as
closed is refused; an account that carries an address is refused, because
ordinary resolution will link it and a mint would race that; an asserted tenant
that is not the one the journal is keyed by is refused, and validated before
the lookup rather than only before the write, so a misconfigured tenant claim
fails consistently instead of intermittently.

**The write is conditional and the answer is re-read.** The binding is appended
only if the account is still unbound, and the response comes from re-reading
what is in force rather than from what was intended. Two interleavings need
this: a racing login that wrote first, and an operator exclusion, which must
read as "no person to enter as" rather than as a fresh mint.

**Every minted person reaches an operator.** The account is queued for
confirmation and the person stays out of the merge picker until an operator
confirms or reassigns it, so a mint is a provisional answer, not a silent one.

**One route per question, not one route with a mode.** Five separate internal
routes exist: resolve by directory principal, resolve by roster address,
provision, read active roles, and the administrative view-as address
resolution. Which one the authenticator calls is decided once by its own
configuration, at the top of the login — never by what a given token happens to
carry. The separation is a security boundary rather than a naming choice, and
it is what makes the shortcut described above unreachable. It matters most for
the view-as route, which matches an address stated by any source in any tenant:
the right latitude for an operator typing a name into a view-as box, and far
too much for a sign-in.

**The roster-address route is tenant-scoped**, unlike the two any-tenant
lookups, because an address does not carry the cross-tenant uniqueness a
directory id does. The tenant is already known by the time a login reaches it.
It fails closed on every shape it cannot answer — no roster configured, no
tenant, an empty address, or an address no live account under the roster states
— and it records when several persons state the address rather than resolving
one silently.

### Consequences

- **Positive:** a person the roster lists can sign in on their own, including
  when the directory publishes no address for them.
- **Positive:** an install whose provider has no directory connector is
  supported without weakening the id-based path for installs that do.
- **Positive:** an exclusion, a merge and a race all resolve correctly, because
  the answer is read from what is in force.
- **Negative:** the roster setting is not reversible in effect. The journal is
  append-only, so minted persons and their queue items remain after it is
  cleared.
- **Negative:** on an install resolving logins by address, clearing the roster
  setting denies **every** login rather than only disabling minting. That dual
  effect is easy to miss during a configuration change.
- **Negative:** five internal routes instead of one is more surface to keep
  service-only and more for the authenticator to know about.
- **Negative:** a contested roster address is answered rather than refused. The
  install chose to resolve logins by address, so refusing would lock out both
  people; the audit record is what makes the choice visible.

### Confirmation

The domain decision layer is covered by unit tests over its refusals — closed,
addressed, tenant mismatch, empty address, unconfigured roster — with no
database involved. The live HTTP suite asserts the service-principal gate on
each route, the conditional write, and that a decided account resolves to no
person. The response shape the authenticator depends on verbatim is pinned by
a serialization test on this side and by the authenticator's own tests on the
other. Absence from the generated contract is structural: the routes are
registered outside the operation builder.

## Pros and Cons of the Options

### Mint from an already-observed roster account, over one route per question (chosen)

- **Pro:** the roster stays the authority on who exists.
- **Pro:** no absent field can reroute a login.
- **Pro:** the mint is adoptable by the next rebuild rather than duplicated.
- **Con:** an irreversible effect from a reversible-looking setting.
- **Con:** more internal surface to keep confined to service principals.

### One resolve route that falls back from id to address

- **Pro:** one route for the authenticator to call; no configuration decides
  the path.
- **Con:** the path is chosen by which field the token happens to carry, so a
  provider that omits an id silently downgrades every login to address
  matching.
- **Con:** an address is reassignable and is stated by every source, so the
  fallback can admit the wrong person.
- **Con:** the same route would then be one parameter away from the view-as
  latitude.

### Refuse, and require an operator to bind every such person by hand

- **Pro:** nothing is ever minted automatically; the queue is the only entry.
- **Con:** the person cannot sign in until someone notices they cannot, which
  is discovered as a support request.
- **Con:** does not help the install whose provider has no directory connector
  at all — there is nothing for the operator to bind to.

## More Information

The roster requirement has a second half worth stating: a source qualifies only
if it can report that a member has left. A source that cannot say so keeps
leavers minted and queued indefinitely, because nothing ever closes their
account.

## Traceability

- Routes: `services/identity-resolution/src/api/handlers.rs` (the
  `internal_person_*` handlers)
- Decision layer: `domain::login_bootstrap`
- Conditional write: `resolution_repo::append_binding_if_unbound`
- Setting: `roster_source_type` in `config.rs`; see DESIGN §4.1
- Related: ADR-0002 (read the relational journal), ADR-0003 (latest-per-source
  semantics), ADR-0011 (case-insensitive identifier comparison)

This decision addresses:

- `cpt-insightspec-fr-identity-login-by-external-id` — the id-based path and
  its refusal to fall back.
- `cpt-insightspec-fr-identity-login-by-roster-email` — the tenant-scoped
  address path and its fail-closed shape.
- `cpt-insightspec-fr-identity-login-provision` — the mint, its preconditions
  and its race behaviour.
- `cpt-insightspec-fr-identity-login-override` — why view-as is a separate
  route.
- `cpt-insightspec-fr-identity-service-principal-gate` — the gate every one of
  them shares.
- `cpt-insightspec-principle-identity-one-question-per-route` — the principle
  this decision establishes.
