# Runtime Mod Authoring Loop Plan

**Date**: 2026-06-30  
**Status**: Planned.  
**Related**:
[`2026-06-21_command_registry_configurable_menus_plan.md`](../../archive_docs/2026-09-02_retired_plans/2026-06-21_command_registry_configurable_menus_plan.md),
[`2026-06-21_document_script_substrate_plan.md`](../../archive_docs/2026-07-03_completed_plans/2026-06-21_document_script_substrate_plan.md),
[`2026-06-23_document_script_followons_plan.md`](../../archive_docs/2026-07-03_completed_plans/2026-06-23_document_script_followons_plan.md),
[`../research/2026-06-04_resource_coordination_brief.md`](../research/2026-06-04_resource_coordination_brief.md)

This plan owns the "theme-changing ergonomics, but for extensions" loop:
create a mod inside Meerkat, let a model edit it, check/build it, load it at
runtime, test it against a fixture, inspect logs, and reload without restarting
the browser.

The trust split is load-bearing:

- **Rhai** is the local command and scripting lane.
- **Wasm components** are the portable untrusted extension lane.
- The shared contract is the host capability surface, not one source language.

---

## Current Grounding

The command side already has the right shape.

- `crates/meerkat/src/shell_eval.rs` *(historical citation)* <!-- doc-audit: historical-path --> is the privileged omnibar Rhai lane.
- It reads a frozen `ShellContext` and emits a `ShellOutcome`.
- It registers one Rhai binding per `Command::ALL` verb.
- It records arg-bearing requests such as `attach_script("path")`,
  `script_event("kind","payload")`, and `detach_script()`.
- `crates/script/rhai/src/lib.rs` provides the shared sandboxed base engine;
  the note-evaluator lane registers no host bindings.

The Wasm side also has a live seed:

- `DocumentScript` attaches a Wasm component to a focused page.
- Host-side permission resolution gates imports before instantiation.
- Installed mods under `<mere_root>/mods/` can auto-bind as DocumentScripts.
- The follow-on docs already carry the settings-lane grant UI and `net.fetch`
  hardening path.

This plan connects those pieces into a mod authoring loop.

---

## Two Mod Classes

### Rhai Command Packs

A Rhai command mod is local automation. It must keep the current shell shape:

```text
read context -> return requested actions -> host validates -> host executes
```

It should never receive a live host handle. The script receives a
`CommandContext` snapshot, then returns one or more action requests.

Sketch:

```rust
pub struct CommandModManifest {
    pub id: String,
    pub label: String,
    pub source: BlobRef,
    pub entrypoint: String,
    pub commands: Vec<CommandModEntry>,
    pub permissions: Vec<CommandCapability>,
}

pub struct CommandModEntry {
    pub id: String,
    pub label: String,
    pub menu_slot: Option<String>,
    pub palette: bool,
}
```

Rhai source:

```rhai
fn run(ctx) {
    if ctx.selection == "" {
        return text("No selection");
    }

    return action("graph.capture_quote", #{
        text: ctx.selection,
        url: ctx.current_url,
    });
}
```

The host validates `graph.capture_quote` against the manifest, current context,
and persona/session policy before invoking the registry command.

### Wasm Component Mods

A Wasm mod is a capability-scoped component. It targets a WIT world and imports
only the capabilities it is granted.

First worlds:

- `command`: context in, action requests out.
- `document-script`: document events in, document mutations/events out.
- `parser`: bytes/text in, parsed blocks/events/diagnostics out.
- `viewer`: graph/document input in, scene or lens contributions out.

Native browser/engine adapters are not runtime browser mods unless they lower
into one of those existing worlds. A new Illume parser is a good Wasm mod. A new
GPU/web engine is a native adapter unless it exposes only parser/viewer outputs.

---

## Install And Reload Commands

Adding a mod is itself a command. The first command set can be plain:

```text
install_mod(path)
reload_mod(id)
unload_mod(id)
run_mod_command(id, args)
```

Those commands should produce normal registry diagnostics:

```text
meerkat.mod.installed
meerkat.mod.reloaded
meerkat.mod.unloaded
meerkat.mod.command_invoked
meerkat.mod.trapped
meerkat.mod.capability_denied
```

The installed mod record should include:

- mod id, version, source hash, build hash
- author/signing key when present
- provided surfaces
- declared capabilities
- granted capabilities
- last check/build result
- last runtime trap or successful run

---

## Hook Surface

Start small.

Readable context:

- current URL
- selected text and selected node
- focused graph node
- visible document metadata
- settings values the manifest names
- command registry entries visible in this context

Writable effects:

- invoke command registry actions by id
- create graph facts through named host actions
- open panes or focus existing panes
- attach, detach, or send events to scripts
- update mod-local settings

Separate grants:

- filesystem
- network
- storage
- personal mesh jobs
- private graph reads
- credentialed browser fetches

The mod does not get these because it is Rhai or Wasm. It gets them only because
the manifest requested them and host policy granted them.

---

## Pseudo IDE Objects

The runtime authoring loop needs first-class objects, not loose files.

```rust
pub struct ModProject {
    pub id: String,
    pub root: BlobRef,
    pub manifest: ModManifest,
    pub source_files: Vec<SourceFileRef>,
    pub build_profile: BuildProfile,
    pub fixtures: Vec<ModFixture>,
}

pub struct RunSession {
    pub project_id: String,
    pub target: RunTarget,
    pub granted_caps: Vec<GrantedCapability>,
    pub last_build: Option<BuildResult>,
    pub last_run: Option<RunResult>,
    pub logs: Vec<ModLogEntry>,
}
```

The Meerkat pane should show:

- source editor
- manifest and capability requests
- build/check result
- fixture picker
- live target
- console/logs
- reload/rollback controls

The model loop uses the same harness:

```text
ask model -> patch source -> check -> build -> load -> run fixture -> show diff/logs
```

The host owns each step, so model output never becomes ambient authority.

---

## Phasing

P1: Rhai command packs.

- dynamic command entries from a manifest
- `CommandContext` snapshot
- action-request return type
- install/reload/unload commands
- palette/menu exposure through the command registry

P2: Wasm mod projects.

- `ModProject` store
- component build/check command
- attach/run/reload against the current `DocumentScript` path
- fixture runner and logs

P3: Model-assisted authoring pane.

- model patch loop over the project files
- check/build/run buttons
- runtime trap display
- rollback to last passing build

P4: Parser and viewer mod worlds.

- parser fixture: bytes/text to blocks/diagnostics
- viewer fixture: graph/document input to scene/lens contribution
- package/install from the project pane

P5: Native adapter boundary.

- document what cannot be runtime-loaded in a browser
- keep GPU/web engines behind native adapter seams

---

## Done Conditions

- A Rhai command pack installs at runtime and appears in the palette without
  adding a Rust enum variant.
- The command receives a read-only context and returns an action request.
- An unauthorized action request is rejected with a visible reason.
- Reloading the Rhai mod changes behavior without restarting Meerkat.
- A Wasm `DocumentScript` project can be edited, checked, loaded, attached,
  exercised with a fixture, and reloaded from one Meerkat pane.
- A trapped mod is detached or rolled back cleanly, and the logs show the failing
  capability/import/turn.
- A model can patch a mod project and drive the same check/build/run loop the
  user sees.

## Progress

- **2026-06-30** - Created from the Rhai/Wasm trust-boundary discussion. This
  plan preserves Rhai as the local command language while assigning the portable
  untrusted extension boundary to Wasm components and WIT worlds.
