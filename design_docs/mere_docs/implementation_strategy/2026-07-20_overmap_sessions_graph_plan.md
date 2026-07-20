# The overmap: sessions as a graph (planning pass)

**Date**: 2026-07-20
**Status**: **RUNGS COMPLETE 2026-07-20** (O0-O3 landed; held items dispositioned below). Executes the overmap ruling (Mark, 2026-07-19 — recorded in
the [node-dissolution facets plan](2026-07-18_node_dissolution_facets_plan.md),
"The overmap" section): sessions are container nodes in a graph one level up;
fork is node lineage at that level; the switcher becomes a graph view.
**Parents**: that ruling; [tearout G4-R](2026-06-24_tearout_gestures_plan.md)
(fork consumes the lineage edge); merecat's
[recycle-bin design](../../../../merecat/design_docs/2026-07-20_recycle_bin_athanor.md)
(the deletion pattern this mirrors at session level);
[Alembic slice D](2026-06-24_alembic_implementation_plan.md) (athanor).

## The three open questions, answered

### 1. Where does the overmap persist? — it doesn't (yet). It derives.

The overmap's truth already exists, flat: `GraphSessionManifest` carries
`root_graph_id` (the container id), `parent_session` (a lineage edge), and
`sub_graph_refs` (containment edges). Minting a stored chartulary graph now
would duplicate manifest truth into a second store with no edit that needs it.

**Ruled here: the overmap is a DERIVED view over ManifestStore.** A pure
builder maps manifests → a graph (one node per session, labeled by display
name; one lineage edge per `parent_session`; containment edges per
`sub_graph_refs`), rebuilt on manifest change. No new storage, "no dang
sidecars" (the recycle-bin doctrine, one level up).

**Promotion gate (consumer-pull):** the overmap becomes a *stored* chartulary
graph (`GraphBearing` all the way down — the container node literally bears
the session graph) only when an overmap-native edit exists that manifests
cannot express: user-drawn cross-session relations, overmap arrangements worth
persisting, or a third level of nesting. Until then, derived.

### 2. Deletion at the overmap level — the recycle bin, one level up.

Mirror the node-level ruling exactly (bin in eidetic, oven in athanor,
distill-before-forget):

- **Close/delete a session** stages a session-level record in the same eidetic
  bin merecat's slice 1 opens (a `DeletedSession`-shaped record, or simply the
  session's **graph engram** — `graph_engram::seal` already freezes a live
  graph into a content-addressed engram; that IS the distillation).
- **Recover** = thaw the engram / re-list the manifest; identity intact
  (`SessionId` + `GraphId` ride the record, the recycle bin's same-node rule).
- **Forget** = athanor's pass drops the staged record on command or schedule,
  engram baked on the way out. The stream → engram-or-delete edge of Mark's
  taxonomy, at session granularity.

**Gated on** merecat recycle-bin slice 1 (the eidetic store as an async port)
landing first — same store, same actor, second record type. Do not build a
parallel bin.

### 3. Cross-session edges — held for murm/moot.

An edge between two containers in the overmap (beyond lineage/containment) is
a *sharing* statement the moment sessions can belong to different
people/devices — which is murm/moot's seam, not a local-graph feature. Held
open on purpose; the derived overmap renders only lineage + containment until
that work names its edge vocabulary.

## Rungs (consumer-pulled; nothing lands without its consumer)

- **O0 — the derived overmap builder (merecat).** Pure fn: `&ManifestStore` →
  a kernel `Graph` (session nodes keyed by `root_graph_id` — the same id the
  `scene.*` facets hang on; lineage + containment edges). Unit-tested. Its
  consumer is O1.
- **O1 — the switcher as a graph view.** Render the O0 graph in a pane (the
  existing canvas machinery; sessions are nodes with labels). Activating a
  session-node = the existing adopt/switch path — the v0 of "navigating to a
  container." The list switcher stays until the graph view earns its keep.
- **O2 — fork draws its lineage.** Nothing beyond G4-R: `fork_session_from`
  writes `parent_session`; the derived overmap renders the new node + edge on
  the next manifest change. (This is why G4-R is sequenced after this pass —
  the fork lands with its overmap consequence already visible.)
- **O3 — session deletion through the bin.** After merecat recycle-bin
  slice 1: close-session stages engram + record; the Removed section gains a
  sessions row; athanor covers both granularities in one pass.
- **Open (unruled):** the promotion gate firing (stored chartulary overmap);
  cross-session edge vocabulary (murm/moot); whether the overmap view itself
  gets `arrangement.*` facets when stored (it should — it is just a graph).

## Progress

- **2026-07-20 (O3 LANDED — mere `3f85112`, merecat `4df6c56`; the plan's
  rungs are COMPLETE):** session deletion resolved even leaner than planned —
  **no session-level bin record at all**. `move_to_trash` relocates the whole
  session directory (graph + facets + its own per-node bin travel inside it),
  so **the manifest trash IS the removed-sessions record**; a `DeletedSession`
  record would have duplicated it into the per-session bin, which cannot even
  hold it (the bin is inside the directory being deleted). mere grew the
  inverse pair (`list_trash` / `restore_from_trash`, same-identity by
  construction, never clobbers a live dir). merecat: close is now
  shell-ordered (`Effect::TrashSession`) — **release the bin store first**
  (open fjall files block the rename on Windows; the receipt caught
  `os error 5`), trash the directory whole, adopt WITHOUT the departing save
  (a post-trash save resurrected the closed session as a zombie dir — the
  second bug the sequencing fixed). The Trail's Removed-sessions section
  derives from the trash; a row click lowers `Action::RecoverSession` and the
  recovered session adopts at its carried facet layout. Receipt
  `overmap_o3.scn` green. **Engram question resolved**: distillation belongs
  to athanor's forget (recycle-bin slice 3 — which must sweep `.trash/` and
  the per-session bins inside it), not to close; close stays light and
  lossless because the directory survives whole until the oven runs.

- **2026-07-20 (O0 + O1 LANDED — merecat `058e6aa`; G4-R had already
  pre-paid O2):** `src/overmap.rs` derives the kernel Graph exactly as planned
  (container-id identity, `mere://session/<id>` urls as the DOM-carried
  targeting key, `CopiedFrom` lineage + `CollectionMember` containment edges;
  duplicate containers collapse defensively). **Identity heal folded in**: the
  plan assumed container ids were real, but merecat's `mint_session` minted
  `GraphId::nil()` — every pre-overmap session would have collided onto one
  overmap node (and their `scene.*` facets all keyed the nil uuid).
  `mint_session` now mints real ids; `session::heal_nil_graph_ids` repairs old
  manifests at boot; adopt migrates nil-keyed scene facets onto the healed
  container once. **O1**: `PaneContent::Overmap` ("Open Overmap pane"),
  rendered on the shared `graph_canvas_swatch` leaf (the Gloss pipeline) —
  lineage generations left → right in a padded band, current session
  selected, session id as `data-key` (probe / `click-node` resolvable),
  Expand jumps to the canvas; a session-node click lowers the ordinary
  `Action::SwitchSession`. 6 tests (O0 graph shape + paint pipeline +
  click→Switch); receipt `scenarios/overmap.scn` green — after a fork the
  pane shows two container nodes joined by the lineage edge, current
  highlighted. **Remaining**: O3 (deletion through the bin, gated on merecat
  recycle-bin slice 1) and the held/promotion items above. The list switcher
  (omnibar `>`) deliberately stays until the graph view earns its keep.

### Held items — disposition (2026-07-20)

- **Stored-overmap promotion**: still gated, correctly. O1's pane is a
  derived view with an analytic layout; no overmap-native *edit* exists yet.
  First likely trigger: user-arranged overmap layout worth persisting (which
  would also answer the arrangement-facets-on-the-overmap question — it is
  just a graph, so `arrangement.*` facets on container ids in a profile-root
  facets store).
- **Cross-session edges**: still murm/moot's seam. The overmap renders
  lineage + containment only until that work names its edge vocabulary;
  nothing here blocks it, and the derived builder gains an edge family by
  adding one loop.
