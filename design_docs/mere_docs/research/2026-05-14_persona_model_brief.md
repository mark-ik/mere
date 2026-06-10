# Persona model brief

**Date**: 2026-05-14
**Status**: Research brief — informs the v1 persona model and what `PersonaId` actually means
**Scope**: Pin down what a *persona* is in Mere, what it owns, and how the user moves between personas. The framing brief §11.2 flagged this as owed before persona-aware UDFs ship; this is that brief.

**Related**:

- [`2026-05-11_browser_multiplexer_framing.md`](2026-05-11_browser_multiplexer_framing.md) §11.2 — the gap this brief fills.
- [`../implementation_strategy/2026-05-14_engine_profile_boundary_plan.md`](../implementation_strategy/2026-05-14_engine_profile_boundary_plan.md) — relies on `PersonaId` to root UDF paths under `<data_root>/personas/<persona_id>/`.
- [`crates/persona/identity/src/lib.rs`](../../../crates/persona/identity/src/lib.rs) — `PersonaId(pub Uuid)` now lives at the persona boundary.
- [`crates/system/session-runtime/src/manifest.rs`](../../../crates/system/session-runtime/src/manifest.rs) — `GraphSessionManifest.persona_id` references that `PersonaId`. v0 always uses `PersonaId::default_persona()`; this brief governs what the not-default world looks like.

---

## 1. The question

**What is a persona?** Three coherent shapes the term could pick out:

1. **One human, one persona.** Each user gets exactly one persona; multi-persona means multi-user-per-machine.
2. **Many humans, one machine — distinct personas per human.** Same machine, different `personas/<uuid>/` directories; switching is a sign-in moment.
3. **One human, many personas.** A single user keeps multiple identities — work persona, research persona, throwaway-probe persona — and switches between them as work-mode rather than user-mode.

Each implies different defaults for how engine profile state, sessions, vaults, and capabilities are scoped.

## 2. The recommendation: one human, many personas

Pick option 3. Reasons:

- **The framing brief's existing pull.** §11.2 already names "work persona vs. research persona vs. throwaway-probe persona" as the likely answer. The engine-profile boundary already treats `<persona_id>` as the *coherence boundary* for cookies and logins — that pattern is mostly meaningful when one user runs *multiple* such bubbles, not when each persona is a separate human.
- **OS-level identity already handles option 2.** If two humans share a machine, they have separate OS user accounts with separate `<data_dir>`s. Mere doesn't need to re-implement that boundary; it's owed downward.
- **It maps to how people actually work.** Tabs and windows segment by *topic*, not by *identity*. Personas in this model do the same job one tier up: a persona segments by *mode of engagement* — what cookies do I want sticky here, what trust do I extend, what kind of intelligence signals do I want feeding me.
- **Throwaway personas become possible.** A "scratch persona" with no durable engrams is a useful first-class concept (compare incognito mode, but persona-scoped rather than tab-scoped). Hard to model under option 1.

Multi-user-per-machine is **out of scope** for Mere v0 — defer to the OS user model.

## 3. What a persona owns

```text
<data_root>/personas/<persona_id>/
├── persona.json              ← PersonaManifest (NEW; v1)
├── engine-profiles/          ← per-engine UDFs (engine_profile_boundary_plan §2)
│   └── <engine_id>/
├── vault/                    ← persona identity material (absorbs mere-identity)
└── settings/                 ← persona-scoped overrides (capability gates, palette, theme tweaks)
```

Sessions stay under `<data_root>/sessions/<session_id>/` (the existing `ManifestStore` layout). Each session's manifest references one `PersonaId`; that's how sessions belong to a persona.

The PersonaManifest holds:

```rust
struct PersonaManifest {
    schema_version: u32,
    persona_id: PersonaId,
    display_name: String,             // "Work", "Research", "Probe"
    created_at: SystemTime,
    last_active_at: SystemTime,       // most recent session activity
    durable: bool,                    // false = throwaway / scratch
    default_engine_profile: EngineProfileBinding,  // PersonaScoped vs SessionScoped default for new sessions
    capability_overrides: PersonaCapabilityOverrides, // empty in v0; capability-gate brief populates
}
```

`durable = false` is the lever for scratch personas: their engine UDF lives under `personas/` but the session manifests carry a flag that consolidates-on-idle clean up the persona's children, and a lifecycle hook archives or deletes the persona on app exit.

### 3.1 Persistence ownership pattern

Persistence follows ownership, not convenience. Every persisted shape has an
owning domain and a storage substrate:

| Persisted shape | Owner | Storage substrate / path |
|---|---|---|
| Persona manifest, vault metadata, persona settings | `persona` | `<data_root>/personas/<persona_id>/` |
| Engine profile bytes / UDFs | Engine profile boundary, under persona policy | `<data_root>/personas/<persona_id>/engine-profiles/<engine_id>/` by default; session/graph override only by explicit policy |
| Session manifest and session policy | `shell` session runtime | `<data_root>/sessions/<session_id>/manifest.json` |
| Graph truth | `graph/graph-kernel` | Session graph store; eidetic may be the byte substrate, but the graph schema belongs to the graph kernel |
| View intent / pane-local state | `shell` + workbench owner for the pane kind | `<session_dir>/views/<frame_id>/<pane_id>.json` |
| Tile lifecycle state | `workbench/verso` | Session/workbench store keyed by tile identity |
| Long-lived artifacts, engrams, model blobs, vector indexes, import payloads | `eidetic` substrate plus the producing domain | content-addressed eidetic manifests / typed payloads |
| Disposable caches and thumbnails | Producing subsystem | Cache directory; recomputable, never authoritative |

The rule is: **schemas live with the domain that interprets them; eidetic
stores durable artifacts and typed payloads; hosts provide I/O and lifecycle
but do not invent persistence schemas.** If a new persisted file appears
without an owning domain and a declared scope (`persona`, `session`, `graph`,
`view`, `pane`, `tile`, or `engine-profile`), it is architectural drift.

Each store exposes a typed repository API (`PersonaStore`, `ManifestStore`,
`ViewIntentStore`, `SessionGraphStore`, etc.) with atomic write semantics.
Direct ad-hoc JSON writes from host code are out of bounds.

### 3.2 Persona identity crate direction

The current `mere-identity` crate resolves into the persona layer. Identity
material is not generic app state and not protocol-specific enough to live in
`murm` or `eidetic-iroh-fetcher`; it is the persona's vault surface. Protocol
crates ask the active persona for derived keys / public identity material, and
the host supplies OS-keychain integration behind the persona vault API.

Target topology:

```text
crates/
  persona/
    identity/           # current PersonaId + key derivation / signing / vault surface
    persona-core/       # optional future split: PersonaManifest + PersonaStore
```

The current efficient shape keeps `PersonaId` in `persona/identity` so the
session runtime and control-plane bus can share the type without adding another
stub crate. If persona manifests/settings grow enough to warrant it, split
`persona-core` later; do not add it as an empty namespace placeholder.

## 4. Persona switch as a session-boundary moment

A persona switch is the user's strongest "context boundary" gesture in the app. The model:

- **One active persona per OS process at a time.** Multiple windows under the same persona is the common case; cross-persona windows in one process is *not* a v1 goal. Forces the boundary to be explicit.
- **Switching closes all current sessions cleanly** (manifests flushed, workers stopped) and opens the target persona's most recent sessions per the manifest store.
- **The action bus knows about it.** `ActionTarget::Persona(PersonaId)` already exists as a target scope; a `SwitchPersona { id }` action goes through the bus + gate + diagnostic spine.

This is intentionally heavy. Persona-switching shouldn't be a fast toggle — it's "I'm changing what kind of work I'm in." If the user wants fast toggling between contexts, that's what *sessions* are for.

## 5. Default persona

v0 has one persona (`PersonaId::default_persona()` with a zero UUID). v1 keeps that as the *default* for first-launch users and existing data. A v1 user with no opinion never has to think about personas — it's a single-persona deployment that just happens to expose the abstraction.

The persona-creation gesture (the palette command `Persona: Create new`) is what flips a user into multi-persona world. Same default-persona uuid stays the bootstrap entry point so persona-unaware code keeps working.

## 6. Persona-scoped capability gates

The capability-gate catalogue brief (next on the queue) will detail this. The persona-level hook this brief commits to:

- **`PersonaCapabilityOverrides`** sits *above* `SessionPolicy.overrides` (already in `manifest.rs` as a placeholder). When the gate checks an action, it consults: 1) session overrides, 2) persona overrides, 3) app defaults. Persona overrides are how "this throwaway persona doesn't get cookie persistence" gets expressed.
- **Persona-scoped action targets** in the bus mean "every session under this persona." Kill-persona-and-everything-under-it becomes a single bus dispatch.

## 7. What persona switching *doesn't* do

Some behaviours pull the metaphor in unhelpful directions. Excluded:

- **Persona is not encryption boundary.** It's a coherence / context boundary. Per-persona at-rest encryption is a separate Phase-4 concern under the vault model.
- **Persona is not network boundary.** Two personas can hit the same network without coordination. The engine-profile boundary handles cookie isolation.
- **Persona is not capability identity.** When a persona reaches the network, the engine signs as the persona's vault identity (Phase 4); persona ≠ DID, but a persona's vault ≈ "DIDs this persona controls."

## 8. v0 → v1 → v2 sequence

**v0 (now):** one persona, default UUID. Engine profile defaults to `PersonaScoped` under that one persona's directory. Switch UI doesn't exist.

**v1 (post-manifest-store wiring):**

- `PersonaManifest` lands as a JSON sidecar at `<data_root>/personas/<persona_id>/persona.json`.
- `PersonaStore` (analogous to `ManifestStore`) loads + persists the persona list at app boot.
- Palette commands: `Persona: Create`, `Persona: Switch to …`, `Persona: Archive`.
- Persona-switch is the heavy boundary moment described in §4.

**v2 (post-capability-gate catalogue):**

- `PersonaCapabilityOverrides` populated.
- Throwaway personas (`durable: false`) work end-to-end: created on demand, archived on persona switch-away, garbage-collected on app exit (or kept around until next boot's idle sweep).

## 9. Open questions

1. **What does "persona switch" mean for an in-flight tear-out flow?** If a user is mid-drag during a switch, the in-progress drag aborts cleanly. Filed for the v1 wiring.
2. **Persona-scoped intelligence signals.** Embeddings, clusters, affinity — do they pool across all sessions in the persona (cheap, coherent) or stay session-scoped (clean, but loses cross-session pattern recognition within a persona)? Lean toward persona-scoped with session filtering at query time. Validate against a concrete intelligence-signal producer when one lands.
3. **The "no persona" case.** Some sessions in headless-server deployments might want to run with no persona binding (cron job, indexer). v0 treats them as "default persona"; v2 might add a `PersonaId::headless()` reserved value. Decide when a headless deployment actually needs it.
4. **Cross-persona discovery.** Can two personas under the same OS user see *that the other exists*? Probably yes — the persona list is metadata, not the persona's contents. Useful for "open this URL in my Work persona" jumps from the active persona. Pin in v1.

## 10. What this brief commits to

- **Personas are one-human-many-personas, not one-machine-many-humans.** OS user accounts handle the latter.
- **`<data_root>/personas/<persona_id>/` is the persona's root.** Engine profiles, vault, settings live here.
- **One active persona per process.** Persona switch is a heavy boundary moment routed through the action bus.
- **The default persona stays the bootstrap entry point** so single-persona users never have to think about it.
- **Persona overrides sit above session overrides** in the capability-gate chain (detail in the gate catalogue brief).
