# gloss

Gloss domain layer for the [mere](https://crates.io/crates/mere) browser: the
host-neutral vocabulary and geometry behind the gloss pane (minimap, outline,
recent). Package `mere-gloss`, library `gloss`.

## Types

| Item | Contents |
| --- | --- |
| `GlossOutlineSnapshot` | `rows: Vec<GlossOutlineRow>`, `metrics: glossary::GraphMetrics` |
| `GlossOutlineRow` | `depth`, `label`, `node: Option<GlossOutlineNode>` |
| `GlossOutlineNode` | `member: GraphMemberId`, `url`, `state: canvas::NodeState`, `selected` |
| `GlossRowIntent` | `Select(String)`, the intent a row queues on click |
| `MinimapFit` | Fit transform; `compute(&[(f32, f32)], w, h) -> Option<Self>` and `apply((f32, f32)) -> (f32, f32)` |

## Functions

| Item | Role |
| --- | --- |
| `build_outline_snapshot(&Graph, impl FnMut(GraphMemberId) -> (NodeState, bool), available_height) -> GlossOutlineSnapshot` | Enriches `glossary::outline_rows` with caller-supplied node state and selection, then caps to the pane height. |
| `cap_outline_rows(Vec<GlossOutlineRow>, available_height) -> Vec<GlossOutlineRow>` | Drops rows past depth 8, truncates to the row budget, and appends a `"+{n} more"` summary row when anything was hidden. |
| `gloss_sections([f32; 4]) -> ([f32; 4], [f32; 4], [f32; 4])` | Splits the pane rect into minimap, outline, and recent rects, top to bottom. |
| `minimap_node_size(selected: bool, size_factor: f32) -> f32` | Edge length of a minimap node square. |
| `minimap_backdrop_scene(edges, rings, w, h, &ChromeTheme) -> netrender::Scene` | Strokes minimap edges and signal rings into a scene. |
| `theme_rgb_css(Color32) -> String` | A chrome token as a CSS `rgb(...)` string. |
| `project_outline(&EngineDocument) -> uxtree::UxTree` | Projects a document's headings into a subtree rooted at a `Role::Group` labeled `"Outline"`. |

Constants: `OUTLINE_ROW_H` (22.0), `OUTLINE_HEADER_H` (18.0), plus `VERSION` and
`STAGE` (`"pre-alpha"`).

`project_outline` ids come from `uxtree::node_id_for_path` over
`gloss/outline/{address}` and `gloss/outline/{address}/heading/{block_index}`.

## Dependencies

| Crate | Why |
| --- | --- |
| `kernel` | `graph::Graph`, the outline source. |
| `glossary` | `outline_rows`, `graph_metrics`, `GraphMetrics`. |
| `canvas` | `NodeState`. |
| `forme` | `GraphMemberId`. |
| `inker` (git, genet `main`) | `EngineDocument`, `Block`, `inline_text`. |
| `netrender` (git) | `Scene`, `ScenePath` for the minimap backdrop. |
| `register-theme` | `chrome::{ChromeTheme, Color32}`. |
| `accesskit`, `uxtree` | The outline projection. |
| `tracing` | Debug spans. |

## Status

Pre-1.0. Snapshot building and event application for a live pane stay in the
host; this crate holds the portable vocabulary and geometry.
