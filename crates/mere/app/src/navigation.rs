// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Navigation — resolve an address to content, then render it through the
//! engine seam.
//!
//! This is the omnibar's other half: [`open`] takes an address, fetches its
//! bytes, and routes them through `inker` via [`crate::engine_tile`]. There is
//! no network fetcher yet (that is the `netfetcher` slice), so [`fetch`]
//! handles only what the host can resolve locally: the seeded `mere://`
//! pages and `file://` paths. Network schemes return an honest placeholder.

use std::path::Path;

use crate::engine_tile::{RenderedTile, render_address};

/// Seeded welcome page, served at `mere://welcome`.
pub(crate) const WELCOME_MD: &str = "\
# Welcome to Mere

A **spatial browser** on the *composition spine*: graph truth → forme →
platen → verso → inker.

Type an address above and press Enter. Navigation routes through `inker`'s
engine policy; this page was parsed by the `nematic` markdown engine.

## What you can open now

- `mere://welcome` — this page
- `file:///path/to/notes.md` — a local markdown / gemtext / text file
- network schemes (`https://`, `gemini://`) — *not yet*; that is the
  `netfetcher` slice

---

    omnibar → fetch(address) → route(content_type) → Engine → EngineDocument
";

/// Resolve `address` to content and render it. Always returns a tile (errors
/// and unsupported schemes become a rendered page, never a panic).
pub fn open(address: &str) -> RenderedTile {
    let (body, content_type) = fetch(address);
    render_address(address, &body, content_type.as_deref())
}

/// Locally resolve an address to `(body, content_type)`. No network.
fn fetch(address: &str) -> (String, Option<String>) {
    if address == "mere://welcome" {
        return (WELCOME_MD.to_string(), Some("text/markdown".to_string()));
    }

    if let Some(name) = address.strip_prefix("mere://") {
        // Any other mere:// address gets a generated page, so orrery nodes
        // (seeded with mere:// urls) open to real rendered content.
        return (
            format!(
                "# {name}\n\nA seeded Mere page at `{address}`.\n\nOpened from the orrery. \
                 Navigation routed through `inker`; rendered by the `nematic` markdown engine."
            ),
            Some("text/markdown".to_string()),
        );
    }

    if let Some(raw) = address.strip_prefix("file://") {
        let path = normalize_file_path(raw);
        return match std::fs::read_to_string(&path) {
            Ok(body) => (body, content_type_for_path(&path)),
            Err(e) => (
                format!("# Cannot read file\n\n`{path}`\n\n{e}"),
                Some("text/markdown".to_string()),
            ),
        };
    }

    (
        format!(
            "# No fetcher yet\n\nMere can't fetch `{address}` yet — network fetch is the \
             `netfetcher` slice. Try `mere://welcome` or a `file://` path to a local \
             `.md` / `.gmi` / `.txt` file."
        ),
        Some("text/markdown".to_string()),
    )
}

/// Turn the part after `file://` into a filesystem path. Handles the
/// `file:///C:/...` Windows form (a leading slash before a drive letter) while
/// leaving POSIX absolute paths (`/home/...`) intact.
fn normalize_file_path(raw: &str) -> String {
    let bytes = raw.as_bytes();
    if bytes.len() > 2 && bytes[0] == b'/' && bytes[2] == b':' {
        raw[1..].to_string()
    } else {
        raw.to_string()
    }
}

/// Content type from a file extension; `None` lets the file engine sniff.
fn content_type_for_path(path: &str) -> Option<String> {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let ct = match ext.as_str() {
        "md" | "markdown" => "text/markdown",
        "gmi" | "gemini" => "text/gemini",
        "txt" | "text" => "text/plain",
        _ => return None,
    };
    Some(ct.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn welcome_routes_to_markdown_engine() {
        let tile = open("mere://welcome");
        assert_eq!(tile.engine_id, "nematic.markdown");
        assert!(!tile.document.blocks.is_empty());
    }

    #[test]
    fn mere_path_renders_a_generated_markdown_page() {
        let tile = open("mere://node/3");
        assert_eq!(tile.engine_id, "nematic.markdown");
        assert!(!tile.document.blocks.is_empty());
    }

    #[test]
    fn unknown_scheme_renders_a_placeholder_not_a_panic() {
        let tile = open("https://example.com");
        // Rendered as markdown; the body names the missing fetcher.
        assert!(tile.document.title.as_deref() == Some("No fetcher yet"));
    }

    #[test]
    fn windows_drive_path_loses_its_leading_slash() {
        assert_eq!(normalize_file_path("/C:/x/y.md"), "C:/x/y.md");
        assert_eq!(normalize_file_path("/home/u/y.md"), "/home/u/y.md");
    }

    #[test]
    fn content_type_by_extension() {
        assert_eq!(content_type_for_path("a.md").as_deref(), Some("text/markdown"));
        assert_eq!(content_type_for_path("a.gmi").as_deref(), Some("text/gemini"));
        assert_eq!(content_type_for_path("a.bin"), None);
    }
}
