//! Measurements — the input side of "the representation measures content;
//! the projection places it."
//!
//! A host measures what each source object needs at each representation
//! rung; solvers read measurements to assign extents and spacing. This is
//! the contract half that fixes proof 1's overlapping spiral: a solver that
//! can see a 40px card clears 40px.

use serde::{Deserialize, Serialize};

use crate::geometry::Size2;
use crate::scene::SourceIx;

/// What one source object needs, as measured by its representation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Measurement {
    /// The size the content wants at its current representation rung.
    pub intrinsic: Size2,
    /// The smallest size the content stays legible at; `None` = shrinkable
    /// to nothing (glyph-degradable).
    pub min: Option<Size2>,
}

/// Per-source measurements for one projection pass. Sources absent from the
/// map are zero-extent (point footprints).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Measurements {
    pub by_source: Vec<(SourceIx, Measurement)>,
}

impl Measurements {
    pub fn get(&self, source: SourceIx) -> Option<&Measurement> {
        self.by_source
            .iter()
            .find(|(s, _)| *s == source)
            .map(|(_, m)| m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_source_is_unmeasured() {
        let m = Measurements::default();
        assert!(m.get(SourceIx(3)).is_none());
    }
}
