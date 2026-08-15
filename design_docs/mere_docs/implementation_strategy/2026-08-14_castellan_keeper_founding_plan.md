# Castellan Keeper Founding: the Identity Surface Moves to Its Port

**2026-08-14.** Executes the correction recorded in the
[family-shared identity plan](2026-08-08_family_shared_identity_plan.md):
graphshell's H4 identity surface is, by the
[port law](../../2026-08-12_family_composition_thesis_brief.md), castellan's
surface — the stack owns the capability (`personae`), the port is its
first-party embeddable embodiment (castellan), and applications compose the
relevant subset (graphshell). Keeping it in graphshell is the failure the law
names: a capability "technically reusable but practically trapped inside its
first application." Mark ruled the founding 2026-08-14 ("let's found").

Anchors: [credential port + gazette brief](../research/2026-08-10_credential_port_gazette_brief.md)
(the split and the vocabulary, ratified 2026-08-10), the
[composition thesis](../../2026-08-12_family_composition_thesis_brief.md)
(the port law and the "castellan owns no repository" correction).

## What moves, and where it lands

Three modules, moved whole with their tests, under one new castellan feature
`keeper` (the office; it covers both halves the brief names):

| From (graphshell) | To (castellan) | What it is |
|---|---|---|
| `src/identity.rs` | `src/view.rs` | The secret-free read model: `IdentitySurfaceSnapshot`, vault/profile/key/device views, `load_carry_view`. The "embeddable half renders *about* secrets and never contains them" contract, as types. |
| `src/identity_projection.rs` | `src/projection.rs` | The cards, the intents (signing decisions, SSH generate/import/remove, device revoke, profile switch, profile create), `render_identity_surface`. |
| `src/native/personae_host.rs` | `src/authority.rs` | `PersonaeHost`: the resident authority. Holds the vault, serves the SSH agent, brokers approvals, applies intents. The authority half. |

Graphshell keeps thin re-export shims at every old path, so no call site in
its endpoint, native hosts, receipt bins, or tests changes. Graphshell also
keeps what is genuinely composition rather than capability:

- `identity_endpoint.rs` — serving the surface over graphshell's projection
  protocol is graphshell-as-host.
- `native/identity_ui.rs` — the native file-picker/passphrase dialogs compose
  castellan's authority with a desktop dialog backend (`light_file_dialog`);
  a host concern.
- `profile.rs` — `GraphshellIdentity` is the application's own identity
  composition (which persona *this app* speaks as).

## Decisions

- **Wire strings keep their values.** The intent/schema constants
  (`graphshell.identity.*`) move but do not rename. They cross the projection
  protocol to admitted browsers and appear in receipts; renaming is a wire
  vocabulary change with its own blast radius and nothing forcing it today.
  Open item: whether the namespace should become `castellan.*` in a
  deliberate wire revision.
- **`keeper` is one feature, not two.** The brief's embeddable/authority split
  suggests two, but the projection already renders pending signing requests
  (`personae::signing`, agent-gated), so the halves share the heavy
  dependency today. Split when a consumer exists that wants views without
  the agent stack (a web host is the likely one).
- **Castellan stops being a name reservation.** Its manifest description and
  README change from "Name reservation" to a dated statement of what is
  implemented: OTP, the reticulum credential seam, and the keeper surface.

## Verified

Four feature combinations, because adding an optional-dep feature is the way
to break the ones nobody runs: default (29 tests), `station-grants`, `keeper`
(46 tests), and `keeper,station-grants`. Graphshell: 106 lib tests, all bins
under `personal-sync`, and the persona-switch receipt rerun green while
importing castellan directly.

Every consumer of the three moved paths is inside graphshell itself (the
endpoint, `browser_host`, `device_broker`, `session_loop`, `app`, and four
receipt bins). No sibling repo names them, so the shims are the whole
compatibility surface.

## Resolved after the move

**`identity_endpoint` stays in graphshell, and the line is not close.** It
names ~23 `chirograph` types plus `graphshell_endpoint`'s serving
machinery (`ProjectionCatalog`, `ProjectionSource`, `IntentSink`) and the
sceno/scenotime scene stack. Castellan's projection names three
(`PortableCardV1`, `CardValueV1`, `ActionFormV1`). The endpoint is the adapter
that *serves* castellan's cards over graphshell's protocol — composition, and
the count is the evidence.

**The `castellan → chirograph` direction is the one real wart, and
it is cheap today.** The three types castellan needs are neutral by content —
"a deliberately small semantic card, not a serialized widget tree" — and the
crate is light (six dependencies: base64, blake3, sceno, scenotime, serde,
serde_json). So nothing is being dragged; what is wrong is only that the
identity port names an application's crate.

Two ways out, both **Mark's call because both are naming decisions**:

1. Extract the card vocabulary (`PortableCardV1`, `CardValueV1`,
   `ActionFormV1`, `ContentHash`) into a neutral crate both depend on. It
   meets the crate bar — a portable subset with a real consumer that wants it
   without the protocol — but it needs a name.
2. Rename `chirograph` to what it is. It is the family's projection
   contract, not graphshell's; the crate is named after the portal that
   happened to define it. This is the same shape of correction the keeper
   founding just made, one layer down.

Doing neither is also defensible while castellan is the only outside consumer.

## Follow-ups, not this pass

- Publishing castellan 0.0.2 with the keeper feature needs chirograph
  publishable first; not scheduled, and entangled with the question above.
- The wire strings (`graphshell.identity.*`) are the third instance of the
  same naming question, at the protocol level rather than the crate level.
  Worth deciding all three together rather than piecemeal.
