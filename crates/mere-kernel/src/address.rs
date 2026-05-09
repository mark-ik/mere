/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Address types and URL classification helpers.
//!
//! All functions here are WASM-clean: they rely on `url::Url::parse`,
//! `mime_guess`, and `infer` — no platform I/O.

use rkyv::{Archive, Deserialize, Serialize};

/// Address type hint for renderer selection.
///
/// Always derived from the URL scheme — never stored independently.
/// Obtain via `node.address.address_kind()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Archive, Serialize, Deserialize)]
pub enum AddressKind {
    /// Served over HTTP/HTTPS — default; Servo renders.
    #[default]
    Http,
    /// Local filesystem path (`file://` URL).
    File,
    /// Inline data URL payload.
    Data,
    /// Graphshell clip-address route (`verso://clip/...` or legacy `graphshell://clip/...`).
    GraphshellClip,
    /// Local filesystem directory path.
    Directory,
    /// Any other or unresolved scheme.
    Unknown,
}

/// Typed address carrying both the scheme classification and the raw URL string.
///
/// Replaces the two-field stopgap of `url: String` + `AddressKind`. The raw
/// URL string is always recoverable via [`Address::as_url_str`]. Structured
/// parsing (hostname extraction, path manipulation) continues to happen at the
/// render/utility layer via `cached_host_from_url` and host-side address utilities.
///
/// Uses `String` payloads rather than `url::Url` / `PathBuf` so the type
/// remains `rkyv`-serializable and WASM-clean without pulling parser
/// dependencies into the graph model layer.
#[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize)]
#[rkyv(derive(Debug, PartialEq))]
pub enum Address {
    /// HTTP or HTTPS content URL.
    Http(String),
    /// Local filesystem file (`file://` URL, non-directory).
    File(String),
    /// Inline data URL payload (`data:` scheme).
    Data(String),
    /// Graphshell clip route (`verso://clip/<id>` or `graphshell://clip/<id>`).
    /// The payload is the clip id, not the full URL.
    Clip(String),
    /// Local filesystem directory path (`file://` URL ending with `/`).
    Directory(String),
    /// Any other or unresolved scheme — stores the raw URL.
    Custom(String),
}

impl Address {
    /// Return the raw URL string for this address.
    pub fn as_url_str(&self) -> &str {
        match self {
            Address::Http(s)
            | Address::File(s)
            | Address::Data(s)
            | Address::Directory(s)
            | Address::Custom(s)
            | Address::Clip(s) => s.as_str(),
        }
    }

    /// Derive the legacy `AddressKind` from this address.
    ///
    /// Provided as a bridge accessor during the migration so callers that
    /// still pattern-match on `AddressKind` continue to compile unchanged.
    pub fn address_kind(&self) -> AddressKind {
        match self {
            Address::Http(_) => AddressKind::Http,
            Address::File(_) => AddressKind::File,
            Address::Data(_) => AddressKind::Data,
            Address::Clip(_) => AddressKind::GraphshellClip,
            Address::Directory(_) => AddressKind::Directory,
            Address::Custom(_) => AddressKind::Unknown,
        }
    }
}

/// Construct a typed [`Address`] from a raw URL string.
///
/// Uses the same scheme-detection logic as the legacy `address_kind_from_url`,
/// but embeds the URL (or clip id for clip routes) directly in the variant.
pub fn address_from_url(url: &str) -> Address {
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        Address::Http(url.to_owned())
    } else if lower.starts_with("data:") {
        Address::Data(url.to_owned())
    } else if lower.starts_with("verso://clip/") || lower.starts_with("graphshell://clip/") {
        Address::Clip(url.to_owned())
    } else if lower.starts_with("file://") {
        if file_url_uses_directory_syntax(url) {
            Address::Directory(url.to_owned())
        } else {
            Address::File(url.to_owned())
        }
    } else {
        Address::Custom(url.to_owned())
    }
}

/// Infer `AddressKind` from a URL scheme.
pub fn address_kind_from_url(url: &str) -> AddressKind {
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        AddressKind::Http
    } else if lower.starts_with("data:") {
        AddressKind::Data
    } else if lower.starts_with("verso://clip/") || lower.starts_with("graphshell://clip/") {
        AddressKind::GraphshellClip
    } else if lower.starts_with("file://") {
        if file_url_uses_directory_syntax(url) {
            AddressKind::Directory
        } else {
            AddressKind::File
        }
    } else {
        AddressKind::Unknown
    }
}

fn file_url_uses_directory_syntax(url: &str) -> bool {
    // AddressKind classification must be deterministic from URL semantics alone,
    // independent of local filesystem state.
    url::Url::parse(url)
        .ok()
        .is_some_and(|parsed| parsed.path().ends_with('/'))
}

pub fn cached_host_from_url(url: &str) -> Option<String> {
    url::Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_owned))
}

/// Detect MIME type from URL + optional content bytes.
///
/// Detection order:
/// 1) Extension lookup via `mime_guess` (cheap, synchronous)
/// 2) Content-byte sniffing via `infer` only when extension lookup is
///    missing or ambiguous
///
/// Returns `None` when neither source yields a known MIME type.
pub fn detect_mime(url: &str, content_bytes: Option<&[u8]>) -> Option<String> {
    let no_fragment = url.split('#').next().unwrap_or(url);
    let no_query = no_fragment.split('?').next().unwrap_or(no_fragment);
    // Strip file:// scheme so mime_guess sees a plain path.
    let path = no_query
        .strip_prefix("file://")
        .unwrap_or(no_query)
        .trim_start_matches('/');
    // Reconstruct a rooted path string for mime_guess.
    let guess_path = format!("/{path}");
    let guessed: Vec<String> = mime_guess::from_path(&guess_path)
        .into_iter()
        .map(|m| m.to_string())
        .collect();

    let is_ambiguous = guessed.len() > 1
        || guessed
            .first()
            .map(|m| m == "application/octet-stream")
            .unwrap_or(false);

    if !guessed.is_empty() && !is_ambiguous {
        return guessed.first().cloned();
    }

    if let Some(bytes) = content_bytes {
        if let Some(kind) = infer::get(bytes) {
            return Some(kind.mime_type().to_string());
        }
    }

    guessed.first().cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_mime_returns_pdf_for_pdf_path() {
        assert_eq!(
            detect_mime("file:///home/user/document.pdf", None),
            Some("application/pdf".to_string())
        );
    }

    #[test]
    fn detect_mime_returns_text_plain_for_txt_path() {
        assert_eq!(
            detect_mime("file:///notes/readme.txt", None),
            Some("text/plain".to_string())
        );
    }

    #[test]
    fn detect_mime_returns_none_for_no_extension() {
        assert!(detect_mime("https://example.com/page", None).is_none());
    }

    #[test]
    fn detect_mime_uses_magic_bytes_when_extension_is_missing() {
        let pdf_header = b"%PDF-1.7\n1 0 obj\n";
        assert_eq!(
            detect_mime("https://example.com/no-extension", Some(pdf_header)),
            Some("application/pdf".to_string())
        );
    }

    #[test]
    fn detect_mime_prefers_extension_when_unambiguous() {
        let pdf_header = b"%PDF-1.7\n1 0 obj\n";
        assert_eq!(
            detect_mime("file:///home/user/readme.txt", Some(pdf_header)),
            Some("text/plain".to_string())
        );
    }

    #[test]
    fn detect_mime_falls_back_to_extension_when_magic_unknown() {
        let unknown = b"not a known magic signature";
        assert_eq!(
            detect_mime("file:///home/user/data.json", Some(unknown)),
            Some("application/json".to_string())
        );
    }

    #[test]
    fn address_kind_from_url_http() {
        assert_eq!(
            address_kind_from_url("http://example.com"),
            AddressKind::Http
        );
        assert_eq!(
            address_kind_from_url("https://example.com"),
            AddressKind::Http
        );
    }

    #[test]
    fn address_kind_from_url_file() {
        assert_eq!(
            address_kind_from_url("file:///home/user/file.txt"),
            AddressKind::File
        );
    }

    #[test]
    fn address_kind_from_url_data_clip_directory_and_unknown() {
        assert_eq!(
            address_kind_from_url("data:text/plain,hello"),
            AddressKind::Data
        );
        assert_eq!(
            address_kind_from_url("verso://clip/clip-123"),
            AddressKind::GraphshellClip
        );
        assert_eq!(
            address_kind_from_url("file:///tmp/sample-dir/"),
            AddressKind::Directory
        );
        assert_eq!(
            address_kind_from_url("gemini://gemini.circumlunar.space/"),
            AddressKind::Unknown
        );
        assert_eq!(
            address_kind_from_url("ftp://files.example.com/"),
            AddressKind::Unknown
        );
    }

    #[test]
    fn address_kind_from_url_file_directory_classification_is_syntax_based() {
        assert_eq!(
            address_kind_from_url("file:///tmp/sample-dir/"),
            AddressKind::Directory
        );
        assert_eq!(
            address_kind_from_url("file:///tmp/sample-dir"),
            AddressKind::File
        );
    }

    #[test]
    fn address_from_url_http() {
        assert_eq!(
            address_from_url("https://example.com"),
            Address::Http("https://example.com".to_string())
        );
        assert_eq!(
            address_from_url("http://example.com/path"),
            Address::Http("http://example.com/path".to_string())
        );
    }

    #[test]
    fn address_from_url_file_and_directory() {
        assert_eq!(
            address_from_url("file:///home/user/file.txt"),
            Address::File("file:///home/user/file.txt".to_string())
        );
        assert_eq!(
            address_from_url("file:///tmp/mydir/"),
            Address::Directory("file:///tmp/mydir/".to_string())
        );
    }

    #[test]
    fn address_from_url_data_and_custom() {
        assert_eq!(
            address_from_url("data:text/plain,hello"),
            Address::Data("data:text/plain,hello".to_string())
        );
        assert_eq!(
            address_from_url("gemini://gemini.circumlunar.space/"),
            Address::Custom("gemini://gemini.circumlunar.space/".to_string())
        );
    }

    #[test]
    fn address_from_url_clip_stores_full_url() {
        assert_eq!(
            address_from_url("verso://clip/clip-abc-123"),
            Address::Clip("verso://clip/clip-abc-123".to_string())
        );
        assert_eq!(
            address_from_url("graphshell://clip/clip-xyz"),
            Address::Clip("graphshell://clip/clip-xyz".to_string())
        );
    }

    #[test]
    fn address_as_url_str_roundtrips_for_all_variants() {
        let cases = [
            "https://example.com",
            "file:///home/user/file.txt",
            "file:///tmp/mydir/",
            "data:text/plain,hello",
            "gemini://example.com/",
            "verso://clip/clip-abc",
            "graphshell://clip/clip-xyz",
        ];
        for url in cases {
            let addr = address_from_url(url);
            assert_eq!(
                addr.as_url_str(),
                url,
                "as_url_str should return the original URL for {url}"
            );
        }
    }

    #[test]
    fn address_address_kind_bridge_matches_legacy_inference() {
        let cases = [
            ("https://example.com", AddressKind::Http),
            ("http://example.com", AddressKind::Http),
            ("file:///home/user/file.txt", AddressKind::File),
            ("file:///tmp/mydir/", AddressKind::Directory),
            ("data:text/plain,hello", AddressKind::Data),
            ("verso://clip/clip-123", AddressKind::GraphshellClip),
            ("graphshell://clip/clip-456", AddressKind::GraphshellClip),
            ("gemini://example.com/", AddressKind::Unknown),
        ];
        for (url, expected_kind) in cases {
            assert_eq!(
                address_from_url(url).address_kind(),
                expected_kind,
                "address_kind() mismatch for {url}"
            );
            assert_eq!(
                address_kind_from_url(url),
                expected_kind,
                "address_kind_from_url mismatch for {url}"
            );
        }
    }
}
