# Knot Authoring Consumer Plan

**Date:** 2026-07-27
**Status:** all Knot-owned work in the reconciled sequence is complete locally:
A1 through A4, typed Inspector clip insertion, production Resolve/Run
providers, sanitized HTML lowering, and the sealed attributable resolve cache.
Deterministic and real-process receipts are green, including the OS-headed
Genet Probe drive. Exact selected-range clipping is complete for Genet's
static retained document producer. Livery and scripted documents remain
producer-specific selection seams; Knot already accepts and preserves an
explicit selector without inventing one.

**Companions:** the completed [Knot port plan](2026-07-25_knot_port_plan.md),
the reconciled
[Djot editor and clipping plan](2026-06-24_djot_editor_knot_nodes_plan.md),
the [Graphshell remote projection host plan](2026-07-22_graphshell_remote_projection_host_plan.md),
Genet's `docs/2026-07-25_text_editing_primitive_plan.md`, and the reconciled
[evaluation/export plan](../../nematic_docs/implementation_strategy/2026-06-12_knot_evaluation_export_plan.md).

## Ruling

The missing consumer is a **local Cambium editor over a disclosed,
versioned text resource, with save returning to Knot as a typed Graphshell
intent**.

Source bytes may cross the authorized presentation boundary. File authority,
vault keys, grants, causal history, encryption profiles, and writes remain
inside Knot. Keystrokes, selection, IME, undo, highlighting, outline, folds,
and preview stay local in the retained editor. Save is the domain mutation.

This does not require Graphshell's heavier live-pane protocol. It extends the
existing resource and intent planes:

```text
KnotEndpoint
  -> EditableTextV1 resource
  -> Graphshell client
  -> Turnstone Cambium Knot editor

Turnstone
  -> SaveTextV1 intent
  -> Knot target + grant + base-version validation
  -> disk write or sealed vault + signed sync event
  -> revision bell
  -> Graphshell resume and refreshed resource
```

The portable Graphshell crates know a generic editable-text codec. They do not
depend on Knot, Cambium, Genet, Turnstone, or a document model. Turnstone is the
first retained UI consumer. A Graphshell host without editable-text support
continues to receive the existing card or glyph fallback.

## Starting findings (historical)

These were the live seams when this plan was cut. The current receipt below
records their implementation.

- `graphshell-protocol` 1.1 has native glyph, portable card, and image
  capabilities. It has no editable-text codec or typed save payload.
- `KnotEndpoint` presents cards and glyphs, advertises no actions, and rejects
  every intent as read-only. Its vault disclosure test correctly proves that
  neither key nor authored body crosses today.
- `ports/graphshell::sessions` is a receipt helper, not a product session. It
  spawns an endpoint, resolves resources, invokes advertised actions with empty
  payloads, then shuts the process down. Authoring needs a retained carrier and
  client state.
- `KnotEditor` already supplies the local buffer behavior this consumer needs.
  `KnotEditor::scratch` has no filesystem path, so its local `save()` cannot
  bypass the endpoint.
- Turnstone already has retained Cambium panes, Genet layout/input routing,
  Workbench composition, and Graphshell client dependencies. It has no retained
  external Graphshell session or Knot pane.
- `KnotSyncStore::author` and `author_communal` already mint signed causal
  document events. `KnotVault::put` alone is durable local storage but is not
  the complete replicated write path.
- `knot_endpoint` opens a directory or fixture only. A real sealed-vault
  process launch still needs Personae startup-unlock wiring; a raw key in CLI
  arguments or environment variables is not acceptable.

## Protocol shape

Graphshell gains a minor-version-compatible generic capability:

```rust
PresentationCapability::EditableText
PresentationCodec::EditableTextV1

struct EditableTextV1 {
    address: String,
    media_type: String,
    encoding: TextEncoding,       // v1: UTF-8
    source: String,
    base_token: Vec<u8>,          // opaque, endpoint minted
    derived: Option<DerivedTextV1>, // effect result, optionally sealed by Knot
}

struct DerivedCacheInfoV1 {
    effect: String,
    sources: Vec<String>,
    provider_version: String,
    policy_fingerprint: String,
    fetched_at_unix_ms: u64,
    source_revision: u64,
}

struct SaveTextV1 {
    base_token: Vec<u8>,
    source: String,
}

struct KnotEffectV1 {
    base_token: Vec<u8>,
    confirmed: bool,
}
```

The save action advertises schema `graphshell.editable-text.save/v1` and
`IntentEffect::DomainTruth`. The invocation's target selects the document;
the payload does not repeat a path or vault id.

The opaque base token is document-specific:

- for a file, it binds stable file identity, current bytes, and the endpoint's
  accepted source version;
- for a vault or Commons document, it binds the exact causal head or heads the
  editor observed.

The scene epoch/revision still proves which projection the user acted on. The
base token prevents an unrelated directory change from becoming a false
document conflict and prevents a newer version of the same document from being
overwritten. On an unrelated revision bell, the client resumes and advances
its observed scene revision while retaining a dirty editor whose base token is
unchanged. If that document's token changed, Save returns `Stale` and does not
write.

Editable resources always use `CacheRetention::MemoryOnly` with
`purge_on_revocation = true`. Lock, revocation, session close, and endpoint
death drop the cached source and the retained editor buffer. Persistent
Graphshell caches never contain editable source.

Protocol 1.3 may disclose attribution for a Knot-sealed derived result. The
sealed result remains endpoint-side, bound to the source token and revision,
and never becomes the save buffer. Protocol 1.2 clients still receive derived
text without the 1.3 cache metadata.

## Four rungs

### Current receipt

- A1 is complete: protocol 1.2 carries strict editable-text resources and save
  payloads; protocol 1.3 adds optional derived-cache attribution. Older clients
  retain card/glyph fallback, and the Graphshell host has a retained
  mount/resolve/invoke/resume/close session. Derived text never becomes the
  editor's save buffer.
- A2 is complete for writable directory
  documents and injected personal/communal vault stores. Save validates the
  snapshot target, grant, observed revision, format, size, and document base
  token before writing. Vault save authors one signed sync event and
  rematerializes the sealed view from the accepted projection.
- A3 is complete in Turnstone. One background hub owns the retained stdio
  carrier; each visible document owns one local Cambium `KnotEditor`. Mouse,
  keyboard, IME, undo, highlighting, outline, folds, preview, explicit Save,
  reload, stale conflict state, and revision-bell refresh are wired without a
  carrier round trip per keystroke.
- A4's executable receipts cover real file save/restart, two-client stale-save
  refusal, unrelated directory churn, signed sealed-vault persistence,
  startup-unlocked persona-vault process launch, ciphertext opacity, and
  lock-time purge. Turnstone's `scenarios/knot_authoring.scn` is the final
  OS-headed Genet Probe receipt: it opens the rooted file through the real
  endpoint process, activates the retained content session, clicks Resolve and
  Run by retained-DOM selector, asserts the disclosed and evaluated derived
  text, and captures the composed frame. Probe quiescence reads the Knot
  session's actual in-flight state.
- Inspector clip is complete. Genet sessions emit a host-neutral semantic clip;
  Turnstone sends `knot.clip.insert/v1`; Knot validates provenance, base token,
  grant, size, target, and source URI before appending through the ordinary
  file or signed-vault save path. The static producer now returns exact
  selected text, links scoped to that range, and a typed DOM-range selector.
  It retains whole-document clipping with `selector: None` when there is no
  selection. Livery still supplies that whole-document fallback; the scripted
  lane supplies no clip yet.
- Resolve and Run are complete for the first production capability set.
  Graphshell carries strict `knot.transclusion.resolve/v1` and
  `knot.block.run/v1` payloads. Knot owns `auto` / `ask` / `never`, scheme and
  language allowlists, recursion and Rhai operation budgets, Commons
  received-document confirmation, rooted file fetch authority, and the
  derived revision. Turnstone presents the advertised buttons, sends explicit
  confirmation for Ask, queues Auto on open, and drops a derived preview on
  local edit.
- Sealed resolve caching is complete. Personal-vault results use Personae's
  sealed-record storage. Commons results are additionally wrapped by the
  current group-data epoch and become unavailable after rotation, even while an
  older key remains retained. Entries bind the source token and revision,
  fetched URLs, provider version and relevant configuration, policy
  fingerprint, and fetched time.
  Removing a projected document collects its cache record. Directory results
  remain memory-only because they have no sealing profile. Run results remain
  process-local until an evaluator declares an explicit cacheability contract.
- The real-process effect receipt resolves a rooted Markdown include and runs a
  Rhai fence through the built `knot_endpoint`; both manual Ask and Auto on
  reopen produce derived refreshes while the authored `.knot` file remains
  byte-identical.

### A1. Versioned editable text and a retained session

Add `EditableTextV1`, `SaveTextV1`, the capability and codec tags, serde
fixtures, resource validation, and capability fallback to
`graphshell-protocol` and `graphshell-client`. Advance the protocol minor
version while keeping 1.1 card/glyph clients usable.

Extract a retained local endpoint session from the G4 receipt helper:

- owns `StdioCarrier` and `ClientState`;
- discovers and mounts without invoking actions automatically;
- resolves a selected resource on demand;
- sends typed intents with the client's current epoch/revision;
- consumes revision bells and runs the existing resume path;
- closes the endpoint and purges memory-only resources deterministically.

The existing static G4 receipt becomes a caller of this session rather than the
only session implementation.

**Done when:** a generic fixture advertises editable text plus a card fallback;
an editable-capable client receives and decodes the text; a card-only client
remains usable; malformed text or save payloads fail closed; and closing or
revoking the retained session removes the source bytes from client state.

### A2. Writable Knot endpoint

Give `KnotEndpoint` a snapshot-local `InstanceId -> document` binding and an
injected authoring authority. A document advertises **Edit** and **Save** only
when all applicable conditions hold:

- the target is a supported UTF-8 text document;
- a directory target resolves beneath the configured root, is not ignored, and
  is writable without following authority outside the root;
- a vault is unlocked;
- the principal holds the typed write grant;
- a replicated vault or Commons document has an admitted signer, sync store,
  and encryption profile;
- the projection has one editable causal version. An unresolved multi-writer
  conflict stays visible and does not masquerade as ordinary Save.

On Save, the endpoint validates session, target, advertised action, payload
schema, observed epoch/revision, grant, size limit, UTF-8, and base token before
touching truth.

- A file save uses the existing format-aware writer, preserves the selected
  format, refreshes the directory source, and lets the watcher attribution and
  revision bell report the change.
- A replicated vault or Commons save authors the corresponding
  `KnotSyncEvent::Put` through the existing personal or communal cipher. That
  signed operation is recorded truth; the local sealed document view is
  rematerialized from the accepted projection. The endpoint must not
  independently `KnotVault::put` and then try to author a second truth.
- A changed base token returns `IntentResult::Stale` and leaves current truth
  byte-for-byte unchanged.

The endpoint refreshes its snapshot after an accepted save. The existing
payload-free revision bell tells the client to resume; source bytes never ride
the notice.

The endpoint binary gains explicit directory and vault launch modes. Production
vault launch recovers its root through Personae/session-runtime startup unlock.
Test-only constructors may inject a key directly; command-line arguments and
environment variables may not carry it.

**Done when:** read-only and locked documents advertise no edit/save action; a
writable file and an unlocked granted vault document do; an accepted save
changes exactly the named document and rings once; and stale, malformed,
ungranted, conflicted, or out-of-root saves change nothing.

### A3. Turnstone authoring pane

Turnstone keeps one retained Knot endpoint session and mounts an editable
resource into a Workbench pane. The first implementation may consume
`KnotEditor::scratch(address, source)` directly; because it has no local path,
all durable writes still travel through `SaveTextV1`.

The pane reuses the existing host machinery:

- Cambium `TextInput` and `TextCommand` for committed text, selection, undo,
  and IME;
- Genet/Parley for shaped movement, hit testing, caret and selection geometry;
- `KnotReadout` for highlights, outline, folds, and preview;
- Turnstone's retained pane, focus, compositor, and scenario paths.

Open is an advertised Knot action from the document card. Save is explicit
through the same retained session, initially button plus `Ctrl+S`. Autosave is
a user setting added only after explicit Save is proven; it uses the same
revision and base-token checks.

When a bell arrives:

- an unchanged document token advances the mounted scene revision without
  discarding the dirty local buffer;
- a changed token leaves the local buffer intact, marks it stale, and offers
  reload or explicit conflict handling;
- lock or revocation closes the editor and purges its source immediately.

Turnstone never constructs `KnotEndpoint`, opens a Knot file, calls
`KnotVault`, holds a vault key, or writes a document path.

**Done when:** a real Turnstone pane edits locally with mouse, keyboard, IME,
undo, highlighting, outline, and preview; Save crosses the Graphshell intent
path; and the pane remains responsive without a carrier round trip per
keystroke.

### A4. File and sealed-vault receipts

Run the completed lane against both authorities:

1. open a real `.knot` file through `knot_endpoint`, edit in Turnstone, save,
   receive one bell, resume, restart both processes, and read the saved bytes;
2. open a sealed vault through endpoint-owned startup unlock, edit and save,
   verify ciphertext at rest, reopen, and recover the source only after unlock;
3. open one document in two clients, save from the first, then prove the
   second client's old base token is refused without overwriting;
4. change an unrelated file while the editor is dirty, resume the scene, and
   prove the edited document can still save because its base token did not
   change;
5. for the sealed lane, prove the accepted write appears in the signed
   personal or Commons projection and preserves visible conflict behavior;
6. lock or revoke while the editor is open and prove client resource/cache
   state no longer contains the source bytes.

The headed receipt drives the pane through Genet Probe, not OS-coordinate
guessing. The protocol tests remain headless and deterministic; the final
receipt is a real retained stdio process lane.

**Done when:** file and vault editing survive restart, stale save refuses,
unrelated churn does not destroy work, revision-bell refresh is real, sealed
bytes remain opaque at rest, and the host never receives the vault key.

## Closed follow-ons and remaining producer seam

### Inspector clip action. Complete locally

The first follow-on uses the same retained session, target binding, write grant,
base token, stale refusal, and revision-bell refresh. It gets its own typed
`knot.clip.insert/v1` payload so `ClippedFrom` provenance remains a domain fact;
it is not disguised as an unstructured full-text replacement. The host-neutral
`crates/import::web_clip` producer supplies the semantic clip. Knot validates
and records the document mutation and provenance.

Creating a new clip document requires an explicitly advertised Create action.
Appending to the open document uses the existing target and base token.

### Run and Resolve. Complete for the first production capability set

Transclusion Resolve and block Run reuse the same intent advertisement,
capability, consent, stale-revision, receipt, and revision-bell path from the
[evaluation/export plan](../../nematic_docs/implementation_strategy/2026-06-12_knot_evaluation_export_plan.md).
The result is attached to the editable presentation as derived text tied to the
current base token. It is not written to disk or into a signed vault journal.

Current user settings:

- `TURNSTONE_KNOT_RESOLVE_MODE=auto|ask|never`
- `TURNSTONE_KNOT_RUN_MODE=auto|ask|never`
- `TURNSTONE_KNOT_RESOLVE_SCHEMES` and
  `TURNSTONE_KNOT_RUN_LANGUAGES`
- `TURNSTONE_KNOT_RESOLVE_MAX_DEPTH` and `TURNSTONE_KNOT_RUN_MAX_OPS`

The shipped providers are rooted `file:` transclusion for directory endpoints
and sandboxed Rhai evaluation. Anonymous HTTP(S) and the read-only smolweb
providers now join the rooted-file lane under the same Knot authority. The
html5ever-backed reader fragment lane also sanitizes fetched HTML before Knot
splices derived blocks.

Knot persists successful resolve results only for sealed vault sources. A
personal result uses the vault seal. A Commons result also uses the current
group-data epoch, so rotating that epoch makes the old cache unavailable.
Graphshell 1.3 carries source, provider, policy, revision, and fetched-time
attribution without disclosing an encryption epoch or key. Turnstone shows the
real fetched age. Effect revocation, source revision, policy change, provider
change, lock, and current-epoch change all reduce restoration to a cache miss.
Resolve refresh starts again from authored source; a completely failed refresh
reports the failure and retains a still-valid cached document for offline use.

### Selected-range clipping. Static complete; Livery and scripted remain

Knot's `knot.clip.insert/v1` payload accepts an optional selector and records it
with the source URI. Exact selection therefore stays with the document producer
that owns retained layout:

1. `DocumentSession` carries the ordinary press, move, and release lifecycle
   plus a read-only text-target query for automation.
2. Genet's static incremental layout maps pointer positions to retained DOM text
   byte offsets. The static session owns the anchor and focused range, paints
   the highlight, and emits selected normalized text, a versioned DOM-range
   selector, and links scoped to that range.
3. Genet Probe owns the generic `select-text` gesture. Turnstone only resolves
   one unambiguous live-session target into window coordinates; Probe drives the
   same pointer route as a person.
4. Inspector forwards the resulting `DocumentClip`. Knot validates and stores
   the selector unchanged.

The headed `scenarios/knot_selected_clip.scn` receipt selects only
`only this linked finding`, activates Inspector through Probe's native button
role, and saves through a real writable Knot endpoint. The saved block contains
only that phrase, its in-range link, and a `dom-range` selector; the surrounding
preface and suffix are absent. Unit receipts pin the producer and driver seams
in `static_session_pointer_selection_scopes_clip_and_selector`,
`text_selection_is_a_probe_owned_pointer_gesture`, and
`a_button_role_selector_honors_the_native_button_role`.

Livery remains a distinct producer seam. Its retained `TextFrame` keeps shaped
inline fragments but currently discards the source byte spans and Parley layout
needed to map a pointer back to stable source positions. That mapping belongs
in the Livery session rather than Turnstone or Knot. Scripted documents
separately need retained selection and clip production. Until those producers
land, whole-document clipping with `selector: None` remains the explicit
fallback where available.

## Stop rules

- No direct Turnstone filesystem or vault write.
- No vault key, file path authority, signer seed, or encryption epoch in a
  presentation resource or intent result.
- No keystroke-per-intent protocol and no live-pane protocol for this consumer.
- No persistent Graphshell cache of editable source.
- No Save advertisement without an actual endpoint-side grant and write path.
- No whole-document overwrite after a stale token or unresolved causal
  conflict.
- No clip provenance encoded only in prose.
- No automatic fetch or evaluation while received content remains unconsented.
