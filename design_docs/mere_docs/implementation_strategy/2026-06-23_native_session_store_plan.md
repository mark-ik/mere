# Mere-native session + storage store

**Date**: 2026-06-23
**Status**: Findings resolved; foundational thread (persistent shared jar + flip
SESSION layer + standard `Cookie` shape) in progress this session.
**Origin**: surfaced building the verso serval→scrying flip
([flipcarrier plan](../../verso_docs/implementation_strategy/2026-06-23_serval_scrying_flipcarrier_plan.md)).
Carrying a login across a flip forced the question: *what is Mere's own session
state, where does it live, and how much of its shape is standardized?*

---

## The insight

In modeling how to carry session into the system WebView, we are really designing
Mere's native session state. The WebView is not a new model to copy. It is the
third implementation of one **standardized** model (RFC 6265bis cookies, the WHATWG
Storage Standard) that Mere already carries twice. That shared standard is exactly
what makes a flip possible: engines differ in *rendering* fidelity but agree on
*session semantics*, so cookies and storage move losslessly between them.

So the work is not "build a session store." It is: **converge on the one Mere
already uses, make it persistent + durable + partitioned, and let every consumer
(fetch, the script runtime, the flip, the WebView) read the same store.**

## Findings (verified 2026-06-23)

### Three jars, one standard library

| Where | What it is | Used by meerkat today |
| --- | --- | --- |
| **netfetcher** `cookie_jar.rs` | `InMemoryCookieJar` + the `CookieStore` trait. Full RFC 6265bis: domain/path match, `Secure`, `Max-Age` over `Expires`, `SameSite` gating, longest-path serialization. Built on the [`cookie`](https://crates.io/crates/cookie) crate. **Pluggable** (`Box<dyn CookieStore>`). | **Yes** — the http(s) WHATWG-Fetch lane (`fetch::do_fetch`). |
| **serval** `components/shared/net` (Servo net) | Full Servo jar: `cookies_for_url`/`set_cookie_for_url`, `CookieSource` (the HTTP-vs-script HttpOnly gate), `CookieStoreId` partitioning. Same `cookie` crate. | No — meerkat chose netfetcher over the heavy Servo resource thread. |
| **system WebView** (scrying) | The native OS cookie store. Black box: `set_cookie` in, `request_all_cookies` out. | Only at a flip boundary. |

All three model the cookie record with the **same `cookie` crate**, so the record
*is* the standard type, not a per-consumer invention.

### The real gap: no session persists

`fetch::do_fetch` builds a fresh `netfetcher::FetchContext::permissive()` **per
call** (`fetch.rs:257`, and again for subresources at `283`). Each fetch gets a new
`InMemoryCookieJar` that is dropped when the fetch ends. So a `Set-Cookie` from page
A is gone by the time page B is fetched: **logins do not persist across navigations
at all yet.** The flip's SESSION layer found "no host-side cookie jar" because there
is no *living* jar to read.

netfetcher already anticipates the fix. Its `FetchContext` doc says it is
"caller-owned and shareable across requests"; the seams take `&self` with interior
mutability and are `Send + Sync`, so one shared context (or one shared
`CookieStore`) is the intended shape. meerkat simply news up a throwaway each time.

### Storage (the non-cookie half)

serval's script runtime has an in-memory `localStorage` (one origin per runtime, no
persistence; `script-runtime-api/platform.rs`). `sessionStorage`, IndexedDB, and any
durability or partitioning are unbuilt. This is the genuinely-greenfield part of the
model; cookies are mostly already there.

## Standards (the "store stuff" is determined)

| Concern | Standard | State in Mere |
| --- | --- | --- |
| Cookie record + domain/path/secure/expiry match | RFC 6265bis (the `cookie` crate) | netfetcher: done; serval-net: done |
| Script-vs-HTTP access (HttpOnly), SameSite gating | RFC 6265bis + `CookieSource`/`SameSiteContext` | netfetcher: stored + gated; HttpOnly access gate not yet wired to a script reader |
| `localStorage` / `sessionStorage` | WHATWG HTML §Web Storage | serval: in-memory localStorage only |
| Partitioning, quota, persistence, buckets | WHATWG Storage Standard (storage key = origin + top-level site) | serval's `CookieStoreId` is the partition key; not wired in netfetcher |
| Async cookie JS API | Cookie Store API (W3C WICG) | not yet |
| Partitioned third-party access | Storage Access API (W3C) | not yet |

The portable `verso-api::Cookie` must mirror this record (it currently omits
`SameSite`, expiry, and `Partitioned` — lossy across a flip).

## Architecture (resolved)

One **Mere session substrate**, standard-shaped, consumed by every engine:

- **Owner**: netfetcher's `CookieStore` seam (the lane meerkat fetches through).
  Keep serval's Servo-net jar as a partitioning *reference*, not a routing target
  (its weight is why meerkat chose netfetcher).
- **Persistence**: an eidetic-backed `CookieStore` (and, later, storage-area store)
  behind the existing trait. The seam is already pluggable; this is a drop-in impl.
- **Partitioning**: per the Storage Standard, key by origin + top-level site (and
  per-persona for Mere's multi-persona model). serval's `CookieStoreId` is the model.
- **Script integration**: serval's `document.cookie` / Cookie Store API and
  `localStorage`/`sessionStorage` read the **same** substrate (serval's
  `FetchHandler` is already host-injected; add a cookie/storage seam beside it), so a
  login made in JS and one made over HTTP share state.
- **Flip**: verso reads the substrate at the flip boundary to fill the SESSION layer
  (forward) and writes back on flip-back (a login made *inside* the WebView comes
  home). The WebView is synced one-shot, never continuously mirrored (charter §7).

## Threads

1. **Persistent shared jar** *(this session)* — meerkat holds one long-lived
   `CookieStore` injected into every `FetchContext`, so sessions persist across
   navigations. High-value on its own (logins survive browsing), independent of the
   flip. v1 is a single unpartitioned process jar.
2. **Flip SESSION layer** *(this session)* — the trigger reads `cookies_for(url)`
   from the shared jar to fill `PortableViewState.cookies`; the flip sets them on the
   WebView before navigating. A logged-in serval page flips to a logged-in WebView.
3. **Standard `verso-api::Cookie`** *(this session)* — add `same_site`, `expires`,
   `partitioned`, so the interchange is lossless.
4. **Durability + partitioning** *(future)* — an eidetic-backed `CookieStore`,
   partitioned by origin + top-level site + persona. Replaces the v1 process jar.
5. **Lossless structured read** *(future)* — a structured cookie accessor on
   netfetcher's `CookieStore` (today `cookies_for` returns `name=value` header
   strings; the flip rebuilds attributes lossily from the URL). Needed so
   `Secure`/`HttpOnly`/`SameSite`/`Domain` cross faithfully.
6. **Script ↔ substrate + storage areas** *(future)* — serval `document.cookie` /
   Cookie Store API on the shared jar; durable + partitioned `localStorage` /
   `sessionStorage` / IndexedDB per the Storage Standard.
7. **Flip-back SESSION** *(future)* — read the WebView jar (`request_all_cookies`)
   into the substrate on flip-back (§5 of the flipcarrier plan).

## Progress

- **2026-06-23 (findings)**: mapped the three jars, the per-fetch persistence gap,
  netfetcher's shareable-context intent, and the standards table. Confirmed the
  session model is mostly *already implemented* (netfetcher, RFC 6265bis) and the
  work is persistence + convergence, not a new build.
