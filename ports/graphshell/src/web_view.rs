//! Cambium and Genet composition for the browser presenter.

use std::cell::RefCell;
use std::rc::Rc;

use cambium::{AnyView, GenetAppRunner, GenetCtx, GenetElement, View, el, text};
use genet_layout::{NoImageLoader, ScrollOffsets};
use genet_scripted_dom::ScriptedDom;
use netrender::Scene;
use paint_list_api::PaintList as _;

use graphshell::action_draft::ActionDraftSemantics;

type ChromeChild = Box<dyn AnyView<(), (), GenetCtx, GenetElement>>;

#[derive(Clone)]
pub(crate) struct ChromeModel {
    pub active_session: String,
    pub local_active: bool,
    pub selection: String,
    pub detail_open: bool,
    pub detail_address: String,
    pub action_status: String,
    pub viewport_label: String,
    pub product_status: String,
    pub arrangement: String,
    pub physics_paused: bool,
    pub action_draft: Option<ActionDraftSemantics>,
}

fn pill(label: String, active: bool) -> impl View<(), (), GenetCtx, Element = GenetElement> {
    el("div", text(label)).attr("class", if active { "pill active" } else { "pill" })
}

/// A painted projection of the same draft that supplies browser and native
/// semantics. Input reaches the browser semantic controls, while the canvas
/// chrome mirrors choices, labels, descriptions, errors, and selection state.
fn action_form_view(draft: Option<&ActionDraftSemantics>) -> ChromeChild {
    let Some(draft) = draft else {
        return Box::new(el("div", text("")).attr("class", "action-form hidden"));
    };
    let fields: Vec<ChromeChild> = draft
        .fields
        .iter()
        .map(|field| {
            let choices: Vec<ChromeChild> = field
                .choices
                .iter()
                .map(|choice| {
                    let marker = if choice.selected { "(o)" } else { "( )" };
                    let description = choice
                        .description
                        .as_deref()
                        .map(|description| format!(" · {description}"))
                        .unwrap_or_default();
                    Box::new(
                        el(
                            "div",
                            text(format!("{marker} {}{description}", choice.label)),
                        )
                        .attr(
                            "class",
                            if choice.selected {
                                "action-choice selected"
                            } else {
                                "action-choice"
                            },
                        ),
                    ) as ChromeChild
                })
                .collect();
            let required = if field.required {
                "required"
            } else {
                "optional"
            };
            Box::new(
                el(
                    "div",
                    (
                        el("div", text(format!("{} · {required}", field.label)))
                            .attr("class", "action-field-label"),
                        el("div", text(field.description.clone())).attr("class", "action-help"),
                        el("div", choices).attr("class", "action-choices"),
                    ),
                )
                .attr("class", "action-field"),
            ) as ChromeChild
        })
        .collect();
    let error = draft.error.clone().unwrap_or_default();
    Box::new(
        el(
            "div",
            (
                el("div", text(draft.label.clone())).attr("class", "action-form-title"),
                el("div", text(draft.explanation.clone())).attr("class", "action-help"),
                el("div", fields).attr("class", "action-fields"),
                el("div", text(error)).attr("class", "action-form-error"),
                el("div", text(format!("Submit · {}", draft.submit_label)))
                    .attr("class", "action-submit"),
            ),
        )
        .attr("class", "action-form"),
    )
}

fn chrome_view(model: ChromeModel) -> impl View<(), (), GenetCtx, Element = GenetElement> {
    let detail_class = if model.detail_open {
        "detail"
    } else {
        "detail hidden"
    };
    el(
        "div",
        (
            el(
                "div",
                (
                    el(
                        "div",
                        (
                            el("div", text("GRAPHSHELL")).attr("class", "brand"),
                            el("div", text("Mere reference host")).attr("class", "subtitle"),
                        ),
                    )
                    .attr("class", "identity"),
                    el(
                        "div",
                        (
                            pill("Local Mere".into(), model.local_active),
                            pill("Remote mount".into(), !model.local_active),
                        ),
                    )
                    .attr("class", "sessions"),
                ),
            )
            .attr("class", "topbar"),
            el(
                "div",
                (
                    el("div", text("ACTIVE SESSION")).attr("class", "eyebrow"),
                    el("div", text(model.active_session)).attr("class", "session-name"),
                    el("div", text("SELECTION")).attr("class", "eyebrow"),
                    el("div", text(model.selection.clone())).attr("class", "selection"),
                    el(
                        "div",
                        text(format!(
                            "{} · physics {}",
                            model.arrangement,
                            if model.physics_paused {
                                "paused"
                            } else {
                                "running"
                            }
                        )),
                    )
                    .attr("class", "hint"),
                ),
            )
            .attr("class", "rail"),
            el(
                "div",
                (
                    el("div", text("OBJECT DETAIL")).attr("class", "eyebrow"),
                    el("div", text(model.selection.clone())).attr("class", "detail-title"),
                    el("div", text(model.detail_address)).attr("class", "address"),
                    el("div", text("Open with Graphshell Inspect")).attr("class", "action"),
                    action_form_view(model.action_draft.as_ref()),
                    el("div", text(model.action_status)).attr("class", "action-status"),
                ),
            )
            .attr("class", detail_class),
            el(
                "div",
                (
                    el("div", text("LOCAL GRAPH PRODUCT")).attr("class", "eyebrow"),
                    el("div", text(model.product_status)).attr("class", "product-status"),
                ),
            )
            .attr("class", "product-proof"),
            el(
                "div",
                (
                    el("div", text("WEBGPU")).attr("class", "proof-pass"),
                    el("div", text(model.viewport_label)).attr("class", "proof-copy"),
                ),
            )
            .attr("class", "proof"),
        ),
    )
    .attr("class", "shell")
}

fn stylesheet(width: u32, height: u32) -> String {
    let narrow = width < 720;
    let rail_width = if narrow {
        width.saturating_sub(24)
    } else {
        236
    };
    let detail_width = if narrow {
        width.saturating_sub(24)
    } else {
        326
    };
    let topbar_height = if narrow { 104 } else { 64 };
    let rail_top = if narrow {
        height.saturating_sub(126)
    } else {
        82
    };
    let detail_top = if narrow { 122 } else { 82 };
    let detail_left = if narrow {
        12
    } else {
        width.saturating_sub(detail_width + 18)
    };
    format!(
        r#"
.shell {{ width: {width}px; height: {height}px; color: #dce5e8;
  font-family: sans-serif; font-size: 13px; }}
.topbar {{ position: absolute; left: 0px; top: 0px; width: {width}px;
  height: {topbar_height}px; display: flex; background-color: #101820;
  border-bottom: 1px solid #29404a; }}
.identity {{ width: 244px; padding: 13px 16px; }}
.brand {{ color: #f0c674; font-size: 17px; }}
.subtitle {{ color: #79919a; font-size: 11px; margin-top: 2px; }}
.sessions {{ display: flex; padding: 12px 8px; }}
.pill {{ padding: 8px 13px; margin-right: 8px; border-radius: 16px;
  background-color: #17242c; color: #8da2a9; }}
.pill.active {{ background-color: #294b52; color: #f4dfae; }}
.rail {{ position: absolute; left: 12px; top: {rail_top}px; width: {rail_width}px;
  height: 154px; padding: 14px; border-radius: 12px; background-color: #101820;
  border: 1px solid #29404a; }}
.eyebrow {{ color: #6e8790; font-size: 10px; margin-bottom: 5px; }}
.session-name {{ color: #f0c674; margin-bottom: 15px; }}
.selection {{ color: #e8eeee; font-size: 15px; margin-bottom: 12px; }}
.hint {{ color: #789099; font-size: 10px; }}
.detail {{ position: absolute; left: {detail_left}px; top: {detail_top}px;
  width: {detail_width}px; min-height: 174px; padding: 16px; border-radius: 12px;
  background-color: #14212a; border: 1px solid #41616b; }}
.detail.hidden {{ display: none; }}
.detail-title {{ color: #f5e7c4; font-size: 18px; margin-bottom: 9px; }}
.address {{ color: #89a6af; font-size: 11px; margin-bottom: 18px; }}
.action {{ background-color: #d39b4a; color: #152028; padding: 9px 12px;
  border-radius: 7px; margin-bottom: 9px; }}
.action-status {{ color: #8db8a4; font-size: 11px; }}
.action-form {{ margin-top: 10px; padding-top: 10px; border-top: 1px solid #35505b; }}
.action-form.hidden {{ display: none; }}
.action-form-title {{ color: #f5e7c4; font-size: 13px; margin-bottom: 5px; }}
.action-help {{ color: #789099; font-size: 10px; margin-bottom: 7px; }}
.action-field {{ margin-top: 8px; }}
.action-field-label {{ color: #a9c7ba; font-size: 11px; }}
.action-choices {{ margin-top: 4px; }}
.action-choice {{ color: #9fb1b7; font-size: 11px; padding: 2px 0px; }}
.action-choice.selected {{ color: #f0c674; }}
.action-form-error {{ color: #e99387; font-size: 10px; margin-top: 7px; }}
.action-submit {{ margin-top: 8px; background-color: #d39b4a; color: #152028; padding: 7px 9px;
  border-radius: 6px; font-size: 10px; }}
.product-proof {{ position: absolute; left: 300px; bottom: 12px; max-width: 520px;
  padding: 8px 12px; border-radius: 10px; background-color: #101820;
  border: 1px solid #29404a; }}
.product-status {{ color: #a9c7ba; font-size: 11px; }}
.proof {{ position: absolute; left: 12px; top: {proof_top}px; display: flex;
  padding: 7px 10px; border-radius: 13px; background-color: #101820; }}
.proof-pass {{ color: #8cc7a6; margin-right: 8px; }}
.proof-copy {{ color: #718891; }}
"#,
        proof_top = height.saturating_sub(if narrow { 158 } else { 42 }),
    )
}

pub(crate) fn build_chrome_scene(
    model: ChromeModel,
    width: u32,
    height: u32,
) -> Result<Scene, String> {
    let dom = Rc::new(RefCell::new(ScriptedDom::new()));
    let view_model = model.clone();
    let runner = GenetAppRunner::new(dom, move |_: &()| chrome_view(view_model.clone()), ());
    let dom = runner.dom();
    let dom_ref = dom.borrow();
    let sheet = stylesheet(width, height);
    let sheets = [sheet.as_str()];
    let layout = genet_layout::lay_out_content(&*dom_ref, &sheets, &NoImageLoader, width, height);
    let (list, _, _) = layout.emit_band(&*dom_ref, 0, height, &ScrollOffsets::default());
    Ok(paint_list_render::translate_paint_cmd_stream(
        list.viewport(),
        list.commands(),
        list.fonts(),
        list.images(),
    )
    .scene)
}
