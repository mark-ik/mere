> **Recovered research. Never implemented. Not current direction.**
>
> Recovered 2026-08-18 from the git history of the archived `graphshell`
> repository (`Code/archive/graphshell`), whose entire `design_docs/` tree is
> deleted at HEAD, so this text survives only in history.
>
> - Original path: `design_docs/graphshell_docs/research/2026-03-24_tool_comparison_product_lessons.md`
> - Source commit: `401e2fcc`
>
> None of it was ever built as written. It is filed here as research, not as a
> plan and not as a statement of where Mere is going. *Graphshell* throughout
> is the old browser product that was absorbed into Mere, not the remote
> projection host that took the name back on 2026-07-22. The foundation plan
> and PLANNING_REGISTER it defers to were donor files and no longer exist
> locally, so the traceability table at the top points nowhere; the survey
> itself stands on its own.
>
> The competitor set it surveys (TheBrain, Obsidian, Notion, Logseq, Anytype,
> Tinderbox, Roam Research, BrainTool) overlaps the later
> `2026-06-25_borrowed_ideas_brief.md` in this folder, which is the current and
> narrower treatment of the same ground and reached its own ruling on plex
> re-centering. Read this one for the wider survey and for its closing table of
> open product questions, several of which are still open.

---

# Tool Comparison — Product Lessons for Graphshell

**Date**: 2026-03-24
**Tools surveyed**: TheBrain, Obsidian, Notion, Logseq, Anytype, Tinderbox, Roam Research, BrainTool
**Scope**: Product and UX lessons that do not require immediate architecture decisions.
Lessons that DO require architecture decisions have already been incorporated into
`implementation_strategy/plan-featureFirstNowRefactorFriendlyLater.prompt.md`.

This document is a reference for feature planning, UX design, and roadmap sequencing.
It is not authoritative for implementation order — that is the PLANNING_REGISTER.

---

## What Is Already in the Foundation Contract

These lessons from the comparison were significant enough to drive immediate contracts.
They are recorded here for traceability but the decisions live in the foundation plan.

| Lesson source | Decision taken |
|---|---|
| Logseq migration disaster | `Address` enum variants + `NodeId`/`OpId` newtypes locked in Phase 0 |
| Firefox / Anytype process model | IPC crossing types (`ViewerRequest`, `ContentEvent`) must be serializable; named in Contract 3 |
| VSCode extension API | `CanvasTopologyPolicy`, `ViewerProvider`, registry key schemas declared semver-stable |
| Anytype CRDT vs WAL | WAL + `OpId` is Phase 0 foundation; async peer merge semantics explicitly deferred |
| Obsidian plugin ecosystem | Mod API is the ecosystem moat; extension API stability is non-negotiable |

---

## 0. The Closest Direct Analog: TheBrain

TheBrain (TheBrain Technologies, 1998–present) is the most direct precedent for
Graphshell. It is a commercial desktop app ($250/year or lifetime license) where
files, web links, notes, and arbitrary concepts are "thoughts" — nodes in a navigable
graph. The graph IS the primary interface, which is nearly unique among software.

### The plex model — neighborhood always centered

TheBrain never renders the full graph. When you navigate to a thought, it becomes the
center of the plex. Parents appear above it, children below, and "jump thoughts"
(non-hierarchical connections) to the sides. The rest of the graph is invisible. This
is the fundamental UX decision that makes TheBrain scale to extremely large brains —
users report active brains with 100,000–500,000 thoughts.

**Implication for Graphshell**: the full-graph force-directed canvas is correct for
spatial overview and exploration, but a neighborhood-centered mode should be equally
first-class. For large graphs, the default view may need to be the neighborhood of the
active node, with the full graph available as an explicit zoom-out action. This is
also what makes "reveal in graph" meaningful — it reveals you in a neighborhood, not
in a wall of nodes.

### Animated transitions build spatial memory

When you click a new thought in TheBrain, the plex smoothly animates as the new
thought becomes central. The animation communicates the path: "I came from the left,
where the research cluster was." Users consistently report that this animation is why
they can maintain orientation in a large brain. The animation is not decoration — it
is the spatial memory mechanism.

Graphshell's physics engine already produces smooth settlement animations. The lesson:
double-click navigation from the graph canvas to a node pane should animate — the
canvas should zoom toward the node as the pane opens — so the user always knows where
the content they're reading lives in the graph.

### Jump thoughts solve the many-to-many problem with position, not just line style

TheBrain encodes relationship type in spatial position: parents above, children below,
jumps to the sides. Users instantly read the relationship from where a node appears,
not from inspecting an edge label or color.

Graphshell's edge families use color and style to encode relationship type.
Position-based encoding (containment edges pull toward a parent position, association
edges pull laterally) is a further refinement that the physics engine's
`FamilyPhysicsPolicy` already makes possible — the policy weights determine which
direction edges pull nodes.

### The fatal gap: web content requires leaving TheBrain

When you click a web-link thought, TheBrain opens it in your default browser. You
leave TheBrain to read the content. Every web resource in your brain is a dead link
until you switch apps.

This is the single most important gap Graphshell closes. Web content renders inside
the pane while the graph canvas remains visible alongside it. You never leave the
graph. The browsing session IS the graph session. This is not a marginal improvement
over TheBrain — it is a categorical difference in what the product is.

### The lifetime license lesson: ownership creates loyalty

TheBrain offers a lifetime license. Users who paid for it years ago are among the most
loyal users in any knowledge management tool. The reason: they know their data is
theirs forever. They cannot be locked out by a subscription lapse or a service
shutdown.

The open-source equivalent of the lifetime license is local-first data. A user whose
graph lives on their own machine as an open WAL format cannot be locked out. This
should be named explicitly in user-facing descriptions — not as a technical
implementation detail, but as a trust-building product promise.

---

## 1. Spatial UX and Graph Legibility

### The graph view is decorative in every competitor except Graphshell

In Obsidian, Logseq, and Anytype, the graph view is a secondary visualization of a
primarily text- or outline-based system. Most users open it occasionally, find it
interesting, and never use it as a primary navigation surface. Anytype's graph is the
most direct analog — and even there, users default to Sets and Collections.

Graphshell's bet is that web content *living inside* graph nodes changes this dynamic.
You are not organizing notes about the web — you are browsing from within the graph.
This is a meaningful differentiator, but it means the graph must be legible to a new
user in a way that none of the competitors' graphs need to be.

**Implication for first-launch UX**: the opening state of Graphshell should be a small,
clearly navigable graph — not a blank canvas. A first-time user should understand
spatial browsing within the first session without reading documentation.

### Graph degrades at scale in every competitor

Obsidian's graph becomes unusable around 1,000 nodes. Logseq's is similar. Anytype's
graph view is rarely opened with large vaults. None of them have solved this.

Graphshell has the right architectural answers: Barnes-Hut physics, active/warm/cold
node lifecycle, and planned LOD rendering. But these must be tested under realistic
load before users hit the wall.

**Proposed acceptance criterion** (for a future milestone gate, not Phase 0):
> The graph view remains interactive and navigable with 500 active nodes and 2,000
> warm/cold nodes rendered at reduced fidelity. This must pass before a public release.

### "Reveal in graph" / "Reveal in pane" as a first-class navigation primitive

Obsidian has "Open in graph" and "Reveal file in explorer." Users rely on these to
navigate between the spatial and content representations of the same thing.

Graphshell's Address-as-identity model makes this possible structurally. The action
"reveal the node for the current pane in the graph canvas" and its inverse "open a
pane for the selected graph node" should be first-class keyboard actions from the
start, not features added after the graph is functional.

### Neighborhood view as a legibility mechanism

All four tools have a version of "show me just the connections around this node":
Obsidian's local graph pane, Logseq's linked references, Anytype's relations list,
Notion's backlinks. The full graph is incomprehensible at scale; the neighborhood view
is how users navigate it.

Graphshell's Lasso Zoning and filter scopes are the right structural answer. But
"show neighborhood of selected node" should be a named, keyboard-accessible action,
not just a consequence of manually configuring a filter.

---

## 2. Views as Projections of Graph Data

### Multiple views of the same data is a product primitive

Notion is the clearest example: the same database renders as table, kanban, calendar,
gallery, or timeline — all from the same underlying records. Users take this for
granted and are frustrated when tools lack it.

Graphshell's architecture already separates graph truth from workbench layout. This
makes multiple view types structurally possible. The planned views to eventually
support:

| View type | Notes |
|---|---|
| Graph (spatial) | Current primary view; force-directed |
| List | Flat or grouped list of nodes; easiest to build |
| Timeline | Nodes arranged on a time axis via traversal/creation timestamps |
| Outline / tree | Hierarchical projection via containment edge family |
| Kanban | Nodes in columns by a semantic tag value |

None of these need to be built now. The point is that the data model is already
correct for supporting them — the view type must never be encoded in the graph truth.

### Sets / named persistent filter views

Anytype's most-used feature. A Set is a saved, named filter query: "all nodes of type
Task where tag = Active." Users live in Sets far more than in the graph view.

Logseq's query blocks and Obsidian's Dataview plugin fill the same role for their
users.

The equivalent in Graphshell is a named, persistent filter scope that survives
session restores and can be pinned to a pane or frame. This is not a Phase 0 feature,
but the filter scope data model should be designed to support persistence without
requiring a graph schema change when it lands.

### Outline / tree is a first-class planned projection

Logseq's insight: bullet hierarchy is a graph with a specific edge type
(parent → child via containment). A tree view and a graph view are dual representations
of the same data.

Graphshell's `ContainmentRelation` and `ArrangementRelation` edge families are exactly
this. A tree/outline view of nodes connected by containment edges is a natural and
useful projection. The Navigator subsystem already projects relations into a tree —
making this a first-class user-facing view is a low-cost extension once the edge
families are stable.

---

## 3. Quick Capture and Onboarding Friction

### The "inbox" / scratch node problem

Logseq's daily journal is its most important onboarding tool. New users can capture
content immediately without deciding where it belongs in a graph. Organization is
deferred — you capture now, connect later.

Obsidian has a similar pattern via the Quick Capture plugin and daily notes.

Graphshell's spatial model has natural friction here: before you can add a node, you
need somewhere to put it. For a new user with an empty graph, this is a blank canvas
problem.

**Proposal**: a designated "inbox" node or frame that accepts URL drops, typed URLs,
and clipped content without requiring graph placement. Nodes captured to the inbox are
unconnected; the user connects them to the graph when ready. This is conceptually an
unfiled node state, which the active/warm/cold lifecycle can accommodate.

### Templates for quick-start node patterns

All four tools have templates. Notion's template gallery is a community distribution
mechanism. Obsidian's core templates and Logseq's built-in templates reduce friction
for recurring node patterns (research session, project, person, etc.).

Graphshell's mods system is the right vehicle for distributing templates. A mod can
contribute named node templates (initial tags, initial edge patterns, initial pane
layout) via the registry. This is a natural extension of the mod system once it is
stable.

---

## 4. Data Format as Ecosystem API

### The serialization format is more durable than the code API

Obsidian's community built hundreds of external tools that read markdown files
directly — not through the plugin API, but through the file format. Notion's external
integrations use the HTTP API, not internal types. Logseq's EDN format enabled
community query tools.

The WAL event schema and the node/edge JSON serialization format in Graphshell are
what third-party tools will build on. These formats should be treated as external APIs
and documented as such — not as internal implementation details.

**Implication**: when the WAL format and the `PersistedNode` / `PersistedEdge`
serialization shapes are stable (Phase 2), they should be documented in
`design_docs/` with the same care as the extension API traits. A changelog for
serialization format changes should exist before a public release.

### Block-level identity for clipping

Logseq and Obsidian both operate at block granularity for connections — you can link
to a specific paragraph, not just a page. Anytype's Relations connect specific Objects,
not just pages.

Graphshell's DOM extraction / clipping feature extracts elements from web pages into
the graph. The identity question is: what is the `NodeId` of a clipped element? Is it
a child of the page node? How is it addressed if the source page changes?

The containment edge family handles the structural relationship, but the clipping
feature needs to commit to a sub-node addressing model before it ships. A clipped
element without a stable address relative to its source page becomes an orphan the
moment the source page changes. This is not a Phase 0 concern but must be decided
before clipping ships.

---

## 5. Ecosystem and Extensibility

### The plugin/mod ecosystem is the community moat

Obsidian's plugin ecosystem is why users don't leave. The marginal cost of adding
functionality is near zero for community developers, which means the long tail of user
needs gets filled without Obsidian's core team doing anything. VSCode has the same
dynamic.

Graphshell's mod system via `inventory::submit!` and the registry traits is the right
architecture. The implication: the extension API documentation and onboarding story
for mod authors is as important as the feature implementation. A mod system with no
documented examples is an unused mod system.

### Don't frontload a predefined taxonomy

Anytype ships with ~100 predefined Object Types and unlimited user-definable Relations.
Most new users are overwhelmed by the options before they have done anything useful.

The lesson: start with minimal, obvious node types (Web, Note, File, Applet) and let
semantic tagging and mods add richness. The UDC semantic tagging system is an additive
layer, not a required taxonomy. Users should never need to understand UDC to use
Graphshell for basic browsing.

---

## 6. Sync Architecture Validation

### Local-first is product differentiation, not a technical curiosity

Notion's most persistent complaint is "what if Notion goes down?" Logseq's community
existed partly as a reaction to Notion's cloud dependency. Anytype's entire value
proposition is the local-first alternative to Notion.

Graphshell's local-first WAL model is not just a technical decision — it is the
clearest product differentiator against every cloud-native competitor. This should be
named explicitly in user-facing product descriptions: "your graph is on your machine;
sync is additive, not foundational."

### Sync must be built on a principled foundation

Every tool in this comparison has had sync problems:
- Obsidian Sync has had conflict resolution issues
- Notion is cloud-only with no offline write capability
- Logseq Sync was in beta for years and remains fragile
- Anytype replaced IPFS with a custom CRDT protocol (Any-sync) after IPFS proved
  insufficient for mutable state sync

The WAL + `OpId` (UUID v7) foundation in Graphshell is more principled than most of
these. The known gap — divergent-state merge semantics for async P2P — is explicitly
deferred and named. When Verse sync is actively built, the choice between a merge
policy layer on top of the WAL and adopting an existing CRDT library (Automerge,
Yjs, or similar) should be evaluated against what the iroh and Matrix integration
already provides. Do not build a custom CRDT.

---

## 7. Other Notable Tools

### TheBrain's historical precedent: TouchGraph (2002–2006)

TouchGraph was a Java applet that visualized Google search results and Amazon
recommendations as interactive force-directed graphs you could navigate by clicking
nodes. It was commercially deployed, used by real users, and directly validated the
"web content as a navigable force-directed graph" concept. It shut down not because
the concept failed but because it was an overlay on top of normal browsing — you
clicked a node and it opened in a separate browser window. The context switch killed
the experience.

The lesson: the concept is sound and has been proven commercially. The reason
Graphshell can succeed where TouchGraph couldn't is that Servo-inside-the-graph
eliminates the context switch. TouchGraph failed at the same seam TheBrain fails at:
leaving the graph to view content.

### Gource — file system as animated force-directed graph

Gource visualizes version control history as an animated force-directed graph of
files and contributors. It is not a file explorer or a browser — it is a read-only
visualization. But watching a Gource render of a large project immediately communicates
what Graphshell is trying to do for browser sessions. The files cluster by directory,
edges appear and disappear as commits land, and you can see the shape of development
over time.

The relevant lesson: Gource proves that even technical audiences who work with files
every day find a force-directed graph VIEW of those files illuminating — not because
the graph is their primary interface, but because it reveals patterns that a file tree
hides. Graphshell's graph canvas should be equally illuminating about browsing
patterns: which domains cluster together, which nodes are hubs, which periods of
activity produced which clusters.

### Tinderbox — agents and multiple views

Tinderbox (Eastgate Systems, 2002–present, Mac-only, $249) is a powerful note
management tool with a long history and a loyal power-user community. Two features
are directly relevant to Graphshell:

**Agents validate the AgentRegistry concept.** Tinderbox agents are rules that
automatically move or tag notes based on their attributes. Users who discover agents
describe them as "the feature that makes Tinderbox irreplaceable." The lesson: users
want the graph to organize itself according to rules they define, not just physics.
Graphshell's planned AgentRegistry (background agents that observe graph state and
emit mutations) is exactly this, and Tinderbox proves it is a high-retention feature
once users discover it.

**Multiple views (map, outline, chart, treemap, timeline) validate the unified view
model.** Tinderbox shows the same notes in five different view types depending on
the task. Users do not want one view — they want the right view for the current
cognitive mode. This is direct validation for `unified_view_model.md`: the data is
one thing, the view is another.

### Roam Research — flat structure, backlinks, block references

Roam Research (2020) popularized the "daily notes + bidirectional links" model that
Logseq and Obsidian then adopted. Its specific contributions:

**Flat structure + backlinks = the graph IS the organization.** Roam has no folders.
Everything is a page and the emergent structure comes from backlinks. This is a bold
bet that the graph structure is sufficient organization without an explicit hierarchy.
For users who adopt it, the graph view becomes genuinely useful — not decorative —
because every connection in the graph was explicitly made by the user. Graphshell's
traversal edges create the same emergent structure automatically from navigation
behavior, which is even lower friction.

**Block references at scale create a dense, connected knowledge graph.** Roam's block
reference feature (embed any block from any page anywhere) creates extremely dense
graphs where the same content appears in multiple contexts. For Graphshell, the NodeId
model handles this for whole nodes. Block-level transclusion (a DOM-clipped fragment
appearing in multiple node contexts) is a future capability but the architectural
foundation — stable NodeId + containment edges — is already correct.

**The community creates the workflows; the tool provides the primitives.** Roam's
\#roamcult community created Zettelkasten, P.A.R.A., and dozens of other workflows on
top of Roam's primitives. The tool did not ship these workflows — the community built
them. Graphshell's mod system is the correct vehicle for this. Do not ship a workflow.
Ship powerful primitives and let the community create workflows as mods.

### BrainTool — structured browser tab organization

BrainTool is a browser extension (Chrome, Firefox) that organizes open tabs as an
org-mode-compatible outline in a sidebar. It is a much lighter-weight tool than
TheBrain — no spatial graph, just a structured tree — but it addresses the same core
problem: browser tabs lack metastructure.

**The market for structured tab organization exists and is underserved.** BrainTool
has a small but loyal user base of technical users who want more structure than tab
groups but less overhead than TheBrain. Tree Style Tab (Firefox) has a large user
base for the same reason. This is the market Graphshell is targeting — users who have
felt the pain of flat tab management and are willing to use a more structured tool.

**Manual categorization is a tax that causes abandonment.** BrainTool requires users
to assign tabs to topics manually. Many users start enthusiastically and abandon after
a few weeks because the categorization overhead is too high. Graphshell's automatic
edge creation from navigation history (traversal edges) reduces this friction — the
graph structure emerges from usage rather than from explicit categorization. The first
graph a user sees should already have meaningful structure from their navigation
behavior, not an empty canvas requiring manual organization.

---

## 8. Summary of Open Product Questions

These are not architecture decisions but should be answered before the relevant
feature lane starts.

| Question | Relevant feature |
|---|---|
| What is the first-launch graph state? | Onboarding UX |
| What is the inbox / scratch capture model? | Quick capture, clipping |
| What is the sub-node address model for clipped elements? | DOM extraction / clipping |
| What filter scope data needs to survive session restore? | Named persistent filter views (Sets) |
| At what node count does the graph view become a milestone gate? | Performance / LOD |
| What is the WAL format versioning and changelog policy? | Storage, external tooling |
| What is the first mod template to ship as a reference implementation? | Mod ecosystem onboarding |
