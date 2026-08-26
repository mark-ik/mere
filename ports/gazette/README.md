# gazette

**Gazette** is the Mere platform's directory port: who someone is, where they
can be reached, and what they publish.

Named for the official gazette, and the name carries the whole roadmap in its
three senses. A *gazetteer* is an index. To be *gazetted* is to be officially
announced, and thereby resolvable. A *gazette* is the paper you read. Those
were never three crates, which is why the resolver was promoted from the
dramatis tier to a port on 2026-08-23 rather than having one founded beside
it.

Like [castellan](https://crates.io/crates/castellan), the port splits in two:

- **the embeddable half** — contact cards, and the one recipient picker Knot,
  Moot, and Signalman all draw instead of three private lists;
- **the authority half** — resolution, feed fetching, and trust state, living
  with the resident, which is the always-on party and therefore the natural
  poller.

Reading a friend's feed reveals your interest to their host, so which
persona's network face does the fetching is a first-class setting here, not an
afterthought.

## State

**Built:** the embeddable contact Ledger projection and WebFinger resolution.
The Ledger reads contacts × selected facets, carries contributor provenance,
addresses repeated list/row/detail instances, composes recipient-picker and
Ledger clauses through `chirograph::CoordinatedSelection`, emits a semantic
table, and cites its contact and facet authorities separately through
`incipit::ShelfmarkV1`.

WebFinger resolution
([RFC 7033](https://www.rfc-editor.org/rfc/rfc7033)) — an `acct:user@host`
handle to its JRD document, with aliases and links classified into typed
peer-discovery endpoints (gemini capsules, gopher resources, misfin mailboxes,
ActivityPub actors, HTTP profile pages, and a typed catch-all). NIP-05,
atproto-did, and the moot web-of-trust directory land beside it behind the
same facade.

**Unbuilt:** attaching the Ledger and recipient picker to the live
[gaz](https://crates.io/crates/gaz) store in a host, feed polling (whose engine is
`mere-crawl`), and the reading room over extracted articles. The blocking
`reqwest` needs an async port before a resident polls with it.

The feed plan names `fleece`, but this is still dependency intent rather than a
live poll path. The implementation packet in
`genet/design_docs/2026-08-26_fleece_followthrough_plan.md` first adds a narrow
supplied-HTML-to-`Article` helper. Polling, storage, and the reading-room surface
remain unbuilt Gazette work.

## Boundaries

The boundaries are the point: not
[castellan](https://crates.io/crates/castellan) (which guards and presents
*you* — gazette finds and keeps *the other players*; two outward faces of the
dramatis tier pointing opposite ways), not [gaz](https://crates.io/crates/gaz)
(the contact store this port composes rather than replaces), not a delivery
layer (private grants, cross-service posting, and inboxes are moot and murm
territory — gazette reads what is already public), and not the highlights
(what you keep is Knot's; what memory makes of it is alembic's).

Lives in the [mere](https://github.com/merely-made/mere) workspace at
`ports/gazette`.

## License

MIT OR Apache-2.0 today; the repository moves to MPL-2.0 in the license
sweep's P1.
