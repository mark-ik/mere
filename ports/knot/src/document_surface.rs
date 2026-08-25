//! The first product-owned Knot document surface.
//!
//! This model deliberately wraps [`KnotEditor`] rather than introducing a
//! second document buffer. Cambium's [`TextInput`] remains the sole mutable
//! source; snapshots are read-only projections of that state.

use std::path::Path;

use cambium::{CaretSelection, TextCommand, TextInput};

use crate::editor::KnotEditor;
use crate::writer::{DocumentFormat, SaveOutcome};

/// Whether a document has a native file target or is still an unsaved scratch
/// document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnotDocumentSourceKindV1 {
    File,
    Scratch,
}

/// Stable identity for the source currently owned by a document session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnotDocumentSourceV1 {
    pub kind: KnotDocumentSourceKindV1,
    pub address: String,
}

/// The session's current ability to write its source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnotDocumentWritePostureV1 {
    FileTarget,
    Scratch,
}

/// The outcome of the most recent save intent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnotDocumentSaveOutcomeV1 {
    Written,
    Unchanged,
    Refused,
    Failed,
}

/// A typed refusal produced when a document cannot accept a requested write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnotDocumentRefusalV1 {
    /// A scratch document has no path to which its source can be committed.
    ScratchHasNoSaveTarget,
}

/// A save failure retained in the next snapshot for host presentation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnotDocumentSaveFailureV1 {
    pub message: String,
}

/// An error applying a document intent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KnotDocumentIntentErrorV1 {
    Refused(KnotDocumentRefusalV1),
    SaveFailed(KnotDocumentSaveFailureV1),
}

/// Read-only product state for one selected Knot document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnotDocumentSnapshotV1 {
    pub source: KnotDocumentSourceV1,
    pub display_label: String,
    pub format: DocumentFormat,
    pub text: String,
    pub selection: CaretSelection,
    pub dirty: bool,
    pub write_posture: KnotDocumentWritePostureV1,
    pub last_save_outcome: Option<KnotDocumentSaveOutcomeV1>,
    pub refusal: Option<KnotDocumentRefusalV1>,
    pub last_save_failure: Option<KnotDocumentSaveFailureV1>,
}

/// The first product intents accepted by a Knot document session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KnotDocumentIntentV1 {
    Edit(TextCommand),
    Save,
}

/// A single selected document over the existing [`KnotEditor`].
pub struct KnotDocumentSession {
    editor: KnotEditor,
    last_save_outcome: Option<KnotDocumentSaveOutcomeV1>,
    refusal: Option<KnotDocumentRefusalV1>,
    last_save_failure: Option<KnotDocumentSaveFailureV1>,
}

impl KnotDocumentSession {
    /// Open one existing local `.djot` or legacy `.knot` source.
    pub fn open(path: impl Into<std::path::PathBuf>) -> Result<Self, String> {
        Ok(Self {
            editor: KnotEditor::open(path)?,
            last_save_outcome: None,
            refusal: None,
            last_save_failure: None,
        })
    }

    /// Start a document with no native save target.
    pub fn scratch(address: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            editor: KnotEditor::scratch(address, source),
            last_save_outcome: None,
            refusal: None,
            last_save_failure: None,
        }
    }

    /// Project the current editor state without mutating or replacing its
    /// authoritative source buffer.
    pub fn snapshot(&self) -> KnotDocumentSnapshotV1 {
        let path = self.editor.path();
        let kind = if path.is_some() {
            KnotDocumentSourceKindV1::File
        } else {
            KnotDocumentSourceKindV1::Scratch
        };
        KnotDocumentSnapshotV1 {
            source: KnotDocumentSourceV1 {
                kind,
                address: self.editor.address().to_owned(),
            },
            display_label: display_label(path, self.editor.address()),
            format: self.editor.format(),
            text: self.editor.source().to_owned(),
            selection: self.editor.selection(),
            dirty: self.editor.is_dirty(),
            write_posture: if path.is_some() {
                KnotDocumentWritePostureV1::FileTarget
            } else {
                KnotDocumentWritePostureV1::Scratch
            },
            last_save_outcome: self.last_save_outcome,
            refusal: self.refusal,
            last_save_failure: self.last_save_failure.clone(),
        }
    }

    /// Borrow the one retained input buffer for a Cambium editor component.
    pub fn input(&self) -> &TextInput {
        self.editor.input()
    }

    /// Mutably borrow the same input buffer for Cambium's focused-text seam.
    pub fn input_mut(&mut self) -> &mut TextInput {
        self.editor.input_mut()
    }

    /// Apply one product intent through the editor's sole mutation path.
    pub fn apply(
        &mut self,
        intent: KnotDocumentIntentV1,
    ) -> Result<KnotDocumentSnapshotV1, KnotDocumentIntentErrorV1> {
        match intent {
            KnotDocumentIntentV1::Edit(command) => {
                self.editor.apply(command);
            }
            KnotDocumentIntentV1::Save => self.save()?,
        }
        Ok(self.snapshot())
    }

    fn save(&mut self) -> Result<(), KnotDocumentIntentErrorV1> {
        if self.editor.path().is_none() {
            self.last_save_outcome = Some(KnotDocumentSaveOutcomeV1::Refused);
            self.refusal = Some(KnotDocumentRefusalV1::ScratchHasNoSaveTarget);
            self.last_save_failure = None;
            return Err(KnotDocumentIntentErrorV1::Refused(
                KnotDocumentRefusalV1::ScratchHasNoSaveTarget,
            ));
        }

        match self.editor.save() {
            Ok(outcome) => {
                self.last_save_outcome = Some(match outcome {
                    SaveOutcome::Written => KnotDocumentSaveOutcomeV1::Written,
                    SaveOutcome::Unchanged => KnotDocumentSaveOutcomeV1::Unchanged,
                });
                self.refusal = None;
                self.last_save_failure = None;
                Ok(())
            }
            Err(error) => {
                self.last_save_outcome = Some(KnotDocumentSaveOutcomeV1::Failed);
                let failure = KnotDocumentSaveFailureV1 { message: error };
                self.last_save_failure = Some(failure.clone());
                Err(KnotDocumentIntentErrorV1::SaveFailed(failure))
            }
        }
    }
}

fn display_label(path: Option<&Path>, address: &str) -> String {
    path.and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .unwrap_or_else(|| address.to_owned())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use cambium::TextCommand;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn file_session_round_trips_edit_save_drop_and_reopen() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("field-note.djot");
        fs::write(&path, "# Field note\n").unwrap();

        let mut session = KnotDocumentSession::open(&path).unwrap();
        let clean = session.snapshot();
        assert_eq!(clean.source.kind, KnotDocumentSourceKindV1::File);
        assert_eq!(clean.display_label, "field-note.djot");
        assert_eq!(clean.format, DocumentFormat::Djot);
        assert_eq!(clean.text, "# Field note\n");
        assert!(!clean.dirty);
        assert_eq!(clean.write_posture, KnotDocumentWritePostureV1::FileTarget);

        let dirty = session
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
                "\nA body.\n".into(),
            )))
            .unwrap();
        assert!(dirty.dirty);
        assert_eq!(dirty.text, "# Field note\n\nA body.\n");

        let saved = session.apply(KnotDocumentIntentV1::Save).unwrap();
        assert!(!saved.dirty);
        assert_eq!(
            saved.last_save_outcome,
            Some(KnotDocumentSaveOutcomeV1::Written)
        );
        drop(session);

        let reopened = KnotDocumentSession::open(&path).unwrap();
        assert_eq!(reopened.snapshot().text, "# Field note\n\nA body.\n");
        assert!(!reopened.snapshot().dirty);
    }

    #[test]
    fn scratch_save_is_an_explicit_typed_refusal() {
        let mut session = KnotDocumentSession::scratch("memory:field-note", "# Scratch\n");
        let result = session.apply(KnotDocumentIntentV1::Save);
        assert_eq!(
            result,
            Err(KnotDocumentIntentErrorV1::Refused(
                KnotDocumentRefusalV1::ScratchHasNoSaveTarget
            ))
        );
        let snapshot = session.snapshot();
        assert_eq!(snapshot.write_posture, KnotDocumentWritePostureV1::Scratch);
        assert_eq!(
            snapshot.last_save_outcome,
            Some(KnotDocumentSaveOutcomeV1::Refused)
        );
        assert_eq!(
            snapshot.refusal,
            Some(KnotDocumentRefusalV1::ScratchHasNoSaveTarget)
        );
    }

    #[test]
    fn failed_save_is_visible_in_the_following_snapshot() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("field-note.djot");
        fs::write(&path, "# Field note\n").unwrap();
        let mut session = KnotDocumentSession::open(&path).unwrap();
        fs::remove_file(&path).unwrap();
        session
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
                "body\n".into(),
            )))
            .unwrap();

        let result = session.apply(KnotDocumentIntentV1::Save);
        let failure = match result {
            Err(KnotDocumentIntentErrorV1::SaveFailed(failure)) => failure,
            other => panic!("expected typed save failure, got {other:?}"),
        };
        let snapshot = session.snapshot();
        assert_eq!(
            snapshot.last_save_outcome,
            Some(KnotDocumentSaveOutcomeV1::Failed)
        );
        assert_eq!(snapshot.last_save_failure, Some(failure));
    }
}
