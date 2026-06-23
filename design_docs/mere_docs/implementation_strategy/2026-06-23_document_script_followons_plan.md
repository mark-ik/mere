# DocumentScript Follow-ons Plan

Status: **planning + executing 2026-06-23**. Spun out of the now-substantially-complete
[document_script_substrate_plan](2026-06-21_document_script_substrate_plan.md) (P0 -> P2.5 +
host permission resolution + omnibar trigger all shipped). This plan is the **continuation
outline** for the four remaining follow-ons, in Mark's chosen order:

1. Persistent Session-override store (+ Graph/Surface scopes)
2. Auto-attach (mod-manifest / origin binding)
3. Render refinements (Resize/Resource re-layout for the script path; subresource re-requests)
4. P2.6 AOT, then the fiber-async `fetch` capability

It is written to survive a context compaction: the **Continuation essentials** section below
captures the state, seams, file map, and working discipline a fresh context needs to resume.

---

## Continuation essentials (read first after a compaction)

**Repo / build.** `c:\Users\mark_\Code\repos\mere` (rust-toolchain pins 1.93; wasmtime 45 needs
it). Build/test from the repo dir: `cargo test -p meerkat`, `cargo test -p document-host`. The
document-core test guest is prebuilt at
`crates/script/document-host/guest/target/wasm32-wasip2/release/document_core_guest.wasm`
(rebuild: `cd crates/script/document-host/guest && cargo build --target wasm32-wasip2 --release`;
there is also `guest-bomb/` for the quota tests). Both guests `generate!` from the single shared
`crates/script/document-host/wit/world.wit`.

**Working discipline (load-bearing).**
- **Commit only my paths.** Mark continuously edits meerkat in parallel (main.rs, menus.rs,
  render.rs, window_view.rs, input.rs, views.rs, settings_store.rs, …). Always `git add <explicit
  paths>` then `git diff --cached --name-only` before committing; never `git add -A` unless Mark
  says "commit the working tree" (then snapshot all his dirty files in one WIP-checkpoint commit).
- **No `Co-Authored-By: Claude` / "Generated with Claude Code" trailers**, ever.
- **Transient build breaks in his files** (e.g. an `OrreryCard` field mid-refactor) are ~90%
  transient — wait/retry or proceed with my own work; don't fix his file.
- **600-LOC ceiling** (mere). `card.rs` (~1043), `render.rs`, `input.rs`, `main.rs` are already
  over (his) — only flip visibility there, don't add. Keep heavy logic in `content/script.rs`.
- **dead_code**: a `pub(crate)`/`pub` item unused in non-test code warns under meerkat's
  `[lints] workspace`. Land each feature **with its caller** (or it warns).

**The shipped architecture (what to build on).**
- **document-host crate** (`crates/script/document-host/`, own workspace member, kernel-free):
  - `DocumentScript::attach(component_path, dom: ScriptedDom, grant, quota) -> Self`,
    `.deliver_event(kind, payload) -> TurnOutcome`, `.dom()`, `.revision()`,
    `.detach() -> ScriptedDom`. Every guest call is **guarded** (per-turn epoch deadline +
    `StoreLimits` mem cap, watchdog spun only during the call). Sync surface (drives async exports
    via `pollster::block_on`).
  - `Grant { log: CapPermission, document: CapPermission }`, `CapPermission {Allow,Prompt,Deny}`,
    `Quota { mem_bytes, epoch_deadline_ticks }` (Default = 64MiB / 200 ticks ~1s), `TurnOutcome
    {Applied(u64),Conflict(u64),UnknownNode(u64),Refused(String)}`.
  - WIT world `document-core` (`wit/world.wit`): imports `log`, `caps` (always-linked
    `granted()->list<string>`), `document-host` (`inspect`); exports `activate`/`handle-event`/
    `deactivate`. `dom_view.rs` projects/applies a `ScriptedDom` (the §11.3 wiring).
  - `runtime.rs`: `DocumentScriptRuntime` implements `register-mod-loader::WasmModRuntime` (P2.4
    bridge for **extension** mods — seeds its own DOM; distinct from page scripts).
- **meerkat content actor** (`crates/meerkat/src/content.rs` + `content/script.rs`, both in the
  **bin**, declared in `main.rs`):
  - `content/script.rs` (`pub(crate) mod script`): `mirror_to_scripted_dom(&impl LayoutDom) ->
    ScriptedDom` (copies elements+attrs+text), `ScriptInstance` (owns the `DocumentScript` + its
    `ContentLayout<NodeId>` + page sheets; `attach`/`deliver`/`dom`/`layout`/`detach`;
    re-lays-out on a script mutation), `grant_from_resolved(log,document)` /
    `resolve_attach_permissions(ScriptCapPolicy)` (the kernel-permissions seam), `ScriptCapPolicy {
    log: Option<Permission>, document: Option<Permission> }`.
  - `content.rs`: `Content.script: Option<ScriptInstance>`; `ContentCommand::{AttachScript{
    component_path, log, document, viewport_gen}, DeliverEvent{kind,payload,viewport_gen},
    DetachScript{viewport_gen}}`; `ContentUpdate::ScriptOutcome{nav,outcome}`; the `render` branch
    emits from the script's `ScriptedDom` via the generic `scene_from_content_band`, **superseding**
    the static path. **Hybrid**: unscripted pages keep the fast `StaticDocument` path
    (`content.html`). `attach_script` / `deliver_event` helper fns gate on `is_serval_html_lane`.
- **host wiring**:
  - `constellation.rs`: `attach_script(member, path, log, document)`, `deliver_script_event(member,
    kind, payload)`, `detach_script(member)` (send the ContentCommands; mirror `request_find`).
  - `shell_eval.rs` (the omnibar `>` rhai lane, **lib**): `ShellOutcome.{attach_script:
    Option<String>, detach_script: bool, script_event: Option<(String,String)>}` recorded by the
    `attach_script("path")` / `detach_script()` / `script_event("k","p")` bindings (the
    `sparql("…")` pattern); ghost-completed.
  - `command_drain.rs` (`WindowCtx`, **bin**): `submit_omnibar_command` drains those outcome fields
    -> `self.focused_member()` -> `crate::content::script::resolve_attach_permissions(Default)` ->
    `self.shared.content.constellation.attach_script(...)`. `self.shared.content.constellation` is
    the reach; `self.focused_member()` the target.
- **kernel::permissions** (`crates/graph/graph-kernel/src/permissions.rs`): `Permission
  {Inherit,Allow,Prompt,Deny}` (Serialize/Deserialize derived), `SettingScope
  {App,Persona,Session,Graph,Surface}` (BROAD_TO_NARROW), `ScopedPermission{scope,permission}`,
  `ResolvedPermission{effective,decided_by}`, `resolve_permission(chain,default)` (max-restrictive
  narrowing). No storage exists; this plan's #1 builds it.

**Commits this session (the substrate arc).** mere: `bb30ed0` P2.4, `4b5c71d` §11.4, `dfb4ec6`
P2.5a, `620e850` P2.5c, `f45f228` P2.5 perms adapter, `c07f5e1` trigger+resolution (+ WIP
checkpoints `fc949c9`/`bde8342` of Mark's tree he authorized). serval: `67cce3c`/`95622930`
(perf timers). All green at each step; document-host 17 tests, meerkat 73 lib + 109 bin.

---

## Follow-on 1 — Persistent Session-override store (+ Graph/Surface scopes)

**Goal.** Make the live resolver actually narrow. Today `command_drain` passes
`ScriptCapPolicy::default()` (no override) so every attach gets the App default (Allow). Store a
real **Session-scope** opinion per capability and feed it in; leave Graph/Surface as `Inherit`
until their stores exist (the chain just omits them).

**Approach / files.**
- `crates/system/session-runtime/src/settings_store.rs` (Mark's hot file — coordinate / commit
  only this hunk): add to `PersistedSettings` a `#[serde(default)] script_permissions:
  ScriptPermissionPrefs` where `ScriptPermissionPrefs { log: Option<Permission>, document:
  Option<Permission> }`. `kernel::permissions::Permission` already derives Serialize/Deserialize —
  but session-runtime likely does **not** dep `kernel`; either (a) add the `kernel` dep (cleanest,
  reuse the enum) or (b) store a local serde enum and map in meerkat. **Check session-runtime's
  Cargo.toml first.** Prefer (a) if it doesn't pull a heavy graph; else (b).
- The host load site (where `PersistedSettings` is loaded into the running shell — grep
  `load_settings` / `PersistedSettings` in `main.rs`/`app_handler.rs`/`frame_ops.rs`): build a
  `ScriptCapPolicy` from the loaded prefs and thread it to `command_drain`'s attach call (replace
  `Default::default()`). Likely store the `ScriptCapPolicy` on the shell state the `WindowCtx`
  already reaches (e.g. alongside the other settings the host caches).
- UI (optional, defer-able): a `pelt/privacy` or appearance toggle in the settings lane to set the
  Session `document` opinion. The settings lane is Mark's active work; coordinate or defer the UI
  and just wire storage + resolution first.
- Graph/Surface scopes: a per-graph store would live with the session graph; per-surface with the
  view-intent sidecar. Heavier; **defer past the Session cut** unless Mark wants them now.

**Done.** Setting `document: Deny` at the session scope makes `>attach_script(...)` fail at
instantiation (the resolver returns Deny -> grant omits the import -> guest can't instantiate);
default (no opinion) still attaches. Test the resolver narrowing already exists
(`attach_permissions_resolve_with_narrowing`); add a round-trip test for the settings field.

**Gotcha.** Don't let session-runtime grow a kernel dep that pulls petgraph/etc. if it's heavy —
check. The resolver lives in `content::script` already; #1 only adds the *storage* + the *load->
ScriptCapPolicy* path.

## Follow-on 2 — Auto-attach (mod-manifest / origin binding)

**Goal.** Scripts attach on navigation automatically, not only via the omnibar.

**Approach / files.** A host-side **binding registry**: `origin-glob -> (component_path, grant
policy)`. On `Show` (navigation) in the content actor — or in the host's navigation handler — match
the page origin and call the existing `constellation.attach_script(...)` path (reuse it; do **not**
route page scripts through the P2.4 `WasmModRuntime` bridge, which seeds its own DOM for extension
mods). Source of bindings (sub-fork, confirm with Mark):
- **mod-manifest**: extend `register-mod-loader::ModManifest` so a `ModType::Wasm` mod can declare
  a DocumentScript + target origin globs (a `provides`/new field). The mod loader already exists;
  add an "origin binding" record it surfaces. Most "extension"-shaped.
- **user binding**: a per-origin setting (origin -> script path), edited in the settings lane,
  persisted per persona. Simpler, no manifest change; closer to follow-on #1's store.

Recommend the **user-binding** first (smaller, reuses #1's persistence model), with the
mod-manifest path as the "installed extension" form later.

**Done.** Navigate to a bound origin -> its script auto-attaches (and a `ScriptOutcome` reports it).
Detach on navigation away / origin change.

**Gotcha.** Re-attach semantics on re-navigation (don't double-attach); the grant still flows
through `resolve_attach_permissions` (so #1's narrowing applies to auto-attached too).

## Follow-on 3 — Render refinements (script path)

**Goal.** Make the scripted-page render path as complete as the static one: (a) Resize/Resource
re-layout; (b) subresource re-requests.

**Approach / files.**
- `content/script.rs`: add `ScriptInstance::relayout(&mut self, loader, w, h)` (re-lay-out the
  current `dom()` at a new viewport / after a new subresource; updates `self.viewport`), and expose
  the subresource **wanted** set the layout build records (the `ResourceLoader` already collects
  wants; surface them so the actor can emit `ContentUpdate::Wanted`).
- `content.rs`:
  - `Resize` arm: if `content.script.is_some()`, call `inst.relayout(loader, w, h)` before render
    (today it only clears `content.html`).
  - `Resource` arm: same — `inst.relayout(...)` so a newly-arrived image decodes into the scripted
    layout.
  - The `render` **script branch** currently returns early **before** the `wanted` handling — thread
    the wanted set from the script build/relayout through and emit `ContentUpdate::Wanted` so the
    kernel fetches subresources for scripted pages too.

**Done.** A scripted page resizes correctly and decodes subresources that arrive after attach.

**Gotcha.** Keep the per-turn re-layout (already in `ScriptInstance::deliver`) and these
viewport/resource re-layouts consistent (one private helper). Watch `content.rs` LOC (currently
~600 after P2.5c — push logic into `content/script.rs`).

## Follow-on 4 — P2.6 AOT, then fiber-async `fetch`

**4a. AOT (P2.6).** Compile trusted bundled components to `.cwasm` at build time; load via
`wasmtime::component::Component::deserialize` with codegen disabled, keeping the Cranelift compile
off the actor hot path. In document-host: a `DocumentScript::attach_precompiled(cwasm_path, ...)`
(or detect `.cwasm`) using an engine configured for deserialize. `.cwasm` is a per-target build
artifact, **never committed** (gitignore). `deserialize` is `unsafe` (trusts the bytes) -> only for
first-party bundled components; JIT (`from_file`) stays for untrusted. (Plan §11.5.)

**4b. fiber-async `fetch`.** A **sync-signature** WIT `fetch: func(request) -> result<response,
error>` (a new `net` interface + a `Network` grant) implemented as a host `async fn`; turns are
already invoked via `call_async`, so the guest's plain `fetch()` **suspends the turn's fiber**
during real I/O while the host thread is not blocked. Proven on stock WT45 in
`crates/probes/wasmtime-async-p1/` (§11.7-7). Files: `wit/world.wit` (add `net`/`fetch` + import it
in `document-core`; regen both guests), document-host host impl (the async `fetch` backed by the
content actor's fetcher — `netfetcher` for http(s), `errand` for smolweb), the grant (map
`ModCapability::Network` / a `CapPermission` for `net`; extend `Grant` + `grant_from_resolved` +
`resolve_attach_permissions` with a `net` capability). The content actor supplies the fetch backend
(it already owns the subresource fetch seam).

**Done.** A bundled script loads from `.cwasm` with no Cranelift on the hot path; a granted script
can `fetch()` a URL mid-turn (suspending, non-blocking) and mutate the page with the result.

**Gotcha.** 4b's host `fetch` must actually drive the I/O during fiber suspension — the content
actor is sync, so the `fetch` host fn blocks that actor thread for the I/O duration (acceptable: one
content actor per origin; or move to a real async executor later). Network is a powerful capability:
default it `Prompt`/`Deny` at the App scope (unlike `log`/`document` which default Allow), so a
`net`-requiring script needs an explicit grant.

---

## Findings

- The full slice for the trigger landed in **clean files** (shell_eval / content::script /
  constellation / command_drain) with no touch to Mark's hot bin or `settings_store` — by routing
  arg-bearing omnibar verbs through the `sparql`-style record-into-`ShellOutcome` pattern and a
  `content` **submodule** (avoids the `main.rs`/`lib.rs` mod-decl collision). Follow-on #1 is the
  first that *must* touch `settings_store` (his), so coordinate.
- `kernel::permissions` had zero consumers/storage before this work; the resolver
  (`resolve_attach_permissions`) is the first, and it lives host-side in `content::script` (the
  document-host stays kernel-free per §11.4).
- The hybrid (StaticDocument unscripted / ScriptedDom-mirror scripted) means **two render paths** in
  `content.rs`; follow-on #3 must keep both correct.

## Progress

- **2026-06-23** — Plan created as the pre-compaction continuation outline (Mark: "all four in
  order; outline what you need before compaction"). Captures the shipped substrate state, the four
  follow-ons with files/approach/gotchas/done-conditions, and the working discipline. Starting
  follow-on #1 (Session-override store). No follow-on code yet in this entry.
- **2026-06-23 (follow-on #1 — Session-override store, landed).** `settings_store.rs` gains a
  `ScriptPermissionPrefs { log, document: Option<Permission> }` (reusing `kernel::permissions::
  Permission`, which session-runtime already deps + is serde-derived) and a `#[serde(default)]
  script_permissions` field on `PersistedSettings`; round-trip tested (session-runtime: 71 green).
  Wiring stayed in **clean files** (no main.rs/presentation cache): `command_drain` **loads
  settings on demand at attach** (a rare explicit action) → maps to `ScriptCapPolicy` →
  `resolve_attach_permissions`, so a session `document: Deny` now fails the attach at instantiation;
  `frame_ops::persist_settings` **preserves** the on-disk `script_permissions` (loads it before the
  save so the runtime-state reconstruction does not clobber it). My code compiles clean (verified:
  the only build error is Mark's in-flight `PersonaSettings.command_usage`, an unrelated dirty-file
  refactor; my paths compile against HEAD). **Deferred**: a settings-lane UI to edit the opinion;
  Graph/Surface scopes (the chain omits them = Inherit).
  **Commit: combined with Mark's command-registry work (interleaved).** The tree is now **green**
  (Mark's `command_usage` / `record_command_usage` refactor landed; meerkat 73 lib + 109 bin +
  session-runtime 71 all pass). But #1's code is **interleaved** with his command-registry work in the
  same files and cannot be split by pathspec: `settings_store.rs` is cleanly mine, but its new
  *required* `script_permissions` field forces `frame_ops.rs` (the `PersistedSettings` construction) to
  commit with it — and `frame_ops.rs` (his `command_usage:` on the `PersonaSettings` construction +
  `record_command_usage` method) and `command_drain.rs` (his `record_command_usage(cmd.verb())` call)
  both carry his work. So only this plan doc separates. Resolution: Mark commits his command-registry
  work (sweeping my #1's `settings_store`/`frame_ops`/`command_drain` hunks), **or** authorizes a
  combined working-tree commit. #1 itself is **done + green**. Next: follow-on #2 (auto-attach).
  *(Committed combined with Mark's command-registry work in `9824ef4`, authorized.)*
- **2026-06-23 (follow-on #2 — auto-attach origin bindings, landed + green, committed cleanly).**
  User-binding form (the recommended first cut). A dedicated **`session-runtime/script_bindings_store`**
  (`script-bindings.json`, `ScriptBinding { origin, component_path }`, load/save, round-trip tested)
  rather than a `PersistedSettings` field — keeps it cleanly mine + dodges the `frame_ops`/settings
  co-edit churn, and it is better-scoped ("installed scripts" vs flat prefs). `content::script` gains
  `ResolvedScriptBinding`, `origin_matches` (exact host or `*.`-suffix glob), `binding_for`, and
  `load_resolved_bindings(mere_root, prefs)` (loads the file + resolves each against the session
  permission policy from #1). `Constellation` gains a `script_bindings` field + `set_script_bindings`,
  and **`drive`'s fresh-Show branch auto-attaches** the matched binding via the same `AttachScript` the
  omnibar verb sends (tied to fresh-Show so re-navigation re-attaches, a steady frame does not). The
  host push is **one block in `main.rs`** (next to `set_disabled_engines`): `set_script_bindings(
  load_resolved_bindings(&mere_root, &saved_settings.script_permissions))`. 4 new tests (meerkat 111
  bin: origin/binding matching; session-runtime 73: bindings round-trip). **Committed cleanly** (all
  files mine this round: `script_bindings_store.rs`, session-runtime `lib.rs`, `content/script.rs`,
  `constellation.rs`, `main.rs`). **Deferred**: a settings-lane UI to edit bindings; re-push on
  bindings-file change (today pushed once at startup); the mod-manifest "installed extension" form.
  Next: follow-on #3 (script-path render refinements). *(Committed cleanly in the next commit.)*
- **2026-06-23 (follow-on #3 — script-path render refinements, landed; cleanly mine).** The scripted
  (`ScriptInstance`) lane now matches the static lane's completeness. `ScriptInstance::relayout(loader,
  w, h)` re-lays-out the current (script-mutated) DOM with the page sheets. `content.rs`: extracted the
  subresource-`Wanted` tail into `emit_fresh_wanted`, added `relayout_script` (re-lay-out + ship
  wants), and now (a) **Resize** re-lays-out the script at the new viewport, (b) **Resource**
  re-lays-out so a newly-arrived subresource decodes, (c) `attach_script` / `deliver_event` ship the
  subresources their layout build/rebuild wants (so a scripted page — and a script that adds an
  `<img>` — gets its subresources fetched, closing the loop the early-return render branch left open).
  All in `content.rs` + `content/script.rs` (**cleanly mine**). Compile-verified (the content files
  built clean; the build was red only on Mark's dirty `gyre/Cargo.toml` geometry-dep change breaking
  render.rs — uncommitted, not in HEAD, so #3 commits against a green HEAD). No new unit test (it is
  render-path wiring; the unit-testable pieces — mirror / grant / binding matching — are already
  covered). **Deferred**: a Retheme path for scripts (the serval lane themes via its own CSS, so
  unchanged). Next: follow-on #4 (P2.6 AOT, then fiber-async `fetch`).
