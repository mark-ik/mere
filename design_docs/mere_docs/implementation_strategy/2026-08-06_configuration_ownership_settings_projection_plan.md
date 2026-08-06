# Configuration Ownership and Settings Projection Plan

**Date**: 2026-08-06
**Status**: implementation complete through C6; C7 deferred by design
**Scope**: cross-product umbrella. Owns the settings taxonomy, the ledger, the
provider contract, and the mere-local work. Sibling products (Woodshed, Hocket,
Isometry, Cleromancy) do their own splits under their own design_docs plans;
this plan holds pointers, not their work.
**Code**: `genet/components/genet-host-api/tile.rs` (the `SettingsRef` lane,
tile.rs:144), `genet/components/config` (opts/prefs),
`mere/crates/system/session-runtime/src/{application_settings_store.rs,device_settings_store.rs,settings_store.rs}` +
`persona_settings_store.rs`, `mere/ports/knot/src/settings.rs`,
`mere/ports/graphshell/src/native/owner_settings.rs`,
`turnstone/src/{apparatus_pane.rs,settings_provider.rs,settings_pane.rs}`,
`woodshed/crates/woodshed-core/src/{settings.rs,storage.rs}`,
`woodshed/crates/woodshed-views/src/{settings_provider.rs,stage/settings.rs}`,
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
| mere `PersistedSettings` | `<session_dir>/settings.json` (settings_store.rs:18) | session/dataspace policy | C1 storage split landed; legacy fields remain readable |
| mere `ApplicationSettings` | `<data_root>/application/settings.json` | application | C1 storage split landed |
| mere `DeviceSettings` | `<data_root>/device/settings.json` | device | C1 storage split landed |
| mere `PersonaSettings` | `personas/<id>/settings/ui.json` | persona / local | mostly right; `session_count` is runtime bookkeeping riding in a settings file, note only |
| knot `KnotSyncSettings` | `personas/<id>/knot-sync.json` | persona / device pairing, public material only | model citizen; its doc comment is the contract principle |
| graphshell `OwnerSettings`/`SyncSettings`/`LaneSettings` | profile-keyed (owner_settings.rs:104) | persona + application | fine; joins the contract at C4+ |
| turnstone viewer override | `browser_nodes.json` sidecar | artifact/entity | model citizen: applied through routing respawn, "not a stored preference pretending to be one" (apparatus_pane.rs:16) |
| turnstone scene facets | `scene.physics_damping` container facet | artifact | model citizen, and the precedent: this field already *left* mere's settings.json for scene scope (settings_store.rs:91) |
| woodshed `AppSettings` | flattened into `PersistedSession` (storage.rs:80) | personal prefs + artifact state in one file | split at C5, in woodshed's own plan |
| hocket `UpdateSettings` | `update-settings.json` through `pelt/update` provider | device | C5 settings file landed; `HOCKET_SETTINGS` is an isolated-file override |
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

### C1 ruling and landed storage boundary

`settings.json` remains a session/dataspace sidecar. Its typed record now owns
only session policy: script permissions, crawl controls, capture consent, and
trace retention. Application preferences moved to
`<data_root>/application/settings.json`, and device startup unlock moved to
`<data_root>/device/settings.json`. The old session file remains forward-readable:
`load_settings_with_legacy` returns the session record plus an explicit migration
payload for the application/device fields. Saving the new session record omits
those legacy fields, so a host can migrate them without inventing a universal
settings store. `physics_damping` remains the precedent for this move: the old
field is tolerated on load and the owning scene facet is authoritative.

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
- **C1 — Resolve the mere scope contradictions.** Storage split slice landed
  2026-08-06. The remaining receipt is host adoption of application settings
  through C3/C4. The settings_store and persona_settings_store doc comments now
  agree with the taxonomy; every persisted field carries an axis ruling; the
  moved fields have named destinations and a legacy migration loader. The
  headline call is settled: `settings.json` stays session/dataspace, with
  application and device preferences split out.
- **C2 — Supersede the 2026-06-21 plan.** Done 2026-08-06: archived to
  `archive_docs/2026-07-13_superseded_plans/` with a banner, live links
  repointed, DOC_README updated, durable ideas carried here.
- **C3 — Contract module.** Done 2026-08-06: genet-host-api carries
  `SettingSpec`/`SettingsProvider` beside `SettingsRef`, doc-commented with
  the four axes, and Turnstone's application provider compiles and persists
  three real application settings through it.
- **C4 — Two product projections, then extract.** Done 2026-08-06: Turnstone's
  app-level settings pane (the pane its Apparatus header defers) and
  Woodshed's Settings section both render provider-described settings through
  Cambium controls. `genet-host-api::settings::SettingsProjection` was
  extracted only after both providers existed; it shares provider resolution,
  while each product retains its own Cambium control state and rendering.
  Two consumers before extraction, per the component-catalog discipline.
- **C5 — Per-product splits, per-product plans.** Done 2026-08-06: Woodshed
  has a dated plan and separate `SettingsStorage` lane for `AppSettings`, with
  legacy flat-session migration; Hocket has a dated plan and a contract-backed
  atomic `update-settings.json` provider replacing `HOCKET_UPDATE_POLICY`.
  This plan tracks the pointers only.
- **C6 — Opt-in persona sync.** Done 2026-08-06. Graphshell's existing
  `SetHandlerPreference` event is a real ordinary persona preference. The
  existing `handler_preferences` lane is disabled by default and rides the
  personal-graph carrier; no transport or mixed-file settings path was added.
  The focused test now proves an opted-in peer projects the preference, an
  unconfigured peer retains but does not project it, and the existing
  secret-bearing facet rejection still holds.
- **C7 — Latecomers.** Deferred by design 2026-08-06. Isometry and Cleromancy
  have no real personal setting to expose, so there is no contract adoption to
  implement and no artificial setting was manufactured for this plan.

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
- 2026-08-06: **C1 storage split slice landed.** `PersistedSettings` now contains
  session/dataspace policy only; application and device stores own the moved
  fields at explicit data-root paths. Legacy session files remain readable
  through `load_settings_with_legacy`; the wallet startup path reads
  `DeviceSettings`. `cargo test -p session-runtime --lib`: 238 passed.
- 2026-08-06: **C3 contract and first real provider landed.**
  `genet-host-api::settings` now carries the typed axes, controls, values, and
  object-safe provider boundary; `cargo test -p genet-host-api`: 15 passed.
  Turnstone's `ApplicationSettingsProvider` describes and writes theme id,
  theme mode, and UI zoom through the contract; its focused receipt passes 3
  tests. Six stale `AdvertisedAction` literals needed `input_form: None` to
  restore the existing opaque-action path in Knot and Turnstone.
- 2026-08-06: **C4 two-consumer projection landed.** Turnstone's retained
  Settings pane resolves `pelt/appearance`, maps provider-described text and
  number controls through Cambium, and routes Apply clicks back to the
  provider. Woodshed's existing Settings Appearance page now resolves the
  same provider projection and applies its theme choice through a typed
  provider write. `genet-host-api::settings::SettingsProjection` was extracted
  after both consumers existed. Receipts: `cargo test -p genet-host-api` (16),
  Turnstone settings tests (5), and `cargo test -p woodshed-views
  settings_provider` (2).
- 2026-08-06: **C5 product persistence splits landed.** Woodshed's
  `PersistedSession` now carries artifact/session state only; `AppSettings`
  uses a separate `SettingsStorage` file lane, and `decode_session` returns
  legacy flat settings for migration. Hocket replaces `HOCKET_UPDATE_POLICY`
  with an atomic `update-settings.json` provider at `pelt/update`. Receipts:
  Woodshed storage tests (7), `cargo check -p woodshed-genet`, and the Hocket
  provider persistence test (1).
- 2026-08-06: **C6 persona-sync receipt landed.** Graphshell's existing
  `handler_preferences` lane is explicitly documented as
  `scope=persona`, `movement=persona-synced`, `mutability=live`,
  `security=ordinary`; its opt-in and secret-free behavior is covered by
  `personal_sync::tests::persona_handler_preference_is_opt_in_and_secret_free`,
  alongside the existing two-replica transport convergence test.
  **The receipt needs its feature flag to run:** `personal_sync` is gated behind
  the non-default `personal-sync` feature (graphshell/src/lib.rs:37), so a plain
  `cargo test -p graphshell` silently skips it and reads as if the test does not
  exist. Run
  `cargo test -p graphshell --features personal-sync --lib personal_sync`
  (12 passed 2026-08-06).
- 2026-08-06: **C7 disposition recorded.** Isometry and Cleromancy remain
  latecomers until a real personal setting exists; both stay outside the
  implementation gate.
