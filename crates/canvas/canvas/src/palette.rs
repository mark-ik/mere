// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The node accent palette: the one table every representation of a node tints
//! from, so a node reads as the same node wherever it appears (canvas gnode,
//! workbench tab, gloss outline row, status-cluster glyph). The rule is uniform:
//! **selection wins over activation state**. (Representations carry node
//! identity.)
//!
//! Two consumer shapes, one source:
//!
//! - **DOM consumers** paste [`custom_property_declarations`] into whichever
//!   rule is the root of their document or panel, then reference the colors from
//!   descendant rules with `var(--node-open-bg)` and friends. The cascade does
//!   the rest, so a representation inherits node identity without threading a
//!   color through Rust.
//! - **Imperative painters** (chisel leaves, paint commands, the tab contract)
//!   read [`accent`] and convert with [`unit`], since they paint outside the
//!   cascade and cannot resolve a `var()`.

use crate::types::NodeState;

/// A node's fill and label color for one activation state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeAccent {
    /// The tile / dot / tab fill.
    pub bg: [u8; 3],
    /// The label color that reads against `bg`.
    pub fg: [u8; 3],
}

/// Selected: amber, dark label. Wins over every activation state.
pub const SELECTED: NodeAccent = NodeAccent {
    bg: [232, 150, 40],
    fg: [28, 22, 10],
};
/// Open: a live actor is showing real fetched content.
pub const OPEN: NodeAccent = NodeAccent {
    bg: [58, 140, 94],
    fg: [238, 250, 243],
};
/// Closed: real fetched content, but no actor showing it right now.
pub const CLOSED: NodeAccent = NodeAccent {
    bg: [166, 72, 72],
    fg: [250, 240, 240],
};
/// Idle: local / settings / synthesized / blank / errored. The default fill.
pub const IDLE: NodeAccent = NodeAccent {
    bg: [54, 92, 156],
    fg: [245, 247, 252],
};

/// The canvas caption label riding beside a gnode (not a node fill, so it is not
/// a [`NodeAccent`]).
pub const CAPTION_FG: [u8; 3] = [216, 222, 234];

/// The accent for a node, applying the selection-wins rule.
pub fn accent(selected: bool, state: NodeState) -> NodeAccent {
    if selected {
        return SELECTED;
    }
    match state {
        NodeState::Open => OPEN,
        NodeState::Closed => CLOSED,
        NodeState::Idle => IDLE,
    }
}

/// The slug naming a node's accent, shared by the CSS custom-property names
/// (`--node-{slug}-bg`) and by consumers that build a modifier class from it
/// (`.gloss-outline-dot-{slug}`). Keeps the two vocabularies from drifting.
pub fn state_slug(selected: bool, state: NodeState) -> &'static str {
    if selected {
        return "selected";
    }
    match state {
        NodeState::Open => "open",
        NodeState::Closed => "closed",
        NodeState::Idle => "idle",
    }
}

/// A color as the CSS `rgb(r, g, b)` form.
pub fn rgb(c: [u8; 3]) -> String {
    format!("rgb({}, {}, {})", c[0], c[1], c[2])
}

/// A color as 0..1 components, for painters that take floats.
pub fn unit(c: [u8; 3]) -> [f32; 3] {
    [
        f32::from(c[0]) / 255.0,
        f32::from(c[1]) / 255.0,
        f32::from(c[2]) / 255.0,
    ]
}

/// The `--node-*` custom-property declarations, as a run of `name: value;` pairs
/// ready to paste inside a rule body. Put them on the root element of a document
/// (or the root of a panel) and every descendant rule can `var()` them.
pub fn custom_property_declarations() -> String {
    let mut out = String::new();
    for (slug, a) in [
        ("idle", IDLE),
        ("open", OPEN),
        ("closed", CLOSED),
        ("selected", SELECTED),
    ] {
        out.push_str(&format!(
            "--node-{slug}-bg: {}; --node-{slug}-fg: {}; ",
            rgb(a.bg),
            rgb(a.fg)
        ));
    }
    out.push_str(&format!("--node-caption-fg: {};", rgb(CAPTION_FG)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_wins_over_every_activation_state() {
        for state in [NodeState::Open, NodeState::Closed, NodeState::Idle] {
            assert_eq!(accent(true, state), SELECTED);
            assert_eq!(state_slug(true, state), "selected");
        }
    }

    #[test]
    fn each_state_maps_to_its_own_accent() {
        assert_eq!(accent(false, NodeState::Open), OPEN);
        assert_eq!(accent(false, NodeState::Closed), CLOSED);
        assert_eq!(accent(false, NodeState::Idle), IDLE);
    }

    #[test]
    fn declarations_define_every_var_the_sheets_reference() {
        let decls = custom_property_declarations();
        for slug in ["idle", "open", "closed", "selected"] {
            assert!(
                decls.contains(&format!("--node-{slug}-bg:")),
                "missing {slug} bg"
            );
            assert!(
                decls.contains(&format!("--node-{slug}-fg:")),
                "missing {slug} fg"
            );
        }
        assert!(decls.contains("--node-caption-fg:"));
    }

    #[test]
    fn declarations_carry_the_palette_values() {
        let decls = custom_property_declarations();
        assert!(decls.contains("--node-selected-bg: rgb(232, 150, 40);"));
        assert!(decls.contains("--node-open-bg: rgb(58, 140, 94);"));
        assert!(decls.contains("--node-closed-bg: rgb(166, 72, 72);"));
        assert!(decls.contains("--node-idle-bg: rgb(54, 92, 156);"));
    }

    #[test]
    fn unit_normalizes_to_zero_one() {
        assert_eq!(unit([255, 0, 255]), [1.0, 0.0, 1.0]);
        let [r, g, b] = unit(OPEN.bg);
        assert!((r - 58.0 / 255.0).abs() < 1e-6);
        assert!((g - 140.0 / 255.0).abs() < 1e-6);
        assert!((b - 94.0 / 255.0).abs() < 1e-6);
    }
}
