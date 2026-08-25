//! Cambium-backed source editing with a derived Knot readout.

use std::fs;
use std::path::{Path, PathBuf};

use cambium::{CaretSelection, TextCommand, TextInput};
use illume::{Fold, OutlineItem, Span};
use inker::EngineDocument;
pub use knot_editor_host::EditOutcome;
use knot_editor_host::KnotEditor as SharedKnotEditor;

use crate::writer::{DocumentFormat, SaveOutcome, file_address, write_if_distinct};

/// One Djot or legacy `.knot` editor session.
///
/// Cambium's [`TextInput`] owns the sole source buffer. Highlighting, outline,
/// folds, and preview are re-derived by the shared editor model.
pub struct KnotEditor {
    path: Option<PathBuf>,
    address: String,
    format: DocumentFormat,
    editor: SharedKnotEditor,
}

impl KnotEditor {
    /// Open a file-backed Djot or legacy Knot source.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, String> {
        let path = path.into();
        let format = DocumentFormat::from_path(&path)
            .filter(|format| matches!(format, DocumentFormat::Knot | DocumentFormat::Djot))
            .ok_or_else(|| {
                format!(
                    "KnotEditor requires a .djot or .knot file: {}",
                    path.display()
                )
            })?;
        let original = fs::read(&path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        let source = String::from_utf8(original)
            .map_err(|error| format!("{} is not UTF-8: {error}", path.display()))?;
        let address = file_address(&path)?;
        Ok(Self {
            path: Some(path),
            editor: SharedKnotEditor::scratch(address.clone(), source),
            address,
            format,
        })
    }

    /// Start an unsaved editor with a caller-selected address.
    pub fn scratch(address: impl Into<String>, source: impl Into<String>) -> Self {
        let source = source.into();
        let address = address.into();
        Self {
            path: None,
            editor: SharedKnotEditor::scratch(address.clone(), source),
            address,
            format: DocumentFormat::Djot,
        }
    }

    pub fn input(&self) -> &TextInput {
        self.editor.input()
    }

    pub fn input_mut(&mut self) -> &mut TextInput {
        self.editor.input_mut()
    }

    pub fn source(&self) -> &str {
        self.editor.source()
    }

    /// The stable source address used by the editor readout.
    pub fn address(&self) -> &str {
        &self.address
    }

    pub fn format(&self) -> DocumentFormat {
        self.format
    }

    pub fn selection(&self) -> CaretSelection {
        self.editor.selection()
    }

    /// Apply a logical edit, motion, undo, or IME command through Cambium's
    /// single mutation path.
    pub fn apply(&mut self, command: TextCommand) -> EditOutcome {
        self.editor.apply(command)
    }

    /// Apply the byte-plus-affinity selection returned by a layout host.
    pub fn apply_layout_selection(&mut self, selection: CaretSelection) -> EditOutcome {
        self.apply(TextCommand::SetSelection(selection))
    }

    pub fn highlights(&self) -> Vec<Span> {
        self.editor.highlights()
    }

    pub fn outline(&self) -> Vec<OutlineItem> {
        self.editor.outline()
    }

    pub fn folds(&self) -> Vec<Fold> {
        self.editor.folds()
    }

    pub fn preview(&self) -> Result<EngineDocument, String> {
        self.editor.preview()
    }

    pub fn is_dirty(&self) -> bool {
        self.editor.is_dirty()
    }

    /// Write the committed source bytes back to the opened file.
    pub fn save(&mut self) -> Result<SaveOutcome, String> {
        let path = self
            .path
            .as_deref()
            .ok_or_else(|| "scratch Knot editor has no save path".to_string())?;
        if !self.editor.is_dirty() {
            return Ok(SaveOutcome::Unchanged);
        }
        let source = self.editor.source().to_owned();
        let existing = fs::read(path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        let outcome = write_if_distinct(path, &existing, source.as_bytes())?;
        self.editor.accept_saved_source(&source);
        Ok(outcome)
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use cambium::{CaretAffinity, CaretPosition};
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn commands_drive_the_one_source_used_by_every_readout() {
        let mut editor = KnotEditor::scratch("memory:note", "# One\n");
        assert_eq!(editor.outline().len(), 1);

        let outcome = editor.apply(TextCommand::Insert("\n## Two\n".into()));
        assert_eq!(
            outcome,
            EditOutcome {
                state_changed: true,
                source_changed: true,
            }
        );
        assert_eq!(editor.outline().len(), 2);
        assert!(!editor.highlights().is_empty());
        assert!(!editor.preview().unwrap().blocks.is_empty());

        editor.apply(TextCommand::Undo);
        assert_eq!(editor.source(), "# One\n");
        assert_eq!(editor.outline().len(), 1);
    }

    #[test]
    fn layout_selection_preserves_byte_affinity() {
        let mut editor = KnotEditor::scratch("memory:note", "abc");
        let selection = CaretSelection {
            anchor: CaretPosition {
                byte: 0,
                affinity: CaretAffinity::Downstream,
            },
            focus: CaretPosition {
                byte: 2,
                affinity: CaretAffinity::Upstream,
            },
        };
        editor.apply_layout_selection(selection);
        assert_eq!(editor.selection(), selection);
    }

    #[test]
    fn ime_preedit_is_not_committed_or_fed_to_the_readout() {
        let mut editor = KnotEditor::scratch("memory:note", "# One\n");
        let before = editor.preview().unwrap();
        let outcome = editor.apply(TextCommand::SetComposition {
            text: "仮".into(),
            selection: Some((3, 3)),
        });
        assert!(outcome.state_changed);
        assert!(!outcome.source_changed);
        assert_eq!(editor.source(), "# One\n");
        assert_eq!(editor.preview().unwrap(), before);
        assert_eq!(editor.input().render_text(), "# One\n仮");
    }

    #[test]
    fn committed_commands_write_back_to_the_native_file() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("note.knot");
        fs::write(&path, "# One\n").unwrap();
        let mut editor = KnotEditor::open(&path).unwrap();
        editor.apply(TextCommand::Insert("\n## Two\n".into()));
        assert!(editor.is_dirty());
        assert_eq!(editor.save().unwrap(), SaveOutcome::Written);
        assert_eq!(editor.save().unwrap(), SaveOutcome::Unchanged);
        assert_eq!(fs::read_to_string(path).unwrap(), "# One\n\n## Two\n");
    }
}
