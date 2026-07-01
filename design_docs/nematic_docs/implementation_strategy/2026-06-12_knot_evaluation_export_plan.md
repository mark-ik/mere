# Knot Evaluation + Export Plan — live blocks in, any protocol out

**Date**: 2026-06-12
**Status**: Planned.
**What this is**: the effectful half the
[polyglot knot design](2026-05-08_polyglot_knot_design.md) deliberately left
host-side, plus the export dual it promised. Knots (CommonMark and djot
bodies alike) already render protocol-tagged fences inline through the real
engines (`expand_fenced_blocks`, shipped, recursive). This plan adds:
fences that **fetch** (transclusion), fences that **execute** (script
engines), **HTML clippings** rendered at two fidelity tiers, and exporters
so a knot can be served *as* gemtext (or a gophermap) from any server you
host. Engines stay pure throughout — evaluation is a host-driven pass over
descriptor blocks, in the same two-phase spirit as everything else.
**Trust rule (one sentence)**: your own SelfAsserted knots evaluate per
setting; anything received (a moot's flora, a peer's clip) renders inert
until explicit consent — fences degrade to visible source, never to silent
execution or silent fetches.
**Conflict posture**: nematic + inker + netfetcher (the smolweb clients) +
a dev bin — no serval-layout, no meerkat render/input/frame_ops, no pelt.
The consent *UI* and any serval-fragment rendering are named and gated.

---

## Execution order

**K5 → K1 → K2 → K4 → K3.** K5 (exporters) is pure functions with zero new
dependencies — the easy win that also pins down block↔protocol mappings the
other slices reuse. Each slice lands independently.

## K5 — protocol exporters (`to_gemtext` and friends)

The dual of fence expansion, extended beyond `to_markdown()`/`to_knot()`
(both shipped, tested):

- `EngineDocument::to_gemtext()` — downgrade rules documented beside the
  code: headings → `#`/`##`/`###`; paragraph inline links → `=>` link
  lines after the paragraph; images → `=>` lines with alt text; nested
  lists flatten with indent markers in the text; tables → preformatted;
  quotes → `>`. **Protocol fences pass through verbatim where the target
  format matches** — a gemtext fence was already gemtext, zero loss.
- `to_gophermap(ctx)` — gophermaps need server context (host, port,
  selector base), so the signature carries a small context struct.
- `to_text()` — trivial flattening, completes the set.
- Fences with no faithful mapping (e.g. an `html` fence) export their
  semantic sibling blocks (see K4) or a marked omission — loss is visible,
  never silent.
- A `knot-render` dev example bin (the rehearsal-bin pattern):
  `knot-render <file.knot> --as gemtext|gophermap|markdown|text`. Serving
  is any existing server (agate etc.); a Mere-native gemini server is
  explicitly out of scope.

**Done when**: a djot knot with prose, links, a list, and a gemtext fence
exports to spec-valid gemtext with the fence byte-identical; the round-trip
property holds where lossless (fence → export → re-render ≈ direct render);
exporters are pure and dependency-free.

## K1 — transclusion fences (fetch + render inline)

- **Fence form**: info string `include <url>` (plain word, one verb; the
  fence body is optional **fallback content**, rendered when unresolved —
  offline, denied, or pre-consent — so degradation is authored, not
  invented). Works identically in CommonMark and djot knot bodies.
- **Parse (nematic, pure)**: both knot engines emit a new
  `Block::Transclusion { url, fallback }` descriptor.
  `to_knot`/`to_markdown` round-trip it; exporters render the fallback.
- **Resolve pass (inker, host-driven)**: an async
  `resolve_transclusions(doc, fetcher, registry, policy)` walk: policy
  check → fetch → route the response bytes through the engine registry by
  content type (plus sniff) → splice the produced blocks in place →
  record each spliced block's origin in `BlockProvenanceMap` (built for
  this) → optional source-marker badge (a setting). Recursion capped
  (transcluded knots may transclude; default depth 2, configurable) with a
  URL cycle guard.
- **Fetch lanes**: http(s) rides netfetcher; smolweb rides **`errand`**
  (the standalone lib that already speaks gemini / gopher / finger /
  spartan / nex / guppy / misfin / titan and is already wired into
  meerkat's fetch actor). *No new client crate is needed* — the
  2026-06-12 correction below records that the smolweb clients already
  existed; the only gap was real cert pinning, harvested into errand's
  `TofuStore` (gemini now pins per host with a pluggable store —
  in-memory now, durable/eidetic-backed later).
- **Policy is declarative data** (the untrusted-policy rule): a
  `TransclusionPolicy` struct — scheme allowlist × the containing
  document's trust state × mode (`auto` / `ask` / `never`), all settings.
  Default: own SelfAsserted knots auto-resolve; everything else inert.
- Hygiene rider (same files): resolve the `DjotKnotEngine` inconsistency —
  two doc comments say it is *not* in `engines()` (shared `text/x-knot`
  content type), but it is registered; pick one story and make docs and
  code agree.

**Done when**: a knot with `include gemini://…` renders the remote page's
blocks inline with per-block provenance; offline renders the authored
fallback; a received (non-SelfAsserted) knot stays inert; recursion and
cycles capped; policy branches covered by tests with a stub fetcher; the
`knot-render` bin grows `--resolve`.

## K2 — script fences (execute + render inline)

- **Fence form**: `<lang> eval` (e.g. ` ```lua eval `) — a plain ` ```lua `
  fence stays what it is today, a code sample. Evaluation is opt-in **per
  fence** on top of the per-document trust gate.
- **Parse (pure)** → `Block::Evaluation { language, source }`.
- **Evaluate pass (host)**: an inker-level `BlockEvaluator` seam the host
  implements; first backend is **piccolo Lua** through the DOM-neutral
  ScriptEngine seam (pure Rust, no JIT constraint, the fork is in-tree,
  budgets + microtask pumping built in), run constellation-style (panic
  isolation, instruction/time budget, no ambient I/O — piccolo's sandbox
  default). JS (Nova native / Boa wasm) follows through the same seam once
  the Lua lap proves the contract.
- **Output contract (v1)**: the script returns `(format, text)` — `plain`
  becomes a paragraph/preformatted block; `gemtext` / `markdown` / `djot`
  nested-render through the registry (the org-babel move). Canvas-swatch
  outputs (drawing scripts → platen) are named for later, not built.
- **Trust**: same gate as K1, stricter default — own notes `ask` (a
  setting can relax to `auto`), received notes never auto-run.

**Done when**: a `lua eval` fence in an own note renders its output inline,
including a gemtext-returning script nested-rendering; budget exhaustion
yields a visible error block, not a hang; a panicking script isolates; a
received knot's script fences render as inert source.

**Progress — 2026-06-12.** K2a landed and K2b resolved by reuse (the same
"check before you build" lesson as the errand correction):

- **K2a — the inker evaluate pass landed** (`inker::document::evaluate`, 7
  tests). `evaluate_blocks(doc, evaluate, render, policy)` mirrors the
  transclude pass exactly: closure-driven (decoupled from any script engine
  *and* the routing layer), top-level `<lang> eval` fences only, plain
  output → `Paragraph`/`Preformatted`, `gemtext`/`markdown`/`djot` output →
  nested-render via the registry closure, generated-block provenance
  (`evaluated:<lang>`), and the full gate: `EvaluationPolicy`
  (`deny_all` floor, `for_own_notes`), a plain ` ```lua ` sample left
  untouched, denied/failed reported, source fence kept on refusal or error.
  Same no-new-`Block`-variant decision as K1a (the descriptor is a
  `CodeBlock` with `language = "lua eval"`), so inert rendering is free
  everywhere.
- **K2b — the Lua engine already exists; do NOT build a new crate.** Survey
  (applying the errand lesson) found serval's **`script-engine-api`** (the
  DOM-neutral ScriptEngine seam this plan already named) and its
  **`script-engine-piccolo`** backend (the gc-arena DOM plan's G4): `new()`
  / `eval(source)` / `value_to_string(value)` / `Budget` / `pump`, on the
  vendored piccolo fork. Confirmed building + green here (8 tests). The
  abandoned first instinct — a new `crates/script/lua` — would have been a
  second redundant build; reverted before writing it.
- **K2c — the host bridge (deferred, scoped).** Joining the two halves is a
  ~15-line adapter: `|lang, source| { engine.eval(source).and_then(|v|
  engine.value_to_string(&v)).map(EvalOutput::plain) }` (plus a
  `return format, text` convention later). But it pulls serval + piccolo +
  gc-arena, which must not couple to pure nematic — so it lives where
  serval is already linked (meerkat), deferred with the rest of the shell
  wiring, or in a `crates/probes/` spike if a standalone demo is wanted
  first. **One real gap found**: `PiccoloEngine::eval` runs `finish()`
  (unbounded), so it hangs on `while true do end`; the seam's `Budget` is
  on `pump` (microtasks), not the main eval. So "budget, not a hang" needs
  either (a) a bounded-eval method added to the seam (the harvest pattern —
  a defaulted `eval_bounded(source, Budget)` on `ScriptEngine`, piccolo
  overriding with a fuel loop; non-breaking) or (b) the host running eval
  in a wall-clock-bounded worker. Decision deferred to Mark with K2c.

- **K2c + the bounded-eval gap — both landed (Mark's call: harvest + probe).**
  **Harvest** (serval, the errand pattern): added `eval_bounded(source,
  Budget)` to `script-engine-api::ScriptEngine` with a default that runs the
  existing unbounded `eval` (non-breaking — Nova/Boa unchanged), and an
  override in `script-engine-piccolo` that steps the executor with metered
  `Fuel`, the bounded mirror of `Lua::finish`; a `Budget::Steps(n)` cap
  returns "budget exhausted" instead of looping forever. script-engine-api +
  piccolo green (10 tests; the runaway `while true do end` proven caught,
  the engine still usable after). **Probe** (`crates/probes/knot-lua/`, a
  standalone excluded workspace so piccolo + gc-arena never touch the mere
  graph — and sidestepping the sibling `markdown-v0` probe's stale paths):
  renders a knot, runs each `lua eval` fence through a fresh bounded
  `PiccoloEngine`, nested-renders the output. Ran end to end, all four
  done-conditions met with real Lua: a `for`-loop fence rendered
  "the sum of 1..10 is 55"; a gemtext-returning fence nested-rendered as
  live gemtext (heading + `=>` line); the `while true do end` fence was
  **caught** ("budget exhausted after 100000 steps") and the render exited
  cleanly without hanging; a plain ` ```lua ` sample stayed untouched.
  Two notes recorded: `PiccoloEngine::new()` uses `Lua::full()` (io
  included — fine for the trusted own-note demo; sandbox-hardening, e.g.
  `Lua::core()` for untrusted knots, is a follow-up), and the production
  bridge home is meerkat (the probe is the spike). **K2 is functionally
  complete** end to end; the remaining work is the meerkat host wiring +
  consent UI (K3, gated) and the JS backend through the same seam (the Lua
  lap having proven the contract). Next plan slice: K4 (HTML clip tiers) or
  K3 (caching + consent), per priorities.

- **2026-06-13** — **The eval lane got a real polyglot menu: the thin
  `BlockEvaluator` trait + a Rhai backend (8 tests).** Question that drove
  it (Mark): given knot eval needs only a slice of the JS-shaped
  `ScriptEngine` seam, what about Rhai/Rune as pluggable backends? Decision:
  a **thin `BlockEvaluator`** (in inker: `eval_block(source, max_ops) ->
  EvalOutput`, plus a `BlockEvaluators` registry keyed by language tag) is
  the knot-eval contract, deliberately distinct from serval's full DOM
  seam (reflectors, promises) which mod/DOM scripting keeps. `script-rhai`
  (`crates/script/rhai`, pure Rust, no serval) implements it: Rhai is
  sandboxed by default (no file/network) and has a **native operation
  budget** (`set_max_operations`), so the runaway cap is first-class (a
  `loop {}` is caught, not hung) rather than the fuel loop piccolo needed.
  Output convention: a string (format-detected) or an explicit
  `#{format, text}` map. The full path is proven by an integration test (a
  `rhai eval` fence routed through the registry + the inker pass renders
  "sum is 55"). On the scripting-map question: this is the **option-module**
  tier (like piccolo Lua), not a first-party substrate — the Rust+JS
  shipping decision stands, and Rhai-for-*policy* stays superseded by
  declarative data. Rune remains gated on its own trigger (1.0 + sandbox
  warranty). The broader "how polyglot" question spun out a dedicated plan:
  [polyglot block resolver](2026-06-13_polyglot_block_resolver_plan.md)
  (one registry; query / diagram / wasm block kinds beyond more languages).

## K4 — HTML clippings, two fidelity tiers

Djot forbids raw HTML *in prose*; it does not forbid explicit fenced
blocks — clippings stay format-clean.

- **Semantic tier (exists)**: `build_clip_knot` already serializes
  selected blocks from a serval-rendered tile with provenance (per-block
  overrides included). This is the default clip and the export-friendly
  representation.
- **Faithful tier (new)**: clip-time *additionally* captures the source
  fragment in an `html` fence. Rendering it inline needs the
  **reader-mode HTML fragment lane** nematic's own status already names as
  its one pending lane: an `HtmlFragmentEngine` rendering a sanitized
  subset — text, headings, lists, tables, images; scripts, event
  handlers, iframes, and styles stripped (sanitization proven by test,
  not by intention). Parser choice: **html5ever** (spec-grade tokenizer,
  the standards-correct pick and the lineage serval already trusts) over
  lighter non-spec parsers; the heavier dep is confined to nematic's
  optional feature.
- Full-fidelity serval-fragment rendering inside a knot stays a named
  registry slot for post-reshape — the fence and the routing seam are the
  same either way, so upgrading fidelity later touches no format.
- Export: `html` fences export via their semantic sibling (the tiers
  travel together in the knot), never raw HTML into gemtext.
- Distinct from (and feeding) the *browsing* reader-mode lane: rendering
  whole `text/html` pages through nematic is a separate later slice; the
  fragment engine is its seed.

**Done when**: a clipped fragment renders inline matching its source's
semantics (fixture pairs against the serval-rendered original); a
script/onclick-bearing fragment provably renders with them stripped; clip
round-trip keeps both tiers intact; exporting a knot with an `html` fence
produces the semantic downgrade.

## K3 — caching + consent surfaces (gated last)

- Resolved transclusions persist as **engrams** (content + source URL +
  fetched-at, LocalOnly): offline renders show cached content with a
  staleness mark (real age, no placebo); re-fetch governed by a max-age
  setting.
- Script results may opt into the same cache (`eval` fences declaring
  deterministic intent), default off.
- The consent surfaces — the "resolve" / "run" affordance on inert fences
  in received knots — are shell UX, **gated on the window-composition
  reshape** (the E5 pattern); until then the `knot-render` bin's flags are
  the consent mechanism, which keeps the policy code honest ahead of the
  UI.

**Done when**: second render offline serves the cached transclusion with
its age shown; cache entries are LocalOnly engrams the GC pass can walk;
the bin can resolve-with-consent a received knot end to end.

## Out of scope (named)

A Mere-native gemini *server* (exporters produce files; serve with
anything); full readability/article-extraction browsing mode (separate
lane seeded by K4's fragment engine); JS-engine wiring beyond the seam
contract (follows the Lua lap); canvas-swatch script outputs (platen,
later); serval full-fidelity fragments (registry slot, post-reshape);
any change to `mooting`/flora formats (a shared knot is just an engram —
K-lanes read its trust state, nothing more).

## Open questions

- Fence verb spelling: `include` chosen for plainness — `transclude` is
  the precise word but jargon; revisit if `include` collides with a
  future preprocessor sense.
- Whether the source-marker badge on spliced blocks defaults on (visible
  provenance) or off (clean reading) — a setting either way; default
  leans visible-until-trusted.
- TOFU cert-pin store location (file beside the profile vs eidetic
  engrams) — start file-backed, migrate when persona/keys land fully.
- Where `BlockEvaluator`'s piccolo implementation lives: meerkat (it
  already deps the serval components) vs a small dedicated crate so the
  bin can evaluate without the shell. The bin requirement leans toward
  the small crate.

## Progress

- **2026-06-12** — Plan written. Grounding: `expand_fenced_blocks` +
  inline extensions shipped and wired in **both** knot engines;
  `to_markdown`/`to_knot` shipped with tests; `BlockProvenanceMap` +
  `DocumentTrustState` exist (the policy hooks); the protocol registry
  routes smolweb *schemes* but no smolweb *clients* exist yet (netfetcher
  is http(s)-shaped) — K1's one real dependency; djot raw-block semantics
  confirm fenced HTML is format-clean.
- **2026-06-12** — **K5 landed (inker 76 tests, nematic 157 + the bin's 3,
  all green).** Survey first: `to_gemini()` already existed with the
  planned downgrade rules — K5 narrowed to the rest. Added
  `to_gophermap(ctx)` (RFC 1436: prose as `i` info lines, gopher links
  decomposed into native menu entries, other schemes as `URL:` items on
  the serving host, `.` terminator) and `to_text()` in
  `document/render/export.rs`; `GophermapContext` re-exported at inker's
  top level. The `knot-render` bin (nematic example, embedded tests):
  `<file.knot> --as gemtext|gophermap|markdown|text|knot [--djot]`. The
  **verbatim-fence property holds by architecture**: `expand_fenced_blocks`
  turns a gemtext fence into real blocks at parse time, so export restores
  them as live gemtext — proven by the bin's round-trip test (heading +
  link line byte-faithful, no fence wrapper). The live rehearsal caught
  two output bugs unit tests missed: link-only paragraphs double-rendered
  (text line + `=>` line) in both the gemtext and gophermap writers —
  fixed with a shared `is_link_only` rule; and the new exporters printed
  the frontmatter title where the established ones never do — dropped.
  The "serve knots as gemtext" pipeline works end to end: djot knot →
  `knot-render --as gemtext` → `.gmi`. Next slice: K1.
- **2026-06-12** — **K1a landed: the transclusion resolve pass (inker 81
  tests; nematic 157 + 3 green; rehearsed through the bin).** One design
  deviation from the plan, recorded: **no new `Block` variant.**
  The blast-radius survey found exhaustive `Block` matches across
  document-canvas, uxtree, gloss, and platen (meerkat's have catchalls) —
  so the descriptor *is* the existing `CodeBlock` with
  `language = "include <url>"` (the full fence info string already
  survives parsing). Payoff: unresolved fences render as visible source +
  authored fallback **everywhere, automatically** — the trust rule's inert
  rendering with zero cross-crate churn. The dedicated variant waits for
  the consent-affordance slice (K3), where renderers genuinely need it.
  `inker::document::transclude`: `TransclusionPolicy` (enabled / scheme
  allowlist / max depth; `deny_all` floor + `for_own_notes`),
  `resolve_transclusions(doc, fetch, render, policy)` with fetch/render
  as caller closures (decoupled from netfetcher and the routing layer;
  sync in this cut, async adapts at the closure boundary), cycle guard,
  per-pass depth, per-spliced-block `BlockProvenanceMap` records, and a
  faithful `TranscludeOutcome` (resolved / denied-with-reason /
  failed-with-error). v1 limit stated in-code: top-level fences only.
  The bin grew `--resolve` with a `file://` fetcher (paths relative to
  the knot; content type by extension): unresolved output shows the
  fallback fence, resolved output splices the included gemtext in place.
  **Remaining for K1**: K1b — the real network lanes (http(s) via
  netfetcher; the `smol` clients: gemini TLS+TOFU, gopher, finger,
  spartan, guppy, nex) and wiring them as the host's fetch closure.
- **2026-06-12** — **K1b correction: smolweb already had a home — `errand`
  — and the plan was wrong to put it in netfetcher.** Mark caught it: was
  meerkat not already fetching `gemini://`/`gopher://`? It was. Survey
  confirmed [`errand`](https://github.com/sgtmark/errand) (Mark's
  standalone smolweb-transport lib, pulled as a git dep) already speaks
  gemini / gopher / finger / spartan / nex / **guppy / misfin / titan**
  (more than the five I built), exposes `errand::fetch(url) -> Response`,
  and is **already wired** into meerkat's fetch actor (`crates/meerkat/
  src/fetch.rs`: http(s)→netfetcher, smolweb→errand). So the netfetcher
  `smol` module I wrote was fully redundant. **Reverted it entirely**
  (netfetcher back to its baseline, 65 tests, clean tree). The one real
  bit of value in what I wrote was *true* cert pinning — errand's TLS was
  TOFU-**permissive** (accept-any; its `tls.rs` even noted "a later
  revision will pin certificates per host … behind a pluggable trust
  policy"). **Harvested that into errand** as the intended upgrade: a
  `tofu` module (`TofuStore` trait + `InMemoryTofu` + `PermissiveTofu`
  default + process `set_trust_store`), a `PinningVerifier` that checks
  the leaf SHA-256 against the host's pin *inside* the handshake (so the
  request is never sent on a mismatch), `Error::CertificateChanged`, and
  the gemini wiring (lookup → pin-on-first-contact → reject-on-change).
  Default behaviour is unchanged (no store installed = permissive), so
  existing errand users see nothing new; a host opts into pinning with one
  `set_trust_store` call. errand: 38 unit tests + the full TOFU loop
  proven against the **live** `geminiprotocol.net` capsule (pin, match,
  then `CertificateChanged` on a corrupted pin). Titan/misfin (write
  companions) stay permissive — a noted follow-up. **Lesson recorded**:
  check the workspace's existing crates before planning a "new module"
  for a capability — the plan even said "netfetcher is http(s)-shaped,"
  which was true *because* smolweb already lived in errand. **Remaining
  for K1 fully**: the host bridge — meerkat already has errand wired for
  *page* fetches; the transclusion path reuses it (an `errand::Response`→
  `Fetched` map in the resolve closure), meerkat-deferred like all shell
  wiring. Next plan slice: K2 (`lua eval` script fences).
