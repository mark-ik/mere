use std::fmt::Write;

use base64::Engine;
use graphshell_client::{ResolvedContent, ResolvedPresentation};
use graphshell_protocol::{AdvertisedAction, SceneSnapshot, SemanticRole};

use crate::canary::{CanaryError, CanaryRun, run_loopback_canary};

/// One intent outcome shown beside a headed projection receipt.
pub struct IntentReceiptView {
    pub label: String,
    pub result: String,
    pub detail: String,
}

/// One scene item's world-space origin, in explicit item order.
pub struct ScenePlacementView {
    pub x: f32,
    pub y: f32,
}

/// A relation between indexes in [`ProjectionLayoutView::placements`].
pub struct SceneRelationView {
    pub from: usize,
    pub to: usize,
}

/// The spatial part of a disclosed Scenograph scene, kept separate from the
/// resolved presentation payloads that fill each placed item.
pub struct ProjectionLayoutView {
    pub placements: Vec<ScenePlacementView>,
    pub relations: Vec<SceneRelationView>,
}

impl ProjectionLayoutView {
    pub fn from_scene(scene: &SceneSnapshot) -> Self {
        let items = scene.active_items_in_order();
        let by_instance = items
            .iter()
            .enumerate()
            .map(|(index, (instance, _))| (instance.0, index))
            .collect::<std::collections::HashMap<_, _>>();
        let placements = items
            .into_iter()
            .map(|(_, item)| {
                let origin = world_origin(scene, item.space, item.transform.translate)
                    .unwrap_or(item.transform.translate);
                ScenePlacementView {
                    x: origin.x,
                    y: origin.y,
                }
            })
            .collect();
        let relations = scene
            .tables
            .relations
            .iter()
            .flatten()
            .filter_map(|relation| {
                Some(SceneRelationView {
                    from: *by_instance.get(&relation.from.0)?,
                    to: *by_instance.get(&relation.to.0)?,
                })
            })
            .collect();
        Self {
            placements,
            relations,
        }
    }
}

fn world_origin(
    scene: &SceneSnapshot,
    mut space: sceno::SpaceId,
    mut point: sceno::Vec2,
) -> Option<sceno::Vec2> {
    for _ in 0..=scene.tables.spaces.len() {
        let current = scene.tables.spaces.get(space.0 as usize)?.as_ref()?;
        point = current.transform.apply(point);
        match current.parent {
            Some(parent) => space = parent,
            None => return Some(point),
        }
    }
    None
}

/// Product-neutral data for a single live projection receipt.
pub struct ProjectionReceiptView {
    pub eyebrow: String,
    pub title: String,
    pub lede: String,
    pub session: String,
    pub status: String,
    pub presentations: Vec<ResolvedPresentation>,
    pub layout: Option<ProjectionLayoutView>,
    pub intents: Vec<IntentReceiptView>,
}

/// Render one Graphshell projection plus endpoint intent receipts as native,
/// responsive semantic HTML.
pub fn render_projection_receipt(view: &ProjectionReceiptView) -> String {
    let mut html = String::from(
        r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Graphshell projection receipt</title>
<style>
:root{color-scheme:dark;--ink:#f4eedf;--muted:#9eacb4;--line:#314756;--deep:#09141d;--panel:#10212c;--gold:#d8a657;--ok:#65bd85;--bad:#e46d5c}*{box-sizing:border-box}body{margin:0;min-height:100vh;background:radial-gradient(circle at 12% 0%,#193448 0,transparent 38%),linear-gradient(150deg,#071019,#0b1720 50%,#0d1b24);color:var(--ink);font:15px/1.5 Inter,ui-sans-serif,system-ui,-apple-system,"Segoe UI",sans-serif}.shell{width:min(1100px,calc(100% - 40px));margin:0 auto;padding:48px 0 64px}header{display:grid;grid-template-columns:1fr auto;gap:24px;align-items:end;margin-bottom:26px}.eyebrow{margin:0 0 9px;color:var(--gold);font-size:12px;font-weight:800;letter-spacing:.16em;text-transform:uppercase}h1{margin:0;font-size:clamp(32px,5vw,54px);letter-spacing:-.04em;line-height:1.03}.lede{max-width:720px;margin:14px 0 0;color:#b9c5ca;font-size:17px}.session{align-self:start;border:1px solid var(--line);border-radius:999px;padding:8px 13px;color:#b9c5ca;background:#0a1821;font:12px ui-monospace,SFMono-Regular,Consolas,monospace}.projection{border:1px solid var(--line);border-radius:22px;background:rgba(13,29,39,.96);box-shadow:0 18px 55px rgba(0,0,0,.25);overflow:hidden}.projection-head{display:flex;justify-content:space-between;gap:16px;padding:18px 22px;border-bottom:1px solid var(--line)}.status{color:#b9d5c5;font-size:12px}.scene{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:14px;padding:18px}.scene.positioned{position:relative;display:block;height:760px;padding:0;background:radial-gradient(circle at 50% 48%,rgba(44,79,99,.24),transparent 54%),linear-gradient(rgba(69,100,116,.08) 1px,transparent 1px),linear-gradient(90deg,rgba(69,100,116,.08) 1px,transparent 1px);background-size:auto,32px 32px,32px 32px}.routes{position:absolute;inset:0;width:100%;height:100%;pointer-events:none}.routes line{stroke:#527487;stroke-width:.45;vector-effect:non-scaling-stroke}.placed{position:absolute;width:42%;transform:translate(-50%,-50%);z-index:1}.placed .item{box-shadow:0 14px 35px rgba(0,0,0,.28)}.item{position:relative;min-width:0;min-height:176px;border:1px solid #38505f;border-radius:16px;background:var(--panel);overflow:hidden}.card{padding:21px}.card-top{display:flex;justify-content:space-between;gap:14px}.card-kicker{color:#79a9be;font-size:11px;font-weight:800;letter-spacing:.13em;text-transform:uppercase}.card h3,.glyph h3,.placeholder h3{margin:4px 0 14px;font-size:21px}.badges,.actions{display:flex;flex-wrap:wrap;gap:7px}.badge{border:1px solid #415b6b;border-radius:999px;padding:4px 8px;color:#c4d2d8;background:#132a37;font-size:11px}dl{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:12px;margin:18px 0}dl div{border-left:2px solid #34566a;padding-left:10px}dt{color:var(--muted);font-size:11px;text-transform:uppercase}dd{margin:3px 0 0;font-weight:650;overflow-wrap:anywhere}.glyph{display:grid;grid-template-columns:64px 1fr;gap:15px;align-items:center;padding:24px}.glyph-mark{display:grid;place-items:center;width:64px;height:64px;border:1px solid #7a653e;border-radius:20px;color:var(--gold);font-size:32px}.image{display:flex;flex-direction:column}.image img{display:block;width:100%;height:176px;object-fit:cover}.image-meta{display:flex;justify-content:space-between;gap:12px;padding:12px 14px}.placeholder{display:grid;place-items:center;text-align:center;padding:26px}.placeholder-mark{font-size:32px;color:#718795}button{border:1px solid #566f7e;border-radius:10px;padding:8px 11px;background:#1a3443;color:var(--ink);font:inherit;font-size:12px;font-weight:700}.receipts{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:12px;margin-top:18px}.receipt{border:1px solid var(--line);border-radius:14px;padding:15px;background:#0b1922}.receipt strong{display:block}.receipt p{margin:5px 0 0;color:var(--muted);font-size:13px}.accepted{color:var(--ok)}.rejected{color:var(--bad)}@media(max-width:720px){.shell{width:min(100% - 24px,620px);padding-top:28px}header{grid-template-columns:1fr}.session{justify-self:start}.scene,.receipts{grid-template-columns:1fr}.scene.positioned{display:grid;height:auto;padding:18px;background:transparent}.scene.positioned .routes{display:none}.scene.positioned .placed{position:relative;left:auto!important;top:auto!important;width:auto;transform:none}.scene.positioned .placed .item{box-shadow:none}}
</style>
</head>
<body>
<main class="shell">
"##,
    );
    write!(
        html,
        "<header><div><p class=\"eyebrow\">{}</p><h1>{}</h1><p class=\"lede\">{}</p></div><div class=\"session\">{}</div></header>",
        escape(&view.eyebrow),
        escape(&view.title),
        escape(&view.lede),
        escape(&view.session),
    )
    .unwrap();
    let normalized = view.layout.as_ref().and_then(|layout| {
        (layout.placements.len() == view.presentations.len())
            .then(|| normalized_positions(&layout.placements))
    });
    let scene_class = if normalized.is_some() {
        "scene positioned"
    } else {
        "scene"
    };
    write!(
        html,
        "<section class=\"projection\"><div class=\"projection-head\"><strong>Disclosed scene</strong><span class=\"status\">{}</span></div><div class=\"{}\">",
        escape(&view.status),
        scene_class,
    )
    .unwrap();
    if let (Some(layout), Some(positions)) = (&view.layout, &normalized) {
        html.push_str("<svg class=\"routes\" viewBox=\"0 0 100 100\" preserveAspectRatio=\"none\" aria-hidden=\"true\">");
        for relation in &layout.relations {
            let (Some(from), Some(to)) = (positions.get(relation.from), positions.get(relation.to))
            else {
                continue;
            };
            write!(
                html,
                "<line x1=\"{:.3}\" y1=\"{:.3}\" x2=\"{:.3}\" y2=\"{:.3}\"></line>",
                from.0, from.1, to.0, to.1
            )
            .unwrap();
        }
        html.push_str("</svg>");
        for (presentation, (x, y)) in view.presentations.iter().zip(positions) {
            write!(
                html,
                "<div class=\"placed\" style=\"left:{x:.3}%;top:{y:.3}%\">"
            )
            .unwrap();
            render_item(&mut html, presentation);
            html.push_str("</div>");
        }
    } else {
        for presentation in &view.presentations {
            render_item(&mut html, presentation);
        }
    }
    html.push_str("</div></section><section class=\"receipts\" aria-label=\"Intent receipts\">");
    for receipt in &view.intents {
        let class = if receipt.result.eq_ignore_ascii_case("accepted") {
            "accepted"
        } else {
            "rejected"
        };
        write!(
            html,
            "<article class=\"receipt\"><strong>{}</strong><span class=\"{}\">{}</span><p>{}</p></article>",
            escape(&receipt.label),
            class,
            escape(&receipt.result),
            escape(&receipt.detail),
        )
        .unwrap();
    }
    html.push_str("</section></main></body></html>\n");
    html
}

fn normalized_positions(placements: &[ScenePlacementView]) -> Vec<(f32, f32)> {
    let finite: Vec<_> = placements
        .iter()
        .map(|placement| {
            (
                if placement.x.is_finite() {
                    placement.x
                } else {
                    0.0
                },
                if placement.y.is_finite() {
                    placement.y
                } else {
                    0.0
                },
            )
        })
        .collect();
    let (min_x, max_x) = finite
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), (x, _)| {
            (min.min(*x), max.max(*x))
        });
    let (min_y, max_y) = finite
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), (_, y)| {
            (min.min(*y), max.max(*y))
        });
    let x_span = max_x - min_x;
    let y_span = max_y - min_y;
    finite
        .into_iter()
        .map(|(x, y)| {
            (
                normalize_axis(x, min_x, x_span, 24.0, 76.0),
                normalize_axis(y, min_y, y_span, 20.0, 80.0),
            )
        })
        .collect()
}

fn normalize_axis(value: f32, min: f32, span: f32, low: f32, high: f32) -> f32 {
    if span.abs() <= f32::EPSILON {
        (low + high) * 0.5
    } else {
        low + ((value - min) / span) * (high - low)
    }
}

pub fn render_g1_receipt() -> Result<String, CanaryError> {
    run_loopback_canary().map(|run| render_canary_html(&run))
}

pub fn render_canary_html(run: &CanaryRun) -> String {
    let mut html = String::from(
        r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Graphshell G1 · loopback presentation proof</title>
<style>
:root{color-scheme:dark;--ink:#f4eedf;--muted:#9eacb4;--line:#314756;--deep:#09141d;--panel:#10212c;--panel2:#142a37;--gold:#d8a657;--coral:#e46d5c;--sea:#79a9be}*{box-sizing:border-box}body{margin:0;min-height:100vh;background:radial-gradient(circle at 12% 0%,#193448 0,transparent 38%),linear-gradient(150deg,#071019,#0b1720 50%,#0d1b24);color:var(--ink);font:15px/1.5 Inter,ui-sans-serif,system-ui,-apple-system,"Segoe UI",sans-serif}.shell{width:min(1180px,calc(100% - 40px));margin:0 auto;padding:54px 0 64px}header{display:grid;grid-template-columns:1fr auto;gap:24px;align-items:end;margin-bottom:28px}.eyebrow{margin:0 0 9px;color:var(--gold);font-size:12px;font-weight:800;letter-spacing:.16em;text-transform:uppercase}h1{margin:0;font-size:clamp(31px,5vw,56px);font-weight:720;letter-spacing:-.045em;line-height:1.02}.lede{max-width:680px;margin:15px 0 0;color:#b9c5ca;font-size:17px}.session{align-self:start;border:1px solid var(--line);border-radius:999px;padding:8px 13px;color:#b9c5ca;background:#0a1821;font:12px ui-monospace,SFMono-Regular,Consolas,monospace}.profiles{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:20px}.profile{min-width:0;border:1px solid var(--line);border-radius:22px;background:linear-gradient(180deg,rgba(20,42,55,.96),rgba(13,29,39,.96));box-shadow:0 18px 55px rgba(0,0,0,.25);overflow:hidden}.profile-head{display:flex;justify-content:space-between;gap:16px;align-items:start;padding:20px 22px;border-bottom:1px solid var(--line);background:rgba(8,20,28,.5)}.profile h2{margin:0;font-size:19px;letter-spacing:-.015em}.profile p{margin:4px 0 0;color:var(--muted);font-size:13px}.status{display:inline-flex;align-items:center;gap:7px;color:#b9d5c5;font-size:12px;white-space:nowrap}.status:before{content:"";width:8px;height:8px;border-radius:50%;background:#65bd85;box-shadow:0 0 0 4px rgba(101,189,133,.12)}.scene{display:grid;grid-template-columns:1fr;gap:14px;padding:18px;min-height:430px}.item{position:relative;min-height:176px;border:1px solid #38505f;border-radius:16px;background:var(--panel);overflow:hidden}.card{padding:21px}.card-top{display:flex;justify-content:space-between;gap:14px;align-items:start}.card-kicker{color:var(--sea);font-size:11px;font-weight:800;letter-spacing:.13em;text-transform:uppercase}.card h3{margin:4px 0 16px;font-size:22px;letter-spacing:-.025em}.badges{display:flex;flex-wrap:wrap;gap:7px}.badge{border:1px solid #415b6b;border-radius:999px;padding:4px 8px;color:#c4d2d8;background:#132a37;font-size:11px}dl{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:12px;margin:18px 0}dl div{border-left:2px solid #34566a;padding-left:10px}dt{color:var(--muted);font-size:11px;text-transform:uppercase;letter-spacing:.08em}dd{margin:3px 0 0;font-weight:650}.glyph{display:grid;grid-template-columns:64px 1fr;gap:15px;align-items:center;padding:24px}.glyph-mark{display:grid;place-items:center;width:64px;height:64px;border:1px solid #7a653e;border-radius:20px;background:radial-gradient(circle at 35% 30%,#4d4028,#201d19);color:var(--gold);font-size:32px}.glyph h3{margin:0;font-size:19px}.glyph p{margin:5px 0 0}.image{display:flex;flex-direction:column}.image img{display:block;width:100%;height:176px;object-fit:cover;background:#163044}.image-meta{display:flex;justify-content:space-between;gap:12px;align-items:center;padding:12px 14px}.placeholder{display:grid;place-items:center;text-align:center;padding:26px;border-style:dashed;background:repeating-linear-gradient(135deg,#10212c,#10212c 11px,#122633 11px,#122633 22px)}.placeholder-mark{font-size:32px;color:#718795}.placeholder h3{margin:7px 0 2px}.placeholder p{max-width:260px}.actions{display:flex;flex-wrap:wrap;gap:8px;margin-top:15px}button{appearance:none;border:1px solid #566f7e;border-radius:10px;padding:8px 11px;background:#1a3443;color:var(--ink);font:inherit;font-size:12px;font-weight:700;cursor:pointer}button:hover,button:focus-visible{border-color:var(--gold);outline:2px solid rgba(216,166,87,.3);outline-offset:2px}.proof{display:flex;flex-wrap:wrap;gap:9px;margin-top:22px}.proof span{border:1px solid #2c4351;border-radius:9px;padding:7px 10px;background:#0b1922;color:#9fb0b8;font-size:12px}.proof strong{color:#dbe5e8}@media(max-width:760px){.shell{width:min(100% - 24px,620px);padding-top:30px}header{grid-template-columns:1fr}.session{justify-self:start}.profiles{grid-template-columns:1fr}.scene{min-height:0}}
</style>
</head>
<body>
<main class="shell">
<header>
<div><p class="eyebrow">Graphshell · G1 receipt</p><h1>One scene,<br>two capabilities.</h1><p class="lede">A loopback endpoint discloses one Scenograph scene. Graphshell resolves its presentation resources locally, preserving labels and actions as richer codecs fall away.</p></div>
<div class="session">loopback:g1-presentation · rev 1</div>
</header>
<div class="profiles">
"##,
    );
    render_profile(
        &mut html,
        "Rich profile",
        "Card, glyph, and image codecs",
        &run.rich,
    );
    render_profile(
        &mut html,
        "Compact profile",
        "Native glyph codec only",
        &run.compact,
    );
    html.push_str(
        r##"</div>
<div class="proof" aria-label="Proof conditions"><span><strong>Scene:</strong> product-free</span><span><strong>Resources:</strong> content-addressed</span><span><strong>Cache:</strong> session-scoped</span><span><strong>Fallback:</strong> labeled</span><span><strong>Actions:</strong> keyboard reachable</span></div>
</main>
</body>
</html>
"##,
    );
    html
}

fn render_profile(
    html: &mut String,
    title: &str,
    subtitle: &str,
    presentations: &[ResolvedPresentation],
) {
    write!(
        html,
        "<section class=\"profile\" aria-label=\"{}\"><div class=\"profile-head\"><div><h2>{}</h2><p>{}</p></div><span class=\"status\">Live</span></div><div class=\"scene\">",
        escape(title),
        escape(title),
        escape(subtitle)
    )
    .unwrap();
    for presentation in presentations {
        render_item(html, presentation);
    }
    html.push_str("</div></section>\n");
}

fn render_item(html: &mut String, presentation: &ResolvedPresentation) {
    let role = semantic_role(presentation.semantics.role);
    match &presentation.content {
        ResolvedContent::PortableCard(card) => {
            write!(
                html,
                "<article class=\"item card\" aria-label=\"{}\"><div class=\"card-top\"><div><span class=\"card-kicker\">Portable card</span><h3>{}</h3></div><div class=\"badges\">",
                escape(&presentation.semantics.label),
                escape(&card.title)
            )
            .unwrap();
            for badge in &card.badges {
                write!(html, "<span class=\"badge\">{}</span>", escape(badge)).unwrap();
            }
            html.push_str("</div></div><dl>");
            for value in &card.values {
                write!(
                    html,
                    "<div><dt>{}</dt><dd>{}</dd></div>",
                    escape(&value.label),
                    escape(&value.value)
                )
                .unwrap();
            }
            html.push_str("</dl>");
            render_actions(html, &presentation.semantics.actions);
            html.push_str("</article>");
        }
        ResolvedContent::NativeGlyph(glyph) => {
            write!(
                html,
                "<div class=\"item glyph\" role=\"{}\" aria-label=\"{}\"><div class=\"glyph-mark\" aria-hidden=\"true\">{}</div><div><h3>{}</h3><p>Native Graphshell glyph</p>",
                role,
                escape(&presentation.semantics.label),
                escape(glyph.icon.as_deref().unwrap_or("•")),
                escape(&glyph.label)
            )
            .unwrap();
            render_actions(html, &presentation.semantics.actions);
            html.push_str("</div></div>");
        }
        ResolvedContent::Image { mime_type, bytes } => {
            let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
            write!(
                html,
                "<div class=\"item image\" role=\"{}\" aria-label=\"{}\"><img alt=\"{}\" src=\"data:{};base64,{}\"><div class=\"image-meta\"><div><strong>{}</strong><p>Content-addressed image resource</p></div>",
                role,
                escape(&presentation.semantics.label),
                escape(&presentation.semantics.label),
                escape(mime_type),
                encoded,
                escape(&presentation.semantics.label)
            )
            .unwrap();
            render_actions(html, &presentation.semantics.actions);
            html.push_str("</div></div>");
        }
        ResolvedContent::LabeledPlaceholder => {
            write!(
                html,
                "<div class=\"item placeholder\" role=\"{}\" aria-label=\"{}\"><div><div class=\"placeholder-mark\" aria-hidden=\"true\">◇</div><h3>{}</h3><p>Image capability unavailable. The disclosed label and action remain.</p>",
                role,
                escape(&presentation.semantics.label),
                escape(&presentation.semantics.label)
            )
            .unwrap();
            render_actions(html, &presentation.semantics.actions);
            html.push_str("</div></div>");
        }
    }
}

fn render_actions(html: &mut String, actions: &[AdvertisedAction]) {
    if actions.is_empty() {
        return;
    }
    html.push_str("<div class=\"actions\">");
    for action in actions {
        write!(
            html,
            "<button type=\"button\" data-intent=\"{}\" title=\"{}\">{}</button>",
            escape(&action.intent.0),
            escape(&action.explanation),
            escape(&action.label)
        )
        .unwrap();
    }
    html.push_str("</div>");
}

fn semantic_role(role: SemanticRole) -> &'static str {
    match role {
        SemanticRole::Graphic => "img",
        SemanticRole::Article => "article",
        SemanticRole::Image => "img",
    }
}

fn escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receipt_contains_both_resolutions_and_real_actions() {
        let html = render_g1_receipt().unwrap();
        assert!(html.contains("Portable card"));
        assert!(html.contains("Native Graphshell glyph"));
        assert!(html.contains("data:image/svg+xml;base64,"));
        assert!(html.contains("Image capability unavailable"));
        assert_eq!(html.matches("data-intent=\"fixture.open-note\"").count(), 2);
        assert_eq!(
            html.matches("data-intent=\"fixture.inspect-tile\"").count(),
            2
        );
    }

    #[test]
    fn committed_receipt_matches_the_live_loopback_view() {
        assert_eq!(
            render_g1_receipt().unwrap(),
            include_str!("../../../docs/receipts/g1_loopback.html")
        );
    }

    #[test]
    fn generic_projection_receipt_keeps_items_and_intent_outcomes_visible() {
        let run = run_loopback_canary().unwrap();
        let html = render_projection_receipt(&ProjectionReceiptView {
            eyebrow: "Graphshell".into(),
            title: "Live projection".into(),
            lede: "Endpoint-owned truth.".into(),
            session: run.session.0,
            status: "Live".into(),
            presentations: run.rich,
            layout: None,
            intents: vec![IntentReceiptView {
                label: "Change view".into(),
                result: "Accepted".into(),
                detail: "The endpoint gate allowed this intent.".into(),
            }],
        });
        assert!(html.contains("Portable card"));
        assert!(html.contains("class=\"accepted\">Accepted"));
    }

    #[test]
    fn a_single_placement_centers_instead_of_hugging_a_corner() {
        assert_eq!(
            normalized_positions(&[ScenePlacementView { x: 4.0, y: -8.0 }]),
            vec![(50.0, 50.0)]
        );
    }
}
