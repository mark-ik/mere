# Capability-gate catalogue brief

**Date**: 2026-05-14
**Status**: Research brief — enumerates the v0 capability gates and the layered policy model that resolves them. **Resolution superseded 2026-05-30**: the "first-match-wins" chain here is replaced for permissions by the narrowing rule in [`kernel::permissions`](../implementation_strategy/2026-05-27_adoption_roadmap.md) (most-restrictive-wins across a five-scope hierarchy — first-match could let a broader opinion win over a stricter narrower one, the wrong direction for security). The capability *vocabulary* and the three-state decision carry forward; `RequireConsent` is now `Permission::Prompt`.
**Scope**: Concretise the gate set the framing brief §7 names but doesn't enumerate, define the policy chain (action → session → persona → app), and pin down how denials flow into diagnostics. Sits between the framing brief's principle and the gate enforcement landing in the action bus.

**Related**:

- [`2026-05-11_browser_multiplexer_framing.md`](2026-05-11_browser_multiplexer_framing.md) §7 — security principle; §8 — `permission.denied` event shape; §5.8 — action bus that gates attach to.
- [`../implementation_strategy/2026-05-11_typed_action_bus_plan.md`](../implementation_strategy/2026-05-11_typed_action_bus_plan.md) — the bus design these gates plug into. The `PermissionGate` trait now lives in `graphshell-control-plane` (`PermitEverythingGate`, `RefuseEverythingGate`); this brief specifies the *real* policy gate that replaces them.
- [`2026-05-14_persona_model_brief.md`](2026-05-14_persona_model_brief.md) — defines persona-scoped overrides; this brief picks them up.
- [`crates/system/session-runtime/src/manifest.rs`](../../../crates/system/session-runtime/src/manifest.rs) — `SessionPolicy.overrides: Vec<SessionPolicyOverride>` is the placeholder this brief fills.

---

## 1. What a gate is

A **gate** is a named, machine-checkable rule that says *"this kind of action against this kind of target is allowed / denied / requires explicit consent."* Gates check at action dispatch time (the `PermissionGate::check` hook on the bus); they don't enforce after the fact.

Gates are *not* the same as:

- **Trust** — a property of node provenance (semantic). The hyperlink-trust badge in inker is unrelated.
- **Auth** — credentials for a remote service. Vault domain.
- **Capability tokens** (UCAN-shaped). The action bus is in-process; tokens are for remote / federation.

Gates are the *in-process* policy spine. Federation policy reuses the same vocabulary but lives elsewhere.

## 2. The v0 capability set

Per the framing brief §7, four capabilities are obvious today:

| Capability                   | Target                | Gates                                              |
| ---------------------------- | --------------------- | -------------------------------------------------- |
| `attach.cross_session`       | `Session(id)`         | Switcher attach into a *non-current* session       |
| `engine.route_override`      | `Node(graph, key)`    | Force a non-default engine for an address          |
| `engine.profile.escalate`    | `Session(id)`         | Move a session from PersonaScoped→SessionScoped UDF |
| `worker.start`               | `Session(id)`         | Declare a new `WorkerKind` on the manifest         |

Three more become obvious as the multiplexer matures (named here so they're tracked, not declared landed):

| Capability                  | Target                | Notes                                              |
| --------------------------- | --------------------- | -------------------------------------------------- |
| `broadcast.cross_pane`      | `Frame(id)`           | "Navigate every pane in this frame to X"           |
| `ipc.external_action`       | `App`                 | A remoted bus call from outside the process        |
| `clip.capture`              | `Pane(id)`            | Take an engine-surface clip into the clip vault    |

Each carries a stable string name (the `&'static str` already used by `DenyReason::CapabilityMissing` in the bus). v0 ships the four-row table above; the second table lands variant-by-variant as the producer gestures materialise.

## 3. Policy chain

A gate's decision walks four layers, **first-match wins**:

1. **Action-level** — an action carries explicit denial signals from the caller (e.g. a confirm-modal cancel becomes a `Denied{ReasonUser}` on the bus). Rare; only meaningful when the dispatcher already has user intent.
2. **Session overrides** — `SessionPolicy.overrides` on the session's manifest. Per-session deny/allow lists.
3. **Persona overrides** — `PersonaCapabilityOverrides` from the persona manifest. Defaults the user has set for "this whole persona."
4. **App default** — baked in. v0 defaults are listed in §4.

This order is deliberate: per-action wins, per-session lets a user quarantine one risky session, per-persona expresses "this is my locked-down work persona," and app default is the safety net. Skipping layers (e.g. session overrides without persona overrides) is fine — first match wins.

## 4. v0 app defaults

| Capability                   | App default        | Why                                                                                                       |
| ---------------------------- | ------------------ | --------------------------------------------------------------------------------------------------------- |
| `attach.cross_session`       | `Allow`            | The graph switcher is a first-class UI; attaching across sessions is the point.                           |
| `engine.route_override`      | `RequireConsent`   | Forcing an engine off its default route changes content fidelity + trust posture; one-tap confirm modal.  |
| `engine.profile.escalate`    | `RequireConsent`   | Cookies / logins reshuffle; non-reversible without user action. Confirm.                                  |
| `worker.start`               | `Allow`            | Workers are session-declared in the manifest; the user already opted in by adding the kind.               |

`RequireConsent` is a third decision beyond `Allow` / `Deny` — the bus exposes it as `PermissionDecision::RequireConsent(prompt)` and the host paints a tightly-scoped modal. Phase-2-ish UX work; v0 can degrade `RequireConsent → Deny` until the modal lands. Filed as an open question; pick the degradation when the v0 wiring goes in.

## 5. Override shapes

`SessionPolicyOverride` (currently `pub struct SessionPolicyOverride {}` in `manifest.rs`) needs:

```rust
pub struct SessionPolicyOverride {
    pub capability: String,             // e.g. "attach.cross_session"
    pub decision: OverrideDecision,
}

pub enum OverrideDecision {
    Allow,
    Deny(&'static str),                 // reason string for the diagnostic
    RequireConsent,
}
```

`PersonaCapabilityOverrides` mirrors the shape with `Vec<SessionPolicyOverride>` (same enum reused) so the gate-chain lookup is one branch.

Capability names are `&'static str` in the bus's existing `DenyReason::CapabilityMissing(&'static str)` to avoid heap churn on the hot path, but persisted overrides need `String` (the persona manifest may carry user-set entries the binary doesn't know at compile time). The trait that resolves them coerces.

## 6. Gate enforcement at the bus

`SessionPolicyGate` (the v0 real gate) implements `PermissionGate`:

```rust
pub struct SessionPolicyGate<'a> {
    pub manifests: &'a ManifestStore,
    pub personas: &'a PersonaStore,
    pub app_defaults: &'a AppCapabilityDefaults,
}

impl PermissionGate for SessionPolicyGate<'_> {
    fn check(&self, action: &BusAction) -> PermissionDecision {
        let capability = capability_for(action);
        // 1. action-level (caller-supplied; v0 always returns Allow)
        // 2. session
        // 3. persona
        // 4. app default
        ...
    }
}
```

`capability_for(action)` maps every `(ActionTarget, ActionKind)` pair to its capability name. Some actions map to none (`NavigateTo` is unprivileged); those bypass the gate entirely (the trait returns `Allow`).

## 7. Diagnostic shape

Every denial emits the canonical `permission.denied` event from the framing brief §8:

```text
permission.denied { action_target, action_kind, capability, reason }
```

`RequireConsent` does *not* emit `permission.denied` — it emits a separate `permission.consent_requested` event when the modal opens, then either `permission.consent_granted` or `permission.consent_denied`. Three events, not one, so apparatus can show the consent journey. (These three events extend the framing brief §8 catalogue; pin them when the modal lands.)

## 8. What this brief doesn't decide

- **Modal UX for `RequireConsent`.** Owned by the pane-UX brief; this brief just commits to the `RequireConsent` decision existing.
- **Capability hierarchies.** No "broadcast.* inherits from broadcast.cross_pane." v0 capabilities are flat strings. Hierarchy is a v2 ergonomics improvement; sketched here as a follow-up so the namespace stays prefix-friendly (`engine.route_override` / `engine.profile.escalate` foreshadow this).
- **Capability tokens / federation.** Out of scope. The in-process gate is one layer; UCAN-shaped capabilities for cross-node operations are the federation domain (event-DAG substrate brief).

## 9. v0 → v1 → v2 sequence

**v0a (when the action bus migration is further along):**

- `SessionPolicyOverride` shape lands (struct fields + `OverrideDecision` enum).
- `SessionPolicyGate` replaces `PermitEverythingGate` as `HostRoot.permission_gate`.
- Four capabilities wired: `attach.cross_session`, `engine.route_override`, `engine.profile.escalate`, `worker.start`.
- `RequireConsent` degrades to `Deny` until the modal lands.

**v1 (with the persona manifest):**

- `PersonaCapabilityOverrides` carry persona-level entries.
- "Locked-down persona" preset (every `engine.*` capability set to `RequireConsent`).
- Consent modal UX lands; `RequireConsent` works end-to-end.

**v2 (as the second-table capabilities materialise):**

- `broadcast.cross_pane`, `ipc.external_action`, `clip.capture` wired alongside their producer gestures.
- Capability hierarchies (`engine.*` inheriting from `engine`) ergonomics pass.
- Per-capability defaults configurable from the persona settings UI.

## 10. Open questions

1. **`RequireConsent` v0 degradation: `Allow` or `Deny`?** Pick when v0a wires. Lean toward `Deny` so the user notices the missing modal rather than silently letting the action through.
2. **Per-graph overrides.** v0 has session-level and persona-level. Does graph-level overrides ever earn its weight? Likely no — the graph-scoped UDF case is rare and a graph that needs different gating from its session is rarer. Track in case it does.
3. **Persona-scoped action targets.** `ActionTarget::Persona(PersonaId)` exists; what actions can target a persona? `KillPersona`, `BroadcastInPersona`, `RenamePersona`. Each gets a capability when wired. v0 leaves the target reserved.
