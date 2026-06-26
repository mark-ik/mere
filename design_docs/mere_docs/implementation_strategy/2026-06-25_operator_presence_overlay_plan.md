# Operator Presence — watching an agent (or a guest, or yourself) act in the graph

**Status (2026-06-25):** design. A live, ephemeral, multi-scale focus-presence overlay:
a colour-tagged ring around the node an operator is on, descending into the node to
highlight the document section they are reading. One "operator" abstraction with three
sources: an agent, a co-op guest, and you. The rendering half mostly exists; the new
piece is a small ephemeral presence channel. No presence code yet.

The seed: Zed lets you follow an agent's focus and watch it read your files. The same
should work in a graph-native browser, and it should be the *same* primitive as co-op
guest-operator presence and the same UI as the web-clip element inspector.

## The idea: one operator, three sources, two scales

An **operator** is an identity with a colour, a current focus, and a short trail of recent
focus. Three kinds, one model:

- an **agent** (an armillary actor running a geist) whose colour rings the node it is
  accessing,
- a **co-op guest** (a remote persona) whose persona colour rings the node they are reading,
- **you**, the same model seen from inside.

Presence is **multi-scale**, which is the deepening that makes it more than a dot on a map:

- **Graph scale:** a colour-tagged ring (a cartography overlay) around the focused node.
- **Into the node:** descend to the node's document and highlight the specific div /
  section the operator is on (its a11y element, painted at its rect).

Following focus descends scales: the ring leads you to the node, the highlight shows you
the passage. The camera can follow, the way Zed follows a collaborator, so you watch an
agent walk the graph halo by halo and then watch its attention move down the page.

## Why this is mostly already rendered

- **Node-ring overlay (built).** The orrery has a cartography `Overlay` channel:
  `ClusterHalo { members }` haloes each community in its colour, `BridgeEmphasis` bolds
  structural brokers. The overlays are **position-independent** (they carry node
  references and a colour; each consumer paints them at its own positions, so the orrery,
  the gloss lens, and the minimap all show them). A presence ring is one more variant,
  `OperatorFocus { operator, colour, node }`, fed by a different signal.
- **The agent already has a focus (built).** [`agent_harness.rs`](../../../crates/meerkat/src/agent_harness.rs)
  exposes `AgentObservation { focused_node, surfaces (focused: bool), a11y, … }`, the same
  semantic state Apparatus and the a11y projection consume. The agent tracks what it is
  looking at already, so emitting its focus is surfacing a field it already holds, not new
  instrumentation.
- **A document is already a tree of focusable elements (built).** The a11y projection turns
  a rendered document into a semantic element tree (sections, divs), and the
  [find overlay](../../../crates/meerkat/src/find.rs) already highlights document regions
  at their rects. So "highlight the section an operator is on" reuses the rect-highlight
  path with a different target and colour.
- **Node colours + selection rings + per-pane focus (built).** The visual vocabulary
  (identity colour per node, selection ring, a focus node per pane) is there.

## The web-clip inspector is the same primitive, one scale down

Picking a document element is picking a document element, whether your cursor does it or an
operator's focus does. The web-clip inspector (planned, the
[djot editor plan](2026-06-24_djot_editor_knot_nodes_plan.md) P4/P5: hover-pick an element
on a live scrying tile via `elementFromPoint`, capture its HTML subtree, crop its rect,
`build_clip_knot`, assert `ProvenanceSubKind::ClippedFrom`) **is the element-highlight UI**.
So three features share one element-targeting primitive:

> a focusable **target** (a graph node, or a document element) + its **rect / position** +
> a **coloured highlight**, with the **source** being your cursor, an agent's
> `focused_node`, or a guest's presence event.

Your cursor over a clip target is local self-presence at element scale. An agent reading a
section is agent-presence at element scale. They render the same way. Build the inspector
as the local-cursor source of this primitive and presence is the remote/agent source of
the same code.

## The one thing to build: the ephemeral presence channel

Presence is **live and disposable, not durable.** You do not fold "I looked at node X"
into the event-DAG the way a graph edit is folded, or the log fills with glances. So
presence rides an ephemeral channel: emit it locally (an actor message) or gossip it over
iroh, render it, let it expire, never reconcile it into the durable log. A small stream,
per operator:

```
Presence { operator_id, colour, scale: Node | Element, target_ref, trail: [recent targets] }
```

This is the standard multiplayer-presence shape and it is the only genuinely new piece.

## Build path

1. **Agent-local presence first (no network).** The agent harness already holds
   `focused_node`; emit an `OperatorFocus` overlay at node scale in the agent's colour,
   with a camera-follow option and a fading trail. Buildable on the overlay channel plus
   the harness today. This is "watch your AI think in the graph," shippable before
   multiplayer exists, and it is the more novel half.
2. **Into-node highlight.** Descend to the focused node's document and highlight the
   focused a11y element at its rect (reuse the find-overlay rect path). Ties to the a11y
   projection the agent already reads.
3. **The web-clip inspector** built as the local-cursor source of the same element-highlight
   primitive (djot P4/P5), so the inspector and presence share code.
4. **Co-op guests.** The same `OperatorFocus` overlay fed by a remote operator's presence
   over the ephemeral gossip channel when co-op browsing lands (the graph-mutation events
   sketched in the relational-browse / co-op lane). Fans out across windows like the sync
   chip's MW3 fan-out, so a torn-out window shows the same rings.

## Exists vs gap

**Built / named:** the cartography `Overlay` channel (node halos), `agent_harness`
`focused_node` + the a11y projection, the find overlay (document rect highlight), the
scrying / verso-scry live-tile element primitives, node colours + selection rings.

**Gap:** the ephemeral presence channel + the `OperatorFocus` overlay variant; the
into-node element-highlight wiring; the web-clip element inspector (planned, not built);
and remote presence, which waits on the co-op browsing lane.

## Open questions

- **Multiple operators on one node:** stacked / concentric rings vs a blended colour.
- **Trail length and decay**, and whether the trail is a transient comet or, per the
  agent-as-graph idea, the agent's actual operation nodes (so the same walk is *live*
  presence and a *durable* reasoning graph at once).
- **Privacy:** presence reveals what you are reading, so broadcasting your own focus to
  co-op guests is a privacy choice, not a default. It is a presence-shaped instance of the
  persona privacy dial (the [wallet plan](2026-06-25_persona_wallet_carry_layer_plan.md));
  an agent's presence is local-by-default, a guest's is opt-in per moot.
- **Element-targeting stability:** a document element reference must survive re-layout
  (anchor on the a11y / DOM node, not a pixel rect), the same anchor problem the clip
  provenance and find overlay already face.
