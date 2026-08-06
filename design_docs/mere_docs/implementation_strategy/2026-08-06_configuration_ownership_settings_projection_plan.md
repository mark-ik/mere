# Configuration Ownership and Settings Projection Plan

**Date**: 2026-08-06
**Status**: planning (ledger done; C2 supersede pass done this session)
**Scope**: cross-product umbrella. Owns the settings taxonomy, the ledger, the
provider contract, and the mere-local work. Sibling products (Woodshed, Hocket,
Isometry, Cleromancy) do their own splits under their own design_docs plans;
this plan holds pointers, not their work.
**Code**: `genet/components/genet-host-api/tile.rs` (the `SettingsRef` lane,
tile.rs:144), `genet/components/config` (opts/prefs),
`mere/crates/system/session-runtime/src/settings_store.rs` +
`persona_settings_store.rs`, `mere/ports/knot/src/settings.rs`,
`mere/ports/graphshell/src/native/owner_settings.rs`,
`turnstone/src/apparatus_pane.rs`,
`woodshed/crates/woodshed-core/src/{settings.rs,storage.rs}`,
`hocket/crates/hocket-genet/src/update.rs`.
**Supersedes**:
[settings_lane_consolidation_plan](../../archive_docs/2026-07-13_superseded_plans/2026-06-21_settings_lane_consolidation_plan.md)
(archived 2026-08-06; its landed work died with meerkat and its settings-as-nodes
metaphor was ruled out by the pane-taxonomy revision; its durable ideas are
carried below under "Carried forward").

---

## The problem

Every product invented settings storage ad hoc, and the seams now contradict
each other in code:

- Mere's main store calls itself "session-wide" (settings_store.rs:4) while the
  persona store describes that same store as "app-scoped"
  (persona_settings_store.rs:5). Its fields mix chrome appearance, crawl
  behavior, capability policy, wallet unlock, and resource caps in one flat
  record.
- Woodshed flattens a well-sectioned `AppSettings` (11 named sections,
  settings.rs:236) into `PersistedSession` beside the rehearsal set, the song
  doc, and practice history (storage.rs:80): personal preference and practice
  artifact share one file.
- Hocket left an explicit IOU: update policy is env-var-backed "pending the
  broader settings work" (update.rs:130).
- Turnstone's Apparatus header names "app-level settings" as "a DIFFERENT,
  later pane" (apparatus_pane.rs:8): the pane does not exist yet.
- The prior consolidation plan solved where settings pages appear, not what a
  setting is, who owns it, or whether it moves between devices.

## The model

> The thing being configured owns the typed setting and stores it beside
> itself. Providers describe settings generically; Cambium renders them; hosts
> apply them; movement and authority are explicit.

Every setting declares four axes:

1. **Scope**: process, device, persona, application, session/dataspace,
   artifact/entity, governed place. Session is a first-class scope, not a
   synonym for application: a mere's sidecar travels with the dataspace, an
   application preference does not.
2. **Movement**: local-only, persona-synced (opt-in), travels-with-artifact,
   governed replication.
3. **Mutability**: startup-only, live, restart-required.
4. **Security**: ordinary value, private value, secret reference. Secret
   material itself stays in Personae or the platform vault; a settings file
   carries at most a reference.

Worked examples across the wing:

- Theme, accessibility: persona or application preference, optionally synced.
- Audio device, GPU backend, update policy per install: device-local.
- Genet engine flags: process scope (opts = startup-only, prefs = live).
- Graph layout, scene physics, Isometry world presentation: artifact scope.
- Hocket project sample rate: project truth (travels with the artifact);
  preferred default sample rate: personal setting.
- Cleromancy preferred deck or house system: personal default; the deck
  actually used in a reading becomes immutable provenance, not a setting.
- Moot membership, moderation, shared-world rules: governed facts, never
  personal preferences.
- Action-form values stay one-shot input unless the user explicitly saves a
  default.

## The ledger (C0, code-verified 2026-08-06)

| Store | Home | Scope today | Verdict |
|---|---|---|---|
| genet `opts` | process globals, startup-immutable (config/lib.rs:6) | process / startup-only | correct as-is |
| genet `prefs` | process globals, runtime-settable (config/lib.rs:8) | process / live | correct as-is |
| mere `PersistedSettings` | `<session_dir>/settings.json` (settings_store.rs:26) | mixed, see C1 | **split or per-field ruling required** |
| mere `PersonaSettings` | `personas/<id>/settings/ui.json` | persona / local | mostly right; `session_count` is runtime bookkeeping riding in a settings file, note only |
| knot `KnotSyncSettings` | `personas/<id>/knot-sync.json` | persona / device pairing, public material only | model citizen; its doc comment is the contract principle |
| graphshell `OwnerSettings`/`SyncSettings`/`LaneSettings` | profile-keyed (owner_settings.rs:104) | persona + application | fine; joins the contract at C4+ |
| turnstone viewer override | `browser_nodes.json` sidecar | artifact/entity | model citizen: applied through routing respawn, "not a stored preference pretending to be one" (apparatus_pane.rs:16) |
| turnstone scene facets | `scene.physics_damping` container facet | artifact | model citizen, and the precedent: this field already *left* mere's settings.json for scene scope (settings_store.rs:91) |
| woodshed `AppSettings` | flattened into `PersistedSession` (storage.rs:80) | personal prefs + artifact state in one file | split at C5, in woodshed's own plan |
| hocket `UpdateSettings` | env var `HOCKET_UPDATE_POLICY` interim (update.rs:125) | device | settings file at C5, in hocket's own plan |
| isometry, cleromancy | none | n/a | join at C7 on the first real personal setting |

The mere `PersistedSettings` fields, by likely axis ruling (the C1 decision
finalizes each):

- Application preference living session-side today: `tab_cap`, `theme_id`,
  `theme_mode`, `shellbar_edge`, `shellbar_hidden`, `ui_zoom`,
  `document_typography`, `disabled_engines`, `snapshot_idle_refresh`,
  `snapshot_byte_cap_mb`.
- Genuinely session-scoped policy (their own doc comments say so):
  `script_permissions` (Session-scope permission opinions), `crawl_scope`,
  `crawl_depth`, `crawl_sitemap`, `crawl_max_pages`, `capture_consent`,
  `retention_keep_n`.
- Device policy: `startup_unlock_mode` (wallet unlock is a property of this
  install, not of a dataspace).

## The contract (C3)

A `SettingSpec` / `SettingsProvider` boundary, built as a **module in
genet-host-api beside the existing `SettingsRef` lane** (tile.rs:144). It
extends the seam that already addresses settings pages; it is not a second
addressing scheme and not a crate until an enforced wall or external consumer
demands one.

- `SettingSpec` describes one setting: id, label, the four axes, control
  shape, current value.
- `SettingsProvider` resolves a `SettingsRef` namespace to specs and applies
  typed writes.
- **Not a store.** Storage stays with the owner. What providers may share is
  mechanism (atomic tmp+rename write, serde-default tolerance), never schema.
  knot/settings.rs:12 states it exactly: "The two share a shape, not a
  subject; if a third consumer appears, the atomic-write mechanism is what to
  extract, not the schema."
- The ledger above stays a document. No runtime registry of all settings, no
  universal JSON store, no framework crate.

## Carried forward from the superseded plan

- The pelt `SettingsRef` lane as the projection address space, with the
  namespace map re-based onto the scope axis: `pelt/*` = application scope,
  object facets = artifact/entity scope (now realized as Turnstone's Apparatus
  analyzer, not settings tiles), `moot:<id>/*` = governed scope (future).
- The line that held: deep config moves to a settings surface, quick gestures
  stay in the menu.
- Diagnostics are not settings. Read-only inspection belongs to Steward per
  the pane-taxonomy revision, never to a settings page.
- Ruled out and staying ruled out: settings-as-nodes (`settings://` scheme,
  synthesized graph nodes). Turnstone's Apparatus header records the ruling.

## Phases (done conditions)

- **C0 — Ledger.** Done (this doc).
- **C1 — Resolve the mere scope contradictions.** Done when: the
  settings_store and persona_settings_store doc comments agree with each other
  and with the taxonomy; every `PersistedSettings` field carries a four-axis
  ruling in its doc comment; wrongly-scoped fields have a named destination and
  the moves follow the `physics_damping` precedent (move, tolerate the legacy
  field on load, comment the trail). The headline call: does `settings.json`
  stay a session/dataspace file with app-prefs split out, or the reverse.
- **C2 — Supersede the 2026-06-21 plan.** Done 2026-08-06: archived to
  `archive_docs/2026-07-13_superseded_plans/` with a banner, live links
  repointed, DOC_README updated, durable ideas carried here.
- **C3 — Contract module.** Done when genet-host-api carries
  `SettingSpec`/`SettingsProvider` beside `SettingsRef`, doc-commented with
  the four axes, and one real provider compiles against it.
- **C4 — Two product projections, then extract.** Done when Turnstone's
  app-level settings pane (the pane its Apparatus header defers) and
  Woodshed's Settings section both render provider-described settings through
  Cambium controls, and the shared projection code is extracted only after
  both exist. Two consumers before extraction, per the component-catalog
  discipline.
- **C5 — Per-product splits, per-product plans.** Done when Woodshed has a
  dated plan in its own design_docs splitting `AppSettings` persistence from
  the `PersistedSession` artifacts, and Hocket has one replacing
  `HOCKET_UPDATE_POLICY` with a settings file through the contract. This plan
  tracks the pointers only.
- **C6 — Opt-in persona sync.** Done when at least one movement=persona-synced
  setting demonstrably syncs between two devices, opt-in, with security=secret
  material excluded. Rides the existing knot/graphshell lanes; this plan
  invents no transport. Gated behind C1 and C5 (sync a clean scope, never a
  mixed file).
- **C7 — Latecomers.** Isometry and Cleromancy adopt the contract when each
  has a real personal setting to expose. Explicitly not a gate to
  manufacture settings for.

## Findings

- Code-verified 2026-08-06: every claim in "The problem" and "The ledger"
  checked against the cited file:line. The two mere store doc comments do
  contradict; `physics_damping`'s exit comment (settings_store.rs:91) is the
  worked precedent for C1 moves; hocket's `from_env` doc names this work as
  its blocker; `SettingsRef` is live contract at genet-host-api tile.rs:144
  with `pelt/appearance` in the default tile fixture (tile.rs:608).
- The superseded plan's P1-P3 landed work (settings tiles, pelt provider
  pages, node facets pages) was meerkat host code. Meerkat is deleted; none of
  it survives to migrate. What survives is contract (`SettingsRef`) and
  doctrine (deep-config line, diagnostics split), both carried above.

## Progress

- 2026-08-06: Plan founded from the stack-wide settings review (session with
  the verified cross-product audit: genet, mere, knot, graphshell, turnstone,
  woodshed, hocket, isometry, cleromancy). Four-axis model adopted with
  session/dataspace as an explicit scope. C0 ledger authored from code, not
  from docs. C2 executed same session: old plan archived with banner, six
  live-doc links and three archived-doc links repointed, DOC_README entries
  updated (old entry marked superseded, this plan indexed).
