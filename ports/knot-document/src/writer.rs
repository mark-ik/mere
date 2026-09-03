// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use std::fs;
use std::path::Path;
#[cfg(feature = "engine")]
use std::path::PathBuf;

/// Formats Knot can author directly. Djot is the native current format; `.knot`
/// remains a compatibility format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentFormat {
    Knot,
    Markdown,
    Djot,
    Json,
}

impl DocumentFormat {
    pub fn from_path(path: &Path) -> Option<Self> {
        match path
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("knot") => Some(Self::Knot),
            Some("md" | "markdown") => Some(Self::Markdown),
            Some("djot") => Some(Self::Djot),
            Some("json") => Some(Self::Json),
            _ => None,
        }
    }
    pub fn media_type(self) -> &'static str {
        match self {
            Self::Knot => "text/vnd.knot",
            Self::Markdown => "text/markdown",
            Self::Djot => "text/djot",
            Self::Json => "application/vnd.knot.document+json",
        }
    }
    pub fn from_media_type(value: &str) -> Option<Self> {
        match value {
            "text/vnd.knot" => Some(Self::Knot),
            "text/markdown" => Some(Self::Markdown),
            "text/djot" => Some(Self::Djot),
            "application/vnd.knot.document+json" | "application/json" => Some(Self::Json),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveOutcome {
    Unchanged,
    Written,
}

#[doc(hidden)]
pub fn write_if_distinct(path: &Path, before: &[u8], after: &[u8]) -> Result<SaveOutcome, String> {
    if before == after {
        return Ok(SaveOutcome::Unchanged);
    }
    fs::write(path, after)
        .map_err(|error| format!("could not write {}: {error}", path.display()))?;
    Ok(SaveOutcome::Written)
}

pub(crate) fn file_address(path: &Path) -> Result<String, String> {
    let path = fs::canonicalize(path)
        .map_err(|error| format!("could not resolve {}: {error}", path.display()))?;
    #[cfg(windows)]
    {
        let text = path.to_string_lossy();
        Ok(format!(
            "file:///{}",
            text.strip_prefix(r"\\?\")
                .unwrap_or(&text)
                .replace('\\', "/")
        ))
    }
    #[cfg(not(windows))]
    {
        Ok(format!("file://{}", path.to_string_lossy()))
    }
}

#[cfg(feature = "engine")]
mod engine {
    use super::*;
    use inker::{DocumentTrustState, Engine, EngineDocument, EngineInput};
    use nematic::knot::djot::blocks_to_djot;
    use nematic::{DjotKnotEngine, MarkdownEngine};
    use std::io;

    impl DocumentFormat {
        pub fn validate_source(self, address: &str, source: &str) -> Result<(), String> {
            self.parse(address, source.as_bytes()).map(|_| ())
        }
        pub fn to_commonmark(self, address: &str, bytes: &[u8]) -> Result<Vec<u8>, String> {
            self.parse(address, bytes)
                .map(|document| document.to_markdown().into_bytes())
        }
        fn parse(self, address: &str, bytes: &[u8]) -> Result<EngineDocument, String> {
            if self == Self::Json {
                return serde_json::from_slice(bytes)
                    .map_err(|error| format!("invalid Knot document JSON: {error}"));
            }
            let input = EngineInput {
                address: address.to_owned(),
                body: std::str::from_utf8(bytes)
                    .map_err(|error| format!("document is not UTF-8: {error}"))?
                    .to_owned(),
                content_type: Some(self.media_type().to_owned()),
            };
            match self {
                Self::Knot | Self::Djot => DjotKnotEngine::new()
                    .render(&input)
                    .map_err(|error| format!("could not parse Djot document: {error}")),
                Self::Markdown => MarkdownEngine::new()
                    .render(&input)
                    .map_err(|error| format!("could not parse Markdown document: {error}")),
                Self::Json => unreachable!(),
            }
        }
        fn serialize(self, document: &EngineDocument) -> Result<Vec<u8>, String> {
            let text = match self {
                Self::Knot => document_to_knot(document),
                Self::Markdown => document.to_markdown(),
                Self::Djot => blocks_to_djot(&document.blocks),
                Self::Json => {
                    serde_json::to_string_pretty(document)
                        .map_err(|error| format!("could not encode Knot document JSON: {error}"))?
                        + "\n"
                }
            };
            Ok(text.into_bytes())
        }
    }
    pub struct AuthoredFile {
        path: PathBuf,
        format: DocumentFormat,
        document: EngineDocument,
        original: Vec<u8>,
        dirty: bool,
    }
    impl AuthoredFile {
        pub fn open(path: impl Into<PathBuf>) -> Result<Self, String> {
            let path = path.into();
            let format = DocumentFormat::from_path(&path)
                .ok_or_else(|| format!("unsupported Knot authoring format: {}", path.display()))?;
            let original = fs::read(&path)
                .map_err(|error| format!("could not read {}: {error}", path.display()))?;
            let document = format.parse(&file_address(&path)?, &original)?;
            Ok(Self {
                path,
                format,
                document,
                original,
                dirty: false,
            })
        }
        pub fn path(&self) -> &Path {
            &self.path
        }
        pub fn format(&self) -> DocumentFormat {
            self.format
        }
        pub fn document(&self) -> &EngineDocument {
            &self.document
        }
        pub fn document_mut(&mut self) -> &mut EngineDocument {
            self.dirty = true;
            &mut self.document
        }
        pub fn save(&mut self) -> Result<SaveOutcome, String> {
            if !self.dirty {
                return Ok(SaveOutcome::Unchanged);
            }
            let encoded = self.format.serialize(&self.document)?;
            let outcome = write_if_distinct(&self.path, &self.original, &encoded)?;
            self.original = encoded;
            self.dirty = false;
            Ok(outcome)
        }
        pub fn save_as(
            &self,
            path: impl AsRef<Path>,
            format: DocumentFormat,
        ) -> Result<SaveOutcome, String> {
            let path = path.as_ref();
            let existing = match fs::read(path) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
                Err(error) => return Err(format!("could not read {}: {error}", path.display())),
            };
            write_if_distinct(path, &existing, &format.serialize(&self.document)?)
        }
        pub fn canonicalize(
            format: DocumentFormat,
            address: &str,
            bytes: &[u8],
        ) -> Result<Vec<u8>, String> {
            format
                .parse(address, bytes)
                .and_then(|document| format.serialize(&document))
        }
    }
    fn document_to_knot(document: &EngineDocument) -> String {
        let mut output = String::new();
        let frontmatter = document.title.is_some()
            || document.provenance.canonical_uri.is_some()
            || document.provenance.fetched_at.is_some()
            || document.provenance.source_label.is_some()
            || document.trust != DocumentTrustState::Unknown;
        if frontmatter {
            output.push_str("---\n");
            if let Some(value) = &document.title {
                output.push_str(&format!("title: {value}\n"));
            }
            if let Some(value) = &document.provenance.canonical_uri {
                output.push_str(&format!("source: {value}\n"));
            }
            if let Some(value) = &document.provenance.fetched_at {
                output.push_str(&format!("captured: {value}\n"));
            }
            if let Some(value) = &document.provenance.source_label {
                output.push_str(&format!("source_label: {value}\n"));
            }
            let trust = match document.trust {
                DocumentTrustState::Trusted => Some("trusted"),
                DocumentTrustState::Tofu => Some("tofu"),
                DocumentTrustState::Insecure => Some("insecure"),
                DocumentTrustState::Broken => Some("broken"),
                DocumentTrustState::Unknown => None,
            };
            if let Some(value) = trust {
                output.push_str(&format!("trust: {value}\n"));
            }
            output.push_str("---\n\n");
        }
        output.push_str(&blocks_to_djot(&document.blocks));
        output
    }
}
#[cfg(feature = "engine")]
pub use engine::AuthoredFile;

#[cfg(all(test, feature = "engine"))]
mod tests {
    use std::fs;

    use inker::EngineDocument;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn foreign_formats_reach_a_fixed_point_after_one_parse_write() {
        let cases = [
            (
                DocumentFormat::Markdown,
                b"# Heading\n\nA *small* note.\n".as_slice(),
            ),
            (
                DocumentFormat::Djot,
                b"# Heading\n\nA small note.\n".as_slice(),
            ),
            (
                DocumentFormat::Json,
                br#"{"address":"memory:test","title":null,"content_type":"text/plain","lang":null,"provenance":{},"trust":"Unknown","diagnostics":[],"blocks":[]}"#,
            ),
        ];
        for (format, source) in cases {
            let once = AuthoredFile::canonicalize(format, "memory:test", source).unwrap();
            let twice = AuthoredFile::canonicalize(format, "memory:test", &once).unwrap();
            assert_eq!(once, twice, "{format:?} did not reach a fixed point");
        }
    }

    #[test]
    fn a_canonical_knot_round_trip_is_byte_exact() {
        let source =
            b"---\ntitle: Field note\ntrust: tofu\n---\n\n# Field note\n\norchard observations\n";
        let canonical =
            AuthoredFile::canonicalize(DocumentFormat::Knot, "memory:field", source).unwrap();
        assert_eq!(
            AuthoredFile::canonicalize(DocumentFormat::Knot, "memory:field", &canonical).unwrap(),
            canonical
        );
    }

    #[test]
    fn untouched_files_never_enter_the_write_path() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("foreign.md");
        fs::write(&path, "#  Deliberately foreign spacing\n").unwrap();
        let mut file = AuthoredFile::open(&path).unwrap();

        let original_permissions = fs::metadata(&path).unwrap().permissions();
        let mut read_only = original_permissions.clone();
        read_only.set_readonly(true);
        fs::set_permissions(&path, read_only).unwrap();
        assert_eq!(file.save().unwrap(), SaveOutcome::Unchanged);

        fs::set_permissions(&path, original_permissions).unwrap();
    }

    #[test]
    fn caller_selects_the_output_format() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("note.md");
        let target = temp.path().join("note.json");
        fs::write(&source, "# Note\n\nA body.\n").unwrap();
        let file = AuthoredFile::open(&source).unwrap();
        assert_eq!(
            file.save_as(&target, DocumentFormat::Json).unwrap(),
            SaveOutcome::Written
        );
        let document: EngineDocument = serde_json::from_slice(&fs::read(target).unwrap()).unwrap();
        assert_eq!(document.title.as_deref(), Some("Note"));
    }
}
