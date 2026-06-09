# System Diagnostics and Accessibility Plan

**Date**: 2026-06-08
**Status**: Planning. Follow-on to the apparatus pane/theme switcher pass.
**Related**: [apparatus pane + runtime theme switcher](2026-06-08_apparatus_pane_and_theme_switcher_plan.md), [frame tree in meerkat](2026-06-08_frame_tree_in_meerkat_plan.md), [peripheral panes architecture](../technical_architecture/2026-06-06_peripheral_panes_architecture.md), [Graphshell harvest brief](../research/2026-05-17_graphshell_harvest_brief.md), [Graphshell docs full harvest](../research/2026-05-27_graphshell_docs_full_harvest.md), [spatial chrome IR brief](../research/2026-05-15_spatial_chrome_ir_brief.md), [spatial chrome modular adoption plan](2026-05-15_spatial_chrome_modular_adoption_plan.md).

Build one host observability spine for **diagnostics, tracing, UX events,
UxTree/accessibility, probes, and agent harnesses**. The near-term user surface is
the **Apparatus** pane. The long-term consumers are tests, OS accessibility
bridges, and a Hermes/Burn-style agent harness that can drive the application
through typed observations and actions rather than raw pixel puppetry.

---

## Findings

### What is already live enough to use

- `crates/system/ux-events` is the right UX-event seam. It already defines
  observer/probe hooks, apparatus-facing diagnostics bridging, command-surface
  telemetry, and a stable place for user-facing event semantics distinct from
  low-level tracing.
- `crates/system/registry/register-diagnostics` is useful as the channel catalog,
  descriptor/config layer, sampling policy, invariant store, and portable emit
  scaffold. It should configure and classify diagnostics; it should not become
  the live UI state store.
- `crates/forme/uxtree` already produces deterministic accessibility-shaped
  trees and can stitch subtrees into a `TreeUpdate`. This is the bridge between
  Mere's semantic surfaces and AccessKit.
- `crates/shell/frame` now gives meerkat a real pane tree. Its projected leaves
  are the natural unit for diagnostics, focus, accessibility bounds, and agent
  action targets.
- `crates/platen/domain/apparatus` is a useful domain skeleton for an
  accessibility projection, but the current rendered pane belongs in `meerkat`
  like roster/gloss/comms. The domain crate should not be mistaken for a full UI.
- `crates/shell/chrome` still has valuable Graphshell-era shell semantics:
  toolbar, omnibar, command palette, focus authorities, host intents, routing
  hints, and the eventual `project_chrome(state) -> UxTree` direction.
- `crates/system/registry/register-input` has useful action/binding/conflict
  vocabulary. The current keymap is in meerkat, but the registry gives Apparatus
  and Settings a future shape for shortcuts, command discoverability, and conflict
  diagnostics.
- The Graphshell harvest docs are useful for policy language: capability
  declaration, non-silent a11y degradation, schema-first diagnostics, watchdog
  invariants, per-frame UxTree snapshots, focus-region state, and probeable UX
  contracts.

### What should not be revived

- The old substrate-as-host plan is not the current plan. Serval/meerkat owns the
  host path now; Graphshell's substrate material is a source of concepts and OS
  plumbing warnings, not a host mandate.
- `register-viewer` / `register-renderer-types` should not become the canonical
  route selector for current meerkat content. The live content route is host
  dispatch plus `inker`/content actors. Renderer-registry events are still useful
  as diagnostic vocabulary once a second engine lane exists.
- The diagnostics channel catalog is too broad to expose wholesale. Wire a small
  live subset first, then let orphan-channel reporting tell us what deserves a
  descriptor.
- Apparatus should not swallow Inspector, Gloss, Steward, or Settings. It owns
  system health and debug trace. Content identity/provenance belongs to Inspector;
  async job management belongs to Steward; graph commentary belongs to Gloss.

---

## Architecture

Add a bounded `HostObservability` store owned by `meerkat::App`.

It should collect:

- `DiagnosticRecord`: registry channel, severity, payload summary, span id,
  source, timestamp.
- `TraceRecord`: tracing target/name/level, active span, elapsed time, optional
  actor or pane id.
- `UxRecord`: semantic user event from `ux-events`, including surface, command,
  focus, pane, and result when known.
- `ActorRecord`: lifecycle events for fetch/sync/comms/content/agent actors.
- `ProbeRecord`: UX or invariant probe results, including pass/fail/degraded and
  the surface that failed.
- `A11ySnapshot`: root tree metadata, focus node, node count, missing label
  count, missing bounds count, and per-surface capability state.
- `AgentObservation`: compact, typed state for harnesses: focused surface,
  visible panes, enabled actions, selected graph node, pending diagnostics,
  current modal/palette, and accessible action targets.

Sources:

1. `tracing` subscriber layer for low-level spans/events.
2. `register-diagnostics::emit` global sender for structured diagnostic events.
3. `ux-events` observers and `UxChannelObserver` for semantic UI events.
4. Meerkat actor inbox drains in `user_event` for actor lifecycle and faults.
5. Frame-tree/layout rebuilds for pane bounds, focus targets, and accessibility
   surface summaries.
6. UxTree projection passes after layout/render state changes.

Sinks:

1. Apparatus pane sections: Overview, Events, Tracing, Probes, Accessibility,
   Actors, Agent.
2. Headless tests and smoke probes.
3. OS AccessKit adapter once internal tree quality is stable.
4. Hermes/Burn agent harness API.

The store is an observation cache only. It must never become the authority for
graph truth, frame layout, actor state, command dispatch, or accessibility
semantics.

---

## Initial Channel Set

Start with a small current-channel map rather than the full donor catalog:

- `meerkat.startup.started`, `meerkat.startup.succeeded`,
  `meerkat.startup.failed`
- `meerkat.frame.layout_changed`, `meerkat.frame.pane_summoned`,
  `meerkat.frame.pane_closed`, `meerkat.frame.divider_dragged`
- `meerkat.ui.action_dispatched`, `meerkat.ui.surface_opened`,
  `meerkat.ui.surface_dismissed`, `meerkat.ui.focus_changed`
- `meerkat.theme.activated`, mapped to the existing theme registry language
- `meerkat.actor.fetch.started/succeeded/failed`
- `meerkat.actor.sync.started/succeeded/failed`
- `meerkat.actor.comms.started/succeeded/failed`
- `meerkat.actor.content.respawned`, `meerkat.actor.content.failed`
- `meerkat.a11y.tree_built`, `meerkat.a11y.bounds_missing`,
  `meerkat.a11y.label_missing`, `meerkat.a11y.focus_missing`
- `meerkat.probe.failed`, `meerkat.probe.degraded`
- `meerkat.agent.spawned`, `meerkat.agent.intent_dropped`,
  `meerkat.agent.action_applied`

Use the Graphshell convention that long-running work emits
`started -> succeeded | failed`, with descriptors declaring timeout invariants.

---

## Phases

### D0 - inventory and mapping

- Map current meerkat events, panes, actors, and commands to the initial channel
  set above.
- Identify which donor channels are directly reusable, which need `meerkat.*`
  names, and which stay latent.
- Record accessibility capability state per surface: orrery, workbench, roster,
  apparatus, gloss, comms, modal/palette.

Done when the mapping is small enough to implement without a registry migration
and explicit enough that new channels do not appear ad hoc.

### D1 - HostObservability store and apparatus upgrade

- Add `meerkat/src/observability.rs` with bounded rings and summary counters.
- Feed current apparatus diagnostics from that store instead of hand-building a
  four-row snapshot.
- Expand Apparatus into sections: Overview, Events, Actors, Probes,
  Accessibility, Agent.

Done when the current theme/apparatus pane shows live recent events and actor
state without changing app authority boundaries.

### D2 - UX events and probes

- Wire command palette, pane summon/close, theme switching, roster selection,
  frame divider drag, destructive confirmations, and focus changes through
  `ux-events`.
- Add `UxProbe`s for the first real risks: modal overlap, missing destructive
  confirmation, focus lost after pane close, and unavailable action shown as
  enabled.
- Bridge UX events into diagnostics through `UxChannelObserver`.

Done when common interactions produce both semantic UX records and diagnostic
records, and at least two probes fail in tests when the guard is deliberately
broken.

### D3 - diagnostics registry bridge

- Instantiate a diagnostics registry/config in meerkat and register the initial
  channel set.
- Install the global diagnostic sender and route emitted records into
  `HostObservability`.
- Surface descriptor state, sampling, orphan channels, and invariant violations
  in Apparatus.

Done when a missing `succeeded/failed` event after a `started` event becomes a
  visible invariant warning.

### D4 - tracing ring

- Add a tracing layer that records selected `meerkat`, actor, frame, render,
  diagnostics, and a11y spans into the bounded trace ring.
- Keep verbose targets opt-in and sampled.
- Link trace records to diagnostic records when span ids are available.

Done when Apparatus can answer "what just happened?" for a pane action or actor
failure without requiring terminal logs.

### D5 - UxTree snapshot and internal a11y audit

- Build one current app UxTree snapshot by stitching chrome/frame/pane/content
  roots after layout.
- Ensure every frame leaf has stable id, role, label, bounds, focusability, and
  enabled action metadata.
- Add an internal a11y audit pass: missing label, missing bounds, duplicate id,
  invisible focused node, action without command route.

Done when Apparatus can show tree health and focused node state, and tests can
  assert the tree contains root/window/frame/pane/content nodes with stable ids.

### D6 - OS AccessKit bridge

- Only after D5, emit AccessKit `TreeUpdate`s for the host window.
- Wire focus changes from app state to AccessKit focus.
- Preserve non-silent degradation: surfaces that cannot emit meaningful a11y must
  declare degraded/unavailable with an owner and exit criterion.

Done when a Windows screen reader can traverse the shell/panes at least as named
  regions, and unavailable surfaces are reported explicitly rather than silently.

### D7 - agent harness

- Define `AgentObservation` and `AgentAction` as typed data, not screenshot
  prompts: open pane, invoke command, select node, activate focused action,
  set theme, drag divider by semantic target, request content preview.
- Expose a harness loop that can read an observation, apply one action, and
  receive the resulting observation plus diagnostics.
- Let Hermes/Burn or a Gemma-class model sit behind that typed loop.

Done when a test agent can open Apparatus, switch themes, summon Roster, select a
  node, and report any a11y/probe failures without coordinate scripting.

### D8 - inspector/steward split

- Move content-level provenance, trust, parse diagnostics, and document structure
  into Inspector.
- Move async operation management and retries into Steward.
- Keep Apparatus as the aggregate system trace and health pane.

Done when Apparatus remains legible under real browsing load instead of becoming
  an undifferentiated debug dump.

---

## Agent Readiness Answer

A Gemma-class model could plausibly puppet the application **after D5-D7**, if
the harness exposes a typed observation/action protocol. It should not be asked
to operate the app through raw pixels first. The model needs:

- a compact tree of visible surfaces and enabled actions;
- stable ids for panes, commands, nodes, and focus targets;
- recent diagnostics and probe failures;
- clear action results, including blocked/degraded reasons;
- a small action vocabulary with no hidden side effects.

Without that, it will be brittle: it may click the right pixels in a demo, but it
will not understand pane ownership, disabled commands, focus traps, or degraded
accessibility states.

---

## First Slice

1. Add `meerkat/src/observability.rs` with bounded rings for diagnostics, UX
   events, traces, probes, actors, and a11y summaries.
2. Replace `apparatus_diagnostics()` with an observability snapshot and render
   recent events in the Apparatus pane.
3. Emit `ux-events` for theme switch, apparatus summon, roster summon, pane close,
   frame divider drag, and focus changes.
4. Register the first `meerkat.*` diagnostic descriptors and install the sender.
5. Add tests for:
   - theme switch emits UX + diagnostic records;
   - apparatus summon/close preserves focus;
   - missing label/bounds appears in the a11y audit;
   - `started` without terminal event trips an invariant.

---

## Progress

- 2026-06-08: Plan written. Grounded in the live `meerkat`, `ux-events`,
  `register-diagnostics`, `uxtree`, `frame`, `chrome`, `register-input`, and
  Graphshell harvest docs. No code yet.
- 2026-06-08: **D1/D2 seed landed.** Added `meerkat::observability` as a bounded
  host-local observation cache, expanded the shared `SurfaceId` vocabulary for
  Roster / Gloss / Apparatus / Comms / Workbench panes, and wired pane
  open/close events through `ux-events` into diagnostic records. Apparatus now
  renders Overview, UX Events, Actors, Accessibility, Diagnostics, and Probes
  sections from the snapshot. Actor events are recorded from the kernel inbox
  drain for fetch, sync, content respawns, and comms updates. A coarse a11y
  summary records visible surfaces and explicitly marks the OS AccessKit bridge
  degraded until the real bridge lands. Full `register-diagnostics` descriptor
  registry, tracing layer, and probe execution remain D3/D4 follow-ons.
- 2026-06-08: **D3 landed + D4 seed.** `HostObservability` now owns a local
  `DiagnosticsRegistry`, registers the first `meerkat.*` and pane UX channel
  descriptors, and runs every diagnostic through registry sampling/orphan
  tracking/invariant observation before it enters the Apparatus ring. Startup,
  fetch, and comms use the `started -> succeeded | failed` convention, with
  completion invariants registered for startup/fetch/comms. Apparatus now shows a
  Registry section (registered count, orphan channels, invariant violations) and
  a Tracing section. `meerkat` installs the portable
  `register_diagnostics::emit` sender and drains `DiagnosticEvent`s into the same
  cache, so extracted registries can feed Apparatus without depending on meerkat.
  Remaining D4 work is a real `tracing` subscriber layer over host spans.
- 2026-06-08: **D4 landed + D5 internal seed.** `meerkat` now installs a
  `tracing-subscriber` layer that mirrors selected `meerkat`, `frame`, and
  `uxtree` spans/events into the same portable diagnostics receiver used by
  `register_diagnostics::emit`, so Apparatus can show recent host trace activity
  without tailing terminal logs. The a11y refresh now builds an internal
  AccessKit-shaped `uxtree` snapshot by stitching a host window root, a chrome
  root, and the live `frame::project_frame` subtree. Apparatus reports tree root,
  focus node, node count, missing label/description count, missing bounds count,
  duplicate ids, and audit findings. The OS bridge remains explicitly degraded;
  the next D5/D6 work is attaching real bounds/content subtrees and then pushing
  `TreeUpdate`s through an AccessKit platform adapter.
- 2026-06-08: **D5 bounds/content slice landed.** The internal a11y snapshot now
  uses `frame::project_frame_with` to attach available domain subtrees under
  frame leaves: `mere-orrery` for the graph pane, `workbench` for the tiled
  workbench, the apparatus skeleton for Apparatus/System panes, and stable
  generic roots for Roster/Gloss/Comms/Tile/Custom panes. Meerkat stamps the
  host-computed frame root, pane leaf, and pane-content-root bounds into the
  AccessKit-shaped tree after projection, preserving the frame crate's
  geometry-free ownership. Focus now resolves to the active frame leaf when
  chrome does not own focus. Unit tests cover frame leaf stable ids, host bounds
  attachment, and a11y audit focus/bounds failures. Remaining D5 work is richer
  descendant bounds and pane-specific subtrees for Roster/Comms/Gloss content.
- 2026-06-08: **D5 pane subtree slice landed.** Roster, Gloss, and Comms now
  project pane-specific internal `UxTree` subtrees instead of generic content
  roots. Roster exposes member rows as list items and reuses row hit-test bounds
  when the host has rendered them; Gloss exposes graph nodes as link-like items
  with URL values and focused-node state; Comms exposes conversation list items,
  selected thread messages, and the draft text input. This gives Apparatus and
  harness code a real semantic surface to inspect before any OS AccessKit bridge
  is enabled. Remaining D5/D6 work is making descendant bounds stable across the
  first render for every pane type, then emitting platform `TreeUpdate`s through
  an AccessKit adapter.
- 2026-06-09: **D6 Windows AccessKit bridge landed.** Meerkat now factors the
  internal a11y projection into a reusable `TreeUpdate` path and installs a
  Windows `accesskit_windows::SubclassingAdapter` before the winit window is
  shown. The same stitched `uxtree` snapshot feeds Apparatus and the OS bridge;
  render, resize, and focus changes refresh the tree and push updates when the
  adapter is active. Non-Windows platforms remain explicitly degraded for this
  slice rather than silently pretending to have an OS bridge. Remaining D6 work
  is richer action handling, platform bridges beyond Windows, and visual/manual
  screen-reader verification.
- 2026-06-09: **D7 typed agent harness landed.** Meerkat now has a feature-gated
  in-process `agent_harness` module with typed `AgentObservation`,
  `AgentAction`, and one-step `AgentStep` results. The harness reads visible
  frame surfaces, active theme, focused node, enabled action descriptors,
  diagnostics/probes, and the same a11y snapshot Apparatus sees, then routes
  actions through existing host methods for opening panes, switching themes,
  invoking commands, selecting nodes by URL, and requesting focused-node
  previews. Blocked actions emit `meerkat.agent.intent_dropped`; applied actions
  emit `meerkat.agent.action_applied`. The first tests cover opening Apparatus,
  switching themes, summoning Roster/Comms, selecting nodes, and reporting
  blocked selections without coordinate scripting. Remaining D7 work is a stable
  external transport for Hermes/Burn, richer semantic divider targets, and a
  narrower public schema once the action vocabulary settles.
