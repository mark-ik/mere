//! The persona picker: one list, one convention, every Merely application.
//!
//! Turnstone, Woodshed, Knot, Hocket and the rest all ask the same question at
//! startup — which persona am I? — and they all answer it against the same
//! shared vault ([`identity::roster`]). What varies is only where the list is
//! shown. So the list itself lives here: how a persona reads in a row, which
//! one is marked as in use, what a vault with nothing in it says.
//!
//! Built on Cambium's [`command_picker`], which supplies the interaction
//! (keyboard navigation, selection, dismissal) already. This crate is the
//! view-model: roster in, rows out, chosen persona back.
//!
//! The shape an application takes, illustrative rather than compile-ready:
//!
//! ```text
//! let opened = bootstrap::open_storage(&dir, Unlock::from_env())?;
//! let list = roster::read_roster(&*opened.storage, &dir, &opened.description)?;
//! let view = persona_picker(&state, &list);
//! // on PickerEvent::Chose(id): roster::remember_profile(&dir, &id)?, then reopen.
//! ```

#![warn(missing_docs)]

use cambium::{
    Action, CommandEvent, CommandItem, CommandState, GenetCtx, GenetElement, View, command_picker,
    map_action,
};
use identity::roster::{Roster, RosterEntry};
use identity::vault::ProfileId;

/// The id of the row that asks for a new persona, rather than choosing one.
///
/// A profile id could in principle collide with this, which is why the row is
/// matched by this constant and not by position: a persona literally named
/// `new-persona` would still be pickable if it existed, because the create row
/// is always appended last and the first match wins.
const CREATE_ROW_ID: &str = "\u{0}persona-picker:create";

/// What the picker reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PickerEvent {
    /// A persona was chosen. The caller decides whether to remember it
    /// ([`identity::roster::remember_profile`]) — the picker does not write to
    /// the vault.
    Chose(ProfileId),
    /// The user asked for a persona that does not exist yet. Naming it is the
    /// application's flow, not a row in a list; [`identity::roster::create_profile`]
    /// is what it calls with the name.
    CreateRequested,
    /// Dismissed without choosing.
    Dismissed,
}

impl Action for PickerEvent {}

/// A [`CommandState`] labelled for the persona list.
///
/// Ordinary [`CommandState`] otherwise, so an application that wants its own
/// label or id can build one directly.
pub fn picker_state() -> CommandState {
    CommandState::default()
        .with_label("Persona")
        .with_id("persona-picker")
}

/// The roster as picker rows: one per persona, then a row for making one.
///
/// Public because the rows are the convention worth sharing. An application
/// that shows personas somewhere other than a picker — a settings pane, a
/// menu — composes these into its own surface and gets the same reading.
///
/// The right-hand column carries the honest signal: the persona in use says so,
/// and the others report how many protocol slots they hold, which is what tells
/// two similarly-named personas apart.
pub fn roster_items(roster: &Roster) -> Vec<CommandItem> {
    let mut items: Vec<CommandItem> = roster.entries.iter().map(persona_item).collect();
    items.push(
        CommandItem::new(if roster.is_empty() {
            "Create your first persona…"
        } else {
            "New persona…"
        })
        .with_id(CREATE_ROW_ID),
    );
    items
}

fn persona_item(entry: &RosterEntry) -> CommandItem {
    let label = if entry.display_name.trim().is_empty() {
        entry.id.0.clone()
    } else {
        entry.display_name.clone()
    };
    let column = if entry.chosen {
        "in use".to_string()
    } else {
        match entry.slot_count {
            0 => String::new(),
            1 => "1 key".to_string(),
            n => format!("{n} keys"),
        }
    };
    let item = CommandItem::new(label).with_id(entry.id.0.as_str());
    if column.is_empty() {
        item
    } else {
        item.with_shortcut(column)
    }
}

/// The persona picker.
///
/// The chosen persona comes back as a [`ProfileId`] rather than an index, so a
/// roster that changes between render and activation cannot select the wrong
/// person. An activation that resolves to nothing is dropped as a
/// [`PickerEvent::Dismissed`], which is the safe direction: choosing nobody
/// beats choosing somebody else.
pub fn persona_picker(
    state: &CommandState,
    roster: &Roster,
) -> impl View<CommandState, PickerEvent, GenetCtx, Element = GenetElement> + use<> {
    let items = roster_items(roster);
    // The ids come off the rows themselves, so the mapping back cannot drift
    // from what was drawn.
    let ids: Vec<String> = items.iter().map(|item| item.id.clone()).collect();
    map_action(command_picker(state, &items), move |_state, event| {
        let CommandEvent::Activate(path) = event else {
            return PickerEvent::Dismissed;
        };
        match path.first().and_then(|index| ids.get(*index)) {
            Some(id) if id == CREATE_ROW_ID => PickerEvent::CreateRequested,
            Some(id) => PickerEvent::Chose(ProfileId(id.clone())),
            None => PickerEvent::Dismissed,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, name: &str, slots: usize, chosen: bool) -> RosterEntry {
        RosterEntry {
            id: ProfileId(id.into()),
            display_name: name.into(),
            slot_count: slots,
            chosen,
        }
    }

    fn roster(entries: Vec<RosterEntry>) -> Roster {
        let chosen = entries
            .iter()
            .find(|e| e.chosen)
            .map(|e| e.id.clone())
            .unwrap_or(ProfileId("default".into()));
        Roster {
            entries,
            chosen,
            description: "test storage".into(),
        }
    }

    #[test]
    fn the_persona_in_use_says_so_and_the_others_report_their_keys() {
        let items = roster_items(&roster(vec![
            entry("work", "Work", 3, true),
            entry("alt", "Late Night Alt", 1, false),
            entry("bare", "Bare", 0, false),
        ]));
        assert_eq!(items[0].shortcut.as_deref(), Some("in use"));
        assert_eq!(items[1].shortcut.as_deref(), Some("1 key"));
        assert_eq!(
            items[2].shortcut, None,
            "a persona with no keys says nothing"
        );
    }

    #[test]
    fn a_persona_without_a_display_name_falls_back_to_its_id() {
        let items = roster_items(&roster(vec![entry("work", "   ", 0, false)]));
        assert_eq!(items[0].label, "work");
    }

    #[test]
    fn every_row_carries_its_profile_id_so_activation_resolves_by_identity() {
        // The property the index-free mapping rests on: rows are addressed by
        // who they are, not by where they sit.
        let items = roster_items(&roster(vec![
            entry("work", "Work", 0, true),
            entry("alt", "Alt", 0, false),
        ]));
        let ids: Vec<&str> = items.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(ids, ["work", "alt", CREATE_ROW_ID]);
    }

    #[test]
    fn a_fresh_vault_offers_the_first_persona_rather_than_an_empty_list() {
        let items = roster_items(&roster(Vec::new()));
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "Create your first persona…");
        assert_eq!(items[0].id, CREATE_ROW_ID);
    }

    #[test]
    fn a_persona_named_like_the_create_row_is_still_pickable() {
        // The create row is matched by a sentinel id holding a NUL, which a
        // profile id from the vault cannot be. Belt and braces: it is appended
        // last, and the first match wins.
        let items = roster_items(&roster(vec![entry(
            "new-persona",
            "New persona…",
            0,
            false,
        )]));
        assert_ne!(items[0].id, CREATE_ROW_ID);
        assert_eq!(items[1].id, CREATE_ROW_ID);
    }
}
