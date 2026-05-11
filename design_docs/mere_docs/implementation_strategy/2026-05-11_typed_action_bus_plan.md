# Typed action bus — refactor plan

**Date**: 2026-05-11
**Status**: Implementation plan — pre-build
**Scope**: Introduce a typed action bus with target-scoped dispatch in `mere-host`. Every keybinding, palette invocation, AccessKit action, and future IPC call routes through it. Replaces the current pattern of direct `cx.dispatch_action` calls with a single dispatch path that has explicit places to attach permission gates and diagnostics. Precedes (and is a hard prerequisite for) the capability-gate catalogue and any external IPC.

**Related**:

- [`../research/2026-05-11_browser_multiplexer_framing.md`](../research/2026-05-11_browser_multiplexer_framing.md) — §5.8 (command bus), §7 (security principle), §8 (diagnostics). This plan operationalises those sections.
- Current actions: [`crates/mere-host/src/actions.rs`](../../../crates/mere-host/src/actions.rs).
- Current dispatch points: [`crates/mere-host/src/lib.rs`](../../../crates/mere-host/src/lib.rs) (the `cx.listener(...)` blocks in `Render::render`).
- Palette dispatch: [`crates/mere-host/src/bootstrap.rs`](../../../crates/mere-host/src/bootstrap.rs) (`cx.dispatch_action(action.as_ref())`).

---

## 1. Goal + done conditions

**Goal:** route every action invocation through one typed dispatch function that knows the action's target, can check permission, and emits a diagnostic event — without changing what users see today.

**Done when:**

- A `BusDispatch` (or similar) function exists in a new module: `mere-host/src/action_bus.rs`.
- Every existing action defined by the `actions!` macro (`Quit`, `FocusOmnibar`, `OpenPalette`, `GoBack`, `GoForward`, `Reload`, `CycleShellbarPosition`, `CycleWorkbenchStripPosition`, `OpenNewWindow`, `ToggleWorkbench`, `ToggleGloss`, `ToggleApparatus`, `SummonOrreryForNewGraph`, `ToggleGraphSwitcher`, `TearOutTileToNewGraphMinimized`, `TearOutTileToNewGraphVisible`, `TearOutTileAsStickyNote`) has a corresponding `ActionKind` variant **and** a target scope on dispatch.
- Every keybinding handler that calls `cx.dispatch_action` (or invokes `HostRoot` methods via `cx.listener`) routes through `BusDispatch` instead.
- Palette invocation (`bootstrap::run`'s `PaletteInvoke` handler) routes through `BusDispatch`.
- A pluggable `PermissionGate` is attached to the bus; v0 implementation is a permit-everything gate (so behaviour is unchanged) — but the hook is there for the capability-gate catalogue to attach a real implementation.
- A diagnostic event (`action.dispatched`, with `target`, `kind`, `result`) fires on every dispatch. `action.denied` fires when the gate refuses.
- Mere's full keymap still works exactly as before. No user-visible behaviour change.

**Explicitly NOT in scope:**

- External IPC (the eventual serialized shell over the bus). Future work.
- The full capability-gate catalogue. This plan builds the *spine*; the catalogue brief enumerates the gates and lands the v1 `PermissionGate` implementation.
- Rewriting the gpui `actions!` macro types or replacing gpui's action system. The bus *wraps* gpui dispatch; it doesn't replace it.
- AccessKit action wiring. The Windows AccessKit adapter is still stubbed; bus integration there lands when the adapter does. The bus is designed to support it.
- Bus serialization to disk / wire format. Not needed until IPC.

## 2. Why a typed bus, not just gpui actions

gpui's `actions!` macro generates Rust types and lets `cx.dispatch_action` route them to handlers. That works fine for simple in-process dispatch. It doesn't:

- Know who/what an action is targeting (window? pane? specific session? all sessions in a persona?).
- Provide a uniform place to attach permission policy.
- Provide a uniform place to emit diagnostics.
- Provide a uniform place to expose the action via future IPC or scripting.

A thin typed layer on top of gpui actions gets all four for the cost of one indirection.

## 3. The types

```rust
// in mere-host/src/action_bus.rs (new module)

/// Where an action operates. Composable with `kind` to address any
/// scope from "everything" to "this specific node."
#[derive(Clone, Debug, PartialEq)]
pub enum ActionTarget {
    /// Action affects the whole application. Quit, open-new-window,
    /// app-wide settings.
    App,
    /// Action scoped to a persona — e.g., persona-level UDF
    /// management, persona switching. Reserved; no v0 actions use it.
    Persona(PersonaId),
    /// Action scoped to a session. Most multi-window operations
    /// (open-new-window-for-session, kill-session, fork).
    Session(SessionId),
    /// Action scoped to a frame layout. Layout templates, save/restore.
    Frame(FrameId),
    /// Action scoped to a single pane. Close-pane, focus-pane,
    /// toggle-panel-here, tear-out.
    Pane(PaneId),
    /// Action scoped to a graph node. Reserved; node-level actions
    /// (rename, pin, follow) will fill this in.
    Node(GraphId, mere_kernel::graph::NodeKey),
    /// Action scoped to a verso-tile surface. Reserved.
    Surface(SurfaceId),
}

/// What the action does. One variant per current and planned action.
/// Variants carry only the arguments not already encoded in the
/// target — the target says *who* the action affects; the kind says
/// *what* happens to them.
#[derive(Clone, Debug, PartialEq)]
pub enum ActionKind {
    Quit,
    FocusOmnibar,
    OpenPalette,
    OpenNewWindow,
    GoBack,
    GoForward,
    Reload,
    CycleShellbarPosition,
    CycleWorkbenchStripPosition,
    ToggleWorkbench,
    ToggleGloss,
    ToggleApparatus,
    SummonOrreryForNewGraph,
    SummonOrreryForGraph(GraphId),       // graph switcher row click
    ToggleGraphSwitcher,
    TearOutTile { mode: TearOutMode },   // three current modes
    ClosePane,                            // target carries pane_id
    FocusTile { index: usize },           // target carries pane_id
    CloseTile { index: usize },           // target carries pane_id
    NavigateTo { address: String },
    // Reserved for follow-up plans:
    KillSession,                          // manifest plan
    TearOutTileAsLeaf,                    // tearout-operations brief — explicit name
    TearOutTileAsBranch,                  // tearout-operations brief — new graphlet
    TearOutTileAsFork,                    // tearout-operations brief — new session
    PromoteLeafToBranch,                  // toast "Branch" button
    PromoteLeafToFork,                    // toast "Fork" button
    ConsolidateBranchToEngram,            // memory-tiers brief
    ConsolidateForkToEngram,              // memory-tiers brief
    SnapshotSessionToEngram,              // memory-tiers brief
    BroadcastNavigate { address: String }, // multiplexer §5.10
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TearOutMode {
    NewGraphMinimized,
    NewGraphVisible,
    StickyNote,
}

pub struct BusAction {
    pub target: ActionTarget,
    pub kind: ActionKind,
}
```

`SurfaceId`, `PersonaId`, `SessionId` are pulled from their owning crates (`verso-tile`, `mere-host::manifest`, `mere-frame`).

## 4. The dispatch path

```rust
pub fn dispatch(
    action: BusAction,
    cx: &mut App,
) -> DispatchResult {
    // 1. Resolve the dispatcher for this target+kind. For most actions
    //    it's a `HostRoot::*` method; for App-scoped lifecycle it's
    //    an App-level handler.
    let decision = current_gate(cx).check(&action, cx);
    match decision {
        PermissionDecision::Allow => {
            emit_diag(DiagEvent::ActionDispatched { target: action.target.clone(), kind: action.kind.clone() });
            execute(action, cx)
        }
        PermissionDecision::Deny(reason) => {
            emit_diag(DiagEvent::PermissionDenied {
                target: action.target.clone(),
                kind: action.kind.clone(),
                reason,
            });
            DispatchResult::Denied(reason)
        }
    }
}

pub trait PermissionGate {
    fn check(&self, action: &BusAction, cx: &App) -> PermissionDecision;
}

pub enum PermissionDecision {
    Allow,
    Deny(&'static str),
}
```

`execute` is where the action actually runs. v0: a `match` on `(target, kind)` that calls into the existing `HostRoot` methods. As actions move from gpui-direct-dispatch to bus-dispatch, their old listener bodies become arms of `execute`.

`current_gate(cx)` reads the current persona's permission policy from the manifest store. v0: `PermitEverythingGate` (no actual gating, but the indirection is there). The capability-gate catalogue brief lands a real `SessionPolicyGate`.

## 5. Wiring — three call sites

The bus replaces three patterns in the existing code:

### 5.1 Keybinding listeners

Today (excerpt from `lib.rs`):

```rust
let go_back = cx.listener(
    |this, _: &actions::GoBack, _w: &mut Window, cx: &mut Context<Self>| {
        this.go_back(cx);
    },
);
// ... 14 similar listener bodies ...
.on_action(go_back)
// ...
```

After:

```rust
let go_back = cx.listener(
    |this, _: &actions::GoBack, _w: &mut Window, cx: &mut Context<Self>| {
        action_bus::dispatch(
            BusAction {
                target: this.current_action_target_for(ActionKind::GoBack),
                kind: ActionKind::GoBack,
            },
            cx,
        );
    },
);
```

`current_action_target_for` is a helper on `HostRoot` that maps a kind → the natural target given current state (e.g., `GoBack` targets the active workbench's pane; `Quit` targets `App`; `OpenNewWindow` targets `App`).

Refactor pattern: write `dispatch_kind!(this, cx, kind)` macro that produces the listener body, since they're all this shape. Cuts ~50 lines of `lib.rs` listener boilerplate.

### 5.2 Palette invocation

Today (`bootstrap.rs`):

```rust
cx.subscribe(&palette, |_host, _palette, ev: &PaletteInvoke, cx| {
    match cx.build_action(&ev.action_name, None) {
        Ok(action) => cx.dispatch_action(action.as_ref()),
        Err(e) => tracing::warn!(...),
    }
}).detach();
```

After:

```rust
cx.subscribe(&palette, |_host, _palette, ev: &PaletteInvoke, cx| {
    match action_bus::parse_action_name(&ev.action_name) {
        Ok(action) => { action_bus::dispatch(action, cx); }
        Err(e) => tracing::warn!(action = %ev.action_name, error = %e, "palette parse failed"),
    }
}).detach();
```

`parse_action_name` maps the palette's string action name into a `BusAction`. v0: a hand-coded match. Future: derived from the `ActionKind` enum via a `strum` or hand-written round-trip.

### 5.3 Direct host-method invocations from mouse handlers

Shellbar buttons (`render_panel_buttons`) and tile clicks today call `HostRoot` methods directly via `cx.listener`. After the bus lands, those listeners build a `BusAction` and dispatch instead:

```rust
// Before
cx.listener(|this, _: &MouseUpEvent, _, cx| this.toggle_panel(PaneContent::Workbench, cx))

// After
cx.listener(|_this, _: &MouseUpEvent, _, cx| {
    action_bus::dispatch(
        BusAction { target: ActionTarget::App, kind: ActionKind::ToggleWorkbench },
        cx,
    );
})
```

Same applies to graph-switcher row clicks (`SummonOrreryForGraph(id)`), close-pane buttons (`ClosePane` targeted at the specific `Pane(pane_id)`), and tile-strip clicks (`FocusTile { index }` / `CloseTile { index }` targeted at the workbench's pane).

## 6. Target inference — `current_action_target_for`

Many keybinding-triggered actions don't carry an explicit target — Cmd-R reloads "the active surface," not "this specific pane id." A helper on `HostRoot` resolves the implicit target:

```rust
impl HostRoot {
    pub(crate) fn current_action_target_for(&self, kind: ActionKind) -> ActionTarget {
        match kind {
            // App-scoped
            ActionKind::Quit
            | ActionKind::OpenPalette
            | ActionKind::OpenNewWindow
            | ActionKind::ToggleGraphSwitcher
            | ActionKind::CycleShellbarPosition
            | ActionKind::CycleWorkbenchStripPosition
                => ActionTarget::App,

            // Workbench-scoped (active workbench's pane)
            ActionKind::GoBack
            | ActionKind::GoForward
            | ActionKind::Reload
            | ActionKind::FocusOmnibar
            | ActionKind::NavigateTo { .. }
            | ActionKind::TearOutTile { .. }
            | ActionKind::FocusTile { .. }
            | ActionKind::CloseTile { .. }
                => self.active_workbench
                    .map(ActionTarget::Pane)
                    .unwrap_or(ActionTarget::App),

            // Toggle-panel actions affect the window (frame), not
            // a specific pane.
            ActionKind::ToggleWorkbench
            | ActionKind::ToggleGloss
            | ActionKind::ToggleApparatus
            | ActionKind::SummonOrreryForNewGraph
                => ActionTarget::Frame(self.frame_layout.id.clone()),

            // ... etc.
        }
    }
}
```

The point: target inference is centralised. If a keybinding's "natural" target changes (e.g., per-pane navigation history), only this function changes — not every dispatch site.

## 7. Diagnostics

Two new events:

```text
action.dispatched { target, kind }       // every successful dispatch
action.denied     { target, kind, reason } // gate said no
```

These flow into the apparatus event buffer like everything else. Apparatus already lists every captured event; the bus adds two more variants.

Volume concern: high-frequency actions (mouse-driven splitter drag, omnibar keystrokes) shouldn't go through the bus or they'll flood diagnostics. Rule: **the bus carries discrete user-intent-shaped actions, not continuous gestures.** Splitter drag stays where it is. Each keystroke in the omnibar input stays where it is. Submit-on-Enter goes through the bus (it's a `NavigateTo`).

## 8. File-size discipline

Mere's 600-LOC ceiling:

- `action_bus.rs` — target ~400 LOC (types + dispatch + target inference helpers + serde for future IPC).
- If it overshoots, split: `action_bus/types.rs` (enums), `action_bus/dispatch.rs` (execute + gate), `action_bus/serde.rs` (parse_action_name + reverse).

`lib.rs` shrinks slightly — the 14 listener blocks collapse into the `dispatch_kind!` macro applied 14 times.

## 9. Testing approach

**Unit:**

- Every `ActionKind` variant has a round-trip test through `parse_action_name`.
- `current_action_target_for` returns the expected target for every kind, including edge cases (no active workbench → falls back to `App`).
- `PermitEverythingGate` permits everything; a fixture `RefusingGate` denies everything; `dispatch` honours both.

**Integration:**

- Replay a fixture sequence of `BusAction`s; assert resulting `HostRoot` state matches direct-dispatch equivalent.
- Inject a denying gate; assert no host-state changes happen and `action.denied` fires.

**No behaviour regressions:** the existing keybinding integration tests (if any) keep passing; if there are none, this plan adds one for each kind that goes through the bus.

## 10. Sequencing

One commit per logical chunk; each leaves the codebase green:

1. **`action_bus` module skeleton.** Types only (`BusAction`, `ActionTarget`, `ActionKind`, `PermissionGate`, `PermissionDecision`). Unit tests for serde + `Display`/`Debug`. No callers.
2. **`PermitEverythingGate` + `dispatch` + `execute`.** `execute` is empty (`unimplemented!()`) for every kind. Tests cover the gate path and diagnostic emission with a stub `execute`.
3. **`current_action_target_for` helper.** Tested independently.
4. **Migrate one keybinding family at a time.** Group of ~3 kinds per commit. Order:
   - App lifecycle (`Quit`, `OpenNewWindow`).
   - Navigation (`GoBack`, `GoForward`, `Reload`).
   - Chrome (`CycleShellbarPosition`, `CycleWorkbenchStripPosition`, `FocusOmnibar`, `OpenPalette`).
   - Panel toggles (`ToggleWorkbench`, `ToggleGloss`, `ToggleApparatus`, `SummonOrreryForNewGraph`).
   - Graph switcher (`ToggleGraphSwitcher`, `SummonOrreryForGraph`).
   - Tear-out (`TearOutTile`).
5. **Migrate mouse-handler call sites.** Shellbar buttons, tile clicks, switcher rows.
6. **Migrate palette invocation.** `bootstrap.rs` `PaletteInvoke` handler routes through bus.
7. **Cleanup.** Remove direct `cx.dispatch_action` paths; confirm only the bus dispatches actions.

After step 7: the capability-gate catalogue plan can plug its real `SessionPolicyGate` in and have it work everywhere.

## 11. Configurability

Per project preference:

- A user-configurable gate. The default is `PermitEverythingGate`; user-keymap-style config could swap in a `SessionPolicyGate` with custom overrides. Not exposed in v0 (no UI), but the wiring exists.
- Diagnostics verbosity: `bus.diag_level = Verbose | Discrete | None`. `Verbose` logs every dispatch; `Discrete` logs only denials + lifecycle-changing dispatches; `None` disables. Default `Discrete`.

## 12. Risks

- **Action target ambiguity.** "Reload" might mean "reload this specific tile" or "reload every tile in the active workbench" or "reload the active surface." The `Pane` target + the active-workbench fallback are the v0 answer. If multi-pane scoping turns out to matter sooner than expected, the helper grows new branches.
- **Bus overhead.** Each dispatch now goes through a permission check and a diagnostic emission. For the ~16 current actions, this is invisible. If high-frequency actions get added later, they should bypass the bus (or the bus needs a fast-path). Rule of thumb in §7: continuous gestures don't bus.
- **`execute` becomes a god match.** A single match over every `(target, kind)` pair grows linearly. If it pushes over the file-size ceiling, split by target (one sub-module per `ActionTarget` variant).
- **gpui action types still coexist.** The `actions!` macro keeps generating per-action Rust types — those are what keybindings actually fire. The bus is the wrapper, not the replacement. If gpui ever exposes a way to register a global "every dispatched action goes through this hook," the listener boilerplate (`dispatch_kind!`) disappears. Until then, the listeners are the bridge.

## 13. What this plan unblocks

Once the bus lands:

- **Capability-gate catalogue** can ship a real `SessionPolicyGate` and have it apply everywhere.
- **External IPC** becomes a serialized shell over `BusAction` — `mere send-action --target session:foo --kind navigate --to mere://X`.
- **AccessKit action routing** (when the Windows adapter wakes up) lands as one method on the bus, not 16 separate listeners.
- **"Broadcast to all panes"** (multiplexer §5.10) becomes a kind variant + a fan-out in `execute`, not a separate code path.
- **Action recording / replay** (debugging tool; not v1) is implementable by intercepting at the bus.
