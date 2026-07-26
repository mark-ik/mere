# Application Prospects Brief

**Date:** 2026-07-24
**Status:** direction brainstorm with Mark; no code task. Records the
differentiator filter, the candidate applications it surfaced, the
composition thesis, and the separators with their dispositions. Spun-out
siblings from the same session: the
[shared-engram commons brief](mere_docs/research/2026-07-24_shared_engram_commons_brief.md)
(mere side) and the pelt/knot direction doc (genet side,
[`docs/2026-07-24_pelt_knot_direction.md`](https://github.com/mark-ik/genet/blob/main/docs/2026-07-24_pelt_knot_direction.md)).

## The filter

Any application built on one capability has incumbents. The novel
applications live where three or more of the stack's differentiators
intersect, because no incumbent holds that combination. The differentiators:

- a graph data model with atomic facets (chartulary `Container` as the one
  node, per the
  [one-node facets layer map](mere_docs/technical_architecture/2026-07-18_one_node_facets_layer_map.md));
- physics fields as a graph primitive (conatus: numen defines, quint
  evaluates, seiche integrates);
- DOM-carried automatability (genet-probe: a target is drivable only via
  identity the DOM itself carries);
- p2p unicast and multicast over radio mesh and IP alike (retinue plus murm
  over p2panda, one service boundary per the
  [low-power managed network plan](mere_docs/implementation_strategy/2026-07-24_low_power_managed_network_plan.md));
- local inference and embeddings (vates, sibylla, the burn lanes);
- an owned rendering engine embeddable behind `genet-host-api`;
- a portable scene contract (sceno), already consumed by mere, isometry, and
  graphshell (see the
  [scene contract note](../crates/scenograph/design_docs/2026-07-22_scene_contract_note.md));
- a trust plane with attenuable, revocable delegation (personae).

## Candidate applications

**1. Field telemetry commons.** Sensor nodes on the mesh: the stocked boards
(Heltec V4, T114) already expose I2C/ADC/GPIO, so attachments are commodity
probes in weatherproof kits, not new board designs. A reading is one codicil
op into a telemetry engram, sized for LoRa budgets; readings land in a graph
of real places; numen fields interpolate over the spatial graph and seiche
integrates over time, so the map is live rather than a dashboard of gauges;
genet renders it; personae scopes who reads whose land; a bridging member
node replicates public engrams onward over IP. Merely sells kits and
deployments; the differentiator stays in software.
*Disposition (Mark): liked, deliberately later; builds on the mesh after
chat, calls, and the application platform. One consequence recorded now: a
sensor personality makes timer-aware Light-sleep load-bearing, noted in the
managed-network plan's D2.*

**2. Mesh knowledge commons.** A shared graph over LoRa and IP with
trust-scoped replication; chat is the incumbent ceiling on mesh networks, so
a syncing permissioned graph is an unoccupied category, and the reason a
Merely radio is more than a Meshtastic clone. Spun out to the
[shared-engram commons brief](mere_docs/research/2026-07-24_shared_engram_commons_brief.md),
whose ruling is that chat is the smallest commons: one spine, built once.

**3. Mesh stewardship console.** The owner-policy surface the managed-network
plan implies: the mesh itself rendered as a mere graph, offers and delegation
chains as edges, traffic classes animated from V8's real counters, owner
settings edited where the host already owns them. Commercial companion to the
radio business and a whole-stack demonstration in one surface.
*Disposition: endorsed direction; rides the V5-V11 receipts; no separate plan
yet.*

**4. Attenuated-authority agent workshop.** Agents as denizens through the
participant gate, driving the same DOM-identified UI humans use via
genet-probe, their work landing as journal ops with provenance, their
authority a personae delegation chain (attenuation, depth bounds), revocation
as the kill switch. Harnesses today sandbox by folder permission; this
sandboxes by certificate. Extends the
[agent harness brief](mere_docs/research/2026-07-03_agent_harness_brief.md)'s
run loop with the authority grammar and probe-driven hands.
*Disposition: endorsed; the first receipt is genet-side (an agent drives
pelt, see the genet direction doc).*

**5. Flow-native modeling.** Stocks and flows over the graph: facets carry
quantities and equations, quint evaluates, seiche integrates; a spreadsheet's
cells are a degenerate graph of this. The same machinery as telemetry pointed
at abstract quantities (budgets, ecology, production planning).
*Disposition: candidate, unscheduled.*

**6. Performing documents.** Scenograph's score, choreography, inhabited
scene ladder over the journal edit spine: any graph's history is already a
score, so replaying a document's construction as a choreographed scene yields
tutorials and post-mortems from data the stack already persists.
*Disposition: candidate; gated on the sceno freeze list in the scene contract
note.*

## The composition thesis

The prospects compose because the stack meets at three seams:

1. **`genet-host-api`**: any host embeds the engine without inheriting a
   product (the genet pelt port boundary doc, 2026-07-24);
2. **engram content classes over eidetic, murm, and retinue**: any host syncs
   content without inheriting a transport;
3. **the sceno scene contract**: spatial arrangement travels between
   products.

Products (pelt, turnstone, isometry, hocket, woodshed, graphshell endpoints)
are thin hosts wiring the three seams together. Knot documents illustrate the
composition: one document is a page in pelt over http, an engram in turnstone
over the mesh, and a placed item in a score.

## Separators, with dispositions (2026-07-24)

1. **Engine identity rides on the livery cutover** (genet's
   [cutover plan](https://github.com/mark-ik/genet/blob/main/docs/2026-07-24_livery_fullweb_cutover_and_servo_retirement_plan.md)).
   Agreed; in progress; the long pole, grind not risk.
2. **Agent-drives-pelt probe receipt.** Endorsed; queued so it does not
   compete with livery focus; done-condition recorded in the genet direction
   doc. Does not wait on fullweb.
3. **Publishing.** Gated on livery obviating stylo; then the genet publish
   rings plan applies.
4. **Pelt's daily-drive 80 percent.** Ruled as a split: engine-side fact
   surfaces land in genet components (turnstone and pelt both benefit);
   per-host persistence (history, bookmarks, sessions, settings) diverges by
   design. Detail in the genet direction doc.
5. **Text editing.** Ruled table stakes for the engine and toolkit: one
   primitive at the cambium/genet layer, three consumers (toolkit
   `text_input`, fullweb forms then contenteditable, knot editor). Detail in
   the genet direction doc.
6. **The commons spine.** Two decisions (multi-writer convergence, group
   keys) are not covered by the managed-network plan; named and homed in the
   commons brief. Blocking the day chat is scoped, not before.
7. **Scenograph freeze.** Nearest and cheapest: the intent vocabulary
   question, the `measure` module question, emphasis channels, and hit
   resolution ownership, per the scene contract note. Woodshed unblocks the
   day they land.

Nothing on the list is a research problem; all four legs are past invention,
and what remains is receipts in a defensible order. The LXMF posture was also
updated this session: see the 2026-07-24 addendum in the
[LXMF research brief](mere_docs/research/2026-07-06_lxmf_key_addressed_mail_research.md).
