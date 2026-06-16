# Lane 0 — Sidequest Sprint

**Date**: 2026-06-16
**Status**: In progress.
**Scope**: The Tier-A "glue-only" wins from the
[in-the-wings audit](../research/2026-06-15_in_the_wings_and_browser_bar_audit.md) §2/§9 —
capabilities whose substrate is built and whose meerkat dependency edge already exists, so each is
a `Command` + thin host method, no new dep, no model change. Real user-visible wins with zero
architecture churn.
**Related**: the audit doc (the source list), the 2026-06-16 cross-agent handoff (below).

---

## Handoff context (2026-06-16, from the serval-layout + render-lane agent)

What is **already done** in-repo (do not re-do): `wry.web` deleted (51504fc); favicons + human
`node_display_label` + caption-beside tiles; HTML-lane band scroll + inline-link nav; the
**tiled-render foundation** (Lane 2) is *theirs and in progress* (windowed document render Phase 1,
`c2ddff8`; the retained-text plan doc). **Do not touch** Lane 2 / serval-layout / card-render
areas, or the serval threads that agent owns (image-only inline links, per-band re-harvest caching,
serval static-layout fidelity, the HTML-lane retained-packet → find-in-page/selection question).

**Blocked, needs cross-repo coordination:** adopting the built serval input seams (`on_wheel`
view-routing, transform-aware hit-test, pointer-cancel, keyboard escapes) is gated on the
external-texture element view in xilem-serval (unstarted, that agent's repo). Coordinate before
adopting.

**Correction to the audit:** the forme parked-submodule delete (Lane 4 cleanup) is **not
mechanical** — chrome's `frame_model.rs` consumes the parked `tree`/`layout` types
(`OwnedTreeRow`/`SplitBoundary`/`TabEntry`). Scope any deletion to the genuinely-dead submodules
(graphlet/lens/parity/pressure/reconciliation) or migrate chrome first.

---

## Sidequests (done-conditions, not dates)

Each reachable from a normal affordance (palette + omnibar at minimum), not only by typing a verb.

1. **JSON-LD graph export** — `Command::ExportGraph` writes `linked_data::to_jsonld_string(graph)`
   to a file and echoes the path. `linked-data` is already a meerkat dep; ingest is wired, export
   was dormant. *Done when* a user can export the focused graph to JSON-LD and is told where it
   landed. *(First cut writes to `<mere_root>/exports/`; a native Save-As dialog is a follow-up.)*
2. **Recover a deleted node** — a context action re-mints an `eidetic::DeletedNode` tombstone
   (it already carries url + title + tags) back into the graph. The Trail › Removed rows are
   currently inert. *Done when* a removed node can be restored from the Trail pane.
3. **Relation-kind picker** — a submenu over the existing `command_drain.rs:25` string→`SemanticSubKind`
   map on the two-node-selection context menu, so a point-and-click edge can be Cites/Supports/
   Contradicts/… instead of always `UserGrouped`. *Done when* the kind is choosable without the
   omnibar.
4. **Tessera score + ticket on the chip** — fold the synced tessera log to a `Ledger::score` and
   surface the dialable ticket (today only `tracing::info!`-logged) in the sync chip. *Done when*
   the chip shows earned standing and the user can read/copy their ticket from the UI.
5. **Trail shellbar button** — add a Trail toggle to the shellbar (and/or a key) to match the other
   panes (today palette/`>trail`-only). *Done when* Trail has a direct affordance.
6. **Barnes-Hut repulsion** — add `BarnesHutRepulsion` to the live gyre simulation (a one-line
   `add_force` plus tuning). *Done when* large graphs lay out on the O(n log n) path.
7. **Steward per-row controls** — wire the Steward pane rows to the existing
   retry/stop/pin `node_ops` methods so a live operation is controllable in-pane, not only via the
   palette. *Done when* a Steward row's controls act on its operation.

---

## Progress

- **2026-06-16** — Plan written from the audit's Tier A + the cross-agent handoff. Starting with
  #1 (JSON-LD export) and #2 (recover-deleted-node): the cheapest, and #2's data model was shaped
  for exactly this.
- **2026-06-16** — **#1 JSON-LD export DONE.** `Command::ExportGraph` (palette "Export graph
  (JSON-LD)" + `>export_graph`) writes `linked_data::to_jsonld_string(focused graph)` to
  `<mere_root>/exports/graph-<unix-secs>.jsonld` and echoes "Exported N node(s) → the file path". New
  `export.rs` module + host-action drain arm + the five exhaustive `Command` matches (enum / ALL /
  is_host_action / verb / label) + `run_command`. `cargo check -p meerkat` clean (21s); 12
  command-spine tests green incl. the verb-uniqueness guard. Follow-ups: a native Save-As dialog and
  a menu/shellbar affordance (palette + omnibar suffice for now). Next: #2 recover-deleted-node.
- **2026-06-16** — **#2 recover-deleted-node DONE.** The Trail › Removed rows are now clickable
  buttons keyed `recover:<node_id>`; a click re-mints that eidetic tombstone (url + title + tags)
  back into the focused graph and selects it. New orrery `recover_node(url, title, tags)` (mint +
  restore title via `set_node_title` + tags via `insert_node_tag`, kept inside the orrery where
  graph mutation belongs); meerkat `recover_deleted_node` (find tombstone by node_id, recover,
  persist); a Trail-pane click block + `trail_leaf_rect` (mirroring the apparatus pane, the only
  list pane that had click wiring). `cargo check -p meerkat` clean (17s); new orrery test
  `recover_node_re_mints_with_restored_title_and_tags` green. Follow-ups: prune the tombstone on
  recover (today it is left, and re-mint is duplicate-friendly per the node-identity model); a
  right-click "Recover" on the row as an alternative to the whole-row button.
