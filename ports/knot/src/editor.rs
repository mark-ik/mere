//! Cambium-backed source editing with a derived Knot readout.

use std::fs;
use std::path::{Path, PathBuf};

use cambium::{CaretSelection, TextCommand, TextInput};
use illume::{Fold, OutlineItem, Span};
use inker::EngineDocument;
use knot_editor_host::KnotReadout;

use crate::writer::{SaveOutcome, file_address, write_if_distinct};

/// What changed when one platform-neutral command was applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditOutcome {
    pub state_changed: bool,
    pub source_changed: bool,
}

/// One `.knot` editor session.
///
/// Cambium's [`TextInput`] owns the sole source buffer. Highlighting, outline,
/// folds, and preview are re-derived from that buffer by [`KnotReadout`].
pub struct KnotEditor {
    path: Option<PathBuf>,
    address: String,
    original: Vec<u8>,
    input: TextInput,
    readout: KnotReadout,
}

impl KnotEditor {
    /// Open a file-backed native Knot source.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, String> {
        let path = path.into();
        if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_none_or(|extension| !extension.eq_ignore_ascii_case("knot"))
        {
            return Err(format!(
                "KnotEditor requires a .knot file: {}",
                path.display()
            ));
        }
        let original = fs::read(&path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        let source = String::from_utf8(original.clone())
            .map_err(|error| format!("{} is not UTF-8: {error}", path.display()))?;
        let address = file_address(&path)?;
        Ok(Self {
            path: Some(path),
            address,
            original,
            input: TextInput::new(source),
            readout: KnotReadout::new(),
        })
    }

    /// Start an unsaved editor with a caller-selected address.
    pub fn scratch(address: impl Into<String>, source: impl Into<String>) -> Self {
        let source = source.into();
        Self {
            path: None,
            address: address.into(),
            original: source.as_bytes().to_vec(),
            input: TextInput::new(source),
            readout: KnotReadout::new(),
        }
    }

    pub fn input(&self) -> &TextInput {
        &self.input
    }

    pub fn source(&self) -> &str {
        self.input.text()
    }

    pub fn selection(&self) -> CaretSelection {
        self.input.caret_selection()
    }

    /// Apply a logical edit, motion, undo, or IME command through Cambium's
    /// single mutation path.
    pub fn apply(&mut self, command: TextCommand) -> EditOutcome {
        let before = self.input.text().to_string();
        let state_changed = self.input.apply(command);
        EditOutcome {
            state_changed,
            source_changed: self.input.text() != before,
        }
    }

    /// Apply the byte-plus-affinity selection returned by a layout host.
    pub fn apply_layout_selection(&mut self, selection: CaretSelection) -> EditOutcome {
        self.apply(TextCommand::SetSelection(selection))
    }

    pub fn highlights(&self) -> Vec<Span> {
        self.readout.highlights(self.input.text())
    }

    pub fn outline(&self) -> Vec<OutlineItem> {
        self.readout.outline(self.input.text())
    }

    pub fn folds(&self) -> Vec<Fold> {
        self.readout.folds(self.input.text())
    }

    pub fn preview(&self) -> Result<EngineDocument, String> {
        self.readout
            .rendered(&self.address, self.input.text())
            .map_err(|error| format!("could not render Knot preview: {error}"))
    }

    pub fn is_dirty(&self) -> bool {
        self.input.text().as_bytes() != self.original
    }

    /// Write the committed source bytes back to the opened file.
    pub fn save(&mut self) -> Result<SaveOutcome, String> {
        let path = self
            .path
            .as_deref()
            .ok_or_else(|| "scratch Knot editor has no save path".to_string())?;
        let bytes = self.input.text().as_bytes();
        let outcome = write_if_distinct(path, &self.original, bytes)?;
        self.original = bytes.to_vec();
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
