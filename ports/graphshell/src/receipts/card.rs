//! Reading receipts back: the card a person actually looks at.
//!
//! R3. A receipt replicates as an ordinary graph node, so without this it
//! renders like any other: a title, an address, some tags. That is precisely
//! the failure the lane exists to prevent — the provenance is present, sitting
//! in facets, and invisible.
//!
//! This builds the card that answers the 2027 question ("what did the frame
//! look like on Mint X11, and what commit was that?") with the pixels and the
//! provenance in one place. It lives here rather than in the sync host because
//! the facet vocabulary is this module's; the host only asks.
//!
//! Rendered where the receipts already are, in the projection the resident
//! host publishes, rather than in a new pane in another app. Any admitted
//! session reads it over the ordinary session protocol, so a turnstone lens
//! needs `graphshell-client`, not a dependency on personal sync.

use graphshell_protocol::{ContentHash, PortableCardV1};
use serde_json::Value;

use super::manifest::{FACET_ARTIFACTS, FACET_RUN};

/// One artifact as the card presents it.
struct Capture {
    name: String,
    blob: Option<ContentHash>,
    bytes: u64,
}

/// Build a receipt's card from its facets, or `None` when the node is not a
/// receipt.
///
/// Takes the two facet values rather than a graph, so this is a pure function
/// a test can drive without a replica.
pub fn receipt_card(
    title: &str,
    address: &str,
    run: Option<&Value>,
    artifacts: Option<&Value>,
) -> Option<PortableCardV1> {
    let run = run?;
    let captures = captures_from(artifacts);

    let exit = run.get("exit_code").and_then(Value::as_i64);
    let passed = exit == Some(0);
    let dirty = run.get("dirty").and_then(Value::as_u64).unwrap_or(0);

    let mut values = vec![
        field("Repo", run.get("repo")),
        field("Scenario", run.get("scenario")),
        field("Machine", run.get("target")),
        field("System", run.get("os")),
        field("Session", run.get("session")),
        // Commit and dirtiness together: a receipt from a dirty checkout is
        // not attributable to a commit alone, and reading the hash without
        // that caveat is worse than reading nothing.
        pair(
            "Commit",
            match dirty {
                0 => text(run.get("commit")),
                n => format!("{} (+{n} uncommitted)", text(run.get("commit"))),
            },
        ),
        field("Ran", run.get("ran_at")),
        pair(
            "Result",
            match exit {
                Some(0) => "passed".to_string(),
                Some(code) => format!("failed (exit {code})"),
                None => "unknown".to_string(),
            },
        ),
    ];

    values.push(pair(
        "Captures",
        if captures.is_empty() {
            "none".to_string()
        } else {
            captures
                .iter()
                .map(|c| format!("{} ({})", c.name, human_bytes(c.bytes)))
                .collect::<Vec<_>>()
                .join(", ")
        },
    ));
    values.push(pair("Address", address.to_string()));

    let mut badges = vec!["Receipt".to_string()];
    badges.push(if passed { "Passed".into() } else { "Failed".into() });
    if dirty > 0 {
        // A badge rather than a footnote: this is the one fact that decides
        // whether the capture can be trusted as evidence about a commit.
        badges.push("Dirty checkout".into());
    }

    Some(PortableCardV1 {
        title: title.to_string(),
        values,
        badges,
        // The blobs themselves. A session resolves these through the ordinary
        // content path, which is what turns "provenance beside the pixels"
        // into something clickable rather than a list of hex.
        media: captures.into_iter().filter_map(|c| c.blob).collect(),
    })
}

/// Whether a node carries the receipt vocabulary at all.
pub fn is_receipt(run: Option<&Value>) -> bool {
    run.is_some()
}

/// The facet ids a caller must fetch to build a card.
pub const CARD_FACETS: [&str; 2] = [FACET_RUN, FACET_ARTIFACTS];

fn captures_from(artifacts: Option<&Value>) -> Vec<Capture> {
    let Some(Value::Array(items)) = artifacts else {
        return Vec::new();
    };
    items
        .iter()
        .map(|item| Capture {
            name: item
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("unnamed")
                .to_string(),
            blob: item
                .get("blake3")
                .and_then(Value::as_str)
                .and_then(parse_hash),
            bytes: item.get("bytes").and_then(Value::as_u64).unwrap_or(0),
        })
        .collect()
}

fn parse_hash(hex: &str) -> Option<ContentHash> {
    if hex.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (index, pair) in hex.as_bytes().chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(pair).ok()?;
        out[index] = u8::from_str_radix(text, 16).ok()?;
    }
    Some(ContentHash(out))
}

fn text(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string()
}

fn field(label: &str, value: Option<&Value>) -> graphshell_protocol::CardValueV1 {
    pair(label, text(value))
}

fn pair(label: &str, value: String) -> graphshell_protocol::CardValueV1 {
    graphshell_protocol::CardValueV1 {
        label: label.to_string(),
        value,
    }
}

fn human_bytes(bytes: u64) -> String {
    match bytes {
        0 => "empty".to_string(),
        n if n < 1024 => format!("{n} B"),
        n if n < 1024 * 1024 => format!("{:.0} KiB", n as f64 / 1024.0),
        n => format!("{:.1} MiB", n as f64 / (1024.0 * 1024.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn run(exit: i64, dirty: u64) -> Value {
        json!({
            "repo": "woodshed",
            "package": "woodshed-genet",
            "scenario": "design_docs/scenarios/frame.scn",
            "target": "mark@thinkpad",
            "platform": "linux",
            "os": "Linux 6.9.4",
            "commit": "abc123def456",
            "dirty": dirty,
            "session": "wayland (wayland-0)",
            "ran_at": "2026-08-10T14:31:05Z",
            "exit_code": exit,
        })
    }

    fn artifacts() -> Value {
        json!([{
            "name": "frame.png",
            "bytes": 4096,
            "blake3": "94061c9510edfeb114dc8a2102339f42c7e6a2aff2a7fdc922bef9903071860e",
            "sha256": "00",
        }])
    }

    fn labelled<'a>(card: &'a PortableCardV1, label: &str) -> &'a str {
        card.values
            .iter()
            .find(|v| v.label == label)
            .map(|v| v.value.as_str())
            .unwrap_or("<missing>")
    }

    /// A node without the run facet is not a receipt and must fall through to
    /// the ordinary node card.
    #[test]
    fn a_plain_node_is_not_a_receipt() {
        assert!(receipt_card("Some page", "https://x", None, None).is_none());
        assert!(!is_receipt(None));
    }

    #[test]
    fn the_card_carries_the_provenance_a_reader_needs() {
        let card = receipt_card(
            "woodshed · frame on thinkpad · ok",
            "receipt:woodshed/thinkpad/x/2026",
            Some(&run(0, 0)),
            Some(&artifacts()),
        )
        .unwrap();

        assert_eq!(labelled(&card, "Repo"), "woodshed");
        assert_eq!(labelled(&card, "Machine"), "mark@thinkpad");
        assert_eq!(labelled(&card, "System"), "Linux 6.9.4");
        assert_eq!(labelled(&card, "Session"), "wayland (wayland-0)");
        assert_eq!(labelled(&card, "Commit"), "abc123def456");
        assert_eq!(labelled(&card, "Result"), "passed");
        assert!(card.badges.contains(&"Passed".to_string()));
    }

    /// The capture rides as a real content hash, which is what makes it
    /// openable rather than a line of hex.
    #[test]
    fn captures_become_media_hashes() {
        let card = receipt_card("t", "receipt:x", Some(&run(0, 0)), Some(&artifacts())).unwrap();
        assert_eq!(card.media.len(), 1);
        assert_eq!(
            card.media[0].to_string(),
            "94061c9510edfeb114dc8a2102339f42c7e6a2aff2a7fdc922bef9903071860e",
        );
        assert_eq!(labelled(&card, "Captures"), "frame.png (4 KiB)");
    }

    /// The fact that decides whether the pixels are evidence about a commit.
    #[test]
    fn a_dirty_checkout_is_said_twice() {
        let card = receipt_card("t", "receipt:x", Some(&run(0, 3)), None).unwrap();
        assert_eq!(labelled(&card, "Commit"), "abc123def456 (+3 uncommitted)");
        assert!(
            card.badges.contains(&"Dirty checkout".to_string()),
            "a reader skimming badges must see it too, got {:?}",
            card.badges,
        );
    }

    #[test]
    fn a_failed_run_says_so_and_names_the_code() {
        let card = receipt_card("t", "receipt:x", Some(&run(101, 0)), None).unwrap();
        assert_eq!(labelled(&card, "Result"), "failed (exit 101)");
        assert!(card.badges.contains(&"Failed".to_string()));
        assert!(!card.badges.contains(&"Passed".to_string()));
    }

    /// A receipt whose captures never arrived still reads as a receipt.
    #[test]
    fn missing_or_malformed_artifacts_degrade_rather_than_vanish() {
        let card = receipt_card("t", "receipt:x", Some(&run(0, 0)), None).unwrap();
        assert_eq!(labelled(&card, "Captures"), "none");
        assert!(card.media.is_empty());

        let bad = json!([{ "name": "x.png", "blake3": "not-a-hash" }]);
        let card = receipt_card("t", "receipt:x", Some(&run(0, 0)), Some(&bad)).unwrap();
        assert!(card.media.is_empty(), "a bad hash is dropped, not guessed");
        assert_eq!(labelled(&card, "Captures"), "x.png (empty)");
    }

    #[test]
    fn a_sparse_run_facet_reads_as_unknown_rather_than_panicking() {
        let card = receipt_card("t", "receipt:x", Some(&json!({})), None).unwrap();
        assert_eq!(labelled(&card, "Repo"), "unknown");
        assert_eq!(labelled(&card, "Result"), "unknown");
    }
}
