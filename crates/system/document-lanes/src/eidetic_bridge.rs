/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Opt-in bridge from Fleece's caller-owned extraction output to Eidetic.
//!
//! Fleece deliberately knows neither a page's resolved URL nor its acquired
//! bytes. A host supplies both as [`CaptureIdentity`]; this module preserves
//! that evidence beside Fleece's canonical DOM text and a W3C Web Annotation
//! target. It never fetches, resolves URLs, or decides that a capture is
//! shareable. The typed save helper writes `LocalOnly`; promotion is a later
//! product action with its own authority and privacy decision.

use eidetic::{
    BlobManifest, BlobSource, Hash, ManifestId, ModerationState, PrivacyClass, ProvenanceOrigin,
    ProvenanceRecord, Result, SchemaRef, Timestamp, TrustEnvelope, TrustLevel, TypedPayload,
};
use fleece::{
    CanonicalTextRecordV1, CanonicalTextSelectorProjection, ExtractedDocument, TextAnchor,
};
use serde::{Deserialize, Serialize};

const RFC_5147: &str = fleece::RFC5147_CONFORMS_TO;

/// Canonical bytes of the schema codicil describing this bridge's typed payload.
const FLEECE_ANNOTATION_SCHEMA_PAYLOAD: &[u8] = br#"{"format":"mere-native","schema_id":"mere.document-lanes.FleeceAnnotation/v1","body":{"version":1,"description":"A caller-identified Fleece extraction with a W3C Web Annotation target over its canonical DOM text.","required":["extraction","target","annotation"],"fields":{"extraction":{"type":"object"},"target":{"type":"object"},"annotation":{"type":"object"}}}}"#;

/// The well-known Eidetic schema reference for [`FleeceAnnotationRecord`].
pub static FLEECE_ANNOTATION_SCHEMA_REF: std::sync::LazyLock<SchemaRef> =
    std::sync::LazyLock::new(|| {
        SchemaRef::from_id(ManifestId::from_hash(Hash::of(
            FLEECE_ANNOTATION_SCHEMA_PAYLOAD,
        )))
    });

/// Capture facts that Fleece cannot infer or verify.
///
/// `capture_hash` is typed caller-supplied evidence for the immutable acquired
/// representation. It is intentionally not inferred from Fleece text: the
/// text is a normalized projection, while the capture may be HTML, WACZ, or a
/// host-specific immutable resource.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureIdentity {
    /// The host-resolved canonical source URI, never an unresolved DOM href.
    pub canonical_source: String,
    /// Caller-supplied identity of the acquired immutable capture.
    pub capture_hash: Hash,
}

impl CaptureIdentity {
    pub fn new(canonical_source: impl Into<String>, capture_hash: Hash) -> Result<Self> {
        let canonical_source = canonical_source.into();
        validate_canonical_source(&canonical_source)?;
        Ok(Self {
            canonical_source,
            capture_hash,
        })
    }
}

/// The durable subset of one Fleece extraction needed to reopen an annotation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleeceExtractionRecord {
    /// Caller-owned resolved source and capture evidence.
    pub capture: CaptureIdentity,
    /// Eidetic's BLAKE3 hash of Fleece's preserved canonical text, checked
    /// before an annotation is accepted.
    ///
    /// This is Eidetic's BLAKE3 content hash. It is deliberately distinct from
    /// Fleece's SHA-256 `canonical_text_iri` in [`Self::canonical_text_record`].
    pub canonical_text_hash: Hash,
    /// Fleece's stable preservation contract, including the canonical-text
    /// resource identity, profile, normalization, language/direction evidence,
    /// and reader anchors.
    pub canonical_text_record: CanonicalTextRecordV1,
}

impl FleeceExtractionRecord {
    /// Preserve the Fleece result together with facts Fleece intentionally does
    /// not own. The Fleece preservation record supplies the extraction profile,
    /// normalization, and canonical-text resource identity.
    pub fn from_fleece(capture: CaptureIdentity, document: &ExtractedDocument) -> Result<Self> {
        let canonical_text_record = CanonicalTextRecordV1::from_document(document);
        canonical_text_record.validate().map_err(|error| {
            eidetic::Error::new(format!("invalid Fleece extraction record: {error}"))
        })?;
        let record = Self {
            capture,
            canonical_text_hash: Hash::of(canonical_text_record.canonical_text.as_bytes()),
            canonical_text_record,
        };
        record.validate_integrity()?;
        Ok(record)
    }

    pub fn validate_integrity(&self) -> Result<()> {
        validate_canonical_source(&self.capture.canonical_source)?;
        if self.canonical_text_record.canonical_text.is_empty() {
            return Err(eidetic::Error::new(
                "canonical Fleece text must not be empty",
            ));
        }
        self.canonical_text_record.validate().map_err(|error| {
            eidetic::Error::new(format!(
                "invalid preserved Fleece extraction record: {error}"
            ))
        })?;
        let actual = Hash::of(self.canonical_text_record.canonical_text.as_bytes());
        if actual != self.canonical_text_hash {
            return Err(eidetic::Error::new(format!(
                "canonical Fleece text hash mismatch: expected {}, got {}",
                self.canonical_text_hash, actual
            )));
        }
        Ok(())
    }
}

fn validate_canonical_source(source: &str) -> Result<()> {
    url::Url::parse(source).map(|_| ()).map_err(|error| {
        eidetic::Error::new(format!("canonical source must be an absolute IRI: {error}"))
    })
}

/// W3C Web Annotation's `TextPositionSelector` projection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextPositionSelector {
    #[serde(rename = "type")]
    pub selector_type: String,
    pub start: u64,
    pub end: u64,
}

/// W3C Web Annotation's RFC 5147 character-range selector.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FragmentSelector {
    #[serde(rename = "type")]
    pub selector_type: String,
    pub value: String,
    #[serde(rename = "conformsTo")]
    pub conforms_to: String,
}

/// W3C Web Annotation's `TextQuoteSelector` projection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextQuoteSelector {
    #[serde(rename = "type")]
    pub selector_type: String,
    pub exact: String,
    pub prefix: String,
    pub suffix: String,
}

/// A W3C `SpecificResource` target with Fleece's sibling selectors.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebAnnotationTarget {
    #[serde(rename = "type")]
    pub target_type: String,
    /// The immutable canonical text resource, so Fleece's code-point offsets
    /// have one unambiguous source stream.
    pub source: ExternalWebResource,
    /// The acquired page that supplied the canonical text resource.
    pub scope: String,
    pub selector: Vec<serde_json::Value>,
    /// Eidetic extension: capture identity is distinct from the source URI.
    #[serde(rename = "captureHash")]
    pub capture_hash: Hash,
    /// Eidetic extension: declares the exact text the selectors are measured on.
    #[serde(rename = "canonicalTextHash")]
    pub canonical_text_hash: Hash,
}

/// A W3C External Web Resource describing the selector source stream.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalWebResource {
    #[serde(rename = "id")]
    pub id: String,
    pub format: String,
}

impl WebAnnotationTarget {
    fn from_projection(
        extraction: &FleeceExtractionRecord,
        projection: CanonicalTextSelectorProjection,
    ) -> Result<Self> {
        if !projection.resolves_against(&extraction.canonical_text_record.canonical_text) {
            return Err(eidetic::Error::new(
                "Fleece selector projection does not resolve against canonical text",
            ));
        }
        if projection.resource_iri != extraction.canonical_text_record.canonical_text_iri {
            return Err(eidetic::Error::new(
                "Fleece selector projection names a different canonical-text resource",
            ));
        }
        let fragment = FragmentSelector {
            selector_type: "FragmentSelector".to_owned(),
            value: projection.fragment.value(),
            conforms_to: RFC_5147.to_owned(),
        };
        let position = TextPositionSelector {
            selector_type: "TextPositionSelector".to_owned(),
            start: projection.position.start,
            end: projection.position.end,
        };
        let quote = TextQuoteSelector {
            selector_type: "TextQuoteSelector".to_owned(),
            exact: projection.quote.exact,
            prefix: projection.quote.prefix,
            suffix: projection.quote.suffix,
        };
        Ok(Self {
            target_type: "SpecificResource".to_owned(),
            source: ExternalWebResource {
                id: projection.resource_iri,
                format: extraction.canonical_text_record.media_type.clone(),
            },
            scope: extraction.capture.canonical_source.clone(),
            selector: vec![
                serde_json::to_value(position).map_err(|error| {
                    eidetic::Error::new(format!("position selector JSON: {error}"))
                })?,
                serde_json::to_value(quote).map_err(|error| {
                    eidetic::Error::new(format!("quote selector JSON: {error}"))
                })?,
                serde_json::to_value(fragment).map_err(|error| {
                    eidetic::Error::new(format!("fragment selector JSON: {error}"))
                })?,
            ],
            capture_hash: extraction.capture.capture_hash,
            canonical_text_hash: extraction.canonical_text_hash,
        })
    }
}

/// A complete JSON-LD Web Annotation envelope, including the capture bindings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WebAnnotationEnvelope {
    #[serde(rename = "@context")]
    pub context: Vec<serde_json::Value>,
    pub id: String,
    #[serde(rename = "type")]
    pub annotation_type: String,
    pub target: WebAnnotationTarget,
}

impl WebAnnotationEnvelope {
    fn from_target(target: WebAnnotationTarget) -> Self {
        let id = format!(
            "urn:eidetic:annotation:{}:{}-{}",
            target.capture_hash.to_hex(),
            target_position(&target).0,
            target_position(&target).1,
        );
        Self {
            context: vec![
                serde_json::Value::String("http://www.w3.org/ns/anno.jsonld".to_owned()),
                serde_json::json!({
                    "eidetic": "https://merely-made.org/ns/eidetic#",
                    "captureHash": "eidetic:captureHash",
                    "canonicalTextHash": "eidetic:canonicalTextHash"
                }),
            ],
            id,
            annotation_type: "Annotation".to_owned(),
            target,
        }
    }
}

/// The typed Eidetic payload: extraction evidence, W3C target, and its exact
/// JSON-LD envelope travel together so reopening can validate all three.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FleeceAnnotationRecord {
    pub extraction: FleeceExtractionRecord,
    pub target: WebAnnotationTarget,
    pub annotation: WebAnnotationEnvelope,
}

impl FleeceAnnotationRecord {
    pub fn from_fleece(
        capture: CaptureIdentity,
        document: &ExtractedDocument,
        anchor: &TextAnchor,
    ) -> Result<Self> {
        let extraction = FleeceExtractionRecord::from_fleece(capture, document)?;
        let target = WebAnnotationTarget::from_projection(
            &extraction,
            document.selector_projection(anchor),
        )?;
        let annotation = WebAnnotationEnvelope::from_target(target.clone());
        let record = Self {
            extraction,
            target,
            annotation,
        };
        record.validate_integrity()?;
        Ok(record)
    }

    /// Validate the retained canonical text, both selector forms, capture
    /// bindings, and the serialized interchange envelope before use.
    pub fn validate_integrity(&self) -> Result<()> {
        self.extraction.validate_integrity()?;
        if self.target.target_type != "SpecificResource"
            || self.target.source.id != self.extraction.canonical_text_record.canonical_text_iri
            || self.target.source.format != self.extraction.canonical_text_record.media_type
            || self.target.scope != self.extraction.capture.canonical_source
            || self.target.capture_hash != self.extraction.capture.capture_hash
            || self.target.canonical_text_hash != self.extraction.canonical_text_hash
        {
            return Err(eidetic::Error::new(
                "annotation target does not bind the stored extraction",
            ));
        }
        let anchor = anchor_from_target(&self.target)?;
        let projection =
            fleece::CanonicalTextSelectorProjection::from_anchor(&self.target.source.id, &anchor);
        if !projection.resolves_against(&self.extraction.canonical_text_record.canonical_text) {
            return Err(eidetic::Error::new(
                "annotation selectors do not resolve against stored canonical text",
            ));
        }
        let expected = WebAnnotationEnvelope::from_target(self.target.clone());
        if self.annotation != expected {
            return Err(eidetic::Error::new(
                "annotation JSON-LD envelope does not match the stored target",
            ));
        }
        Ok(())
    }

    /// Serialize the complete W3C Web Annotation JSON-LD envelope.
    pub fn annotation_json_ld(&self) -> Result<Vec<u8>> {
        self.validate_integrity()?;
        serde_json::to_vec(&self.annotation)
            .map_err(|error| eidetic::Error::new(format!("annotation JSON-LD serialize: {error}")))
    }
}

impl TypedPayload for FleeceAnnotationRecord {
    fn schema_ref() -> SchemaRef {
        *FLEECE_ANNOTATION_SCHEMA_REF
    }
}

/// Seed the bridge schema codicil. Idempotent, like Eidetic's browsing schema.
pub async fn bootstrap_fleece_annotation_schema(store: &mut dyn eidetic::Store) -> Result<()> {
    let id = FLEECE_ANNOTATION_SCHEMA_REF.0;
    if eidetic::manifest::load_manifest(store, id).await?.is_some() {
        return Ok(());
    }
    let content_hash = Hash::of(FLEECE_ANNOTATION_SCHEMA_PAYLOAD);
    let local_key = format!("blob:{}", content_hash.to_hex());
    store
        .put(&local_key, FLEECE_ANNOTATION_SCHEMA_PAYLOAD)
        .await?;
    eidetic::manifest::save_manifest(
        store,
        &BlobManifest {
            id,
            schema: *eidetic::META_SCHEMA_REF,
            content_hash,
            byte_size: FLEECE_ANNOTATION_SCHEMA_PAYLOAD.len() as u64,
            created_at: Timestamp::ZERO,
            last_accessed: None,
            sources: vec![BlobSource::Local { key: local_key }],
            privacy: PrivacyClass::PublicPortable,
            provenance: ProvenanceRecord {
                origin: ProvenanceOrigin::Generated,
                upstream: Vec::new(),
                tooling: Some(format!("mere-document-lanes/{}", env!("CARGO_PKG_VERSION"))),
                generated_at: Timestamp::ZERO,
            },
            trust: TrustEnvelope {
                level: TrustLevel::CheckpointAccepted,
                signatures: Vec::new(),
                moderation_state: ModerationState::Accepted,
            },
            schema_metadata: serde_json::Value::Null,
            manifest_version: BlobManifest::CURRENT_VERSION,
        },
    )
    .await
}

/// Save an internally validated annotation record as an Eidetic `LocalOnly`
/// typed payload. Broader publication must be explicit at a higher layer.
pub async fn save_fleece_annotation(
    store: &mut dyn eidetic::Store,
    record: &FleeceAnnotationRecord,
    now_ms: u64,
) -> Result<ManifestId> {
    record.validate_integrity()?;
    eidetic::save_typed(
        store,
        record,
        Vec::new(),
        PrivacyClass::LocalOnly,
        ProvenanceRecord {
            origin: ProvenanceOrigin::Generated,
            upstream: Vec::new(),
            tooling: Some(format!(
                "mere-document-lanes-fleece/{}",
                env!("CARGO_PKG_VERSION")
            )),
            generated_at: Timestamp(now_ms),
        },
        TrustEnvelope {
            level: TrustLevel::SelfAsserted,
            signatures: Vec::new(),
            moderation_state: ModerationState::Unreviewed,
        },
        Timestamp(now_ms),
    )
    .await
}

/// Load a typed annotation record and reject tampered text, selector, target,
/// or JSON-LD envelope facts before returning it to the caller.
pub async fn load_fleece_annotation(
    store: &mut dyn eidetic::Store,
    id: ManifestId,
) -> Result<Option<FleeceAnnotationRecord>> {
    let mut fetcher = eidetic::NoFetcher;
    let Some(record) =
        eidetic::load_typed::<FleeceAnnotationRecord>(store, &mut fetcher, id).await?
    else {
        return Ok(None);
    };
    record.validate_integrity()?;
    Ok(Some(record))
}

fn anchor_from_target(target: &WebAnnotationTarget) -> Result<TextAnchor> {
    if target.selector.len() != 3 {
        return Err(eidetic::Error::new(
            "annotation target must retain fragment, position, and quote selectors",
        ));
    }
    let position: TextPositionSelector = serde_json::from_value(target.selector[0].clone())
        .map_err(|error| eidetic::Error::new(format!("position selector decode: {error}")))?;
    let quote: TextQuoteSelector = serde_json::from_value(target.selector[1].clone())
        .map_err(|error| eidetic::Error::new(format!("quote selector decode: {error}")))?;
    let fragment: FragmentSelector = serde_json::from_value(target.selector[2].clone())
        .map_err(|error| eidetic::Error::new(format!("fragment selector decode: {error}")))?;
    if fragment.selector_type != "FragmentSelector"
        || fragment.conforms_to != RFC_5147
        || position.selector_type != "TextPositionSelector"
        || quote.selector_type != "TextQuoteSelector"
    {
        return Err(eidetic::Error::new(
            "annotation target has unsupported selector types",
        ));
    }
    let fragment = fleece::FragmentSelector::parse(&fragment.value)
        .ok_or_else(|| eidetic::Error::new("fragment selector must use an RFC 5147 char range"))?;
    if fragment.start != position.start || fragment.end != position.end {
        return Err(eidetic::Error::new(
            "fragment and text-position selectors disagree",
        ));
    }
    Ok(TextAnchor {
        position: fleece::TextPositionSelector {
            start: position.start,
            end: position.end,
        },
        quote: fleece::TextQuoteSelector {
            exact: quote.exact,
            prefix: quote.prefix,
            suffix: quote.suffix,
        },
    })
}

fn target_position(target: &WebAnnotationTarget) -> (u64, u64) {
    target
        .selector
        .first()
        .and_then(|value| serde_json::from_value::<TextPositionSelector>(value.clone()).ok())
        .map(|position| (position.start, position.end))
        .unwrap_or((0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use eidetic_fjall::FjallStore;
    use genet_static_dom::StaticDocument;
    use oxjsonld::{JsonLdParser, JsonLdRemoteDocument};
    use oxrdf::{BlankNode, Dataset, GraphName, Literal, NamedNode, NamedOrBlankNode, Quad, Term};
    use std::collections::{HashMap, HashSet};

    // An offline copy of <http://www.w3.org/ns/anno.jsonld>, used under the W3C
    // Software and Document License: <https://www.w3.org/copyright/software-license/>.
    // The loader below accepts only this URI, so the conformance fixture cannot
    // make a network request or silently substitute a host cache.
    const W3C_ANNOTATION_CONTEXT: &str = r#"{
 "@context": {
    "oa":      "http://www.w3.org/ns/oa#",
    "dc":      "http://purl.org/dc/elements/1.1/",
    "dcterms": "http://purl.org/dc/terms/",
    "dctypes": "http://purl.org/dc/dcmitype/",
    "foaf":    "http://xmlns.com/foaf/0.1/",
    "rdf":     "http://www.w3.org/1999/02/22-rdf-syntax-ns#",
    "rdfs":    "http://www.w3.org/2000/01/rdf-schema#",
    "skos":    "http://www.w3.org/2004/02/skos/core#",
    "xsd":     "http://www.w3.org/2001/XMLSchema#",
    "iana":    "http://www.iana.org/assignments/relation/",
    "owl":     "http://www.w3.org/2002/07/owl#",
    "as":      "http://www.w3.org/ns/activitystreams#",
    "schema":  "http://schema.org/",

    "id":      {"@type": "@id", "@id": "@id"},
    "type":    {"@type": "@id", "@id": "@type"},

    "Annotation":           "oa:Annotation",
    "Dataset":              "dctypes:Dataset",
    "Image":                "dctypes:StillImage",
    "Video":                "dctypes:MovingImage",
    "Audio":                "dctypes:Sound",
    "Text":                 "dctypes:Text",
    "TextualBody":          "oa:TextualBody",
    "ResourceSelection":    "oa:ResourceSelection",
    "SpecificResource":     "oa:SpecificResource",
    "FragmentSelector":     "oa:FragmentSelector",
    "CssSelector":          "oa:CssSelector",
    "XPathSelector":        "oa:XPathSelector",
    "TextQuoteSelector":    "oa:TextQuoteSelector",
    "TextPositionSelector": "oa:TextPositionSelector",
    "DataPositionSelector": "oa:DataPositionSelector",
    "SvgSelector":          "oa:SvgSelector",
    "RangeSelector":        "oa:RangeSelector",
    "TimeState":            "oa:TimeState",
    "HttpRequestState":     "oa:HttpRequestState",
    "CssStylesheet":        "oa:CssStyle",
    "Choice":               "oa:Choice",
    "Person":               "foaf:Person",
    "Software":             "as:Application",
    "Organization":         "foaf:Organization",
    "AnnotationCollection": "as:OrderedCollection",
    "AnnotationPage":       "as:OrderedCollectionPage",
    "Audience":             "schema:Audience",

    "Motivation":    "oa:Motivation",
    "bookmarking":   "oa:bookmarking",
    "classifying":   "oa:classifying",
    "commenting":    "oa:commenting",
    "describing":    "oa:describing",
    "editing":       "oa:editing",
    "highlighting":  "oa:highlighting",
    "identifying":   "oa:identifying",
    "linking":       "oa:linking",
    "moderating":    "oa:moderating",
    "questioning":   "oa:questioning",
    "replying":      "oa:replying",
    "reviewing":     "oa:reviewing",
    "assessing":     "oa:assessing",
    "tagging":       "oa:tagging",

    "auto":          "oa:autoDirection",
    "ltr":           "oa:ltrDirection",
    "rtl":           "oa:rtlDirection",

    "body":          {"@type": "@id", "@id": "oa:hasBody"},
    "target":        {"@type": "@id", "@id": "oa:hasTarget"},
    "source":        {"@type": "@id", "@id": "oa:hasSource"},
    "selector":      {"@type": "@id", "@id": "oa:hasSelector"},
    "state":         {"@type": "@id", "@id": "oa:hasState"},
    "scope":         {"@type": "@id", "@id": "oa:hasScope"},
    "refinedBy":     {"@type": "@id", "@id": "oa:refinedBy"},
    "startSelector": {"@type": "@id", "@id": "oa:hasStartSelector"},
    "endSelector":   {"@type": "@id", "@id": "oa:hasEndSelector"},
    "renderedVia":   {"@type": "@id", "@id": "oa:renderedVia"},
    "creator":       {"@type": "@id", "@id": "dcterms:creator"},
    "generator":     {"@type": "@id", "@id": "as:generator"},
    "rights":        {"@type": "@id", "@id": "dcterms:rights"},
    "homepage":      {"@type": "@id", "@id": "foaf:homepage"},
    "via":           {"@type": "@id", "@id": "oa:via"},
    "canonical":     {"@type": "@id", "@id": "oa:canonical"},
    "stylesheet":    {"@type": "@id", "@id": "oa:styledBy"},
    "cached":        {"@type": "@id", "@id": "oa:cachedSource"},
    "conformsTo":    {"@type": "@id", "@id": "dcterms:conformsTo"},
    "items":         {"@type": "@id", "@id": "as:items", "@container": "@list"},
    "partOf":        {"@type": "@id", "@id": "as:partOf"},
    "first":         {"@type": "@id", "@id": "as:first"},
    "last":          {"@type": "@id", "@id": "as:last"},
    "next":          {"@type": "@id", "@id": "as:next"},
    "prev":          {"@type": "@id", "@id": "as:prev"},
    "audience":      {"@type": "@id", "@id": "schema:audience"},
    "motivation":    {"@type": "@vocab", "@id": "oa:motivatedBy"},
    "purpose":       {"@type": "@vocab", "@id": "oa:hasPurpose"},
    "textDirection": {"@type": "@vocab", "@id": "oa:textDirection"},

    "accessibility": "schema:accessibilityFeature",
    "bodyValue":     "oa:bodyValue",
    "format":        "dc:format",
    "language":      "dc:language",
    "processingLanguage": "oa:processingLanguage",
    "value":         "rdf:value",
    "exact":         "oa:exact",
    "prefix":        "oa:prefix",
    "suffix":        "oa:suffix",
    "styleClass":    "oa:styleClass",
    "name":          "foaf:name",
    "email":         "foaf:mbox",
    "email_sha1":    "foaf:mbox_sha1sum",
    "nickname":      "foaf:nick",
    "label":         "rdfs:label",

    "created":       {"@id": "dcterms:created", "@type": "xsd:dateTime"},
    "modified":      {"@id": "dcterms:modified", "@type": "xsd:dateTime"},
    "generated":     {"@id": "dcterms:issued", "@type": "xsd:dateTime"},
    "sourceDate":    {"@id": "oa:sourceDate", "@type": "xsd:dateTime"},
    "sourceDateStart": {"@id": "oa:sourceDateStart", "@type": "xsd:dateTime"},
    "sourceDateEnd": {"@id": "oa:sourceDateEnd", "@type": "xsd:dateTime"},

    "start":         {"@id": "oa:start", "@type": "xsd:nonNegativeInteger"},
    "end":           {"@id": "oa:end", "@type": "xsd:nonNegativeInteger"},
    "total":         {"@id": "as:totalItems", "@type": "xsd:nonNegativeInteger"},
    "startIndex":    {"@id": "as:startIndex", "@type": "xsd:nonNegativeInteger"}
  }
}"#;

    fn fixture() -> FleeceAnnotationRecord {
        let html = "<html><head><title>Bridge proof</title></head><body><main><h1>Bridge proof</h1><p>The durable passage is preserved with selectors.</p></main></body></html>";
        let document = fleece::extract_document(&StaticDocument::parse(html));
        let exact = "durable passage";
        let start = document.page.text.find(exact).expect("fixture text") as u64;
        // Fleece positions are Unicode code points, not UTF-8 byte offsets.
        let start = document.page.text[..start as usize].chars().count() as u64;
        let end = start + exact.chars().count() as u64;
        let anchor = fleece::anchor_for_range(
            &document.page.text,
            fleece::TextPositionSelector { start, end },
            document.contract.quote_context,
        )
        .expect("fixture anchor");
        FleeceAnnotationRecord::from_fleece(
            CaptureIdentity::new("https://example.test/story", Hash::of(html.as_bytes()))
                .expect("capture identity"),
            &document,
            &anchor,
        )
        .expect("Fleece annotation record")
    }

    fn named_node(iri: &str) -> NamedNode {
        NamedNode::new(iri).expect("fixture IRI")
    }

    fn blank_node(id: &str) -> BlankNode {
        BlankNode::new(id).expect("fixture blank node")
    }

    fn collect_blank_nodes(dataset: &Dataset) -> Vec<BlankNode> {
        let mut nodes = HashSet::new();
        for quad in dataset.iter().map(|quad| quad.into_owned()) {
            if let NamedOrBlankNode::BlankNode(node) = quad.subject {
                nodes.insert(node);
            }
            if let Term::BlankNode(node) = quad.object {
                nodes.insert(node);
            }
            if let GraphName::BlankNode(node) = quad.graph_name {
                nodes.insert(node);
            }
        }
        nodes.into_iter().collect()
    }

    fn map_subject(
        subject: &NamedOrBlankNode,
        mapping: &HashMap<BlankNode, BlankNode>,
    ) -> NamedOrBlankNode {
        match subject {
            NamedOrBlankNode::BlankNode(node) => mapping
                .get(node)
                .cloned()
                .unwrap_or_else(|| node.clone())
                .into(),
            NamedOrBlankNode::NamedNode(node) => node.clone().into(),
        }
    }

    fn map_term(term: &Term, mapping: &HashMap<BlankNode, BlankNode>) -> Term {
        match term {
            Term::BlankNode(node) => mapping
                .get(node)
                .cloned()
                .unwrap_or_else(|| node.clone())
                .into(),
            _ => term.clone(),
        }
    }

    fn map_graph(graph: &GraphName, mapping: &HashMap<BlankNode, BlankNode>) -> GraphName {
        match graph {
            GraphName::BlankNode(node) => mapping
                .get(node)
                .cloned()
                .unwrap_or_else(|| node.clone())
                .into(),
            _ => graph.clone(),
        }
    }

    fn mapped_quad(quad: &Quad, mapping: &HashMap<BlankNode, BlankNode>) -> Quad {
        Quad::new(
            map_subject(&quad.subject, mapping),
            quad.predicate.clone(),
            map_term(&quad.object, mapping),
            map_graph(&quad.graph_name, mapping),
        )
    }

    fn isomorphic_dataset(expected: &Dataset, actual: &Dataset) -> bool {
        if expected.len() != actual.len() {
            return false;
        }
        let expected_quads: Vec<_> = expected.iter().map(|quad| quad.into_owned()).collect();
        let actual_quads: HashSet<_> = actual.iter().map(|quad| quad.into_owned()).collect();
        let expected_nodes = collect_blank_nodes(expected);
        let actual_nodes = collect_blank_nodes(actual);
        if expected_nodes.len() != actual_nodes.len() {
            return false;
        }

        fn search(
            index: usize,
            expected_nodes: &[BlankNode],
            actual_nodes: &[BlankNode],
            used: &mut [bool],
            mapping: &mut HashMap<BlankNode, BlankNode>,
            expected_quads: &[Quad],
            actual_quads: &HashSet<Quad>,
        ) -> bool {
            if index == expected_nodes.len() {
                return expected_quads
                    .iter()
                    .map(|quad| mapped_quad(quad, mapping))
                    .all(|quad| actual_quads.contains(&quad));
            }
            for actual_index in 0..actual_nodes.len() {
                if used[actual_index] {
                    continue;
                }
                used[actual_index] = true;
                mapping.insert(
                    expected_nodes[index].clone(),
                    actual_nodes[actual_index].clone(),
                );
                if search(
                    index + 1,
                    expected_nodes,
                    actual_nodes,
                    used,
                    mapping,
                    expected_quads,
                    actual_quads,
                ) {
                    return true;
                }
                mapping.remove(&expected_nodes[index]);
                used[actual_index] = false;
            }
            false
        }

        search(
            0,
            &expected_nodes,
            &actual_nodes,
            &mut vec![false; actual_nodes.len()],
            &mut HashMap::new(),
            &expected_quads,
            &actual_quads,
        )
    }

    #[test]
    fn annotation_json_ld_expands_to_expected_rdf_dataset_offline() {
        let record = fixture();
        let json_ld = record.annotation_json_ld().expect("serialize JSON-LD");
        let actual: Dataset = JsonLdParser::new()
            .for_slice(&json_ld)
            .with_load_document_callback(|url, _options| {
                if url == "http://www.w3.org/ns/anno.jsonld" {
                    Ok(JsonLdRemoteDocument {
                        document: W3C_ANNOTATION_CONTEXT.as_bytes().to_vec(),
                        document_url: url.to_owned(),
                    })
                } else {
                    Err::<JsonLdRemoteDocument, _>(
                        format!("offline test rejected remote @context: {url}").into(),
                    )
                }
            })
            .collect::<std::result::Result<Vec<_>, _>>()
            .expect("expand JSON-LD with the offline W3C context")
            .into_iter()
            .collect();

        let annotation = named_node(&record.annotation.id);
        let source = named_node(&record.target.source.id);
        let scope = named_node(&record.target.scope);
        let target = blank_node("target");
        let position = blank_node("position");
        let quote = blank_node("quote");
        let fragment = blank_node("fragment");
        let oa = "http://www.w3.org/ns/oa#";
        let rdf_type = named_node("http://www.w3.org/1999/02/22-rdf-syntax-ns#type");
        let default_graph = GraphName::DefaultGraph;
        let position_selector: TextPositionSelector =
            serde_json::from_value(record.target.selector[0].clone()).expect("position selector");
        let quote_selector: TextQuoteSelector =
            serde_json::from_value(record.target.selector[1].clone()).expect("quote selector");
        let fragment_selector: FragmentSelector =
            serde_json::from_value(record.target.selector[2].clone()).expect("fragment selector");
        let expected: Dataset = vec![
            Quad::new(
                annotation.clone(),
                rdf_type.clone(),
                named_node(&format!("{oa}Annotation")),
                default_graph.clone(),
            ),
            Quad::new(
                annotation,
                named_node(&format!("{oa}hasTarget")),
                target.clone(),
                default_graph.clone(),
            ),
            Quad::new(
                target.clone(),
                rdf_type.clone(),
                named_node(&format!("{oa}SpecificResource")),
                default_graph.clone(),
            ),
            Quad::new(
                target.clone(),
                named_node(&format!("{oa}hasSource")),
                source.clone(),
                default_graph.clone(),
            ),
            Quad::new(
                target.clone(),
                named_node(&format!("{oa}hasScope")),
                scope,
                default_graph.clone(),
            ),
            Quad::new(
                target.clone(),
                named_node(&format!("{oa}hasSelector")),
                position.clone(),
                default_graph.clone(),
            ),
            Quad::new(
                target.clone(),
                named_node(&format!("{oa}hasSelector")),
                quote.clone(),
                default_graph.clone(),
            ),
            Quad::new(
                target.clone(),
                named_node(&format!("{oa}hasSelector")),
                fragment.clone(),
                default_graph.clone(),
            ),
            Quad::new(
                target.clone(),
                named_node("https://merely-made.org/ns/eidetic#captureHash"),
                Literal::new_simple_literal(record.target.capture_hash.to_string()),
                default_graph.clone(),
            ),
            Quad::new(
                target,
                named_node("https://merely-made.org/ns/eidetic#canonicalTextHash"),
                Literal::new_simple_literal(record.target.canonical_text_hash.to_string()),
                default_graph.clone(),
            ),
            Quad::new(
                source,
                named_node("http://purl.org/dc/elements/1.1/format"),
                Literal::new_simple_literal(record.target.source.format.clone()),
                default_graph.clone(),
            ),
            Quad::new(
                position.clone(),
                rdf_type.clone(),
                named_node(&format!("{oa}TextPositionSelector")),
                default_graph.clone(),
            ),
            Quad::new(
                position.clone(),
                named_node(&format!("{oa}start")),
                Literal::new_typed_literal(
                    position_selector.start.to_string(),
                    named_node("http://www.w3.org/2001/XMLSchema#nonNegativeInteger"),
                ),
                default_graph.clone(),
            ),
            Quad::new(
                position,
                named_node(&format!("{oa}end")),
                Literal::new_typed_literal(
                    position_selector.end.to_string(),
                    named_node("http://www.w3.org/2001/XMLSchema#nonNegativeInteger"),
                ),
                default_graph.clone(),
            ),
            Quad::new(
                quote.clone(),
                rdf_type.clone(),
                named_node(&format!("{oa}TextQuoteSelector")),
                default_graph.clone(),
            ),
            Quad::new(
                quote.clone(),
                named_node(&format!("{oa}exact")),
                Literal::new_simple_literal(quote_selector.exact),
                default_graph.clone(),
            ),
            Quad::new(
                quote.clone(),
                named_node(&format!("{oa}prefix")),
                Literal::new_simple_literal(quote_selector.prefix),
                default_graph.clone(),
            ),
            Quad::new(
                quote,
                named_node(&format!("{oa}suffix")),
                Literal::new_simple_literal(quote_selector.suffix),
                default_graph.clone(),
            ),
            Quad::new(
                fragment.clone(),
                rdf_type,
                named_node(&format!("{oa}FragmentSelector")),
                default_graph.clone(),
            ),
            Quad::new(
                fragment.clone(),
                named_node("http://www.w3.org/1999/02/22-rdf-syntax-ns#value"),
                Literal::new_simple_literal(fragment_selector.value),
                default_graph.clone(),
            ),
            Quad::new(
                fragment,
                named_node("http://purl.org/dc/terms/conformsTo"),
                named_node(&fragment_selector.conforms_to),
                default_graph,
            ),
        ]
        .into_iter()
        .collect();

        assert!(
            isomorphic_dataset(&expected, &actual),
            "expanded JSON-LD dataset differed from the expected W3C Annotation graph"
        );
    }

    #[test]
    fn fleece_annotation_round_trips_after_fjall_reopen() {
        pollster::block_on(async {
            let directory = tempfile::tempdir().expect("temporary Fjall directory");
            let record = fixture();
            let id = {
                let mut store = FjallStore::open(directory.path()).expect("open Fjall");
                bootstrap_fleece_annotation_schema(&mut store)
                    .await
                    .expect("seed schema");
                save_fleece_annotation(&mut store, &record, 1_700_000_000_000)
                    .await
                    .expect("save typed Fleece annotation")
            };

            let mut reopened = FjallStore::open(directory.path()).expect("reopen Fjall");
            let loaded = load_fleece_annotation(&mut reopened, id)
                .await
                .expect("load typed Fleece annotation")
                .expect("record exists after reopen");
            assert_eq!(loaded, record);

            let json: serde_json::Value =
                serde_json::from_slice(&loaded.annotation_json_ld().expect("serialize JSON-LD"))
                    .expect("parse JSON-LD");
            assert_eq!(json["type"], "Annotation");
            assert_eq!(json["target"]["type"], "SpecificResource");
            assert_eq!(
                json["target"]["source"]["id"],
                loaded.extraction.canonical_text_record.canonical_text_iri
            );
            assert!(
                loaded
                    .extraction
                    .canonical_text_record
                    .canonical_text_iri
                    .starts_with("urn:sha256:")
            );
            assert_ne!(
                loaded.extraction.canonical_text_record.canonical_text_iri,
                loaded.extraction.canonical_text_hash.to_string()
            );
            assert_eq!(
                json["target"]["source"]["format"],
                loaded.extraction.canonical_text_record.media_type
            );
            assert_eq!(json["target"]["scope"], "https://example.test/story");
            assert_eq!(
                json["target"]["canonicalTextHash"],
                loaded.extraction.canonical_text_hash.to_string()
            );
            assert_eq!(
                json["target"]["captureHash"],
                loaded.extraction.capture.capture_hash.to_string()
            );
            assert_eq!(
                json["target"]["selector"][0]["type"],
                "TextPositionSelector"
            );
            assert_eq!(
                json["target"]["selector"][2]["value"],
                format!(
                    "char={},{}",
                    target_position(&loaded.target).0,
                    target_position(&loaded.target).1,
                )
            );
            assert_eq!(json["target"]["selector"][1]["type"], "TextQuoteSelector");
            assert_eq!(json["target"]["selector"][2]["type"], "FragmentSelector");
            assert_eq!(json["target"]["selector"][2]["conformsTo"], RFC_5147);
        });
    }

    #[test]
    fn capture_identity_rejects_relative_or_malformed_sources() {
        let capture_hash = Hash::of(b"capture");
        assert!(CaptureIdentity::new("relative/path", capture_hash).is_err());
        assert!(CaptureIdentity::new("not a URL", capture_hash).is_err());
        assert!(CaptureIdentity::new("gemini://example.test/page", capture_hash).is_ok());

        let mut record = fixture();
        record.extraction.capture.canonical_source = "relative/path".to_owned();
        assert!(record.validate_integrity().is_err());
    }

    #[test]
    fn changed_text_or_selector_is_rejected_before_reopen_accepts_it() {
        pollster::block_on(async {
            let mut record = fixture();
            record
                .extraction
                .canonical_text_record
                .canonical_text
                .push('!');
            let mut store = eidetic::MemoryBackend::default();
            let id = eidetic::save_typed(
                &mut store,
                &record,
                Vec::new(),
                PrivacyClass::LocalOnly,
                ProvenanceRecord {
                    origin: ProvenanceOrigin::Generated,
                    upstream: Vec::new(),
                    tooling: None,
                    generated_at: Timestamp::ZERO,
                },
                TrustEnvelope {
                    level: TrustLevel::SelfAsserted,
                    signatures: Vec::new(),
                    moderation_state: ModerationState::Unreviewed,
                },
                Timestamp::ZERO,
            )
            .await
            .expect("write deliberately invalid typed payload");
            assert!(load_fleece_annotation(&mut store, id).await.is_err());

            let mut record = fixture();
            record.target.selector[1]["exact"] = serde_json::Value::String("wrong".to_owned());
            let id = eidetic::save_typed(
                &mut store,
                &record,
                Vec::new(),
                PrivacyClass::LocalOnly,
                ProvenanceRecord {
                    origin: ProvenanceOrigin::Generated,
                    upstream: Vec::new(),
                    tooling: None,
                    generated_at: Timestamp::ZERO,
                },
                TrustEnvelope {
                    level: TrustLevel::SelfAsserted,
                    signatures: Vec::new(),
                    moderation_state: ModerationState::Unreviewed,
                },
                Timestamp::ZERO,
            )
            .await
            .expect("write deliberately invalid typed payload");
            assert!(load_fleece_annotation(&mut store, id).await.is_err());

            let mut record = fixture();
            record.target.selector[2]["value"] = serde_json::Value::String("char=0,1".to_owned());
            assert!(record.validate_integrity().is_err());

            let mut record = fixture();
            record.extraction.capture.capture_hash = Hash::of(b"changed capture");
            assert!(record.validate_integrity().is_err());
        });
    }
}
