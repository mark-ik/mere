/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Process-wide cookie jar + persistence (the fetch sessions cookie store).

use super::*;

/// The process-wide HTTP cookie jar: one persistent RFC 6265bis session shared by
/// every fetch, so a `Set-Cookie` on one page is still set on the next (logins
/// survive navigation) and the verso flip can carry the session into a
/// compatibility-view WebView. Without this each fetch built a throwaway jar, so no
/// session ever persisted. The in-memory jar is still process-global; durability is
/// persona-keyed below, and a live per-persona jar registry is the remaining
/// multi-persona step. The jar is `Send + Sync` (a `Mutex` inside), so the fetch
/// worker, the UI thread, and the scripted rung share the one `Arc`.
pub fn session_jar() -> &'static Arc<InMemoryCookieJar> {
    static JAR: OnceLock<Arc<InMemoryCookieJar>> = OnceLock::new();
    JAR.get_or_init(|| Arc::new(InMemoryCookieJar::new()))
}

/// Adapts the `Arc`-shared [`session_jar`] to netfetcher's `CookieStore` seam, which
/// takes an owned `Box<dyn CookieStore>` per context. Every per-fetch context's box
/// holds a clone of the one `Arc`, so they all read and write the same jar.
struct SharedJar(Arc<InMemoryCookieJar>);

impl CookieStore for SharedJar {
    fn cookies_for(&self, url: &url::Url, ctx: SameSiteContext) -> Vec<String> {
        self.0.cookies_for(url, ctx)
    }
    fn records_for(&self, url: &url::Url, ctx: SameSiteContext) -> Vec<CookieRecord> {
        self.0.records_for(url, ctx)
    }
    fn set_cookie(&self, url: &url::Url, set_cookie_header: &str) {
        self.0.set_cookie(url, set_cookie_header);
        // Mark the jar dirty so the next `persist_cookies` writes it through.
        COOKIES_DIRTY.store(true, Ordering::Relaxed);
    }
}

/// Set whenever a cookie is stored, cleared by [`persist_cookies`]. Lets the durable
/// write skip when nothing changed since the last persist (most fetches set no
/// cookies). Process-global, matching the single [`session_jar`].
static COOKIES_DIRTY: AtomicBool = AtomicBool::new(false);

/// Mark the jar dirty so the next [`persist_cookies`] writes it through. For cookie
/// writes that bypass `SharedJar` — e.g. a script's `document.cookie` via the scripted
/// rung's cookie provider, which uses the raw [`session_jar`]. (Render ladder 2c.)
pub fn mark_cookies_dirty() {
    COOKIES_DIRTY.store(true, Ordering::Relaxed);
}

/// One persisted cookie. A serde mirror of netfetcher's `CookieRecord` (whose
/// `same_site` is the `cookie` crate's enum, not directly serde-friendly), encoded as
/// JSON in its own per-cookie blob in the durable store.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct PersistedCookie {
    name: String,
    value: String,
    domain: String,
    host_only: bool,
    path: String,
    secure: bool,
    http_only: bool,
    /// 0 = Strict, 1 = Lax, 2 = None; absent = unspecified.
    same_site: Option<u8>,
    /// Absolute expiry in Unix seconds; absent = session cookie.
    expires: Option<f64>,
}

impl PersistedCookie {
    fn from_record(r: CookieRecord) -> Self {
        Self {
            name: r.name,
            value: r.value,
            domain: r.domain,
            host_only: r.host_only,
            path: r.path,
            secure: r.secure,
            http_only: r.http_only,
            same_site: r.same_site.map(|s| match s {
                SameSite::Strict => 0,
                SameSite::Lax => 1,
                SameSite::None => 2,
            }),
            expires: r.expires,
        }
    }

    fn into_record(self) -> CookieRecord {
        CookieRecord {
            name: self.name,
            value: self.value,
            domain: self.domain,
            host_only: self.host_only,
            path: self.path,
            secure: self.secure,
            http_only: self.http_only,
            same_site: match self.same_site {
                Some(0) => Some(SameSite::Strict),
                Some(1) => Some(SameSite::Lax),
                Some(2) => Some(SameSite::None),
                _ => None,
            },
            expires: self.expires,
        }
    }
}

/// A cookie's durable identity: `(domain, path, name)`, the RFC 6265 tuple that
/// uniquely names a cookie. The unit of incremental persistence.
type CookieKey = (String, String, String);

/// A mirror of what is currently written to the durable store, per persona, so a
/// persist writes only the cookies that *changed* (incremental) rather than the whole
/// jar each time. Populated on [`load_cookies`] and kept in step by [`persist_cookies`].
pub(crate) fn last_persisted() -> &'static Mutex<HashMap<CookieKey, PersistedCookie>> {
    static LAST: OnceLock<Mutex<HashMap<CookieKey, PersistedCookie>>> = OnceLock::new();
    LAST.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn cookie_key_of(c: &PersistedCookie) -> CookieKey {
    (c.domain.clone(), c.path.clone(), c.name.clone())
}

/// The durable-store blob key for one cookie under `persona`. Keyed by persona so one
/// persona's session never bleeds into another (the native session store's hard
/// partition; v0 is the single default persona, but the key already partitions), then
/// by the hex-encoded `(domain, path, name)` identity (hex so a path's `/` can't
/// collide with the key's own separators).
pub(crate) fn cookie_blob_key(persona: PersonaId, key: &CookieKey) -> String {
    let identity = format!("{}\u{0}{}\u{0}{}", key.0, key.1, key.2);
    let hex: String = identity.bytes().map(|b| format!("{b:02x}")).collect();
    format!("{}{hex}", cookie_prefix(persona))
}

pub(crate) fn cookie_prefix(persona: PersonaId) -> String {
    format!("cookies/{}/", persona.0)
}

/// Persist the shared jar to the durable `store` for `persona`, writing only the
/// cookies that changed since the last persist and deleting blobs for cookies that are
/// gone (expired / cleared). Dirty-gated, so a fetch that set no cookies is a no-op.
/// Each cookie is its own small blob, so one `Set-Cookie` writes one blob, not the
/// whole jar. Runs the async store to completion on the calling (UI) thread, like the
/// rest of meerkat's eidetic use.
pub fn persist_cookies(store: &mut dyn Store, persona: PersonaId) {
    if !COOKIES_DIRTY.swap(false, Ordering::Relaxed) {
        return;
    }
    let current: HashMap<CookieKey, PersistedCookie> = session_jar()
        .all_records()
        .into_iter()
        .map(PersistedCookie::from_record)
        .map(|c| (cookie_key_of(&c), c))
        .collect();

    let mut shadow = match last_persisted().lock() {
        Ok(shadow) => shadow,
        Err(_) => return,
    };
    let mut any_failed = false;

    // Writes: new or changed cookies only.
    for (key, cookie) in &current {
        if shadow.get(key) == Some(cookie) {
            continue;
        }
        match serde_json::to_vec(cookie) {
            Ok(bytes) => {
                match pollster::block_on(store.save_blob(&cookie_blob_key(persona, key), &bytes)) {
                    Ok(()) => {
                        shadow.insert(key.clone(), cookie.clone());
                    }
                    Err(err) => {
                        tracing::warn!(%err, "cookie persist: save failed");
                        any_failed = true;
                    }
                }
            }
            Err(err) => {
                tracing::warn!(%err, "cookie persist: serialize failed");
                any_failed = true;
            }
        }
    }

    // Deletes: cookies that were persisted but are gone from the jar now.
    let removed: Vec<CookieKey> = shadow
        .keys()
        .filter(|k| !current.contains_key(*k))
        .cloned()
        .collect();
    for key in removed {
        match pollster::block_on(store.delete_blob(&cookie_blob_key(persona, &key))) {
            Ok(_) => {
                shadow.remove(&key);
            }
            Err(err) => {
                tracing::warn!(%err, "cookie persist: delete failed");
                any_failed = true;
            }
        }
    }

    // A failed write/delete leaves the shadow out of step; re-arm so the next change
    // retries it rather than the failure being silently dropped.
    if any_failed {
        COOKIES_DIRTY.store(true, Ordering::Relaxed);
    }
}

/// Load `persona`'s persisted cookies from the durable `store` into the shared jar at
/// startup, so a login survives an app restart, and seed the persisted-shadow so the
/// first persist after load does not rewrite everything. A persona with no stored
/// cookies (first run) leaves the jar empty.
pub fn load_cookies(store: &mut dyn Store, persona: PersonaId) {
    let keys = match pollster::block_on(store.iter_keys(&cookie_prefix(persona))) {
        Ok(keys) => keys,
        Err(err) => {
            tracing::warn!(%err, "cookie load: iter_keys failed");
            return;
        }
    };
    let mut records = Vec::with_capacity(keys.len());
    let mut shadow = match last_persisted().lock() {
        Ok(shadow) => shadow,
        Err(_) => return,
    };
    for key in keys {
        match pollster::block_on(store.load_blob(&key)) {
            Ok(Some(bytes)) => match serde_json::from_slice::<PersistedCookie>(&bytes) {
                Ok(cookie) => {
                    shadow.insert(cookie_key_of(&cookie), cookie.clone());
                    records.push(cookie.into_record());
                }
                Err(err) => tracing::warn!(%err, "cookie load: deserialize failed"),
            },
            Ok(None) => {}
            Err(err) => tracing::warn!(%err, "cookie load: read failed"),
        }
    }
    drop(shadow);
    session_jar().load_records(records);
}

/// A `FetchContext` whose cookie jar is the shared [`session_jar`]; the other seams
/// keep the permissive in-memory defaults for now. Built per fetch (cheap: the jar
/// inside is shared by `Arc`), so cookies accumulate across navigations.
pub(crate) fn session_context() -> netfetcher::FetchContext {
    let mut cx = netfetcher::FetchContext::permissive();
    cx.cookies = Box::new(SharedJar(session_jar().clone()));
    cx
}

/// The shared session's cookies for `url`, as portable records for a verso flip.
/// Reads the jar's structured same-site cookies (a flip is a same-origin top-level
/// navigation) and maps each to a [`verso_api::Cookie`], carrying `Domain` / `Path` /
/// `Secure` / `HttpOnly` / `SameSite` / expiry faithfully (the lossless structured
/// read, native session store plan §5). `Partitioned` is not tracked by the jar yet.
pub fn session_cookies_for(url: &str) -> Vec<verso_api::Cookie> {
    let Ok(parsed) = url::Url::parse(url) else {
        return Vec::new();
    };
    session_jar()
        .records_for(&parsed, SameSiteContext::same_site())
        .into_iter()
        .map(|r| verso_api::Cookie {
            name: r.name,
            value: r.value,
            domain: r.domain,
            path: r.path,
            secure: r.secure,
            http_only: r.http_only,
            same_site: r.same_site.map(map_same_site),
            expires: r.expires,
            partitioned: false,
        })
        .collect()
}

/// netfetcher's `SameSite` (the `cookie` crate's) to verso's engine-agnostic one.
pub(crate) fn map_same_site(same_site: SameSite) -> verso_api::SameSite {
    match same_site {
        SameSite::Strict => verso_api::SameSite::Strict,
        SameSite::Lax => verso_api::SameSite::Lax,
        SameSite::None => verso_api::SameSite::None,
    }
}
