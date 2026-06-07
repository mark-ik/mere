use petgraph::Direction;

pub const LAYOUT_ID_DEFAULT: &str = "layout:default";
pub const LAYOUT_ID_GRID: &str = "layout:grid";

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum LayoutMode {
    Free,
    Grid {
        gap: f32,
    },
    Tree {
        direction: Direction,
        layer_gap: f32,
    },
}

#[derive(Debug, Clone)]
pub struct LayoutResolution {
    pub requested_id: String,
    pub resolved_id: String,
    pub matched: bool,
    pub fallback_used: bool,
    pub layout: LayoutMode,
}

pub fn resolve_layout_mode(layout_id: &str) -> LayoutResolution {
    let requested = layout_id.trim().to_ascii_lowercase();
    let fallback_layout = LayoutMode::Free;

    if requested.is_empty() {
        return LayoutResolution {
            requested_id: requested,
            resolved_id: LAYOUT_ID_DEFAULT.to_string(),
            matched: false,
            fallback_used: true,
            layout: fallback_layout,
        };
    }

    let layout = match requested.as_str() {
        LAYOUT_ID_DEFAULT => Some(LayoutMode::Free),
        LAYOUT_ID_GRID => Some(LayoutMode::Grid { gap: 48.0 }),
        _ => None,
    };

    if let Some(layout) = layout {
        return LayoutResolution {
            requested_id: requested.clone(),
            resolved_id: requested,
            matched: true,
            fallback_used: false,
            layout,
        };
    }

    LayoutResolution {
        requested_id: requested,
        resolved_id: LAYOUT_ID_DEFAULT.to_string(),
        matched: false,
        fallback_used: true,
        layout: fallback_layout,
    }
}

pub fn layout_mode_id(layout: &LayoutMode) -> &'static str {
    if matches!(layout, LayoutMode::Grid { gap } if (*gap - 48.0).abs() < f32::EPSILON) {
        LAYOUT_ID_GRID
    } else {
        LAYOUT_ID_DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_mode_lookup_falls_back_for_unknown_id() {
        let resolution = resolve_layout_mode("layout:unknown");

        assert!(!resolution.matched);
        assert!(resolution.fallback_used);
        assert_eq!(resolution.resolved_id, LAYOUT_ID_DEFAULT);
        assert!(matches!(resolution.layout, LayoutMode::Free));
    }
}
