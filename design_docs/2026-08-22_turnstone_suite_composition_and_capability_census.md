# Turnstone Suite Composition and Capability Census

**Date:** 2026-08-22  
**Status:** direction, discussed with Mark 2026-08-22; no code task opened
here. Amended same day (with Mark): the single Moot port ruling (§7.1–7.2),
the gazette port ruling (§7.3), the Alembic workshop ruling (§7.4, §8), the
place-ruling reversal recorded (§3), and the pane-registry cross-reference
(§6). Executed 2026-08-23: Moot and Alembic founded as stubs, gazette
promoted from crate to port. Published 2026-08-24: `mere-moot` 0.0.1 and
`mere-alembic` 0.0.1. Receipts on each item in §7 and §10.  
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

**Sharpened 2026-09-02 (Mark).** Graphshell is a *viewer and redirector*, not
a surface that hosts content for interaction — with one exception, which is
the point: the projection itself, of which Graphshell is the preeminent
manipulator. Viewing is fully in scope: with Genet in the mix, Graphshell
reads simple structured content through Workbench and Genet for free, and it
would make no sense to exclude that. What it does not host is the browser
proper — the engines and the taxonomy of a browser expressed through Mere,
which are Turnstone's. It is fine to read your content in Graphshell. That is not nothing. Authenticated access to all the data of all
the applications, used to customize projections to your own taste, is a
**home-page graph of app graphs**; and that capability is meant to appear in
every application that has its own app graph, not only in Graphshell. This is
the family composition thesis's anti-shell test in its positive form: the
manipulator is a platform capability whose receipt is a second host. One
consequence it settles the same day: the Projection Editor, today a
self-contained module in `ports/graphshell`, is **Scenograph** — the name the
[boundary plan](mere_docs/implementation_strategy/2026-09-02_platform_boundary_and_repository_topology_plan.md)
§1 and P2 free by dissolving the generic `scenograph` facade into Cambium and
reserve "for the scene/projection editor product, built with Cambium rather
than naming the scene runtime itself." Scenograph is a Mere product crate,
Graphshell is its preeminent host, and it appears in every application with
its own graph. The published `scenograph 0.0.3` is the facade; the name is
re-pointed by a real publish of the editor, never by reservation alone. And
Graphshell may compose Workbench for reading without becoming Turnstone, which remains the
meerkat successor shell with the browser engines proper. Knot can use
Workbench too (Mark, same day); Workbench is a component any of them takes,
not a mark of which one is the shell.

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

### The place ruling is reversed (amended 2026-08-22, with Mark)

The [Turnstone place port plan](../../turnstone/design_docs/2026-07-28_turnstone_place_port_plan.md)
ruled that a place is "a Turnstone product composition over reusable
Mere-side domains … not another application or authority layer." Founding the
Moot port (§7.1–7.2) reverses the *application* half of that ruling: the
first-party composition of gemot, commons, murm, and stickleback into a
places surface now lives in a port, which Turnstone composes like any other.
The *authority* half stands unchanged — gemot decides governance, commons
owns the shared graph, murm owns conversation exchange, stickleback owns
retained-operation processing; the port owns no new authority.

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

The source half is planned already (lane correction 2026-08-24): the
[pane registry and graph panes plan](../../turnstone/design_docs/2026-08-08_pane_registry_and_graph_panes_plan.md)
(landed A1) removes `System` and replaces `Custom(String)` with a namespaced
`External` pane source, which covers source identity. A2 is the graph runtime
pool and `PaneId` / `graph_id` propagation lane. It does not own provider
admission. The dedicated
[Knot shared-surface plan](mere_docs/implementation_strategy/2026-08-24_knot_shared_surface_and_port_contribution_plan.md)
owns that seam and uses A2 only for a surface that needs multi-graph context.

The corrected seam separates a data-only surface description containing:

- provider and stable surface id;
- supported source and surface roles;
- source schema and typed snapshot version;
- multiplicity and placement hints;
- potential capabilities.

An application-owned provider admits a source and returns an object-safe
retained session or a typed unavailable reason. Current capabilities belong to
that live session. Commands continue through Turnstone's shell provider
registry, and settings continue through `SettingsProvider` / `SettingsRef`, so
the surface contract does not duplicate either registry.

The host remains responsible for window placement, focus, theme, accessibility
hosting, and command aggregation. The provider remains responsible for its
view state and product actions.

Knot should be the forcing consumer. Its status surface, editor, evidence
view, and sharing controls are already split across Turnstone, while the
standalone package is still a stub. Distillery or Castellan can then prove the
contract is not secretly a Knot interface.

## 7. Missing ports

### 7.1 Communications: earned now

**Ruled 2026-08-22 (with Mark), amending this section and 7.2:** one port,
**Moot**, carries both. The package is `mere-moot` with `[lib] name = "moot"`
— the `mere-signalman` / `mere-gloss` pattern — because crates.io `moot` is
held by an unrelated crate with real code (Battle-Creek-LLC's meeting bot,
0.1.0, 2026-04-29; crates.io does not reassign names for inactivity, and the
owner has not responded to a public ask) and `murmur` is likewise taken. The
port exposes two surfaces: **murmur**, the conversation surface this section
describes, and **moot**, the community surface of 7.2. The granularity
constraint is load-bearing: murmur must mount alone, because Signalman wants
messages and voice drops without governance UI. `mere-comms`
(`crates/shell/comms`) is the murmur surface's model rather than mere
substrate — a WASM-clean inbox (`Conversation`, `Message`, `Draft`,
`ProtocolAdapter` with murm and misfin adapters) that the leverage census had
marked "fold or retire." Domain authority is unchanged: murm owns exchange,
gemot owns governance; the port composes both and owns neither.

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

**Ruled 2026-08-22:** this is the moot half of the single Moot port (see
7.1). "A distinct surface" stands; a distinct *port* does not.

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

**Ruled 2026-08-22 (with Mark):** this is the **gazette port**, founded on
the dramatis tier beside castellan (`crates/dramatis/gaz` + `gazette` +
feeds, with `mere-crawl` as the feed engine per the
[leverage census](2026-08-10_leverage_census_brief.md)). The two readings in
this section are the port law's two halves rather than alternatives: the
shared Dramatis-facing surface — contact cards and the recipient picker — is
the port's embeddable half, and resolution, feed polling, and trust state
are its authority half, hosted by Djinn (see the
[Djinn family resident services plan](mere_docs/implementation_strategy/2026-08-22_djinn_family_resident_services_plan.md)).
The boundary proof asked for above is the second-host test already: the
picker consumed by Knot, Moot, and Signalman.

**Executed 2026-08-23, by promotion rather than founding (ruled with Mark).**
The port could not take the plain `gazette` package name, because the
dramatis resolver crate already held it — package and lib both, published,
484 lines, no library consumers — and bare `gazetteer` is held by a stranger,
so freeing the name by renaming the resolver was unavailable. Ruled: the
resolver *is* the port, one directory earlier than expected.
`crates/dramatis/gazette` moved to `ports/gazette`, keeping its package name,
version, and code; the manifest, README, and module doc were reframed to the
port identity, and the workspace member and dependency entries follow. The
reasoning is the 2026-08-10 brief's own: the word's three senses — an index,
being gazetted, and the paper you read — were the roadmap, and a roadmap
inside one word does not want two crates. This is a deliberate exception to
the port-is-not-its-domain-crate pattern castellan and distillery follow, and
`knot` is its precedent (one port holding directory, vault, editor, sync, and
search). What is built is unchanged — WebFinger resolution — and what is
unbuilt is now stated on the port: the contact and recipient surfaces over
`gaz`, feed polling over `mere-crawl`, and the reading room.

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

**Ruled 2026-08-22 (with Mark):** the workshop is **Alembic**. What makes
this one port rather than a bundle: Athanor was always an agent — "the
steady background furnace that consolidates memory and mints distillates
while you work" is a bounded background actor under a grant — so the
workshop generalizes the furnace, admitting more actors under Servitor
grants, with agent continuity supplied by the same engram and tulpa
machinery. The split follows the castellan mold: the embeddable half is the
recall and memory surfaces of section 8, behind a `recall` feature so a host
can take memory without the workshop; the authority half (Athanor, agent
grants, runs, petitions, revocation) lives with Djinn, whose
[resident services plan](mere_docs/implementation_strategy/2026-08-22_djinn_family_resident_services_plan.md)
already names Athanor jobs. Distinct from Distillery exactly as stated
above: Distillery runs models, Alembic runs work. The package is
`mere-alembic` with `[lib] name = "alembic"` (crates.io `alembic` is the
Linux Foundation's VFX-format binding and will not free). The two-host
receipt above stands as the founding gate, not a reason to defer founding
the stub.

**Re-ruled 2026-09-02 (Mark): Alembic goes inside Distillery as a core
component crate.** The 2026-08-22 separation of the two *ports* is
superseded; the distinction it drew survives as a component boundary inside
Distillery — Distillery runs models, and its Alembic component runs work over
them. What carries over unchanged: the `recall` feature as the embeddable
memory half; the two-host receipt as the founding gate; and the package
identity `mere-alembic` / `[lib] name = "alembic"`, which a move inside
Distillery's tree does not need to change (package identity is not a reason to
preserve the wrong boundary, per the boundary plan). Both questions the
re-ruling left open were ruled and executed the same day. **Athanor goes to
Distillery**, the domain, not Djinn, the resident: the argument for Djinn was
lifetime and scheduling ("Djinn contains the scheduler; Athanor is one
scheduled service"), and Djinn itself already separated ownership from
hosting for the works — `resident_distillery` composes Distillery's authority
and invents nothing — so the same precedent puts Athanor's authority (what a
furnace pass is, what it may propose, the grants, runs, petitions, and
revocation over bounded actors) in Distillery, with Djinn scheduling it and
keeping the proposals-not-truth invariant. **Shape:** flat under the port, as
`ports/graphshell/web` and `ports/knot/desktop` already are —
`ports/distillery/alembic` (moved) and `ports/distillery/athanor` (founded);
no intermediate `ports/` or `crates/` directory. **Names:** `mere-alembic` /
`alembic` unchanged; `mere-athanor` / `athanor`, free on crates.io at the
ruling and claimed the same day: `mere-athanor 0.0.1` published from mere
`c0216cad`.

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
workflow and authority. **Amended 2026-08-22:** it gained one — the workshop
ruling in 7.4 makes these surfaces Alembic's embeddable half; the hosting
here (Graphshell and Turnstone as consumers) is unchanged.

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
5. Found the Moot port (`mere-moot`, lib `moot`) over Murm, Gemot, and
   `mere-comms`, with the murmur surface mountable alone; Turnstone's
   placeholder Comms pane and Signalman are its consumers. *(Amended
   2026-08-22: one port, two surfaces, per 7.1. **Stub founded 2026-08-23**
   at `ports/moot`, MPL-2.0, registered and compiling; no implementation.
   **Published 2026-08-24** as `mere-moot` 0.0.1, the name claimed per the
   heddle lesson.)*
6. Plan the moot community surface as that port's second surface, separate
   from the murmur surface. *(Amended 2026-08-22.)*
7. Found the gazette port; its picker consumed by Knot, Moot, and Signalman
   is the boundary proof 7.3 asked for. *(Amended 2026-08-22. **Executed
   2026-08-23 by promotion**, per the naming ruling in 7.3: the resolver
   crate moved `crates/dramatis/gazette` → `ports/gazette`, keeping its
   package name and code, with the manifest, README, and module doc reframed
   to the port. Compiles from its new home; the picker and feed surfaces
   remain unbuilt.)*
8. Keep the Alembic workshop behind one bounded two-host workflow receipt;
   the receipt is its founding gate, per 7.4. *(Amended 2026-08-22. **Stub
   founded 2026-08-23** at `ports/alembic`, MPL-2.0, with the `recall`
   feature declared and empty; the two-host receipt still gates
   implementation. **Published 2026-08-24** as `mere-alembic` 0.0.1. One
   correction was needed first: the fleece scope's F5 pass had added
   `fleece.workspace = true` to the stub, which nothing in it calls, and an
   unused dep on a versionless branch-git entry cannot be published at all.
   Ruled with Mark: drop the dep, since a reservation stub ships empty
   `[dependencies]` as chatelaine and insigne do, and re-add it with a version
   when alembic has distillation code that consumes an `Article`. The other
   three F5 consumers — crawl, eidetic-search, gazette — are untouched.)*

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
