# Leverage Census Brief

**Date**: 2026-08-10
**Status**: research brief, commissioned by Mark ("there's a bunch of stuff
in mere and genet i'm not sure we're leveraging properly"). A
reverse-dependency census of both platform workspaces, joined with every
external family consumer, so under-leverage claims rest on `cargo metadata`
rather than greps (the esp plan's own method note: dotted
`name.workspace = true` deps escape single-form greps).

**Method.** `cargo metadata --no-deps` per workspace, reverse map over
non-dev dependencies among members; joined with a sweep of every family
repo's manifests for `merely-made/mere` and `merely-made/genet` git deps
(133 external consumer edges across turnstone 51, isometry 24, cleromancy
20, hocket 14, woodshed 12, retinue 8, mesocosm 4) plus mere's own
genet-side patch consumption (14 edges). A crate is flagged only when it has
**zero internal and zero external** consumers; single-consumer crates are
recorded but a single consumer is often the design (a port, an adapter, a
tier under construction).

## 1. Not under-leveraged (the flags Mark raised that dissolve)

- **import** — consumed by turnstone. Stays put; its scope (stored
  browser-data migration) is narrow but wired.
- **incipit** — multiple internal consumers plus turnstone. The id
  vocabulary is doing its leaf job.
- **luggage** — consumed by hocket (the Velopack A/B), published 0.1.0
  2026-08-10.
- **signals** — one consumer (mere-canvas) and that is its founding
  contract (cartography's `IntelligenceSignals`).
- **stickleback, graphshell family, knot, app-host, commons-spine,
  eidetic-fjall, scholia** — all externally consumed (turnstone,
  cleromancy, isometry, woodshed).
- **genet: errand, xpath, illume, genet-extract, script engines** — all
  have internal or external consumers (illume and the inker engines via
  mere's patch table; genet-extract via turnstone; piccolo via turnstone).
- **The games wing is already on the contract**: mesocosm consumes codicil,
  sceno, scenomise, sprigging.

## 2. True zeros, mere (no consumer anywhere)

| Crate | Reading | Suggested disposition |
|---|---|---|
| ~~`mere-mesh-host`~~ | **Resolved 2026-08-12.** Distillery v0 now embeds H0's supervisor and owns its first retention-maintenance projection | Keep Turnstone as the second-consumer proof when it needs the same authority |
| `mere-eidetic-search` | BM25 + fast-fields + hybrid-fusion seam, unconsumed | **Owned 2026-08-12** by the [search surface wiring plan](mere_docs/implementation_strategy/2026-08-12_search_surface_wiring_plan.md); the found precondition is capture (nothing authors `BrowsingTrace` yet) |
| `mere-embed` | **Audited 2026-08-12: not a husk.** The esp re-export plus three built-but-unwired glue modules (persistence, field bridge, canvas search) | Keep; wires in as W4 of the search surface wiring plan |
| `mere-crawl` | Host-neutral frontier, nobody drives it | Hold as capability; census again after the gazette feed pipeline lands (crawl is its natural engine) |
| `moothold` | The t1–t3 federation home, while gemot/mooting/commons carry the live moot work | Reconcile with the moot cluster: absorb, or state the tier boundary that keeps it |
| `register-input`, `register-knowledge`, `register-protocol` | Three of nine registry crates unwired | Registry-cluster audit: wire or fold into the six that are |
| `mere-comms`, `shell-state`, `content-contract`, `mere-persona-picker` | Shell-era pieces with no takers | Verify against the pane-taxonomy and dramatis plans; fold or retire |
| `document-host` | The wasm/effects host consumes servitor and registries, but nothing consumes it | Verify against the knot-effects record before touching; likely wired at runtime by turnstone rather than by dep |
| `scenograph` (facade) | Every consumer takes sceno/scenomise/scenotime directly | Make it the documented entry point or deprecate the facade |
| `mere-eidetic-https-fetcher`, `-iroh-fetcher` | Proven in receipts, unwired in products | Gazette feed pipeline + mesh blob lane are the named destinations |

By-design zeros, no action: ports and stubs (castellan, graphshell-web),
deprecated shims (vates, sibylla), the young dramatis tier
(dramatis facade, chatelaine, emblem, gaz, gazette), tulpa (just homed).

## 3. True zeros, genet

| Crate | Reading | Suggested disposition |
|---|---|---|
| `verso-tile` | Unconsumed anywhere, on both sides | The Blitz/Serval-convergence decision owns it: retire or re-seat after the netrender cut comparison |
| `genet-scripted-worker` | The worker lane, unwired | Verify against the browser/PWA scripting doctrine; wire under pelt scripted or hold |
| `genet-static-html` | **Verified 2026-08-12: truly bare.** A 38-line MPL shell (no module doc, no consumers, no reference beyond the members list); the static lane landed in `genet-static-dom` + `genet-documents`, which never mentions it | Retire or absorb into `genet-static-dom`; nothing hides behind features |
| `cambium-nematic` | **Verified 2026-08-12: bypassed.** Pelt's smolweb lane lowers through genet-documents → nematic → `EngineDocument`; "Cambium-native views and themes for Nematic smolweb content" has no taker | Hold, named destination: the first cambium app embedding smolweb natively (signalman, or a turnstone pane); retire at the convergence checkpoint if unclaimed |

By-design zeros: pelt and genet-wpt (ports), tabard and fleece (today's
stubs), test shims (`*_tests`, `servo-media-examples`,
`servo-default-resources`), the inker engines (consumed by mere's patch
table, invisible to genet-internal counting).

## 4. One-consumer rows worth a glance (not flags)

`arrangements` and `workbench` are single-consumer by architecture (the
composition spine); `seiche` ← mere-canvas only, which is exactly the seat
the [conatus/nexus brief](mere_docs/research/2026-08-10_conatus_nexus_alignment_brief.md)
makes pluggable; `mooting` ← gemot; `murm` ← mere-comms (which itself has
no consumer — the comms chain ends in the air and should be verified
top-down). Isometry consumes sceno + scenomise but **not scenotime**: no
diff, no pick — worth a line in its next plan since picking moved
scene-side at the freeze.

## 5. Sequence

1. ~~`mere-mesh-host` → distillery v0.~~ **Done 2026-08-12:** the port now
   consumes the supervisor and owns the owner-controlled retention sweep.
2. ~~`eidetic-search` into turnstone's search surface, and the `mere-embed`
   husk audit in the same pass.~~ **Planned 2026-08-12:** the search surface
   wiring plan owns both; the audit closed "retire" (embed's glue is live,
   unwired) and found the real precondition (W1 capture).
3. ~~Registry-cluster audit (three unwired registers) + the shell-era
   quartet, one session, verdicts recorded per crate.~~ **Done 2026-08-12:**
   §6 records the verdicts (nothing retires outright; persona-picker is the
   wire-now item; content-contract pairs with genet-scripted-worker as one
   unlanded lane).
4. `verso-tile` at the Blitz/Serval convergence checkpoint. The two genet
   verify items are **done 2026-08-12** (§3 rows): static-html is a bare
   shell (retire/absorb), cambium-nematic is bypassed by pelt's lane
   (hold, named destination).
5. moothold reconciliation when the moot cluster next moves.

## 6. Step-3 audit verdicts (2026-08-12, per crate)

**Registry cluster** — all three are meerkat-era extractions (Slices 50–54
of the 2026-05-01 workspace architecture proposal) whose consuming shell
died with meerkat. None is dead weight on its own terms:

- `register-input` — **hold, named destination**: it is the natural home
  for configurable keybindings (action-id late binding is exactly the
  configurability doctrine's shape). Wire into the cambium host input path
  when a settings surface for bindings lands; retire only if that surface
  is ruled out.
- `register-knowledge` — **hold, named destination**: UDC validation,
  scoring, and color hints serve the tagging/gloss surface. Wire when a
  tagging UI lands; fold toward `mere-gloss` if it never does.
- `register-protocol` — **reconcile**: the scheme-to-handler map with a
  mod provider hook overlaps the participant-gate packs plan (packs
  registering handlers at activation is this crate's provider path).
  Either the pack activation lane consumes it, or it folds into
  `register-viewer`.

**Shell-era quartet** — one keep, one wire-now, one reconcile, one paired
hold:

- `mere-comms` — **keep**: the inbox model (`Conversation`/`Message`/
  `ProtocolAdapter` over murm) is the comms pane waiting for its pane.
  The moot cluster's current motion (radio-scopes-as-moots, 2026-08-12) is
  its likeliest forcing consumer; the comms chain "ending in the air" is
  a missing pane, not a dead model.
- `mere-persona-picker` — **wire now, cheapest item on the board**: a
  finished cambium `command_picker` view-model over `identity::roster`
  that every app needs at startup. First consumer: woodshed pre-release
  (already consumes personae + cambium).
- `shell-state` — **reconcile with the pane taxonomy**: host-neutral
  session-state types (focus authorities, palette, omnibar drafts, frame
  view-models) that predate the Apparatus/Inspector/Steward split. Fold
  what graphshell/turnstone actually project; archive the rest.
- `content-contract` — **paired hold**: transfer-ready content messages
  plus the flat byte transport *for browser workers* — the mere-side end
  of the same lane as genet's `genet-scripted-worker` (also a zero). The
  two zeros are one unlanded lane; they revive together under the
  browser/PWA scripting doctrine or not at all.

Raw census tables (full consumer lists per crate) were generated
2026-08-10; regenerate with the §Method commands rather than trusting this
snapshot after the tree moves.
