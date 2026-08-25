//! W3C Web Annotation target serialization for Fleece text evidence.
//!
//! Knot owns the source identity. Fleece supplies only selectors over its
//! documented text stream, so this module deliberately accepts the source URI
//! from the caller and emits its quote and position descriptions as siblings.

use fleece::TextAnchor;
use serde::Serialize;

/// A W3C Web Annotation `SpecificResource` target.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SpecificResource {
    #[serde(rename = "type")]
    resource_type: &'static str,
    pub source: String,
    pub selector: Vec<SpecificResourceSelector>,
}

/// The two alternative selector descriptions carried by a Fleece anchor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "type")]
pub enum SpecificResourceSelector {
    TextQuoteSelector {
        exact: String,
        prefix: String,
        suffix: String,
    },
    TextPositionSelector {
        start: u64,
        end: u64,
    },
}

impl SpecificResource {
    /// Serialize one Fleece anchor as sibling Web Annotation selectors.
    ///
    /// This is intentionally not an Annotation: Knot's caller still owns a
    /// body, motivation, persistence, and the source resource identity.
    pub fn from_fleece_anchor(source: impl Into<String>, anchor: &TextAnchor) -> Self {
        Self {
            resource_type: "SpecificResource",
            source: source.into(),
            selector: vec![
                SpecificResourceSelector::TextQuoteSelector {
                    exact: anchor.quote.exact.clone(),
                    prefix: anchor.quote.prefix.clone(),
                    suffix: anchor.quote.suffix.clone(),
                },
                SpecificResourceSelector::TextPositionSelector {
                    start: anchor.position.start,
                    end: anchor.position.end,
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use fleece::{TextAnchor, TextPositionSelector, TextQuoteSelector};

    use super::SpecificResource;

    const DOCUMENT: &str = include_str!("../tests/fixtures/fleece_specific_resource.txt");

    fn code_point_offset(document: &str, byte_offset: usize) -> u64 {
        document[..byte_offset].chars().count() as u64
    }

    fn resolve_position(document: &str, start: u64, end: u64) -> String {
        document
            .chars()
            .skip(start as usize)
            .take((end - start) as usize)
            .collect()
    }

    fn resolve_quote(document: &str, quote: &TextQuoteSelector) -> Vec<(u64, u64)> {
        document
            .match_indices(&quote.exact)
            .filter(|(byte_offset, _)| {
                document[..*byte_offset].ends_with(&quote.prefix)
                    && document[*byte_offset + quote.exact.len()..].starts_with(&quote.suffix)
            })
            .map(|(byte_offset, _)| {
                let start = code_point_offset(document, byte_offset);
                (start, start + quote.exact.chars().count() as u64)
            })
            .collect()
    }

    #[test]
    fn serializes_and_independently_resolves_sibling_fleece_selectors() {
        let document = DOCUMENT.trim_end();
        let exact = "Repeat this sentence.";
        let byte_start = document.match_indices(exact).nth(1).unwrap().0;
        let start = code_point_offset(document, byte_start);
        let anchor = TextAnchor {
            position: TextPositionSelector {
                start,
                end: start + exact.chars().count() as u64,
            },
            quote: TextQuoteSelector {
                exact: exact.to_string(),
                prefix: "Middle. ".to_string(),
                suffix: " End.".to_string(),
            },
        };

        let target = SpecificResource::from_fleece_anchor("https://example.test/article", &anchor);
        let serialized = serde_json::to_value(&target).unwrap();
        assert_eq!(serialized["type"], "SpecificResource");
        assert_eq!(serialized["source"], "https://example.test/article");
        assert_eq!(serialized["selector"].as_array().unwrap().len(), 2);
        assert!(serialized.get("refinedBy").is_none());

        assert_eq!(
            resolve_position(document, anchor.position.start, anchor.position.end),
            anchor.quote.exact
        );
        assert_eq!(
            resolve_quote(document, &anchor.quote),
            vec![(anchor.position.start, anchor.position.end)]
        );
    }
}
