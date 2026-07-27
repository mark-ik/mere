//! Per-format document codecs and write-through files.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use inker::{DocumentTrustState, Engine, EngineDocument, EngineInput};
use nematic::knot::djot::blocks_to_djot;
use nematic::{DjotKnotEngine, MarkdownEngine};

/// Formats Knot can author directly.
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
            .and_then(|extension| extension.to_str())
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

    pub(crate) fn from_media_type(media_type: &str) -> Option<Self> {
        match media_type {
            "text/vnd.knot" => Some(Self::Knot),
            "text/markdown" => Some(Self::Markdown),
            "text/djot" => Some(Self::Djot),
            "application/vnd.knot.document+json" | "application/json" => Some(Self::Json),
            _ => None,
        }
    }

    pub(crate) fn validate_source(self, address: &str, source: &str) -> Result<(), String> {
        self.parse(address, source.as_bytes()).map(|_| ())
    }

    fn parse(self, address: &str, bytes: &[u8]) -> Result<EngineDocument, String> {
        if self == Self::Json {
            return serde_json::from_slice(bytes)
                .map_err(|error| format!("invalid Knot document JSON: {error}"));
        }
        let body = std::str::from_utf8(bytes)
            .map_err(|error| format!("document is not UTF-8: {error}"))?
            .to_string();
        let input = EngineInput {
            address: address.to_string(),
            body,
            content_type: Some(self.media_type().to_string()),
        };
        match self {
            Self::Knot | Self::Djot => DjotKnotEngine::new()
                .render(&input)
                .map_err(|error| format!("could not parse Djot document: {error}")),
            Self::Markdown => MarkdownEngine::new()
                .render(&input)
                .map_err(|error| format!("could not parse Markdown document: {error}")),
            Self::Json => unreachable!("handled above"),
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

/// Result of a save attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveOutcome {
    /// The target bytes already matched, or the document was never edited.
    Unchanged,
    /// New bytes were written.
    Written,
}

/// One file-backed editable document with explicit dirty tracking.
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
        let address = file_address(&path)?;
        let document = format.parse(&address, &original)?;
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

    /// Mutable access marks the file dirty before the caller changes it.
    pub fn document_mut(&mut self) -> &mut EngineDocument {
        self.dirty = true;
        &mut self.document
    }

    /// Save back to the source format. An untouched file does not reach the
    /// filesystem write path, even if parsing would normalize its syntax.
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

    /// Save a converted copy in a caller-selected format.
    pub fn save_as(
        &self,
        path: impl AsRef<Path>,
        format: DocumentFormat,
    ) -> Result<SaveOutcome, String> {
        let path = path.as_ref();
        let encoded = format.serialize(&self.document)?;
        let existing = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(error) => {
                return Err(format!("could not read {}: {error}", path.display()));
            }
        };
        write_if_distinct(path, &existing, &encoded)
    }

    /// Parse then write one format without touching a filesystem.
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

pub(crate) fn write_if_distinct(
    path: &Path,
    before: &[u8],
    after: &[u8],
) -> Result<SaveOutcome, String> {
    if before == after {
        return Ok(SaveOutcome::Unchanged);
    }
    fs::write(path, after)
        .map_err(|error| format!("could not write {}: {error}", path.display()))?;
    Ok(SaveOutcome::Written)
}

fn document_to_knot(document: &EngineDocument) -> String {
    let mut output = String::new();
    let has_frontmatter = document.title.is_some()
        || document.provenance.canonical_uri.is_some()
        || document.provenance.fetched_at.is_some()
        || document.provenance.source_label.is_some()
        || document.trust != DocumentTrustState::Unknown;
    if has_frontmatter {
        output.push_str("---\n");
        if let Some(title) = &document.title {
            output.push_str(&format!("title: {title}\n"));
        }
        if let Some(source) = &document.provenance.canonical_uri {
            output.push_str(&format!("source: {source}\n"));
        }
        if let Some(captured) = &document.provenance.fetched_at {
            output.push_str(&format!("captured: {captured}\n"));
        }
        if let Some(label) = &document.provenance.source_label {
            output.push_str(&format!("source_label: {label}\n"));
        }
        let trust = match document.trust {
            DocumentTrustState::Trusted => Some("trusted"),
            DocumentTrustState::Tofu => Some("tofu"),
            DocumentTrustState::Insecure => Some("insecure"),
            DocumentTrustState::Broken => Some("broken"),
            DocumentTrustState::Unknown => None,
        };
        if let Some(trust) = trust {
            output.push_str(&format!("trust: {trust}\n"));
        }
        output.push_str("---\n\n");
    }
    output.push_str(&blocks_to_djot(&document.blocks));
    output
}

pub(crate) fn file_address(path: &Path) -> Result<String, String> {
    let path = fs::canonicalize(path)
        .map_err(|error| format!("could not resolve {}: {error}", path.display()))?;
    #[cfg(windows)]
    {
        let text = path.to_string_lossy();
        let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
        Ok(format!("file:///{}", text.replace('\\', "/")))
    }
    #[cfg(not(windows))]
    {
        Ok(format!("file://{}", path.to_string_lossy()))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

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
