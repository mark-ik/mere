// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! **Gazette**, the Mere platform's directory port: who someone is, where
//! they can be reached, and what they publish.
//!
//! Named for the official gazette, and the name carries the whole roadmap in
//! its three senses. A *gazetteer* is an index. To be *gazetted* is to be
//! officially announced, and thereby resolvable. And a *gazette* is the paper
//! you read. The crate began as `gazette`, spent a year as `gazetteer`, and
//! returned to the original when the bare `gazetteer` crates.io name turned
//! out to be taken and `gazette` free (2026-08-10); it was promoted from the
//! dramatis tier to a port on 2026-08-23, because those three senses were
//! never three crates.
//!
//! Like `castellan`, the port splits in two. The **embeddable half** is what
//! any host composes: contact cards, and the one recipient picker Knot, Moot,
//! and Signalman all draw instead of three private lists. The **authority
//! half** lives with the resident, which is the always-on party and therefore
//! the natural poller: resolution, feed fetching, and trust state. Reading a
//! friend's feed reveals your interest to their host, so which persona's
//! network face does the fetching is a first-class setting, not an
//! afterthought.
//!
//! **Built today:** the embeddable contact Ledger projection and WebFinger
//! resolution. `ledger` reads contacts × selected facets, keeps contributor
//! provenance and repeated instance addresses, composes coordinated selection,
//! emits a semantic table, and cites its two authorities independently.
//!
//! WebFinger resolution
//! ([RFC 7033](https://www.rfc-editor.org/rfc/rfc7033)) — an `acct:user@host`
//! handle to its JRD document, with the document's aliases and links
//! classified into typed peer-discovery endpoints (gemini capsules, gopher
//! resources, misfin mailboxes, ActivityPub actors, HTTP profile pages, and a
//! typed catch-all). The other resolvers land beside it behind the same
//! facade: key-rooted NIP-05 (`/.well-known/nostr.json`) and atproto-did
//! lookups, and the moot web-of-trust directory (member lists as vouched
//! handle-to-key bindings).
//!
//! **Unbuilt:** hosting the Ledger and recipient picker over live `gaz`, feed polling
//! (whose engine is `mere-crawl`), and the reading room over fleeced
//! articles. The blocking `reqwest` below needs an async port before a
//! resident polls with it.
//!
//! The boundaries are the point:
//!
//! - **Not `castellan`.** Castellan guards and presents *you*; gazette finds
//!   and keeps *the other players*. Two outward faces of the dramatis tier,
//!   pointing opposite ways.
//! - **Not `gaz`.** Gaz is the contact store — your records about other
//!   people, petnames, per-endpoint trust, kith and kin. This port composes
//!   it; it does not replace it.
//! - **Not a delivery layer.** Private grants, cross-service posting, and
//!   inbox implementations are moot and murm territory. Gazette reads what is
//!   already public.
//! - **Not the highlights.** What you keep from what you read is Knot's, and
//!   what memory makes of it is `alembic`'s.

use std::time::Duration;

pub mod ledger;

use reqwest::header::ACCEPT;
use serde::Deserialize;
#[cfg(any(test, feature = "test-support"))]
use std::sync::{Mutex, OnceLock};

const WEBFINGER_TIMEOUT: Duration = Duration::from_secs(10);
const WEBFINGER_ACCEPT: &str = "application/jrd+json, application/json;q=0.9";

/// Extract a reader article from supplied HTML without fetching or storing it.
///
/// Gazette's future poller owns response acquisition and persistence. This helper
/// only admits declared HTML, parses it through the static DOM, and asks Fleece
/// whether the document has a readable article.
pub fn article_from_html(content_type: Option<&str>, html: &str) -> Option<fleece::Article> {
    let document = match html_syntax(content_type)? {
        HtmlSyntax::Html => genet_static_dom::StaticDocument::parse(html),
        HtmlSyntax::Xhtml => genet_static_dom::StaticDocument::parse_xml(html),
    };
    fleece::extract_article(&document)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HtmlSyntax {
    Html,
    Xhtml,
}

fn html_syntax(content_type: Option<&str>) -> Option<HtmlSyntax> {
    let media_type = content_type?.split(';').next()?.trim();
    if media_type.eq_ignore_ascii_case("text/html") {
        Some(HtmlSyntax::Html)
    } else if media_type.eq_ignore_ascii_case("application/xhtml+xml") {
        Some(HtmlSyntax::Xhtml)
    } else {
        None
    }
}

#[cfg(any(test, feature = "test-support"))]
#[derive(Clone)]
struct TestFetchImportOverride {
    resource: String,
    result: Result<WebFingerImport, String>,
}

#[cfg(any(test, feature = "test-support"))]
fn test_fetch_import_override() -> &'static Mutex<Option<TestFetchImportOverride>> {
    static OVERRIDE: OnceLock<Mutex<Option<TestFetchImportOverride>>> = OnceLock::new();
    OVERRIDE.get_or_init(|| Mutex::new(None))
}

#[cfg(any(test, feature = "test-support"))]
fn test_fetch_import_override_run_lock() -> &'static Mutex<()> {
    static RUN_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    RUN_LOCK.get_or_init(|| Mutex::new(()))
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct WebFingerDocument {
    pub subject: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub links: Vec<WebFingerLink>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct WebFingerLink {
    pub rel: String,
    #[serde(rename = "type")]
    pub media_type: Option<String>,
    pub href: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebFingerEndpoint {
    pub rel: String,
    pub media_type: Option<String>,
    pub href: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WebFingerImport {
    pub subject: String,
    pub aliases: Vec<String>,
    pub profile_pages: Vec<String>,
    pub gemini_capsules: Vec<String>,
    pub gopher_resources: Vec<String>,
    pub misfin_mailboxes: Vec<String>,
    pub activitypub_actors: Vec<String>,
    pub other_endpoints: Vec<WebFingerEndpoint>,
}

impl WebFingerImport {
    pub fn from_document(document: &WebFingerDocument) -> Self {
        let mut import = Self {
            subject: document.subject.clone(),
            aliases: document.aliases.clone(),
            ..Self::default()
        };

        for alias in &document.aliases {
            classify_untyped_target(alias, &mut import);
        }

        for link in &document.links {
            let Some(href) = link
                .href
                .as_deref()
                .map(str::trim)
                .filter(|href| !href.is_empty())
            else {
                continue;
            };

            let media_type = link
                .media_type
                .as_deref()
                .map(|value| value.trim().to_ascii_lowercase())
                .filter(|value| !value.is_empty());

            if href.starts_with("gemini://") {
                push_unique(&mut import.gemini_capsules, href.to_string());
            } else if href.starts_with("gopher://") {
                push_unique(&mut import.gopher_resources, href.to_string());
            } else if href.starts_with("misfin://") {
                push_unique(&mut import.misfin_mailboxes, href.to_string());
            } else if is_activitypub_media_type(media_type.as_deref()) {
                push_unique(&mut import.activitypub_actors, href.to_string());
            } else if href.starts_with("https://") || href.starts_with("http://") {
                push_unique(&mut import.profile_pages, href.to_string());
            } else {
                import.other_endpoints.push(WebFingerEndpoint {
                    rel: link.rel.clone(),
                    media_type: media_type.clone(),
                    href: href.to_string(),
                });
            }
        }

        import
    }
}

pub fn normalize_resource(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("WebFinger resource cannot be empty.".to_string());
    }

    if trimmed.starts_with("acct:") {
        validate_acct_resource(trimmed)?;
        return Ok(trimmed.to_string());
    }

    if trimmed.contains("://") {
        let parsed = url::Url::parse(trimmed)
            .map_err(|error| format!("Invalid WebFinger URL resource '{trimmed}': {error}"))?;
        if parsed.host_str().is_none() {
            return Err(format!(
                "WebFinger URL resource '{trimmed}' is missing a host."
            ));
        }
        return Ok(parsed.to_string());
    }

    if trimmed.contains('@') {
        let normalized = format!("acct:{trimmed}");
        validate_acct_resource(&normalized)?;
        return Ok(normalized);
    }

    Err(format!(
        "WebFinger resource '{trimmed}' must be an acct: identifier, a bare user@host handle, or a URL."
    ))
}

pub fn endpoint_url(resource: &str) -> Result<url::Url, String> {
    let normalized = normalize_resource(resource)?;
    let origin = origin_for_resource(&normalized)?;
    endpoint_url_with_origin(&origin, &normalized)
}

pub fn parse_document(body: &str) -> Result<WebFingerDocument, String> {
    let document: WebFingerDocument = serde_json::from_str(body)
        .map_err(|error| format!("WebFinger JRD parse failed: {error}"))?;
    if document.subject.trim().is_empty() {
        return Err("WebFinger JRD is missing a subject.".to_string());
    }
    Ok(document)
}

pub fn fetch_document(resource: &str) -> Result<WebFingerDocument, String> {
    let endpoint = endpoint_url(resource)?;
    fetch_document_from_endpoint(&endpoint)
}

pub fn fetch_import(resource: &str) -> Result<WebFingerImport, String> {
    #[cfg(any(test, feature = "test-support"))]
    {
        if let Some(override_state) = test_fetch_import_override()
            .lock()
            .expect("webfinger test fetch override lock poisoned")
            .as_ref()
            .filter(|override_state| override_state.resource == resource)
            .cloned()
        {
            return override_state.result;
        }
    }

    let document = fetch_document(resource)?;
    Ok(WebFingerImport::from_document(&document))
}

#[cfg(any(test, feature = "test-support"))]
pub fn with_test_fetch_import_override<T>(
    resource: &str,
    result: Result<WebFingerImport, String>,
    run: impl FnOnce() -> T,
) -> T {
    let _run_lock = test_fetch_import_override_run_lock()
        .lock()
        .expect("webfinger test fetch override lock poisoned");
    let previous = {
        let mut override_slot = test_fetch_import_override()
            .lock()
            .expect("webfinger test fetch override lock poisoned");
        override_slot.replace(TestFetchImportOverride {
            resource: resource.to_string(),
            result,
        })
    };
    let outcome = run();
    *test_fetch_import_override()
        .lock()
        .expect("webfinger test fetch override lock poisoned") = previous;
    outcome
}

fn fetch_document_from_endpoint(endpoint: &url::Url) -> Result<WebFingerDocument, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(WEBFINGER_TIMEOUT)
        .build()
        .map_err(|error| format!("Failed to build WebFinger HTTP client: {error}"))?;
    let body = client
        .get(endpoint.as_str())
        .header(ACCEPT, WEBFINGER_ACCEPT)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| format!("WebFinger request failed for '{}': {error}", endpoint))?
        .text()
        .map_err(|error| {
            format!(
                "WebFinger response decode failed for '{}': {error}",
                endpoint
            )
        })?;
    parse_document(&body)
}

fn validate_acct_resource(resource: &str) -> Result<(), String> {
    let account = resource.trim_start_matches("acct:");
    let Some((local_part, host_part)) = account.rsplit_once('@') else {
        return Err(format!(
            "WebFinger acct resource '{resource}' must contain a local part and host."
        ));
    };

    if local_part.trim().is_empty() || host_part.trim().is_empty() {
        return Err(format!(
            "WebFinger acct resource '{resource}' must contain a local part and host."
        ));
    }

    Ok(())
}

fn origin_for_resource(resource: &str) -> Result<url::Url, String> {
    if let Some(account) = resource.strip_prefix("acct:") {
        let (_, host_part) = account
            .rsplit_once('@')
            .ok_or_else(|| format!("WebFinger acct resource '{resource}' must contain a host."))?;
        let mut origin = url::Url::parse("https://example.invalid/")
            .expect("static WebFinger origin should parse");
        origin
            .set_host(Some(host_part.trim()))
            .map_err(|_| format!("Invalid WebFinger host '{host_part}'."))?;
        return Ok(origin);
    }

    let parsed = url::Url::parse(resource)
        .map_err(|error| format!("Invalid WebFinger URL resource '{resource}': {error}"))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| format!("WebFinger URL resource '{resource}' is missing a host."))?;

    let mut origin =
        url::Url::parse("https://example.invalid/").expect("static WebFinger origin should parse");
    origin
        .set_host(Some(host))
        .map_err(|_| format!("Invalid WebFinger host '{host}'."))?;
    origin
        .set_port(parsed.port())
        .map_err(|_| format!("Invalid WebFinger port for '{resource}'."))?;
    Ok(origin)
}

fn endpoint_url_with_origin(
    origin: &url::Url,
    normalized_resource: &str,
) -> Result<url::Url, String> {
    let mut endpoint = origin
        .join("/.well-known/webfinger")
        .map_err(|error| format!("Failed to build WebFinger endpoint URL: {error}"))?;
    endpoint.set_query(None);
    endpoint
        .query_pairs_mut()
        .append_pair("resource", normalized_resource);
    Ok(endpoint)
}

fn classify_untyped_target(target: &str, import: &mut WebFingerImport) {
    if target.starts_with("gemini://") {
        push_unique(&mut import.gemini_capsules, target.to_string());
    } else if target.starts_with("gopher://") {
        push_unique(&mut import.gopher_resources, target.to_string());
    } else if target.starts_with("misfin://") {
        push_unique(&mut import.misfin_mailboxes, target.to_string());
    } else if target.starts_with("https://") || target.starts_with("http://") {
        push_unique(&mut import.profile_pages, target.to_string());
    }
}

fn is_activitypub_media_type(media_type: Option<&str>) -> bool {
    media_type.is_some_and(|value| {
        value == "application/activity+json"
            || value.contains("activitystreams")
            || value == "application/ld+json"
    })
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn normalize_resource_accepts_bare_acct_handle() {
        assert_eq!(
            normalize_resource("mark@example.net").expect("acct handle should normalize"),
            "acct:mark@example.net"
        );
    }

    #[test]
    fn endpoint_url_builds_https_well_known_query_for_acct_resource() {
        let endpoint = endpoint_url("acct:mark@example.net").expect("endpoint should build");

        assert_eq!(
            endpoint.as_str(),
            "https://example.net/.well-known/webfinger?resource=acct%3Amark%40example.net"
        );
    }

    #[test]
    fn import_categorizes_aliases_and_links() {
        let document = parse_document(
            r#"{
                "subject": "acct:mark@example.net",
                "aliases": [
                    "https://example.net/profile"
                ],
                "links": [
                    { "rel": "self", "href": "https://example.net/profile" },
                    { "rel": "alternate", "type": "text/gemini", "href": "gemini://example.net/profile" },
                    { "rel": "alternate", "href": "misfin://mark@example.net" },
                    { "rel": "self", "type": "application/activity+json", "href": "https://example.net/users/mark" }
                ]
            }"#,
        )
        .expect("jrd should parse");

        let import = WebFingerImport::from_document(&document);

        assert_eq!(import.subject, "acct:mark@example.net");
        assert!(
            import
                .profile_pages
                .iter()
                .any(|value| value == "https://example.net/profile")
        );
        assert!(
            import
                .gemini_capsules
                .iter()
                .any(|value| value == "gemini://example.net/profile")
        );
        assert!(
            import
                .misfin_mailboxes
                .iter()
                .any(|value| value == "misfin://mark@example.net")
        );
        assert!(
            import
                .activitypub_actors
                .iter()
                .any(|value| value == "https://example.net/users/mark")
        );
    }

    #[test]
    fn supplied_html_article_extraction_is_readable_only() {
        let article = article_from_html(
            Some("text/html; charset=utf-8"),
            r#"<!doctype html><title>Field notes</title><main><article>
                <h1>Field notes</h1>
                <p>The first paragraph carries enough prose to be a reader article.</p>
                <p>The second paragraph confirms this is content, not site chrome.</p>
            </article></main>"#,
        )
        .expect("readable supplied HTML should produce an article");
        assert_eq!(article.title.as_deref(), Some("Field notes"));
        assert!(!article.blocks.is_empty());

        assert!(article_from_html(Some("text/html"), "<nav><a href='/'>Home</a></nav>").is_none());
    }

    #[test]
    fn supplied_non_html_never_enters_the_reader_extractor() {
        assert!(
            article_from_html(
                Some("application/json"),
                r#"{"title":"not HTML","body":"<main>not parsed</main>"}"#,
            )
            .is_none()
        );
    }

    #[test]
    fn supplied_xhtml_uses_the_declared_xml_syntax() {
        assert_eq!(
            html_syntax(Some("application/xhtml+xml; charset=utf-8")),
            Some(HtmlSyntax::Xhtml),
        );
        let article = article_from_html(
            Some("application/xhtml+xml"),
            r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>XML field notes</title></head><body><main><article><h1>XML field notes</h1><p>The first paragraph carries enough prose to be a reader article.</p><p>The second paragraph confirms this is structured XHTML content.</p></article></main></body></html>"#,
        )
        .expect("declared XHTML should produce an article without a sniff marker");
        assert_eq!(article.title.as_deref(), Some("XML field notes"));
    }

    #[test]
    fn fetch_document_from_endpoint_reads_jrd_response() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
        let port = listener.local_addr().expect("address").port();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut request_line = String::new();
            reader.read_line(&mut request_line).expect("request line");
            assert_eq!(
                request_line,
                "GET /.well-known/webfinger?resource=acct%3Amark%40example.net HTTP/1.1\r\n"
            );

            let mut saw_accept = false;
            loop {
                let mut header = String::new();
                reader.read_line(&mut header).expect("header line");
                if header == "\r\n" {
                    break;
                }
                if header.to_ascii_lowercase().starts_with("accept:")
                    && header.contains("application/jrd+json")
                {
                    saw_accept = true;
                }
            }
            assert!(saw_accept);

            let body = r#"{
                "subject": "acct:mark@example.net",
                "aliases": ["https://example.net/profile"],
                "links": [
                    { "rel": "alternate", "type": "text/gemini", "href": "gemini://example.net/profile" }
                ]
            }"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/jrd+json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );

            let mut writer = stream;
            writer
                .write_all(response.as_bytes())
                .expect("response write");
            writer.flush().expect("response flush");
        });

        let origin =
            url::Url::parse(&format!("http://127.0.0.1:{port}/")).expect("origin should parse");
        let endpoint = endpoint_url_with_origin(&origin, "acct:mark@example.net")
            .expect("endpoint should build");
        let document =
            fetch_document_from_endpoint(&endpoint).expect("webfinger fetch should succeed");

        assert_eq!(document.subject, "acct:mark@example.net");
        assert!(
            document
                .links
                .iter()
                .any(|link| link.href.as_deref() == Some("gemini://example.net/profile"))
        );
        server.join().expect("server joins cleanly");
    }
}
