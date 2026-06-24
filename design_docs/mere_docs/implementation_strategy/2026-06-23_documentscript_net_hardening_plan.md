# DocumentScript Network + Mod-Trust Hardening Plan

**Status:** planning + executing 2026-06-23. Spun out of an adversarial multi-agent
review (27 agents, 21 confirmed findings of 23 raised) of the DocumentScript
`net.fetch` backend + mod-manifest auto-attach work landed this session (commits
767e1fa net permission chain, 7ae9539 net.fetch backend, 2ad3117 content-type,
plus the uncommitted mod-manifest plumbing). Sits over the now-complete
[document_script_substrate_plan](2026-06-21_document_script_substrate_plan.md) +
[follow-ons](2026-06-23_document_script_followons_plan.md).

**Headline.** The `net` capability as built is **"open internet *with the user's
ambient cookies*"**: once granted (it defaults Deny), a script may fetch any URL,
the user's logged-in session cookies are attached, and (because the host issues the
request with no initiator origin) the cross-origin response is fully readable — a
credentialed-SSRF + read-then-exfiltrate channel. Live exposure today is ~nil (net
defaults Deny, no real mods exist), but this is the design gap to close **before
`net` is ever user-enabled**. Treat `net.fetch` as prototype-only until §A lands.

Two raised findings were **refuted** (and so validate two choices): the origin
matcher cannot be tricked into *over*-attaching (every divergence fails closed,
granting nothing), and the `content-type` WIT addition is backward-compatible
(`option<string>` is an additive record subtype in the Component Model — stale
guests still instantiate).

---

## A. Critical / high — network egress is unsafe

The proper fix is to make `net` an **origin-scoped, uncredentialed-by-default**
capability instead of a boolean "open internet". Until that lands, the contained
mitigations below bound the blast radius.

- **A1 (critical) — ambient session cookies on any URL + CORS bypass.**
  `ContentNetFetcher` routes through the same `fetch::fetch_page` as top-level
  navigation, whose `do_fetch` builds `session_context()` = the process-wide,
  persisted, logged-in cookie jar, and sets no initiator origin (so netfetcher's
  no-initiator path treats every fetch as same-origin and returns an untainted,
  fully-readable body). A net-granted script can read the user's authenticated
  content on any site and exfiltrate it.
  - **Immediate mitigation (this pass):** script fetches go through an
    **uncredentialed** path (no shared jar) so the user's sessions are never
    attached, plus an SSRF guard (A3) and scheme allowlist (A4).
  - **Proper fix (Mark's design call):** make `net` carry the **set of reachable
    origins** (resolved per binding), enforce a host-side target allowlist at the
    `net::Host::fetch` boundary in document-host, and set a real initiator origin so
    CORS engages. Default-deny cross-origin; require an explicit network-origins
    declaration per script/mod.
- **A2 (high) — no http(s) timeout; a single fetch hangs the tile actor forever.**
  Only the smolweb path has `SMOLWEB_TIMEOUT` (30s); the http path awaits netfetcher
  with no deadline, and netfetcher itself sets none. Because `net.fetch` blocks the
  per-tile content actor thread (pollster + a current-thread tokio runtime) and the
  epoch watchdog cannot interrupt native I/O, a slowloris/black-hole server parks
  that tile indefinitely (no Resize/Scroll/Detach processed). **Fix this pass:** an
  overall `tokio::time::timeout` around the fetch in `ContentNetFetcher` (covers http
  *and* smolweb).
- **A3 (medium) — no SSRF / loopback / private-address block.** `http://127.0.0.1`,
  `http://192.168.x`, `http://169.254.x`, `localhost`, etc. are all reachable, so a
  script can probe the user's local services / intranet. **Fix this pass:** a
  literal-host deny-list in `ContentNetFetcher` (loopback, RFC1918, link-local,
  CGNAT, `localhost`). Resolve-then-pin / DNS-rebinding defence is a follow-on.
- **A4 (medium) — no scheme allowlist.** The guest URL is passed verbatim;
  `fetch::is_fetchable` (already gating every other load path) is never called.
  **Fix this pass:** call `is_fetchable` in `ContentNetFetcher` and fail fast.
- **A5 (medium) — no response body size cap.** `do_fetch` buffers the whole body
  host-side (outside the wasm StoreLimits), so a multi-GB response can OOM the host
  before the 64 MiB guest cap trips. **Fix:** cap the host-side buffer (a streamed
  ceiling + Content-Length precheck). Touches `fetch.rs` (Mark's) — coordinate or do
  alongside the uncredentialed path.
- **A6 (medium) — unbounded request volume per turn.** No per-attachment request
  count or rate limit; a turn can issue a burst. **Fix (follow-on):** a `Quota`
  field (`max_fetches_per_turn` / token bucket) checked in `net::Host::fetch` so both
  runtimes inherit it.

## B. High / medium — mod trust + auto-attach

- **B1 (high) — any `.wasm` dropped in `mods/` auto-attaches with no opt-in or
  signature.** `validate_wasm_binary` checks only the `\0asm` magic; there is no
  install record, signature, hash, or user approval. **Proper fix (Mark's design
  call):** a per-mod user-enablement record (an installed/approved allowlist of
  `mod_id`s) gating which discovered mods become bindings; an unapproved `.wasm` is
  discovered-but-inert. Optionally signature/hash at install. (Mitigated partly by
  B2.)
- **B2 (high) — mod bindings ignore the manifest's declared capabilities.**
  `load_mod_bindings` resolves caps from session prefs alone and never reads
  `manifest.capabilities` (`ModCapability::Network`). **Fix this pass:** intersect —
  a mod that does not declare `Network` gets `net = Deny` regardless of session
  prefs (least privilege; the manifest is the ceiling).
- **B3 (low) — `validate_wasm_binary` accepts core modules / truncated files.** Only
  the magic is checked, not the component preamble. **Fix this pass:** require the
  8-byte component preamble (`\0asm` + `0d 00 01 00`); update the two core-module
  test fixtures.
- **B4 (medium) — `.cwasm` AOT mods are invisible.** Discovery matches only
  `*.wasm`, yet document-host's loader supports `.cwasm` (the AOT distribution
  form). **Fix (follow-on):** accept `.cwasm` in discovery + `validate_wasm_binary`,
  keeping the trusted-only framing for unsafe deserialize.
- **B5 (medium) — `load_mod_bindings` bypasses the `ModRegistry` lifecycle.** No
  duplicate-`mod_id` / dependency / cycle checks; first-match over unspecified
  filesystem order is nondeterministic. (Caps are NOT bypassed — they resolve
  uniformly.) **Fix this pass (light):** dedup + a `tracing::warn!` on shadowed
  origins + a stable sort so selection is deterministic. Full registry lifecycle is
  a deliberate non-goal here (these mods are not WasmModRuntime-activated).
- **B6 (medium) — `host_of` keeps userinfo + port,** so an exact-host binding
  silently fails to match `https://host:8443/` etc. Fail-closed (availability, not
  security). **Fix this pass:** strip userinfo + port, lowercase, in `host_of`.
- **B7 (low) — duplicate auto-attach origins silently shadowed** with no diagnostic
  (subsumed by B5's dedup+warn).

## C. Medium — script lifecycle correctness

- **C1 (medium) — `deactivate` skipped on navigation.** A fresh `Show` overwrites
  `current` (and its `Some(ScriptInstance)`) with no `detach`, and neither
  `ScriptInstance` nor `DocumentScript` has a `Drop` that runs `deactivate`. **Fix
  this pass:** detach the old script in the `Show` arm before replacing `current`.
- **C2 (medium) — re-attach overwrites a live instance without detaching it.**
  `attach_script` does `content.script = Some(inst)` with no detach of an existing
  instance. **Fix this pass:** detach any existing `content.script` first.
- **C3 (medium) — a trapped script stays attached.** `deliver_event` maps a trap to
  a string and returns; the tile stays on the frozen scripted DOM and re-traps every
  turn. The host API contract says the caller should detach on `Err`. **Fix this
  pass:** on a trap, detach + revert to the static page.

## D. Low — minor robustness

- **D1 — `net_fetcher` built before the lane check / when net is denied.** Move the
  lazy build behind the HTML-lane + `grant.net == Allow` checks. **Fix this pass**
  (cheap).
- **D2 — `ContentNetFetcher` hardcodes `status: 200`.** `Fetched` carries no status
  (non-2xx collapses to `Err`), so a guest can never observe 404/3xx. **Fix
  (follow-on):** add `status` to `Fetched` (touches `fetch.rs`) and thread it.

## E. Deeper redesign (track, not this pass)

- **E1 — origin-scoped, uncredentialed `net` capability** (the proper A1/A3 fix):
  `net` becomes a set of reachable origins per binding/grant; host-side allowlist +
  real initiator origin + credentials policy. Reshapes `Grant`/`ResolvedScriptBinding`
  /the WIT request. **Mark's design call.**
- **E2 — true non-blocking fiber suspension** for `net.fetch` (the root of A2 +
  C-actor-freeze + the epoch-counts-wall-clock turn-trap): dispatch onto the existing
  off-thread fetch actor and resume the fiber on completion, so the content actor
  keeps servicing commands and only guest CPU counts against the epoch deadline.
- **E3 — mod install/approval + optional signing** (the proper B1 fix).

---

## Execution log

- **2026-06-24 (A6 per-turn fetch cap — green).** `net.fetch` is now rate-bounded: `Quota` gains
  `max_fetches_per_turn` (default 32), `ScriptHost` counts fetches per turn and `net::Host::fetch`
  refuses past the cap, reset at each `deliver_event` turn start. In document-host so both runtimes
  inherit it. The guest gained a `fetch-twice` event to exercise it; test asserts cap=1 refuses two
  fetches in a turn while cap=2 passes and the budget resets across turns. A cross-turn token-bucket
  (time-based) is a later refinement; per-turn × host-paced turn rate is the bound for now.
  document-host 21 green; meerkat builds (uses `Quota::default()`).
- **2026-06-24 (E1 origin-scoped net — the critical channel closed, green).** `net` is no longer
  "open internet": a script may now fetch **only its own page's origin** (same-origin egress),
  enforced host-side in document-host before the backend runs. `ScriptHost` gains a `net_origins`
  allowlist; `net::Host::fetch` rejects any URL whose host is outside it (`host_in_origins`, exact or
  `*.suffix`, fail-closed on empty); `DocumentScript::attach` takes the allowlist. meerkat derives it
  **same-origin** from the page URL (`attach_script` -> `vec![host_of(content.url)]`), so a granted
  `net` cannot read or exfiltrate to a third-party host. document-host tests: same-origin fetch
  applies, a cross-origin fetch is `Refused` with the DOM untouched (document-host 20, meerkat 78/122
  green). **This closes the A1 cross-origin exfil + cross-site-read channel** (the residual — a
  same-origin fetch still carries that origin's own cookies — is the script's legitimate scope).
  **Still open** (smaller, deferrable): the **uncredentialed** refinement (drop even same-origin
  ambient cookies — Mark's session-store domain, A1 step 1), and **cross-origin net declarations** (a
  mod manifest declaring extra `net` origins beyond its page, with broad-glob guards) — both tracked
  under E1. The host-side `host_of`/`origin_matches` is duplicated in document-host (kept local for
  embedder-independence; a shared crate is the longer-term home).
- **2026-06-23 (review + plan).** Review workflow `w5fbv5nkc` run; this plan written
  capturing all 21 confirmed findings triaged A–E. Next: land the "this pass" fixes
  (A2/A3/A4 in ContentNetFetcher, B2/B3/B5/B6 mod-trust, C1/C2/C3 lifecycle, D1),
  defer A1-proper/E1-E3 + A5/A6/B1/B4/D2 with the critical-credentials caveat (net
  stays disabled until A1/E1). The A1 immediate mitigation (uncredentialed script
  fetch) needs a `fetch.rs` entry point — coordinate with Mark (his file).
