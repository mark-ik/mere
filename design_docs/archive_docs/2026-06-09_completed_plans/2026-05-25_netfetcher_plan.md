# netfetcher Plan — a portable WHATWG-Fetch engine for the Mere ecosystem

**2026-05-25.** Plan for **`netfetcher`**: a standalone crate + repo that is
Servo's `net` *made portable* — the WHATWG Fetch algorithm and its companion
machinery (CORS, cookie jar, HTTP cache, redirects, HSTS, mixed-content, CSP
hooks, content-encoding) lifted off Servo's `ipc-channel` / resource-thread
coupling and exposed as a clean async **library** API, plus an HTTP/3 lane and a
modern TLS stack.

netfetcher is a **Mere-ecosystem network organ**, sibling in spirit to
[`genet`](../../../../genet/) (render engine) and
[`netrender`](../../../../netrender/) (paint→GPU): a reusable, host-agnostic
component Mere owns and drives. **Mere owns networking; genet consumes bytes.**

> Status: **scaffolded (2026-05-25).** `repos/netfetcher/` exists as a
> compile-ready skeleton (the API surface below — `fetch` / `Request` / `Response`
> / `FetchContext` + the three seams — builds and tests green; `fetch` returns a
> Fetch-spec network error until increment 1). No fetch logic implemented yet. This
> doc fixes scope, layering, the de-IPC extraction strategy, the increment ladder,
> and the open questions. Per [`DOC_POLICY.md`](../../DOC_POLICY.md) §8 it moves to
> `archive_docs/` on completion.

---

## 1. Why — the gap

There is **no portable, browser-grade fetch crate in Rust.** The landscape:

- **`reqwest` / `ureq` / `hyper`** are HTTP *clients*. They speak the wire
  (methods, headers, bodies, TLS, pooling) but know nothing of the *Fetch
  standard*: no same-origin/CORS gating, no opaque/cors response tainting, no
  forbidden-header enforcement, no credentials mode, no cookie-jar policy, no
  HTTP cache (RFC 9111), no HSTS upgrade, no mixed-content blocking, no CSP
  `connect-src` consultation. Those are the parts a *web engine* needs.
- **Servo's `net`** implements all of that — it is the most complete
  Fetch-spec implementation in Rust — but it is welded to Servo's architecture:
  the public surface is a `CoreResourceThread` you talk to over
  **`ipc-channel`**, responses arrive as `FetchResponseMsg` stream messages on
  an IPC route, and the whole thing assumes Servo's `constellation` /
  resource-process model. You cannot call it as `fetch(request).await`.

netfetcher fills exactly that gap: **the Fetch algorithm as a directly-callable
async library**, no IPC, no process model, usable by any embedder.

### What already exists in-workspace (and what netfetcher does *not* replace)

Two existing pieces are deliberately **out of scope** as replacements — netfetcher
sits *under* them, or beside them:

- **[`eidetic-https-fetcher`](../../../../mere/crates/eidetic/eidetic-https-fetcher/)**
  (`BlobFetcher` for `BlobSource::Https`) is a *content-addressed blob*
  resolver for the memory layer: a hash-verified GET via `ureq`, where
  `resolve_blob` BLAKE3-checks the body. It is **not** a Fetch implementation
  and never needed to be. Open question §9: whether it eventually becomes a thin
  adapter *over* netfetcher (one HTTP stack, two policies) or stays a
  deliberately-minimal blob path. Default assumption: leave it; revisit only if
  duplication bites.
- **`SessionServiceRunner` / `WorkerKind::FetcherPool`** (see
  [`2026-05-14_session_service_runner_plan.md`](2026-05-14_session_service_runner_plan.md))
  already reserves the *host-side worker slot* that a real fetch engine would
  run inside. netfetcher is the engine; `FetcherPool` is where Mere's host spins
  it up off the UI thread. The plan there explicitly named `FetcherPool` as the
  **first** real worker to land — netfetcher is its payload.

---

## 2. Scope — what netfetcher is

The **WHATWG Fetch** algorithm (https://fetch.spec.whatwg.org/) as a library,
with the surrounding standards it depends on:

| Concern | Standard | v1 posture |
|---|---|---|
| Fetch algorithm (main/http/scheme fetch, response tainting) | WHATWG Fetch | core |
| CORS (preflight, credentials mode, exposed headers) | WHATWG Fetch §3.2 | core |
| Cookie jar (storage, `SameSite`, secure, domain match) | RFC 6265bis | core |
| HTTP cache (freshness, revalidation, `Vary`) | RFC 9111 | increment 2 |
| Redirects (modes: follow/error/manual, 3xx, 308) | WHATWG Fetch | core |
| HSTS (preload list + dynamic) | RFC 6797 | increment 3 |
| Mixed-content blocking / upgrade | W3C MIX | increment 3 |
| CSP `connect-src` consultation | CSP3 (hook, not owner) | increment 3 (hook) |
| Content-encoding (gzip/deflate/br/zstd) | — | core |
| HTTP/1.1 + HTTP/2 | RFC 9110-9113 | core (hyper) |
| HTTP/3 + QUIC | RFC 9114 | increment 4 (quinn/h3) |
| WebSocket | RFC 6455 | increment 5 (optional) |

**Not in scope:** the DOM `fetch()` *binding* (that lives in genet's scripting
tier and *calls* netfetcher), the resource cache of decoded images (genet's
`ImageLoader` / plane territory), media streaming/DRM (a future `net-media`
organ), and P2P transports (murm/iroh territory — though netfetcher's
`Request`/`Response` types may be reused there; see §9).

---

## 3. Layering — who consumes it

```
                    ┌─────────────────────────────────────┐
                    │  netfetcher  (own repo/crate)         │
                    │  fetch(request, &Context) -> Response │
                    │  Fetch algo · CORS · cookies · cache  │
                    │  CSP hook · HSTS · h1/h2/h3 · TLS     │
                    └───────────────┬─────────────────────┘
                                    │ bytes + metadata
        ┌───────────────────────────┼───────────────────────────┐
        │                           │                           │
   ┌────▼─────┐              ┌──────▼───────┐            ┌───────▼────────┐
   │  Mere    │              │   genet     │            │ eidetic        │
   │  host    │              │ (byte        │            │ (BlobFetcher   │
   │ FetcherPool             │  consumer)   │            │  adapter? §9)  │
   │  worker  │              │              │            │                │
   └──────────┘              └──────────────┘            └────────────────┘
        │                           │
   drives netfetcher           genet stays byte-consuming:
   off the UI thread,          its ImageLoader / stylesheet /
   feeds results into          script seams take *bytes*; the
   the graph/session           host (via netfetcher) does the
   layers                      fetching. genet never links
                               netfetcher directly.
```

Key invariants:

- **genet does not depend on netfetcher.** genet's whole design (per its
  Hekate lanes doc and the `ImageLoader` seam in
  [`image_decode.rs`](../../../../genet/components/genet-layout/image_decode.rs))
  is that *fetching is the host's job; genet consumes bytes*. netfetcher is one
  concrete host-side fetcher that feeds those seams. This keeps genet portable
  (it compiles for wasm where netfetcher's native stack can't run) and keeps the
  fetch policy in one place.
- **Mere is the primary consumer/driver.** The host runs netfetcher inside the
  `FetcherPool` worker, applies per-persona/per-session policy (engine profile
  binding → cookie jar + cache partition; capability gates → `connect-src`
  decisions), and routes fetched bytes to whichever renderer tenant asked
  (`genet.web`, nematic engines, etc.).
- **Native-only — the *whole* crate, not just the h3/WebSocket lanes.** netfetcher
  is built on `hyper` + `tokio` (sockets, runtime), which don't run on
  `wasm32`. So netfetcher is a **native** network engine, period; the
  wasm/PWA target uses the **browser's own `fetch`/`WebSocket`** through a thin
  adapter (unbuilt, a separate component). This mirrors the scripting split
  (Nova native; browser JS in the browser): the host platform's native engine on
  desktop, the browser's built-ins in a PWA. (Implementation note: h3 +
  WebSocket are additionally `#[cfg]`-excluded from wasm, but that's moot given
  the core stack is native already.) Corrects the earlier "wasm-friendlier"
  framing in the open questions below — storage-agnostic seams are still good
  design, just not for a wasm *netfetcher*.
- **The JS `fetch()` binding (genet scripting tier) ultimately calls
  netfetcher** — but through the host, not by linking it, preserving the same
  byte-seam discipline.

---

## 4. The de-IPC extraction — strategy

The hard, valuable work is **lifting Servo's `net` off `ipc-channel`** while
keeping its spec-correctness. Approach:

1. **Reference, don't vendor wholesale.** Use Servo's `net` at a pinned commit
   as the *spec oracle and source of algorithm logic*, not as a dependency. The
   genet workspace already pins the full Servo networking stack (hyper 1.9,
   hyper-rustls/rustls 0.23, http, cookie 0.18, content-security-policy 0.8,
   encoding_rs, url 2.5, data-url, brotli/flate2, async-tungstenite,
   ipc-channel, mime/headers/percent-encoding) — those *leaf* crates carry over;
   the `net` *orchestration* gets rewritten around a direct async API.
2. **Replace the transport seam.** Servo's public entry is
   `CoreResourceThread` + `fetch(request) -> IpcReceiver<FetchResponseMsg>`.
   netfetcher's entry is a plain async fn:

   ```rust
   // Illustrative signature — not final.
   pub async fn fetch(request: Request, cx: &FetchContext) -> Response;
   ```

   where streaming bodies surface as an `impl Stream<Item = Result<Bytes>>` (or a
   channel the caller picks), **not** an IPC route. `FetchResponseMsg`'s
   `ProcessResponse* / ProcessRequestBody` state machine becomes ordinary
   async/await + stream items.
3. **Drop the resource-thread/process model.** No `CoreResourceThreadPool`, no
   `constellation` hookups, no cross-process serialization of responses. State
   (cookie jar, HTTP cache, HSTS list, connection pool) lives behind the
   `FetchContext` the caller owns and can share across requests — threading is
   the *embedder's* choice (Mere puts it in the `FetcherPool` worker; a CLI
   tool could use it single-threaded). This directly answers the constellation
   cut: **we cut Servo's IPC/process model deliberately, and a future
   multi-threading approach is the embedder's, expressed through how it shares
   `FetchContext` — not baked into the fetch library.**
4. **Pluggable storage.** Cookie jar and HTTP cache go behind traits
   (`CookieStore`, `HttpCache`) so Mere can back them with persona-scoped /
   session-scoped partitions (per
   [`2026-05-14_engine_profile_boundary_plan.md`](2026-05-14_engine_profile_boundary_plan.md))
   and eidetic can supply durable storage. Ship in-memory defaults.
5. **CSP as a hook, not an owner.** netfetcher consults a
   `trait CspChecker { fn allows_connect(&self, url: &Url) -> Decision }`
   the embedder supplies; netfetcher does not own policy. Mirrors genet's
   "policy lives in the host" discipline and composes with Mere's capability
   gates ([`2026-05-14_capability_gate_catalogue_brief.md`](../research/2026-05-14_capability_gate_catalogue_brief.md)).

---

## 5. Dependency stack (proposed)

All present in the genet pin set except the h3 lane:

- **Transport:** `hyper` 1.x (h1/h2), `hyper-util`, connection pooling.
- **TLS:** `rustls` 0.23 + `hyper-rustls` (ring or aws-lc-rs backend — pick at
  §9), `webpki-roots` / platform roots.
- **HTTP/3:** `quinn` + `h3` / `h3-quinn` (increment 4; the only deps not
  already pinned).
- **Types:** `http` crate (`Request`/`Response`/`HeaderMap`), `url` 2.5,
  `bytes`, `mime`, `headers`, `percent-encoding`.
- **Cookies:** `cookie` 0.18 (+ public-suffix list for domain matching).
- **Encoding:** `brotli`, `flate2`, `zstd`, `encoding_rs` (charset).
- **CSP types:** `content-security-policy` 0.8 (for the hook's decision types).
- **Async runtime:** tokio (see §9 — runtime-agnostic vs tokio-pinned).
- **WebSocket (opt):** `async-tungstenite` (increment 5).

---

## 6. API shape (illustrative — signatures only)

```rust
// All illustrative-signature-only; not compile-ready.

pub struct Request { /* url, method, headers, body, mode, credentials, … */ }
pub struct Response { /* status, headers, body stream, url-list, type, … */ }

/// Caller-owned, shareable across requests. Holds the mutable policy +
/// storage state that the Fetch algorithm threads through.
pub struct FetchContext {
    pub cookies: Arc<dyn CookieStore>,
    pub cache: Arc<dyn HttpCache>,
    pub hsts: Arc<HstsList>,
    pub csp: Arc<dyn CspChecker>,
    pub origin: Origin,          // for CORS gating
    // tls config, connection pool handle, redirect policy, …
}

pub async fn fetch(request: Request, cx: &FetchContext) -> Response;

pub trait CookieStore { /* get/set per (origin, path), SameSite-aware */ }
pub trait HttpCache  { /* RFC 9111 store/lookup/revalidate */ }
pub trait CspChecker { fn allows_connect(&self, url: &Url) -> Decision; }
```

The `http` crate's `Request`/`Response` are the *wire* primitives; netfetcher's
`Request`/`Response` are the *Fetch-spec* wrappers (carrying mode, credentials,
response type/tainting). §9 OQ: how thin the wrapper is.

---

## 7. Increment ladder

Each increment is independently useful and testable against the WPT `fetch/`
suite (see §8).

1. **Core GET + response plumbing.** h1/h2 via hyper+rustls, redirects
   (follow/error/manual), content-encoding, the `fetch()` entry + streaming
   body, in-memory cookie jar (basic), `FetchContext`. Oracle: fetch a known
   page, byte-compare against Servo `net`.
2. **Cookies + cache.** Full RFC 6265bis jar (SameSite/secure/domain), RFC 9111
   HTTP cache with `Vary`, conditional revalidation. Pluggable `CookieStore` /
   `HttpCache` traits + in-memory defaults.
3. **CORS + CSP hook + HSTS + mixed-content.** Preflight, credentials mode,
   response tainting (basic/cors/opaque), forbidden headers, `CspChecker` hook,
   HSTS preload+dynamic, mixed-content upgrade/block.
4. **HTTP/3.** quinn + h3 lane behind a feature/runtime negotiation; Alt-Svc
   discovery.
5. **WebSocket (optional).** `async-tungstenite`, the Fetch-spec WebSocket
   handshake. Gate behind whether Mere/murm actually wants HTTP-origin
   WebSockets vs iroh streams.

POST/PUT/bodies land in increment 1's tail (method+body are core); the ladder is
ordered by *policy depth*, not verb coverage.

---

## 8. Conformance oracle

Two oracles, used together:

- **Servo `net` byte-diff** (early): for increments 1–2, compare netfetcher's
  response (headers, body, redirect chain) against Servo `net` for the same
  request. Catches transport/encoding regressions cheaply.
- **WPT `fetch/` suite** (ongoing): the Web Platform Tests `fetch/` directory is
  the spec-conformance measuring stick for CORS/credentials/tainting/cache.
  Wiring a WPT-fetch harness is a shared need with genet (which wants a
  WPT-runner for layout/CSS); building it once and pointing it at both is the
  efficient path. Tracked as a cross-cutting follow-up, not a netfetcher
  blocker.

---

## 9. Open questions

1. **`Request`/`Response` types — `http` crate vs Fetch-spec types.** Wrap the
   `http` crate thinly, or define standalone Fetch types? Leaning thin-wrapper
   (interop with the ecosystem; genet's scripting tier and murm can share
   them), but response tainting/mode/credentials need somewhere to live.
   **Decided (increment 1), provisionally:** netfetcher owns standalone
   `Request`/`Response` types (tainting/mode/credentials have a natural home);
   the `http` crate is used only at the wire boundary inside `fetch`. Revisit if
   a shared-types consumer (murm) materializes.
2. **Async runtime — tokio-pinned vs runtime-agnostic.** hyper 1.x is
   runtime-agnostic via `hyper-util`; quinn leans tokio. Pin tokio for v1
   (Mere's host already runs one) and revisit agnosticism only if a wasm or
   alt-runtime consumer appears. **Decided (increment 1): tokio pinned.**
3. **TLS backend — ring vs aws-lc-rs.** rustls 0.23 supports both; aws-lc-rs is
   FIPS-capable and increasingly the default. Pick to match whatever the rest of
   the Mere stack settles on (murm/iroh also use rustls — unify).
   **Decided (increment 1): ring** (installed as the process default provider).
   Revisit if the Mere stack standardizes on aws-lc-rs (a one-feature swap).
4. **Pluggable storage ownership.** Do `CookieStore`/`HttpCache` live in
   netfetcher (traits + in-mem default) with Mere supplying durable impls, or
   does eidetic own the durable side? Leaning: traits in netfetcher, durable
   impls in Mere/eidetic (keeps netfetcher storage-agnostic and wasm-friendlier).
   **Decided (increment 1):** traits live in netfetcher with in-memory defaults,
   as `&self` + `Send + Sync` (interior mutability) so a shared `&FetchContext`
   both attaches and records across `.await` on a multi-thread runtime. Durable
   impls remain Mere/eidetic's, supplied later. (This is the resolution of the
   §6 `Arc<dyn …>` interior-mutability question.)
5. **`eidetic-https-fetcher` convergence.** Should it become a thin adapter over
   netfetcher (one HTTP stack), or stay a minimal hash-verified blob GET?
   Default: leave it until duplication actually hurts; they serve different
   policies (content-addressed verify vs Fetch-spec).
6. **Repo placement + naming.** Own repo under `Code/repos/netfetcher/`
   (sibling to genet/netrender), reserved on crates.io? Confirm the `net*`
   naming doesn't collide with `netrender` confusingly (render vs fetch — both
   "net", different organs; acceptable, but note it).
7. **Shared `Request`/`Response` reuse by murm/iroh.** P2P transports aren't
   HTTP, but a unified request/response *vocabulary* across organs could be
   valuable — or premature. Defer; flag if murm work wants it.

---

## 10. Relationship to other plans

- **Consumes nothing upstream of itself**; is consumed by Mere host
  (`FetcherPool` worker — [`session_service_runner_plan`](2026-05-14_session_service_runner_plan.md)),
  genet (indirectly, via byte seams), and possibly eidetic (§9.5).
- **Policy composition:** engine profile boundary
  ([plan](2026-05-14_engine_profile_boundary_plan.md)) supplies cookie/cache
  partitioning; capability gates
  ([brief](../research/2026-05-14_capability_gate_catalogue_brief.md)) drive the
  CSP/`connect-src` hook.
- **Conformance:** shares a WPT-runner harness with genet's CSS/layout
  conformance effort.
- **Sibling organs:** `netrender` (paint→GPU, done), `genet` (render engine),
  `net-media` ([plan](2026-05-26_net_media_plan.md): WebRTC + media decode — a
  separate organ; netfetcher does not do media streaming/DRM).

---

## 11. Integration — wiring netfetcher into a consumer (the next step)

The engine is complete (increments 1–5); nothing calls it yet. This is the
load-bearing next move — it closes the "no consumer" gap and validates the organ
against real use. The shape:

1. **Mere side — the `FetcherPool` worker.** `SessionServiceRunner`'s
   `WorkerKind::FetcherPool` instantiates a `FetchContext` with **host-backed
   seams** (the seams were designed for exactly this):
   - `CookieStore` → eidetic-/persona-scoped durable jar (per the
     [engine-profile-boundary plan](2026-05-14_engine_profile_boundary_plan.md)).
   - `HttpCache` → eidetic-backed durable cache.
   - `CspChecker` → bridges to Mere's
     [capability gates](../research/2026-05-14_capability_gate_catalogue_brief.md)
     (`connect-src` decisions).
   - `HstsStore` / `AltSvcStore` / `PreflightCache` → durable or per-session.

   The worker takes a request (URL + method + headers + initiator origin), runs
   `fetch`, and returns the `Response`.
2. **genet side — byte seams (no genet change).** genet's `ImageLoader`
   (and stylesheet/script seams) already take *bytes*; the host fetches via
   netfetcher and hands them back. genet never links netfetcher. Flow: genet
   reports a needed URL → host fetches → bytes → genet's seam.
3. **CORS stays host-side.** The host supplies the initiating document's
   `origin`; netfetcher does the cross-origin gating; genet just gets bytes
   (success) or nothing (blocked) — it never reasons about CORS.
4. **Streaming vs collect.** netfetcher's `ResponseBody` is a stream; the
   image/stylesheet seams want complete bytes (worker collects), while
   progressive consumers (future) take the stream.
5. **The wasm split (per §3).** Native host → netfetcher. wasm/PWA → a host
   adapter over the **browser's** `fetch`. genet's seams are identical across
   both (they only take bytes), so **genet is unchanged native↔wasm — only the
   host's fetcher differs.**
6. **eidetic-https-fetcher convergence (OQ#5), on contact.** Once netfetcher is
   the host fetch path, eidetic's `BlobFetcher` can either delegate to it (one
   HTTP stack) or stay a minimal hash-verified blob GET. Decide then.

**Smallest first integration:** one native consumer doing a single GET through
netfetcher into genet's `ImageLoader` — proves the whole organ end-to-end
against real use, and surfaces any seam-shape mismatch early.

---

## Findings

*(populated as research/extraction proceeds)*

- Existing in-workspace networking is content-addressed-blob-shaped
  (`eidetic-https-fetcher`, `ureq`, hash-verified), not Fetch-shaped — confirms
  the gap netfetcher fills is real and unoccupied.
- `WorkerKind::FetcherPool` already reserved as the host-side driver slot;
  netfetcher is its intended payload.
- genet's `ImageLoader` / stylesheet / script seams are already byte-taking,
  so the "Mere fetches, genet consumes" layering needs no genet change.

## Progress

- **2026-05-25** — plan created. Scope, layering, de-IPC strategy, dep stack,
  increment ladder, and open questions fixed. No code yet (plan-only).
- **2026-05-25** — repo scaffolded at `repos/netfetcher/` (compile-ready API
  skeleton; git init + weave).
- **2026-05-25** — **increment 1 landed.** Real fetch: h1/h2 GET/POST over
  hyper 1.9 + hyper-rustls 0.27 + rustls 0.23 (ring, webpki-roots) via a
  process-pooled `hyper-util` legacy client; redirect handling (follow with
  spec method-rewrites + 20-cap & url_list / error → network-error / manual →
  opaque-redirect); `Content-Encoding` decode (gzip/deflate/br/zstd, multi-layer,
  lenient); cookie attach + `Set-Cookie` record; CSP `connect-src` hook. Modules:
  `client` / `decode` / `fetch` + the `&self` seams in `context`. **8 tests green**
  against a `mockito` mock server (GET, redirect-follow, redirect-error, gzip
  decode, set-cookie record) + decode unit tests; clean compile in 37.84s.
  Resolved OQ #2 (tokio), #3 (ring), #4 (trait location + interior mutability),
  and #1 provisionally (own types).
- **2026-05-25** — **streaming bodies** (the increment-1 deferral) landed.
  `Response.body` is a `ResponseBody` stream (`io::Result<Bytes>`) yielding
  *decoded* chunks via the `StreamReader → async-compression → ReaderStream`
  pipeline; `Response::bytes()` collects. `flate2`/`brotli`/`zstd` direct deps
  dropped for `async-compression`.
- **2026-05-25** — **increment 2 landed** (cookies + cache). **Cookies:** real
  RFC 6265bis `InMemoryCookieJar` (parse via `cookie` 0.18; domain/path/secure/
  expiry matching; longest-path-first serialization; `Max-Age` over `Expires`;
  `SameSite` stored, enforcement deferred to increment 3 — needs site-for-cookies
  context). **Cache:** RFC 9111 core, with a clean **storage-vs-policy split** —
  `HttpCache` became a dumb `get`/`put`/`enabled` storage seam over a
  `StoredResponse`, while cacheability/freshness/revalidation policy lives in
  netfetcher's `cache` module and drives the fetch loop (fresh hit → no network;
  stale/`no-cache` + `ETag`/`Last-Modified` → conditional GET → `304` → serve
  refreshed stored entry; `no-store` honored; `Vary` responses conservatively not
  cached). This sharpens OQ #4: durable storage is Mere/eidetic's job, *policy* is
  netfetcher's — so a host cache impl can't get RFC 9111 wrong. **22 tests green.**
- **2026-05-26** — **increment 3 landed** (cross-origin security model), in four
  committed slices. **(1)** Request gains an initiator `origin`; response tainting
  (`Basic`/`Cors`/`Opaque`) + simple-request CORS gating (blocked → network error).
  **(2)** HSTS (`HstsStore` seam; known-secure http→https upgrade on initial +
  redirect targets; `Strict-Transport-Security` recorded over https) + mixed-content
  **auto-upgrade** (http target in an https-origin context → https). **(3)**
  **SameSite enforcement** — `CookieStore::cookies_for` grew a `SameSiteContext`;
  Strict/Lax/None gated; same-site computed by registrable-domain approximation (no
  PSL yet). **(4)** **CORS preflight** (OPTIONS round-trip with
  `Access-Control-Request-*`, `Allow-Origin/Methods/Headers` checks, grant cached per
  `Access-Control-Max-Age` via a `PreflightCache` seam) + `Cors` response-header
  filtering (safelist + `Expose-Headers`). Two wrinkles were taken to Mark mid-slice
  and decided: mixed-content **auto-upgrade now / active-passive split later**;
  SameSite **registrable-domain approximation now / PSL later**. **47 tests green**
  (offline via mockito). **Deferred:** active/passive mixed-content split,
  public-suffix-accurate same-site, then increment 4 (HTTP/3) and increment 5
  (optional WebSocket).
- **2026-05-26** — **increment 4 landed** (HTTP/3), in three committed steps.
  **4a:** an `h3_client` transport over quinn 0.11 + h3 0.0.8 (QUIC connect → drive
  the connection concurrently with the request via `tokio::select!` → collect),
  verified by a real offline round-trip against an in-process quinn h3 server
  (rcgen self-signed cert, ALPN h3, no-verify test client) — native-only, wasm-
  excluded. **4b-1:** Alt-Svc discovery — `AltSvcStore` seam + `Alt-Svc` parsing
  (h3 alternative / `ma` / `clear`), recorded off responses; production webpki QUIC
  config. **4b-2 (Mark chose the clean path over a minimal-with-gaps branch):** a
  **transport-abstraction refactor** — both h1/h2 and h3 now yield a common
  `RawResponse { status, headers, body-stream }`, and the shared back half
  (cookie/HSTS/Alt-Svc recording, redirects, tainting, `Cors` filtering, cache,
  content-encoding decode) runs over either; `send_request` prefers h3 when an
  https origin advertised it (bodyless requests) and falls back to h1/h2. So h3 has
  **full back-half parity** (redirects, cache, revalidation), not a gappy side
  path. **51 tests green** — the existing suite passing through the new abstraction
  confirms h1/h2 parity. Honest gap: the combined Alt-Svc→live-h3-success path
  isn't offline-unit-tested (production webpki config won't trust a self-signed
  test server; mockito is http-only) — covered by the 4a transport test +
  routing-decision/recording tests. **Deferred:** h3 for requests with bodies,
  active/passive mixed-content split, PSL same-site.
- **2026-05-26** — **increment 5 landed** (WebSocket), completing the ladder. New
  native-only `websocket` module over `tokio-tungstenite` 0.29: `connect()` does
  the RFC 6455 handshake (`ws://` and `wss://` via rustls) and returns a
  `WebSocket` whose `send`/`recv` use a `WsMessage` enum (text/binary/ping/pong/
  close) decoupled from tungstenite types. Verified by an in-process `ws://` echo
  round-trip (text + binary); `wss://` supported via webpki but not offline-tested.
  **52 tests green.** **Ladder complete (1–5).** Remaining is *not* more protocol
  surface but **integration** (wire netfetcher into a consumer — Mere's
  `FetcherPool` + genet's byte-seams; see §11) and the deferred refinements
  (h3-with-bodies, active/passive mixed-content, PSL same-site). Confirmed
  native-only as a whole (hyper/tokio) — wasm uses the browser's fetch (§3).
