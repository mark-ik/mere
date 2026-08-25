use crate::{DocumentFormat, KnotEditor, SaveOutcome};
use cambium::{CaretSelection, TextCommand, TextInput};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnotDocumentSourceKindV1 {
    File,
    Scratch,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnotDocumentSourceV1 {
    pub kind: KnotDocumentSourceKindV1,
    pub address: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnotDocumentWritePostureV1 {
    FileTarget,
    Scratch,
    ReadOnly,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnotDocumentSaveOutcomeV1 {
    Written,
    Unchanged,
    Refused,
    Failed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnotDocumentRefusalV1 {
    ScratchHasNoSaveTarget,
    ReadOnly,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnotDocumentSaveFailureV1 {
    pub message: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KnotDocumentIntentErrorV1 {
    Refused(KnotDocumentRefusalV1),
    SaveFailed(KnotDocumentSaveFailureV1),
}
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KnotDocumentIntentV1 {
    Edit(TextCommand),
    Save,
}

/// One selected document over the retained Knot editor. There is no second buffer.
pub struct KnotDocumentSession {
    editor: KnotEditor,
    write_posture: KnotDocumentWritePostureV1,
    last_save_outcome: Option<KnotDocumentSaveOutcomeV1>,
    refusal: Option<KnotDocumentRefusalV1>,
    last_save_failure: Option<KnotDocumentSaveFailureV1>,
}
impl KnotDocumentSession {
    pub fn open(path: impl Into<std::path::PathBuf>) -> Result<Self, String> {
        Self::open_with_posture(path, KnotDocumentWritePostureV1::FileTarget)
    }

    /// Opens a local document for inspection while retaining its file identity.
    ///
    /// The session refuses both edit and save intents. A host uses this when it
    /// has deliberately admitted a file without delegating write authority.
    pub fn open_read_only(path: impl Into<std::path::PathBuf>) -> Result<Self, String> {
        Self::open_with_posture(path, KnotDocumentWritePostureV1::ReadOnly)
    }

    fn open_with_posture(
        path: impl Into<std::path::PathBuf>,
        write_posture: KnotDocumentWritePostureV1,
    ) -> Result<Self, String> {
        Ok(Self {
            editor: KnotEditor::open(path)?,
            write_posture,
            last_save_outcome: None,
            refusal: None,
            last_save_failure: None,
        })
    }
    pub fn scratch(address: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            editor: KnotEditor::scratch(address, source),
            write_posture: KnotDocumentWritePostureV1::Scratch,
            last_save_outcome: None,
            refusal: None,
            last_save_failure: None,
        }
    }

    /// Builds an intentionally immutable in-memory document projection.
    pub fn read_only(address: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            editor: KnotEditor::scratch(address, source),
            write_posture: KnotDocumentWritePostureV1::ReadOnly,
            last_save_outcome: None,
            refusal: None,
            last_save_failure: None,
        }
    }
    pub fn input(&self) -> &TextInput {
        self.editor.input()
    }
    /// Borrows the input only when this session delegated text-write authority.
    ///
    /// Hosts should route document mutations through [`Self::apply`]. This
    /// guarded escape hatch remains for compatible editable text hosts.
    pub fn input_mut(&mut self) -> Result<&mut TextInput, KnotDocumentRefusalV1> {
        if self.write_posture == KnotDocumentWritePostureV1::ReadOnly {
            self.refuse_read_only_edit();
            return Err(KnotDocumentRefusalV1::ReadOnly);
        }
        Ok(self.editor.input_mut())
    }

    /// The editable view has already selected its writable branch. Kept crate
    /// private so a product cannot bypass [`Self::input_mut`] for a read-only
    /// session.
    pub(crate) fn input_mut_for_editable_view(&mut self) -> &mut TextInput {
        debug_assert_ne!(self.write_posture, KnotDocumentWritePostureV1::ReadOnly);
        self.editor.input_mut()
    }
    pub fn snapshot(&self) -> KnotDocumentSnapshotV1 {
        let path = self.editor.path();
        let file = path.is_some();
        KnotDocumentSnapshotV1 {
            source: KnotDocumentSourceV1 {
                kind: if file {
                    KnotDocumentSourceKindV1::File
                } else {
                    KnotDocumentSourceKindV1::Scratch
                },
                address: self.editor.address().to_owned(),
            },
            display_label: path
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                .map(str::to_owned)
                .unwrap_or_else(|| self.editor.address().to_owned()),
            format: self.editor.format(),
            text: self.editor.source().to_owned(),
            selection: self.editor.selection(),
            dirty: self.editor.is_dirty(),
            write_posture: self.write_posture,
            last_save_outcome: self.last_save_outcome,
            refusal: self.refusal,
            last_save_failure: self.last_save_failure.clone(),
        }
    }
    pub fn apply(
        &mut self,
        intent: KnotDocumentIntentV1,
    ) -> Result<KnotDocumentSnapshotV1, KnotDocumentIntentErrorV1> {
        match intent {
            KnotDocumentIntentV1::Edit(command) => {
                if self.write_posture == KnotDocumentWritePostureV1::ReadOnly {
                    self.refuse_read_only_edit();
                    return Err(KnotDocumentIntentErrorV1::Refused(
                        KnotDocumentRefusalV1::ReadOnly,
                    ));
                }
                self.editor.apply(command);
                self.refusal = None;
            }
            KnotDocumentIntentV1::Save => self.save()?,
        };
        Ok(self.snapshot())
    }
    fn save(&mut self) -> Result<(), KnotDocumentIntentErrorV1> {
        if self.write_posture == KnotDocumentWritePostureV1::ReadOnly {
            self.last_save_outcome = Some(KnotDocumentSaveOutcomeV1::Refused);
            self.refusal = Some(KnotDocumentRefusalV1::ReadOnly);
            self.last_save_failure = None;
            return Err(KnotDocumentIntentErrorV1::Refused(
                KnotDocumentRefusalV1::ReadOnly,
            ));
        }
        if self.editor.path().is_none() {
            self.last_save_outcome = Some(KnotDocumentSaveOutcomeV1::Refused);
            self.refusal = Some(KnotDocumentRefusalV1::ScratchHasNoSaveTarget);
            self.last_save_failure = None;
            return Err(KnotDocumentIntentErrorV1::Refused(
                KnotDocumentRefusalV1::ScratchHasNoSaveTarget,
            ));
        }
        match self.editor.save() {
            Ok(SaveOutcome::Written) => {
                self.last_save_outcome = Some(KnotDocumentSaveOutcomeV1::Written);
                self.refusal = None;
                self.last_save_failure = None;
                Ok(())
            }
            Ok(SaveOutcome::Unchanged) => {
                self.last_save_outcome = Some(KnotDocumentSaveOutcomeV1::Unchanged);
                self.refusal = None;
                self.last_save_failure = None;
                Ok(())
            }
            Err(message) => {
                self.last_save_outcome = Some(KnotDocumentSaveOutcomeV1::Failed);
                let failure = KnotDocumentSaveFailureV1 { message };
                self.last_save_failure = Some(failure.clone());
                Err(KnotDocumentIntentErrorV1::SaveFailed(failure))
            }
        }
    }

    fn refuse_read_only_edit(&mut self) {
        self.refusal = Some(KnotDocumentRefusalV1::ReadOnly);
        self.last_save_failure = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    #[test]
    fn file_session_round_trips_edit_save_drop_and_reopen() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("field.djot");
        std::fs::write(&path, "# Field\n").unwrap();
        let mut session = KnotDocumentSession::open(&path).unwrap();
        assert_eq!(session.snapshot().format, DocumentFormat::Djot);
        session
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
                "body\n".into(),
            )))
            .unwrap();
        let saved = session.apply(KnotDocumentIntentV1::Save).unwrap();
        assert!(!saved.dirty);
        drop(session);
        assert_eq!(
            KnotDocumentSession::open(&path).unwrap().snapshot().text,
            "# Field\nbody\n"
        );
    }
    #[test]
    fn scratch_save_is_an_explicit_typed_refusal() {
        let mut session = KnotDocumentSession::scratch("memory:field", "");
        assert!(matches!(
            session.apply(KnotDocumentIntentV1::Save),
            Err(KnotDocumentIntentErrorV1::Refused(
                KnotDocumentRefusalV1::ScratchHasNoSaveTarget
            ))
        ));
    }

    #[test]
    fn read_only_file_refuses_edit_and_save_without_mutating_document_state() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("field.djot");
        std::fs::write(&path, "# Field\n").unwrap();
        let mut session = KnotDocumentSession::open_read_only(&path).unwrap();
        let before = session.snapshot();

        assert!(matches!(
            session.apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
                "body\n".into()
            ))),
            Err(KnotDocumentIntentErrorV1::Refused(
                KnotDocumentRefusalV1::ReadOnly
            ))
        ));
        let after_edit = session.snapshot();
        assert_eq!(after_edit.text, before.text);
        assert_eq!(after_edit.selection, before.selection);
        assert!(!after_edit.dirty);
        assert_eq!(
            after_edit.write_posture,
            KnotDocumentWritePostureV1::ReadOnly
        );
        assert_eq!(after_edit.refusal, Some(KnotDocumentRefusalV1::ReadOnly));

        assert!(matches!(
            session.apply(KnotDocumentIntentV1::Save),
            Err(KnotDocumentIntentErrorV1::Refused(
                KnotDocumentRefusalV1::ReadOnly
            ))
        ));
        let after_save = session.snapshot();
        assert_eq!(after_save.text, before.text);
        assert!(!after_save.dirty);
        assert_eq!(
            after_save.last_save_outcome,
            Some(KnotDocumentSaveOutcomeV1::Refused)
        );
        assert!(matches!(
            session.input_mut(),
            Err(KnotDocumentRefusalV1::ReadOnly)
        ));
        assert_eq!(std::fs::read_to_string(path).unwrap(), "# Field\n");
    }

    #[test]
    fn read_only_scratch_is_an_explicit_immutable_projection() {
        let session = KnotDocumentSession::read_only("memory:field", "# Field\n");
        let snapshot = session.snapshot();
        assert_eq!(snapshot.source.kind, KnotDocumentSourceKindV1::Scratch);
        assert_eq!(snapshot.write_posture, KnotDocumentWritePostureV1::ReadOnly);
        assert!(!snapshot.dirty);
    }
}
