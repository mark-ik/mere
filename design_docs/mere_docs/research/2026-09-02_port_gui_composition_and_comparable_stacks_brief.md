# Port GUI Composition and Comparable Stacks

**Date:** 2026-09-02
**Kind:** research brief, from an assessment Mark commissioned the same day
("review our past GUI efforts too… are there stacks like that elsewhere that
we might consider as we're redrawing lines?"). Everything in §§2–4 is verified
against the tree at mere `77a3701f052` and genet `fcb6c8fbd77`; §5 is
web-sourced and cited. Nothing here schedules code; the Distillery walk it
recommends has its own plan.
**Anchors:** the
[platform boundary and repository topology plan](../implementation_strategy/2026-09-02_platform_boundary_and_repository_topology_plan.md)
(the lines being redrawn), the
[projection/scenes direction note](../../2026-08-23_projection_scenes_and_graph_native_platform.md)
(what Cambium owns for GUIs; the nine questions), the
[suite census](../../2026-08-22_turnstone_suite_composition_and_capability_census.md)
(what earns a port; the missing composition seam; the Graphshell charter as
sharpened 2026-09-02), the
[family composition thesis](../../2026-08-12_family_composition_thesis_brief.md)
(composable views; the anti-shell test; the capability named Scenograph), the
[scenograph content catalog](2026-08-18_scenograph_content_catalog.md)
(the ten scene regimes), and turnstone's
`design_docs/2026-07-18_meerkat_harvest.md` (what meerkat was and what
carried).

## 1. The question, in Mark's terms

> the crucial thing is getting useful, interactive projections out of the
> datasets people build by doing the app patterns, and pulling relevant bits
> out. You don't even need inference for a lot of it; just deterministic,
> domain aware design. Mere is… storage, identity, networking, inference, and
> projection, all built on genet, netrender, the transports, etc… are there
> stacks like that elsewhere that we might consider as we're redrawing lines?

Three questions inside that: how a port gets a GUI out of Cambium and the
scene family; what of the earlier GUI work (meerkat above all) still carries;
and which external stacks bundle the same five concerns and where they drew
their lines. Plus the crux, which is not a GUI question at all: deterministic,
domain-aware projections out of app-pattern data.

## 2. Where the port GUI story stands

**The platform half is proven.** Eight of the direction note's nine acceptance
receipts are closed (only the two field-data receipts wait on a consumer), and
§7 of that note already rules what Cambium owns for GUIs: coordinated selection
across repeated instances; a virtualized, keyboard-navigable Matrix; Deck and
panel composition with shared selection; accessible realizations of scales,
axes, legends, thresholds, units, and filters; timeline scrubbers and
before/after controls; scene hosting with unified focus, pointer capture,
overlays, and typed events; portals across nested scene boundaries; keyboard
equivalents for direct manipulation; table, tree, or long-form alternates for
dense or spatial scenes. The boundary plan of the same date puts all of that
in Mere beside the scene family — one layer, one owner.

**The ports, measured against it (2026-09-02):**

| Port | Projection endpoint | Cambium surface | What it has instead |
| --- | --- | --- | --- |
| graphshell | eleven `ProjectionSource` impls (MereHost, Identity, Live, fixtures) | the Projection Editor over Workbench | — |
| knot | one (`ports/knot`) | through knot-document | Turnstone panes |
| castellan | none | none | `PortableCardV1` projections (chirograph types, no endpoint) |
| djinn | none | none | consumes chirograph for personal-sync resources |
| gazette | none | none | the Ledger over `sceno` + `chirograph`, static DOM |
| distillery | none | **yes**, read-only installed surface, admitted by Turnstone | its own configure/inspect binary |
| signalman | none | through Retinue | FT8's `GraphCanvasSwatch` lives in Retinue |
| moot, alembic | none | none | headless stubs |

Two ports have Cambium surfaces, two expose projections, none does both
end to end. The census §6 names the missing piece: a port cannot yet offer one
reusable surface to both its standalone host and Turnstone — pane ids are
`turnstone.*`, `BUILTIN_PANES` is static, `PaneRenderer` is a hardcoded enum.
The corrected seam (a data-only surface description; an application-owned
provider that admits a source and returns an object-safe retained session)
has Knot as its ruled forcing consumer and Distillery as its second provider.

**The scenes exist on paper.** The content catalog names ten regimes with a
founding dataset and a transfer each — Mosaic, Atlas, Tabletop, Chronicle,
Circuit, Loom, Spotlight, Rosette, Fog, Grove — and every port's data already
has a named home among them: Signalman → Atlas and Circuit ("sited radios",
"radio chains"); Distillery → Chronicle and Loom (jobs over time, parallel
streams); Castellan → Spotlight ("a contact's dossier, an entity audit");
Gazette → Matrix. Only Matrix (mer3ly, gazette) and Rosette (knot poems) have
shipped consumers.

**The gap, stated once.** No document walks any port from its dataset to its
scene to its Cambium surface to its host. The Projection Editor's seven tools —
Source, Reading, Encoding, Arrangement, Interaction, Appearance, Provenance —
are the authoring UI for exactly that walk, and it has never been walked. The
[Distillery projection walk plan](../implementation_strategy/2026-09-02_distillery_projection_walk_plan.md)
is the first.

**Rulings the assessment produced, recorded in their homes the same day:**

- **Graphshell** is a viewer and redirector, not a surface hosting content for
  interaction — except the projection itself, of which it is the preeminent
  manipulator. Reading simple structured content through Genet and Workbench
  is in scope (with Genet in the mix it is free); the browser proper, the
  engines and the taxonomy of a browser expressed through Mere, is Turnstone's.
  Knot can use Workbench too. Workbench is a component any of them takes, not
  a mark of which one is the shell. (Census §3.)
- **Scenograph** is the projection manipulator's name — the boundary plan frees
  it by dissolving the generic facade into Cambium and reserves it for the
  scene/projection editor product built with Cambium. A Mere product crate;
  Graphshell its preeminent host; present in every application with its own
  graph. This is the family thesis's anti-shell test in positive form: a
  home-page graph of app graphs, built from authenticated access to every
  application's data, customized to your taste. (Census §3, thesis §3.)
- **Alembic** folds into Distillery as a core component crate; the
  models-versus-work distinction survives as a component boundary. (Census
  §7.4; boundary plan §4.)

## 3. Meerkat, read from the harvest rather than memory

Mark's brief: "Meerkat was rough but lotta work went into it. I would be ok
with graphshell looking similar to that, really (minus a shellbar, plus a
drawer, updated to the current stack and more edits of course)."

The harvest record is precise about what died and what did not.

**Taken at the harvest:** the sprite collider-hull tracer (now
`mere::canvas::sprite_hull`); inspector row semantics (re-cut into turnstone's
`inspector_view.rs`); the scenario/self-drive vocabulary (turnstone
`scenario.rs` + genet-probe); and the ~3,000-line behavioral test corpus,
read as the spec behind the deletion matrix.

**Noted, each with a named home** — the list that matters for "graphshell like
meerkat":

- *The focus card* (`card/`, `render/cards.rs`), meerkat's third node
  representation. Ruled 2026-07-18: cards return **only as Cambium
  primitives** — a catalog `card` component any host composes, never a
  rebuild. The one meerkat idea a viewer wants most, and its home is the layer
  the boundary plan moved today.
- *Command registry vocabulary* (ids, categories, binding declarations) →
  turnstone's palette when it grows categories and keybinding surfaces.
- *Settings-as-nodes* → ruled a bust ("node settings of a settings node type"
  soup); the surviving shape is the retargeting pane, which Apparatus already
  is. Only the page catalog (what was settable) carries, as a checklist.
- Web clip, in-page find → the content lane; theming → tinct/livery; export →
  rung 8; wallet pairing → the comms rung over personae; constellation actor
  pool → Steward's data source; graph delta log → chartulary's journal;
  gnode pool and partitioned raster → superseded by canvas + forest DOM, worth
  a read if node-body perf bites; intel glue (knot completion, infer host,
  ingest, content affinity) → the intel port; idle forgetting → alembic's
  by-session eviction consumer; IME edge cases → a diff-read if bugs surface.

**Left:** `command_drain` + `shell_eval` (the imperative crossroads the Action
spine exists to prevent — doctrine 2's origin story); `sync.rs`; the
Masonry-era and hand-rolled render paths; the stitched a11y bridge; the
~2,500-line dispatch tree; the 85-module `main.rs`; the browser worker (its
compat lane became `verso-tile`).

**The pane taxonomy as revised at the harvest** (with Mark): Apparatus = the
graph-object facet analyzer; Inspector = content and content-metadata
analysis plus clipping; Steward = all operational status, sync and async;
app-level Settings = a distinct pane-shaped surface.

**What this means for Graphshell.** Turnstone already *is* the meerkat
successor shell, re-derived smaller: it registers nineteen panes today
(apparatus, inspector, steward, settings, workbench, overmap, graph, roster,
comms, publishing, trail, transcript, place, …) under that taxonomy, and it
holds the browser engines proper. "Graphshell like meerkat" therefore
resolves to: Graphshell reads structured content through Workbench and Genet,
takes the focus card as a Cambium primitive, hosts Scenograph, and puts a
drawer where the shellbar was. **"Drawer" is new vocabulary** — it appears in
no mere or turnstone document; the shellbar it replaces was an edge-docked
button bar (its edge was a setting) beside an omnibar toolbar. A definition
is Mark's to give before it is a plan; the working guess is a collapsible
edge panel holding what the shellbar and toolbar held.

## 4. The crux: deterministic, domain-aware projections out of app-pattern data

The machinery for "pull the relevant bits out" already has three deterministic
parts, none of which is inference:

1. **Readings** — chirograph selections and reconstitutive reading parameters,
   cited by shelfmark, so a projection is a checkable function of its
   authority rather than a serialization of it.
2. **Scene recipes** — the ten regimes, each a co-varying combination of
   reading, encoding, arrangement, relation form, guides or backdrops, and
   interaction toward one legible purpose, proved on a second dataset.
3. **The nine questions** of the direction note §1, which are a procedure,
   not a taxonomy: which authority elements and values are read; which
   derivation is performed; what an entity becomes; what a relation becomes;
   which geometric law places; which guides, scales, fields, or backdrops are
   required; how every mark maps back to source; which intents change what;
   which second dataset proves the recipe is not an ontology skin.

Run those per port and the binding falls out without a model in the loop.
What is missing is the artifact that holds the answer: a **scene binding** per
port — dataset pattern → recipe → surface — which is exactly what Scenograph
authors and shelfmark cites. Inference stays where the direction already put
it: an optional derivation provider *above* readings (the intel lane, esp and
vates), never something projection depends on. The comparable-stacks survey
below says why that posture is unusual and right.

## 5. Stacks like ours, and the lines they drew

| Stack | Storage | Identity | Network | Inference | Projection | The line it draws, and what to borrow |
| --- | --- | --- | --- | --- | --- | --- |
| **AT Protocol** | PDS: a user repository of lexicon-typed records | DID | Relay firehose | — | **AppView** indexes the firehose into one application's UI | *Data-in-repos versus app-views-as-a-named-layer.* AppView is Mere's projection layer with a name; Lexicon versions records **and** APIs together, which is the score/scene versioning discipline. Difference: their AppView is a big-world server; ours is per-device and granted. |
| **Holochain** | per-agent source chain plus a per-DNA DHT | agent keypair | DHT gossip | — | UI over the conductor's local RPC | ***Integrity versus coordinator zomes*** — deterministic validation split from effectful code by construction. That is "solver proposes, score records" as a crate-placement test. UI-over-local-RPC is Graphshell-over-endpoints. |
| **Urbit** | Clay, a versioned filesystem | Azimuth, on-chain | Ames, encrypted p2p | — | Gall agents; Landscape | *One OS, concerns as vanes.* The closest to "storage, identity, networking, projection as one stack." Borrow the vane discipline — each concern a kernel module with a narrow API — as the post-boundary shape; the monolith and the bespoke language are the anti-pattern. |
| **Solid** | pods (RDF, LDP) | WebID | HTTP | — | applications read and write pods directly | *Data versus app, with a universal schema.* The Drive inversion without p2p and with the up-front schema unification the family thesis rejects (a mere is a dataspace: pay-as-you-go). Little to borrow beyond the framing. |
| **Willow / Meadowcap** | namespaces as (time, path, subspace) | keys only | sideloading and a general sync protocol | — | — | *Storage and capability as one layer.* Meadowcap's delegable, attenuable, provenance-chained capabilities are the thing to compare Notochord/personae grants against; `willow-rs` exists. |
| **Spritely Goblins / OCapN** | — | petnames over global identifiers | CapTP over pluggable **netlayers** | — | — | *Capability transport over swappable substrates.* Netlayers are murm carriers; petnames are gazette's handle resolution and the insigne grammar. |
| **Anytype** | local-first objects, types, relations; CRDTs | recovery phrase | any-sync blind relays | — | graph, sets, views | The closest *UX* analog to types-and-relations-become-views, as a monolithic product. Its "set" views are scene-shaped. |
| **Plan 9** | everything is a file | — | 9P | — | per-process namespaces | *One resource protocol plus per-process composition.* Graphshell's endpoint protocol is 9P for projections; a per-persona namespace that mounts ports' projections is how a shell composes them. |

**Two findings from the table.**

- **No comparable stack carries inference as a layer.** Anytype-class
  products bolt model features on; the protocol stacks omit it. Mere's
  five-noun statement is unusual there, and the posture the direction note
  already takes — inference as an optional derivation provider above readings
  — is the one that keeps projection deterministic and receipt-able.
- **The strongest borrowings are placement tests, not code.** Holochain's
  integrity/coordinator split, Urbit's vane discipline, and AT's "AppView is
  a layer with a name" each give the boundary plan's P0 inventory a way to
  decide where a mixed crate goes: does it validate or act; does it own one
  concern behind a narrow API; is it a projection over data it does not own.

## 6. What this brief deliberately does not do

It does not schedule the Distillery walk (its plan does), does not define the
drawer (Mark's word), does not decide Athanor's authority half or the shape of
the Alembic move (open in the census §7.4), and does not re-open any scene's
categorical status (the direction note owns those).

## Sources

- [AT Protocol glossary](https://atproto.com/guides/glossary);
  [Bluesky federation architecture](https://docs.bsky.app/docs/advanced-guides/federation-architecture);
  [AppViews, AT Protocol community wiki](https://atproto.wiki/en/wiki/reference/core-architecture/appview)
- [Holochain application architecture](https://developer.holochain.org/concepts/2_application_architecture/);
  [Integrity and coordination part ways](https://blog.holochain.org/integrity-and-coordination-part-ways/)
- [Urbit: Arvo](https://docs.urbit.org/build-on-urbit/app-school/1-arvo);
  [Urbit whitepaper](https://media.urbit.org/whitepaper.pdf)
- [Solid, Inrupt](https://www.inrupt.com/solid);
  [The Solid Protocol, OpenCommons](https://opencommons.org/The_Solid_Protocol)
- [Meadowcap introduction](https://gwil.garden/posts/meadowcap-intro.html);
  [willow-rs](https://github.com/earthstar-project/willow-rs)
- [OCapN, Spritely Goblins](https://files.spritely.institute/docs/guile-goblins/0.10/OCapN-The-Object-Capabilities-Network.html);
  [Petnames in an existing chat application](https://files.spritely.institute/papers/implementation-of-petname-system-in-existing-chat-app.html)
- [Anytype's local-first architecture](https://hilton.org.uk/blog/anytype-local-first);
  [Anytype sync and backup](https://doc.anytype.io/anytype/data/sync-and-backup)
- [The use of name spaces in Plan 9](https://9p.io/sys/doc/names.html);
  [Plan 9 from Bell Labs](https://9p.io/sys/doc/9.html)
