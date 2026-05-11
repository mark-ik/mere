/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Address → EngineDocument pipeline.
//!
//! Owns the small amount of host glue that turns a typed URL into a
//! rendered `EngineDocument`:
//!
//! 1. Fetch the address body (`file://` via `std::fs`, `mere://` from
//!    bundled demo content; network schemes are deferred until a real
//!    transport — `serval`, `mere-transport` — lands).
//! 2. Optionally sniff a content-type from the bytes.
//! 3. Route through [`inker::EngineRoutePolicy`], filtered to engines
//!    the registry actually exposes.
//! 4. Dispatch the matched engine.
//!
//! Engines themselves stay portable — fetching is the host's job, per
//! the engine-layer contract — so this module is the single place
//! that touches the filesystem or any future network transport.

use inker::{
    EngineDocument, EngineError, EngineInput, EngineRegistry, EngineRoutePolicy,
    EngineRouteRequest, WorkspaceRouteId, sniff_content_type,
};
use nematic::{
    FeedEngine, FileEngine, FingerEngine, GemtextEngine, GopherEngine, GuppyEngine, KnotEngine,
    MarkdownEngine, MisfinEngine, NexEngine, ScrollEngine, TextEngine,
};

/// The address scheme used for in-app demo / introduction pages.
/// Resolved against the bundled fixtures in [`builtin_body`].
pub const MERE_BUILTIN_SCHEME: &str = "mere";

/// Build the default engine registry for the host: every nematic
/// engine that doesn't require a network transport.
///
/// Network-bound engines (gopher, finger, scroll, misfin, nex,
/// guppy) are still registered so routing decisions match the user's
/// intent, but dispatch will fail until the host gains a transport.
/// That's fine for v0 — the failure surfaces as an `EngineError` the
/// caller can show in the document area.
pub fn default_registry() -> EngineRegistry {
    let mut reg = EngineRegistry::new();
    reg.register(Box::new(MarkdownEngine::new()));
    reg.register(Box::new(GemtextEngine::new()));
    reg.register(Box::new(GopherEngine::new()));
    reg.register(Box::new(FeedEngine::new()));
    reg.register(Box::new(KnotEngine::new()));
    reg.register(Box::new(TextEngine::new()));
    reg.register(Box::new(FileEngine::new()));
    reg.register(Box::new(FingerEngine::new()));
    reg.register(Box::new(ScrollEngine::new()));
    reg.register(Box::new(MisfinEngine::new()));
    reg.register(Box::new(NexEngine::new()));
    reg.register(Box::new(GuppyEngine::new()));
    reg
}

/// Fetch + sniff + route + dispatch an address. Returns an
/// `EngineDocument` ready to hand to the workbench renderer.
pub fn load(
    registry: &EngineRegistry,
    policy: &EngineRoutePolicy,
    address: &str,
) -> Result<EngineDocument, EngineError> {
    let raw = fetch(address)?;
    let sniffed = sniff_content_type(raw.as_bytes()).map(str::to_string);
    let request = EngineRouteRequest {
        workspace_id: WorkspaceRouteId::new("default"),
        view: None,
        node: None,
        address: address.to_string(),
        content_type: sniffed.clone(),
        pinned_engine: None,
    };
    let decision = policy.route_filtered(&request, |id| registry.contains(id));
    let mut input = EngineInput::new(address, raw);
    if let Some(ct) = sniffed {
        input = input.with_content_type(ct);
    }
    tracing::debug!(
        address = %address,
        engine_id = %decision.engine_id,
        "dispatching engine"
    );
    registry.dispatch(&decision, &input)
}

/// Fetch the body for `address`.
///
/// - `file://` reads from disk via [`std::fs`].
/// - `mere://` returns one of the bundled demo bodies (an introductory
///   page when the path is empty, otherwise the path-keyed fixture).
/// - Other schemes return `EngineError::Unsupported` — they'll come
///   online once the host gains a network transport.
fn fetch(address: &str) -> Result<String, EngineError> {
    let (scheme, rest) = split_scheme(address)
        .ok_or_else(|| EngineError::InvalidContent(format!("no scheme in address: {address}")))?;
    match scheme {
        "file" => fetch_file(rest),
        MERE_BUILTIN_SCHEME => Ok(builtin_body(rest)),
        other => Err(EngineError::Unsupported(format!(
            "no transport for scheme `{other}` (only file:// and mere:// are wired in v0)"
        ))),
    }
}

fn split_scheme(address: &str) -> Option<(&str, &str)> {
    let colon = address.find(':')?;
    let scheme = &address[..colon];
    if scheme.is_empty() || !scheme.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.') {
        return None;
    }
    let after = &address[colon + 1..];
    // Tolerate `scheme:path`, `scheme://authority/path`, `scheme:///path`.
    let after = after.strip_prefix("//").unwrap_or(after);
    Some((scheme, after))
}

fn fetch_file(rest: &str) -> Result<String, EngineError> {
    // After split_scheme, rest is the part after `file:` or `file://`.
    // On Windows, `file:///c:/path` decodes to `/c:/path` — strip the
    // leading slash before letting std::path see it. (POSIX paths
    // legitimately start with `/`, so only strip when the next chars
    // look like a drive letter.)
    let path = match rest.strip_prefix('/') {
        Some(stripped)
            if stripped.len() >= 2
                && stripped.as_bytes()[1] == b':'
                && stripped.as_bytes()[0].is_ascii_alphabetic() =>
        {
            stripped.to_string()
        }
        _ => rest.to_string(),
    };
    let path = percent_decode(&path);
    std::fs::read_to_string(&path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => EngineError::NotFound(path),
        _ => EngineError::Io(format!("{path}: {e}")),
    })
}

/// Minimal percent-decoder — enough for `file://` paths the user types
/// or pastes. We don't pull `percent-encoding` for this; the keystroke
/// budget is tiny.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(a), Some(b)) = (
                hex_value(bytes[i + 1]),
                hex_value(bytes[i + 2]),
            ) {
                out.push((a << 4) | b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Bundled bodies for `mere://` addresses. Used as the v0 home page
/// and as a quick way to demo the omnibar without touching the
/// filesystem.
fn builtin_body(rest: &str) -> String {
    // Trim a leading `/` (after split_scheme stripped `//`) so the
    // user can write either `mere:intro` or `mere://intro` or
    // `mere:///intro` and land on the same body.
    let name = rest.trim_start_matches('/');
    match name {
        "" | "intro" | "home" => INTRO.to_string(),
        "engines" => ENGINES.to_string(),
        other => format!(
            "# Unknown built-in page\n\nNo bundled body for `mere://{other}`.\n\nTry one of: `mere://intro`, `mere://engines`."
        ),
    }
}

const INTRO: &str = r#"# mere://intro

Welcome to **Mere** — a graph-shaped browser whose chrome is in place
and whose engine layer is wired in.

Type any of the following in the omnibar and press Enter:

- `mere://engines` — list of currently-loaded content engines
- `file:///path/to/your.md` — open a local file (markdown, gemtext,
  gopher, knot, …)

Network-backed schemes (`https://`, `gemini://`, …) come online when
the corresponding transport lands.
"#;

const ENGINES: &str = r#"# Loaded engines

The host currently routes the following engines through inker:

- `nematic.markdown` — markdown / commonmark
- `nematic.gemtext` — Gemini's text/gemini
- `nematic.gopher` — gophermap
- `nematic.feed` — RSS / Atom
- `nematic.knot` — Mere's polyglot knot format
- `nematic.text` — plain text fallback
- `nematic.file` — extension-routed file viewer (delegates to the
  above by extension)
- `nematic.finger`, `nematic.scroll`, `nematic.misfin`,
  `nematic.nex`, `nematic.guppy` — registered but awaiting a network
  transport.

Routing decisions go: pinned engine → content-type → per-host →
scheme → fallback. Sniffing fills in the content type when the
transport doesn't supply one.
"#;
