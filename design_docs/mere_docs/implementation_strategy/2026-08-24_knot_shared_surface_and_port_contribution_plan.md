# Knot Shared Surface and Port Contribution Plan

**Date:** 2026-08-24
**Status:** in progress; G0 and K0 implemented, primary contract review
complete, reusable Knot surface running
**Scope:** prove one Knot document surface in a standalone host and Turnstone,
then prove the contribution seam with a second port. This plan does not require
or privilege a `.knot` container format, a subprocess boundary, or a universal
plugin API.

**Related:**

- [Turnstone suite composition and capability census](../../2026-08-22_turnstone_suite_composition_and_capability_census.md)
- [Knot port plan](2026-07-25_knot_port_plan.md)
- [Knot authoring consumer plan](2026-07-27_knot_authoring_consumer_plan.md)
- [Knot in Graphshell plan](2026-08-02_knot_in_graphshell_plan.md)
- [Device resident consolidation plan](2026-08-20_device_resident_consolidation_plan.md)
- [Configuration ownership and settings projection plan](2026-08-06_configuration_ownership_settings_projection_plan.md)
- Turnstone `design_docs/2026-08-08_pane_registry_and_graph_panes_plan.md`
- Genet `docs/2026-08-12_meristem_scope_cut_and_component_contract_brief.md`

## 1. Ruling

Knot is a Mere port and a useful application by itself. Its first product is a
Djot-native editor over files in place, with a graph substrate, local search,
portable referenced evidence, and peer replication. Standalone Knot and
Turnstone consume the same Knot product model and Cambium surface.

Turnstone is the compositor. It owns placement, window and pane lifetime,
focus, layout, hit testing, theme, AccessKit hosting, and shell policy. Knot
owns document authority, edits, saves, revisions, evidence, replication, and
refusals. Djinn owns the selected persona's durable resident stores and network
runtimes when that resident mode is selected.

The first forcing surface is `knot.document.v1`: one selected document with an
editor body and a compact status header. The header reports source identity,
format, dirty state, write posture, and the last save outcome. Whole-vault,
watcher, replication, evidence, and sharing dashboards follow after this seam
works in two hosts.

## 2. Findings

### 2026-08-24: the closed seam is larger than `PaneRenderer`

Turnstone's `PaneRenderer` and `BUILTIN_PANES` are closed, but the retained
runtime is also closed. `PaneRenderers` stores one concrete `HashMap<PaneId,
T>` per pane type, and rendering, input, scroll, visibility, and eviction fan
out over those maps. Replacing only the registry enum would leave the runtime
closed.

The live pane plan assigns the namespaced `External` source and built-in
registry to landed A1. A2 remains the graph runtime pool and `PaneId` /
`graph_id` propagation lane. This plan owns the port-contribution seam. A Knot
surface uses A2 only when it actually needs multi-graph context.

### 2026-08-24: `AnyView` does not erase a product session

Cambium's `AnyView` erases a concrete child tree while keeping its `State` and
`Action` types static. Turnstone cannot store arbitrary product-owned runners
as `Box<dyn AnyView<...>>` without adopting each product's state or action
enum. The erased boundary must surround the retained runner itself.

### 2026-08-24: description and runtime are different contracts

The earlier `SurfaceContributionV1` mixed stable metadata, a Rust factory,
live capability facts, commands, and settings. That shape cannot be one
serializable record and would duplicate existing command and settings seams.
The corrected boundary has three parts:

1. a data-only `SurfaceDescriptor`;
2. a provider factory that admits a source and returns a retained session or a
   typed unavailable result;
3. an object-safe retained Cambium session whose concrete product state and
   actions remain hidden.

Turnstone A6 remains the shell command/provider registry.
`genet-host-api::SettingsProvider` and `SettingsRef` remain the settings seam.
A surface descriptor may name those providers or references later; it does not
copy their rows or values.

### 2026-08-24: Knot has pieces, not one global status model

`KnotEditor`, the file writer, vault, watcher, sync store, evidence custody,
and publishing routes are live, but there is no single truthful snapshot that
folds all of them. Requiring that snapshot before the first standalone UI would
hide a large integration project inside a status pane. The first model is one
document session. Broader status is promoted one authority at a time.

### 2026-08-24: Djot is the first authoring route

The newer Knot lane brief rules that authored format is Djot and that `.knot`
remains only a compatibility spelling until a later portable-object consumer
requires more than Djot plus content references. Older completed plans and the
live `KnotEditor` still privilege `.knot`. K0 corrects the live admission path
and uses `.djot` for its receipt; this composition work does not revive the
superseded format assumption.

### 2026-08-24: the editor model is already duplicated

Genet's `knot-editor-host::KnotEditor` already owns the authority-free
`TextInput`, dirty baseline, edit commands, and derived readout, and Turnstone
uses it. `ports/knot::KnotEditor` independently repeats that state while adding
file authority. K0 changes the port editor into a file-authority wrapper around
the Genet model. The new document session composes that wrapper; it does not
create a third source buffer or editor core.

### 2026-08-24: the standalone process mode must be explicit

Local directory mode may hold its authority in the standalone Knot process.
Persona-vault and replicated modes use the one Djinn-owned resident session
when configured. A host must not open a second owner over the same resident
stores. The first receipt uses a selected local file, so it does not depend on
Djinn or a subprocess.

### 2026-08-24: Rootstock already owns the standalone host topology

Genet's `cambium-rootstock` already owns windowing-neutral retained layout,
hit testing, pointer/key/IME/wheel routing, focus, accessibility, and frame
lifecycle. `cambium-genet-winit-host` supplies the desktop event source and
presentation adapter. Standalone Knot should be a product state, view, and hook
set on that host. It must not create another event loop, layout wrapper, or
input vocabulary. G0 erases the concrete runner boundary only; Rootstock and
the host continue to route events around it.

### 2026-08-24: the UI and host must borrow the same text input

The first contract review found one adapter requirement, not a new model. A
Knot component renders the existing `TextInput`, and Rootstock's focused-text
hook must mutably reach that exact input for IME, selection, and caret defaults.
`KnotEditor` and `KnotDocumentSession` therefore expose shared and mutable
borrows of the delegated input. They do not expose a replacement setter or a
second text value.

The retained runtime needs no Knot method. A product-owned constructor can
build its concrete `GenetAppRunner`, consume Knot intents inside its own view,
then return `Box<dyn RetainedSurfaceSession>`. Standalone Knot may use the same
concrete state and view directly with Rootstock's generic host, while Turnstone
holds the erased form.

## 3. Boundary

```text
Knot source authority
        |
KnotDocumentSession
snapshot + typed intent
        |
product-owned Cambium runner
        |
Box<dyn RetainedSurfaceSession>
        |
standalone Genet host | Turnstone pane host
```

### 3.1 Data-only description

`genet-host-api` owns product-neutral descriptive vocabulary:

```text
SurfaceDescriptor {
    provider_id
    surface_id
    label
    accepted_source
    roles
    multiplicity
    placement_hint
    potential_capabilities
}
```

This sketch is illustrative. G0 establishes the compile-ready names.

The descriptor contains stable facts. Current availability belongs to an
admitted session. Executable factories, command handlers, setting values,
resident handles, and product snapshots do not live in the descriptor.

### 3.2 Retained Cambium session

Cambium owns an object-safe `RetainedSurfaceSession` because it already owns
`GenetAppRunner`, DOM event dispatch, focus, and retained component state. A
session exposes:

- its descriptor and current typed availability;
- the retained `DomHandle` and root node used for rendering, accessibility,
  and automation;
- viewport synchronization;
- dispatch after the host has translated native input and resolved a target;
- host effects such as redraw, cursor change, and navigation.

The host still owns layout, hit testing, scene conversion, scrolling policy,
AccessKit root geometry, and lifecycle. Product intents are consumed inside
the product session. Only generic host effects cross back out.

The first implementation erases two different concrete `GenetAppRunner`
types into one collection. That compile receipt is the gate. A trait that only
wraps a fake view is insufficient.

### 3.3 Knot document model

Knot owns:

- `KnotDocumentSnapshotV1`: source identity, display label, document format,
  text, selection, dirty state, write posture, last save outcome, and an
  optional typed refusal;
- `KnotDocumentIntentV1`: edit and save for the first slice;
- `KnotDocumentSession`: opens or creates the existing `KnotEditor`, emits the
  snapshot, applies intents through that editor, and never creates a second
  source buffer.

Close and reopen are host lifecycle operations. Save-as waits for the native
file-selection contract rather than putting platform paths into a portable UI
event.

## 4. Parallel workload

### G0. Neutral descriptor and retained runtime erasure

**Owner:** Genet.
**Files:** `components/genet-host-api/{lib.rs,surface.rs}` and
`components/cambium/cambium/src/{lib.rs,surface.rs}` only, plus focused tests
inside those files.
**Runs beside:** K0.

Implement the data-only descriptor and typed availability in
`genet-host-api`. Implement the object-safe retained-session boundary in
Cambium, using existing Cambium, Rootstock, and host input/effect vocabulary
and the retained DOM. Do not duplicate Rootstock's layout, hit testing, raw
input translation, focus policy, or accessibility machinery.
Prove two concrete runner state/view types coexist as boxed sessions and retain
independent state.

Done when the focused Genet tests compile and pass, the host API remains below
Cambium in the dependency graph, and the runtime trait does not mention Knot,
Turnstone, `sceno::Scene`, winit, or platform AccessKit adapters.

### K0. Narrow Knot document snapshot, intent, and session

**Owner:** Mere `ports/knot`.
**Files:** `ports/knot/src/{lib.rs,document_surface.rs}` and the smallest
necessary additions to `ports/knot/src/editor.rs` only.
**Runs beside:** G0.

Implement the first product model over the existing `KnotEditor`. The session
opens one local `.djot` file, reports its snapshot, applies edit/save intents,
and returns typed refusal for an unavailable save target. Remove the editor's
current `.knot`-only admission rule; legacy `.knot` may remain accepted, but
the first receipt and default product path are Djot. It must use `TextInput` as
the sole mutable source. Refactor the port editor to delegate that buffer,
dirty baseline, and derived readout to `knot-editor-host::KnotEditor`; the port
wrapper keeps path, format, and write authority.

Done when a focused test opens a real fixture, observes clean state, edits,
observes dirty state, saves, drops the session, reopens the file, and observes
the saved text and clean state. A scratch session must refuse save honestly.

### I0. Contract review and adapter decision

**Owner:** primary architecture lane.
**Depends on:** G0 and K0.

Review the concrete APIs together. Remove vocabulary that exists only for one
consumer. Confirm the Knot session can be wrapped without exposing
`KnotDocumentIntentV1` to the host. Record any compile-driven correction here
before Turnstone work begins.

Done when the wrapper can be described without a Knot-specific method on the
runtime trait and without a Turnstone action type in Knot.

### S0a. Reusable Knot document component

**Owner:** Knot.
**Depends on:** I0.

Build the shared Cambium document component over `KnotDocumentSession`. The
component contains the editor and compact status header, consumes save intents
inside product state, and publishes both a concrete state/view pair and a
constructor for the erased retained-session form. It reuses the one delegated
`TextInput`.

Done when a real component test observes edit, dirty, save, and status through
the product state, and a boxed session exposes its descriptor and retained DOM
without a Knot-specific host method.

### S0b. Standalone Knot host

**Owner:** Knot plus a minimal Genet host.
**Depends on:** S0a.

Build a concrete standalone `knot` binary on `cambium-rootstock` through
`cambium-genet-winit-host`. The initial process opens one selected local file
and owns that local session in process. The host supplies file selection,
window lifecycle, Ctrl+S interception, focused-text borrowing, and a semantic
headed receipt around the shared component. It does not reimplement input,
layout, scene conversion, or accessibility.

Done when a headed or app-authored receipt opens, edits, saves, closes, and
reopens one file, plus shows one honest read-only or unavailable state.

### T0. Turnstone contribution consumer

**Owner:** Turnstone.
**Depends on:** S0b and landed A1. A2 is optional.

Add one dynamic retained-session map keyed by `PaneId`, plus provider
registration and source admission. The generic adapter owns Turnstone layout,
hit testing, scene conversion, scroll, focus, accessibility hosting, and
eviction around the boxed session. Mount `knot.document.v1` without a new
`PaneRenderer` arm or a concrete Knot map.

Reuse A6 for commands and A7's `SettingsProvider` direction. Do not add command
or settings arrays to the surface descriptor. Preserve all existing Knot
authority and protocol receipts while changing only composition.

Done when standalone Knot and Turnstone mount the same product session and
Cambium component, and Turnstone renders unavailable and read-only states
through the generic session path.

### P0. Second-provider proof and contract freeze

**Owner:** primary architecture lane plus the selected port.
**Depends on:** T0.

Choose the independent provider with the smallest amount of new UI. Castellan
has a strong secret-free snapshot and typed intents but currently lacks a
Cambium application surface. Distillery or another port may be cheaper if it
already has a retained surface by this gate. The choice is made from the live
tree after T0, not frozen here.

Done when two unrelated products use the same descriptor and retained-session
mechanism, the second adds no provider-specific Turnstone renderer arm, and
the contract is then reduced and frozen.

### F0. Broader Knot surfaces

**Owner:** Knot authority lanes.
**Depends on:** P0 and the relevant resident receipts.

Promote broader status, evidence, and sharing surfaces one authority at a time.
Each snapshot distinguishes absent, denied, locked, stale, unconfigured, and
unhealthy states where those distinctions exist. Mounting a surface grants
projection only; every effect is rechecked by the owning authority.

Done when standalone Knot and Turnstone show the same state transitions and
neither host gains vault, resident, evidence-custody, or publishing authority
through UI admission.

## 5. Merge and ownership rules

- G0 and K0 may edit only their named files. They do not edit this plan,
  repository indexes, root manifests, lockfiles, or Turnstone.
- The primary lane reviews both implementations before either becomes a
  cross-repository dependency.
- Turnstone integration waits because its `Cargo.toml`, `cambium_pane.rs`,
  `chrome_view.rs`, `content.rs`, and `ui.rs` currently contain unrelated
  work. T0 begins after those changes have a clear owner or land.
- Each repository is staged by explicit owned path after a fresh status check.
  Unrelated dirty work is preserved.
- A published-source or cross-repository build is required before changing a
  Git dependency consumer. Machine-local patch redirects do not count as that
  receipt.

## 6. Stop rules

- Do not create or privilege `.knot` for this work.
- Do not serialize a component tree.
- Do not put Rust factories or live capability facts in a data descriptor.
- Do not move Knot authority into Cambium, Turnstone, or Graphshell.
- Do not make A2 own provider contribution.
- Do not duplicate A6 commands or A7 settings.
- Do not build the global Knot status dashboard before the document surface
  proves the retained-session seam.
- Do not freeze the contract before an independent second provider uses it.

## 7. Progress

- 2026-08-24: primary review rejected the first execution order. It found the
  missing retained-runner erasure, split descriptor from live session, narrowed
  the first Knot model to one document, deferred broad status/evidence/sharing,
  and made second-provider selection depend on live UI readiness.
- 2026-08-24: G0 and K0 ran as compile-independent implementation lanes. G0
  added the data descriptor and object-safe runner erasure, including two
  concrete runner types in one collection. K0 added the Djot-first document
  session, shared-editor delegation, real save/reopen receipt, scratch refusal,
  and visible save failure.
- 2026-08-24: primary contract review retained the split boundary and added
  only shared and mutable borrows of the existing `TextInput`. It split S0 into
  the reusable component and its dependent desktop wrapper. The component is
  assigned as the first real erased-session consumer; the desktop wrapper
  follows its concrete API.

## 8. Final done conditions

- standalone Knot and Turnstone use the same Knot document model, intents, and
  Cambium component;
- a boxed retained session hides concrete product state and actions while
  preserving DOM, input, accessibility, automation, and lifecycle behavior;
- descriptors remain data-only, and commands/settings reuse their existing
  host contracts;
- Turnstone adds future port surfaces through registration and admission rather
  than provider-specific renderer branches;
- a second independent provider proves the seam before it is frozen;
- evidence, sharing, and resident status preserve their existing authority and
  custody boundaries;
- deferred broader product work remains explicit in this plan.
