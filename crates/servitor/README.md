# servitor

Capability-scoped resident helpers for graph applications: a **denizen** holds a
scoped structural capability and proposes changes through a validating gate,
attributed and revision-checked.

A denizen is anything admitted to act on a graph, a resident helper (a
servitor), a script, a scenario runner, a remote peer, an agent. It holds a
keyholder identity and a capability, and proposes **petitions** that the gate
validates and commits. A denizen's inner world is an ordinary
`chartulary::GraphLog`, the nested graph a graph-bearing node points at; wiring
a host-graph node to bear it is the host's job.

Headless and app-agnostic. Depends on the dependency-free `mere-capability`
leaf for the shared algebra, `chartulary` for the nested-graph substrate, and
`personae` (imported as `identity`) for the signed delegation grammar.

## Modules

| Module | Public items |
| --- | --- |
| root | `Subject` (a 32-byte public key; `new`, `from_hex`, `to_hex`, `to_author`) |
| `cap` | compatibility re-exports of `Cap`, `Capability`, `ScopePath`, `FacetNamespace`, `CapError`, `assert_capability_laws` from `mere-capability` |
| `grant` | `Grant`, `Mode`, `AuthorityProvider`, `GrantTable` |
| `gate` | `Gate`, `GateError`, `read_projection`, `GRANT_PREFIX`, `PROJECTION_TAG`, `PROJECTION_MEDIA_TYPE` |
| `delegation` | `DelegationTable`, `ChainError`, `cap_path`, `mode_action`, `mode_actions`, `scope_for`, `root_certificate`, `DENIZEN_DOMAIN` |

## Capabilities

`Cap` is a type with a coverage order, not a string with a prefix test. It has
three shapes:

- **Power**: a named member of a closed set (an app's rings). Coverage is
  equality, so adding a new power name cannot widen a grant already issued.
- **Scope**: a `ScopePath`, an unbounded hierarchy compared per segment, so
  `app/nav` does not cover `app/navigate`. `ScopePath::parse` rejects `.` and
  `..`, and an empty path is the root scope.
- **Facet**: a `FacetNamespace`, compared by dot segment, so `web.` covers
  `web.viewer` while refusing `website.viewer` and `denizen.binding`.

Cross-kind coverage is always false. `Capability::covers` must be reflexive and
transitive; `assert_capability_laws` checks that over a sample rather than
trusting it.

`Grant` is `{ subject, cap, mode, expires_at_ms }`. `Mode` is ordered
`Read < Write < Delegate`. Expiry is judged against a host-set clock
(`GrantTable::set_now`); the crate never reads a clock of its own.

## Authority and the gate

`AuthorityProvider::covers(subject, needed, mode)` is the replaceable read
boundary, mirroring `gemot::MootAuthorizationProvider`. `GrantTable` is the
minimal implementation, a flat list of grants answered by each grant's
capability order. The richer provider (meadowcap-shaped structural caps over
graph-cluster-derived namespaces, layered with policy facts) drops in without
the gate changing.

`Gate::petition(provider, nested, subject, claimed, expected, specs)` runs one
pipeline: refuse specs that touch a grant projection, check authority at
`Mode::Write`, check every touched node id falls under the claimed scope by
segment, require a covering facet capability for every `SetFacet` or
`RemoveFacet`, then commit through chartulary's attributed revision-checked
batch. It returns `Committed` or a `GateError` of `Unauthorized`,
`OutOfScope`, `UnauthorizedFacet`, `TouchesProjection`, or `Commit`.

`Gate::project_grant` writes a grant into the nested graph as a read-only
projection node (id `grant:<cap wire form>`, tagged `grant-projection`, media
type `application/vnd.mere.grant+json`). `read_projection` reads it back, or
returns `None` if the node is not fully understood.

## Delegation

Certificates, attenuation, chains, and revocation live in `personae`. This crate
contributes the typed capability view over the same certificates, so the denizen
tier and the moot tier share one delegation system. `DelegationTable` holds a
root key, adopts `SignedDelegationCertificate`s, revokes by `DelegationId`, and
verifies chains (`verify_chain`, `ChainError`). `cap_path`, `mode_action`,
`mode_actions`, and `scope_for` map a `Cap` plus `Mode` into a
`CapabilityScope`.

## License

MIT OR Apache-2.0.
