# Turnstone Suite Composition and Capability Census

**Date:** 2026-08-22  
**Status:** direction, discussed with Mark 2026-08-22; no code task opened here.  
**Scope:** State the current suite as sovereign, embeddable ports; correct the
Graphshell, Castellan, and device-resident split; define the browser taxonomy
that lets Turnstone compose the suite; and distinguish missing ports from
capabilities that merely lack a handler or reusable surface.

**Related:**

- [family composition thesis](2026-08-12_family_composition_thesis_brief.md)
- [application prospects brief](2026-07-24_application_prospects_brief.md)
- [Graphshell reference host plan](mere_docs/implementation_strategy/2026-07-27_graphshell_reference_host_plan.md)
- [device resident consolidation plan](mere_docs/implementation_strategy/2026-08-20_device_resident_consolidation_plan.md)
- [leverage census](2026-08-10_leverage_census_brief.md)
- [Pelt and Knot direction](https://github.com/merely-made/genet/blob/main/docs/2026-07-24_pelt_knot_direction.md)

## 1. Ruling

Turnstone is the flagship composition of a suite of independently useful
tools. It is not the authority for every capability it presents.

Each port has two product obligations:

1. a coherent sovereign tool that can be used on its own where that is useful;
2. an embeddable application surface that another host can compose without
   copying the port's policy or taking custody of its source truth.

Turnstone supplies browsing context, graph context, arrangement, navigation,
automation, and browser taxonomy. A port supplies its own facts, actions,
status, settings, and views. Djinn keeps device services alive after every
visible client closes.

```text
domain authority or application mere
                 |
       admitted Graphshell session
                 |
      product model and typed actions
                 |
        reusable Cambium surfaces
             /            \
     sovereign host     Turnstone
```

The standalone host and Turnstone must consume the same product model and
surface. A second implementation with similar labels does not pass this test.

## 2. The suite

| Tool | Plain job | Authority | What Turnstone composes |
| --- | --- | --- | --- |
| Graphshell | The overmap and local/remote Mere manager | Its local Mere and saved projection preferences | Addresses, relations, provenance, remote lenses, transfers, and handler routing |
| Knot | The word processor, notebook, clipper, and small document IDE | Djot source, document history, merge, references, and Knot sharing policy | Authoring, preview, links, replication, evidence, and publishing controls |
| Pelt | The tiled, nesting document viewer and browser | Per-host browser session and persistence | Document sessions, nested tiles, and engine-backed viewing |
| Distillery | The inference works | Model jobs, leases, manifests, retention, and device policy | Model selection, job status, results, streaming output, and later training |
| Tabard | The appearance workshop | Authored theme definitions | W3C Design Tokens, CSS custom properties, theme structs, and live preview |
| Castellan | Identity and credential management | Personae-backed credential authority and consent | Secret-free identity views, grants, signing, OTP, and approval ceremony |
| Signalman | Device and radio management | Signalman's device-data Mere plus Retinue and Linkboy authority | Inventory, topology, firmware, telemetry, messages, and recovery |
| Djinn | The user-scoped device resident | Process lifetime and exclusive ownership of resident stores and services | Status and management projections only; the resident itself has no document UI |
| Turnstone | The composed workbench and graph web browser | Turnstone browsing state and its own Mere | Context, arrangement, navigation, browser chrome, and composition |

Sibling applications such as Woodshed, Hocket, Isometry, Cleromancy,
Mesocosm, and Paredros retain their own meres and workflows. Turnstone may
mount their granted projections through Graphshell. They do not become
Turnstone ports merely because they can be viewed there.

## 3. Corrections to older posture

### Graphshell becomes smaller and clearer

The 2026-07-27 reference-host plan assigned Graphshell three roles that later
work has separated:

- Graphshell remains the overmap, local Mere host, remote projection client,
  transfer surface, and handler router.
- Castellan owns the identity and credential product surface. Graphshell can
  compose Castellan as its first host.
- Djinn owns the desktop resident composition, application door, route
  catalog, store lifetimes, and orderly shutdown.

The old Graphshell receipts remain evidence for the resident and identity
implementations. Their product ownership labels are superseded by this split.

### Djinn is currently a stale reservation

`ports/djinn` currently describes itself as a possible public name for Knot.
That meaning is obsolete under this ruling. Djinn is the logical
user-scoped resident already embodied first by `graphshell_device_host` and
the device-resident consolidation work.

Desktop Djinn is a background process. Mobile and sandboxed applications may
embed the same resident library. The invariant is one owner for each durable
resource, rather than one executable shape.

### Turnstone composes ports, not their executables

Pelt, Knot, Distillery, Castellan, and Signalman may each have a standalone
host. Turnstone consumes their shared models and surfaces. It does not spawn a
desktop application merely to draw one of its panes, and it does not fork a
private version of the UI.

## 4. What earns a port

A capability warrants a port when most of these are true:

1. It owns a coherent domain truth, authority, or long-lived workflow.
2. The workflow is useful outside Turnstone.
3. It needs product-specific status, settings, errors, and lifecycle.
4. It can expose a coherent embeddable surface without transferring custody.
5. A second host or concrete second-host plan exists.

Use a smaller home when those conditions do not hold:

| Need | Correct home |
| --- | --- |
| Render another media type or protocol | Inker engine or Pelt handler |
| Reusable control or layout pattern | Cambium component |
| Background service lifetime | Djinn service |
| Local or remote application projection | Graphshell session and surface |
| Turnstone-specific browsing workflow | Turnstone |
| Product action offered for an addressed resource | Handler offer plus typed intent |
| Shared operational activity | Djinn status projection or Turnstone Steward surface |

This keeps the port list from becoming a catalog of every ordinary desktop
feature.

## 5. Browser and surface taxonomy

Turnstone should select a surface from facts, capabilities, and user policy:

```text
address
  -> content class and media type
  -> trust, authority, and provenance facts
  -> advertised capabilities
  -> compatible surface offers
  -> user or workspace handler preference
  -> mounted session
```

The initial surface roles are intentionally plain:

- **view**: read or play the resource;
- **edit**: change it through its authority;
- **manage**: administer the owning system or domain;
- **inspect**: show provenance, status, structure, and diagnostics;
- **infer**: run or apply a model;
- **style**: author or apply presentation;
- **share**: disclose or publish under an authority;
- **automate**: grant and run a bounded behavior.

One resource may offer several roles. A Djot document can offer Pelt for
`view`, Knot for `edit`, Distillery for `infer`, and Tabard for `style`
without acquiring four identities. Graphshell retains the address,
provenance, access history, and handler preference.

The role list is routing vocabulary, not a universal domain API. Each selected
surface still speaks its own typed facts and intents.

## 6. The missing composition seam

Turnstone's live pane registry is close to a contribution contract, but it is
still closed:

- every pane id and schema is in the `turnstone.*` namespace;
- `BUILTIN_PANES` is a static table;
- `PaneRenderer` is a hardcoded enum;
- shell rendering matches each renderer in Turnstone;
- port-specific services such as Knot publishing live in Turnstone modules.

This permits a clean internal pane inventory but does not yet let a port offer
one reusable surface to its standalone host and Turnstone.

The missing seam is a narrow surface contribution record containing:

- provider and stable surface id;
- supported source and surface roles;
- source schema and typed snapshot version;
- multiplicity and placement hints;
- command contributions;
- settings contribution, if any;
- a retained component factory or admitted-session factory;
- capability and unavailability facts.

The host remains responsible for window placement, focus, theme, accessibility
hosting, and command aggregation. The provider remains responsible for its
view state and product actions.

Knot should be the forcing consumer. Its status surface, editor, evidence
view, and sharing controls are already split across Turnstone, while the
standalone package is still a stub. Distillery or Castellan can then prove the
contract is not secretly a Knot interface.

## 7. Missing ports

### 7.1 Communications: earned now

The substrate already exists in Murm, Stickleback, `mere-comms`, Gaz, and the
smolweb exchange lanes. Turnstone already registers a Comms pane, but its live
renderer falls through to a labeled placeholder. Signalman also needs
messages and voice drops.

A communications port would own the first-party workflow for:

- direct and invitation-scoped conversations;
- store-and-forward mail and live exchange;
- message history, drafts, delivery, refusal, and retry;
- attachments through shared content custody;
- identity and contact selection through Personae and Gaz;
- voice and calls when the transport and media receipts support them.

It should expose a conversation list, message surface, composer, call state,
and honest delivery status. Turnstone and Signalman are concrete second-host
consumers. This is more than a Comms pane and less than making Graphshell a
second communications authority.

Working ownership: Murm owns conversation exchange; the application owns its
conversation Mere; Castellan and Gaz supply identity and contacts; Djinn keeps
the services alive.

### 7.2 Community and governance: earned as a distinct surface

Moot, Moothold, and Gemot already own governed spaces, membership,
constitutions, moderation, recognition, tessera, and federation. The missing
piece is a coherent first-party application surface.

It should cover:

- find, preview, join, leave, and reconnect ceremony;
- membership and role inspection;
- proposals, decisions, moderation, and appeals;
- storage and compute contributions through Gemot;
- space health, replication, and reachability;
- community content browsing without collapsing every object into chat.

Communications composes inside this surface, but governance does not belong to
the communications port. Graphshell can show a moot in the overmap; the
community surface explains and manages what makes it a moot.

### 7.3 Contacts and directory: probable, boundary to prove

Castellan manages your identities and credentials. Gaz keeps who you know;
Gazette resolves names and handles into trust-stated endpoints. The current
suite has pickers and identity cards but lacks a complete contact workflow.

The first implementation should be a shared Dramatis-facing surface for:

- contacts, petnames, handles, endpoints, and trust state;
- kith and kin grouping;
- discovery and resolution receipts;
- merge, replacement, revocation, and stale-endpoint handling;
- recipient selection reused by Knot, Comms, Signalman, and sharing flows.

It becomes a separate port only if the standalone address-book workflow and
authority boundary prove useful. Until then it can remain a shared surface
consumed by Castellan and the communications port.

### 7.4 Granted agent and automation workshop: probable

Servitor, packs and mods, Genet Probe, typed petitions, watches, transcripts,
and Distillery inference supply most of the substrate. What is absent is the
human-facing place where a person creates, grants, runs, observes, interrupts,
and dissolves an agent.

This is distinct from Distillery. Distillery executes model jobs; the workshop
governs actors that may use models and act in applications.

A first surface needs:

- agent identity and purpose;
- granted reads, writes, actions, and watches;
- model and tool selection;
- run history, pending petitions, refusals, and costs;
- pause, revoke, retry, and dissolve;
- exact attribution into the target application's history.

The port is earned when one bounded agent completes a useful workflow in two
hosts through the same grant and observation surface. Before that receipt,
keep the capability in Servitor, Castellan, Distillery, and host automation.

## 8. Capabilities that need exposure, not another port

### Surface contribution and handler choice

The static Turnstone pane registry and hardcoded renderer enum need the narrow
contribution seam in section 6. `register-protocol` and `register-viewer`
should converge on the same handler decision, with a visible per-role user
preference and a per-resource override.

### Knot application surface

Knot's editor, status, publishing, shared-reader, and evidence work is spread
between `ports/knot` and Turnstone. The planned shared Knot UI plus a real
`knot-editor` host exposes the capability without creating another port.

### Djinn health and service management

The resident needs one typed status model for routes, services, storage,
replication, updates, and shutdown. Turnstone's Steward and device-receipt
panes currently expose narrow slices. Djinn should supply facts; clients may
render a compact status strip or full management surface.

### Recall, memory, and search

Turnstone's Alembic pane remains a placeholder even though Eidetic traces,
lexical recall, embeddings, graph engrams, memory levels, and Athanor plans
exist. This belongs in Graphshell and Turnstone as a recall and memory
surface. It does not need a sovereign port unless it gains an independent
workflow and authority.

### Readability and extraction

`genet-extract` and its Fleece successor already define live document
extraction. Expose Reader View, article extraction, table extraction,
structured clipping, and export through Pelt, Knot, and Turnstone handlers.
Fleece remains an engine capability rather than an application port.

### Files, downloads, transfers, and custody

Pelt can view files, Graphshell can transfer addressed content, Knot retains
referenced blobs, and Turnstone's Steward shows durable download records. The
missing piece is a shared custody surface showing location, owners, hashes,
availability, transfer progress, retention, and release. Graphshell and Djinn
are its natural hosts.

### Packs, mods, and application grants

Servitor, the participant gate, registries, and app admission already provide
the underlying grammar. A shared install and permission surface belongs with
Castellan and Djinn, with product-specific catalogs contributing offers. A
generic app store port would be premature.

### Contacts and recipient pickers

The persona picker and future contact picker should be shared Cambium
components. Every application should consume the same identity and recipient
selection behavior instead of drawing a private list.

### Operational activity and notifications

Steward was intended to combine synchronous and asynchronous status. It now
shows download custody only. Extend it over typed provider activity from
Djinn, Knot, Distillery, Graphshell, and Signalman. OS notification delivery is
a host service over those facts, rather than a new product.

### Diagnostics and developer tools

Apparatus, engine observables, capture/replay, accessibility trees, and Genet
Probe already form a strong inspection substrate. Turnstone and Pelt need a
real shared diagnostic surface. This is a browser/toolkit capability until a
standalone inspection workflow proves otherwise.

### Maps, tables, timelines, and graph arrangements

Atlas, rosters, data grids, timelines, and analytic graph layouts are
presentation and arrangement families. Signalman, Graphshell, Knot, and later
modeling applications should consume them. Each new arrangement does not earn
a port.

## 9. Capabilities still outside the current product floor

These are genuine gaps, but current evidence does not justify ports yet:

- table-first quantitative and flow modeling;
- rich audio, image, and video creation rather than viewing and attachment;
- general public-site deployment across several authoring sources;
- calendar, task, and project-management workflows;
- a terminal, debugger, and full software project model;
- large-scale civic telemetry and geographic operations beyond Signalman's
  owner-scoped map.

Knot can cover lightweight tables, tasks, code blocks, and publishing. Pelt
can cover viewing and playback. A new port should wait for a workflow where
those hosts become structurally wrong, not merely less specialized than an
incumbent application.

## 10. Practical next decisions

1. Amend the Graphshell product description around overmap and routing while
   preserving its completed receipts.
2. Repurpose `ports/djinn` from the Knot-name stub to the logical resident
   package and composition root.
3. Write and execute the Knot shared-UI plan, using status as its first
   standalone plus Turnstone receipt.
4. Extract the smallest surface-contribution seam proven by Knot; use a second
   current port before freezing it.
5. Found the communications port over Murm and `mere-comms`, with Turnstone's
   placeholder Comms pane and Signalman as consumers.
6. Plan the Moot/Gemot community surface separately from messaging.
7. Prototype the shared contact surface before deciding whether it warrants a
   sovereign Dramatis-facing port.
8. Keep the agent workshop behind one bounded two-host workflow receipt.

## 11. Done conditions for the composition thesis

The suite composition is real when:

- the same Knot status and editor components run in standalone Knot and
  Turnstone;
- Djinn remains live while every client UI is closed and exposes one typed
  health model to both;
- a Pelt-compatible viewer and Knot editor open the same addressed document
  under separate `view` and `edit` offers;
- one second port contributes a surface without adding a product-specific
  renderer arm to Turnstone;
- Graphshell opens local and remote application meres while retaining only
  its own graph truth and preferences;
- Castellan manages identity without Graphshell or Turnstone holding secret
  material;
- Signalman remains usable for recovery with Turnstone absent;
- handler selection, status, errors, accessibility, commands, and settings
  survive both sovereign and composed hosts.

