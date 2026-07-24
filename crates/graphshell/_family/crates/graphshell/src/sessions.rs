//! Product-neutral local session mounting for the G4 cross-product proof.

use std::collections::BTreeSet;
use std::fmt::Write;
use std::path::{Path, PathBuf};

use graphshell_client::{ClientState, PresentationResolution, ResolvedPresentation};
use graphshell_protocol::{
    CapabilityProfile, CarrierRequestBody, CarrierResponseBody, EndpointDescriptor,
    IntentInvocation, IntentResult, PresentationCapability, ProjectionSession, ResourceRequest,
};
use graphshell_stdio::StdioCarrier;

use crate::view::{
    IntentReceiptView, ProjectionLayoutView, ProjectionReceiptView, render_projection_receipt,
};

/// One mounted endpoint projection and the label used by Graphshell's switcher.
pub struct SessionProjectionView {
    pub label: String,
    pub projection: ProjectionReceiptView,
}

/// Spawn endpoint processes, discover their projections, and mount each one
/// through the same Graphshell client state machine.
pub fn mount_endpoint_processes(
    programs: &[PathBuf],
) -> Result<Vec<SessionProjectionView>, String> {
    let mut sessions = Vec::new();
    for program in programs {
        sessions.extend(mount_endpoint_process(program)?);
    }
    Ok(sessions)
}

fn mount_endpoint_process(program: &Path) -> Result<Vec<SessionProjectionView>, String> {
    let mut carrier = StdioCarrier::spawn(program, std::iter::empty::<&str>())
        .map_err(|error| format!("could not start {}: {error}", program.display()))?;
    let descriptor = match carrier.request(CarrierRequestBody::Discover)? {
        CarrierResponseBody::Descriptor(descriptor) => descriptor,
        other => return Err(unexpected("descriptor", &other)),
    };
    let views = mount_descriptor(&mut carrier, descriptor);
    let shutdown = carrier.shutdown().map_err(|error| {
        format!(
            "endpoint {} did not stop cleanly: {error}",
            program.display()
        )
    });
    match (views, shutdown) {
        (Ok(views), Ok(())) => Ok(views),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

fn mount_descriptor(
    carrier: &mut StdioCarrier,
    descriptor: EndpointDescriptor,
) -> Result<Vec<SessionProjectionView>, String> {
    let mut views = Vec::new();
    for offer in descriptor.projections {
        let snapshot = match carrier.request(CarrierRequestBody::Snapshot(offer.request))? {
            CarrierResponseBody::Snapshot(snapshot) => *snapshot,
            other => return Err(unexpected("snapshot", &other)),
        };
        let session = snapshot.session.clone();
        let layout = ProjectionLayoutView::from_scene(&snapshot.scene);
        let item_count = snapshot.scene.active_item_count();
        let mut client = ClientState::default();
        client
            .apply_snapshot(snapshot)
            .map_err(|error| format!("Graphshell rejected {session:?}: {error:?}"))?;
        let profile = CapabilityProfile::new([
            PresentationCapability::PortableCard,
            PresentationCapability::NativeGlyph,
            PresentationCapability::Image,
        ]);
        let presentations = resolve_presentations(carrier, &mut client, &session, &profile)?;
        let intents = invoke_advertised_actions(carrier, &client, &session, &presentations)?;
        views.push(SessionProjectionView {
            label: format!("{} · {}", descriptor.label, offer.label),
            projection: ProjectionReceiptView {
                eyebrow: "Graphshell · G4".into(),
                title: offer.label,
                lede: format!(
                    "{} disclosed this scene through the shared projection protocol.",
                    descriptor.label
                ),
                session: session.0,
                status: format!("Live · {item_count} items"),
                presentations,
                layout: Some(layout),
                intents,
            },
        });
    }
    Ok(views)
}

fn resolve_presentations(
    carrier: &mut StdioCarrier,
    client: &mut ClientState,
    session: &ProjectionSession,
    profile: &CapabilityProfile,
) -> Result<Vec<ResolvedPresentation>, String> {
    let instances: Vec<_> = client
        .mounted(session)
        .ok_or_else(|| format!("Graphshell did not mount {}", session.0))?
        .scene
        .active_items_in_order()
        .into_iter()
        .map(|(instance, _)| instance)
        .collect();
    let mut presentations = Vec::new();
    for instance in instances {
        let presentation = loop {
            match client
                .resolve(session, instance, profile)
                .map_err(|error| format!("could not resolve {}: {error:?}", session.0))?
            {
                PresentationResolution::Ready(presentation) => break presentation,
                PresentationResolution::NeedsResource(request) => {
                    let response =
                        match carrier.request(CarrierRequestBody::Resource(ResourceRequest {
                            session: request.session,
                            resource: request.resource,
                        }))? {
                            CarrierResponseBody::Resource(response) => response,
                            other => return Err(unexpected("resource", &other)),
                        };
                    client
                        .apply_resource(response)
                        .map_err(|error| format!("resource was rejected: {error:?}"))?;
                }
            }
        };
        presentations.push(presentation);
    }
    Ok(presentations)
}

fn invoke_advertised_actions(
    carrier: &mut StdioCarrier,
    client: &ClientState,
    session: &ProjectionSession,
    presentations: &[ResolvedPresentation],
) -> Result<Vec<IntentReceiptView>, String> {
    let mounted = client
        .mounted(session)
        .ok_or_else(|| format!("Graphshell did not mount {}", session.0))?;
    let ack = client
        .acknowledgement(session)
        .ok_or_else(|| format!("Graphshell did not acknowledge {}", session.0))?;
    let instances: Vec<_> = mounted
        .scene
        .active_items_in_order()
        .into_iter()
        .map(|(instance, _)| instance)
        .collect();
    let mut seen = BTreeSet::new();
    let mut receipts = Vec::new();
    for (target, presentation) in instances.into_iter().zip(presentations) {
        for action in &presentation.semantics.actions {
            if !seen.insert(action.intent.0.clone()) {
                continue;
            }
            let result = match carrier.request(CarrierRequestBody::Intent(IntentInvocation {
                session: session.clone(),
                target,
                observed_epoch: ack.epoch,
                observed_revision: ack.revision,
                intent: action.intent.0.clone(),
                payload: Vec::new(),
            }))? {
                CarrierResponseBody::Intent(result) => result,
                other => return Err(unexpected("intent result", &other)),
            };
            receipts.push(intent_receipt(action.label.clone(), result));
        }
    }
    Ok(receipts)
}

fn intent_receipt(label: String, result: IntentResult) -> IntentReceiptView {
    match result {
        IntentResult::Accepted => IntentReceiptView {
            label,
            result: "Accepted".into(),
            detail: "The endpoint admitted and lowered the advertised intent.".into(),
        },
        IntentResult::Rejected { reason } => IntentReceiptView {
            label,
            result: "Rejected".into(),
            detail: reason,
        },
        IntentResult::Stale { .. } => IntentReceiptView {
            label,
            result: "Stale".into(),
            detail: "The endpoint refused an intent based on an older observation.".into(),
        },
    }
}

fn unexpected(expected: &str, actual: &CarrierResponseBody) -> String {
    format!(
        "endpoint returned {} while Graphshell expected {expected}",
        match actual {
            CarrierResponseBody::Descriptor(_) => "a descriptor",
            CarrierResponseBody::Snapshot(_) => "a snapshot",
            CarrierResponseBody::Resource(_) => "a resource",
            CarrierResponseBody::Resume(_) => "a resume reply",
            CarrierResponseBody::Intent(_) => "an intent result",
        }
    )
}

/// Render independently mounted projections behind keyboard-reachable session
/// tabs. Each panel uses the existing responsive projection receipt unchanged.
pub fn render_session_switch_receipt(sessions: &[SessionProjectionView]) -> String {
    let mut html = String::from(
        r##"<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Graphshell G4 session switch</title>
<style>
:root{color-scheme:dark;font-family:Inter,ui-sans-serif,system-ui,sans-serif;background:#071019;color:#f4eedf}
*{box-sizing:border-box}body{margin:0}.switcher{position:sticky;top:0;z-index:2;display:flex;gap:8px;padding:12px;background:#0b1720;border-bottom:1px solid #314756;overflow:auto}
button{border:1px solid #566f7e;border-radius:999px;padding:8px 13px;background:#132a37;color:#f4eedf;font:inherit;font-weight:700;white-space:nowrap}
button[aria-selected="true"]{border-color:#d8a657;background:#2c2a22}iframe{display:block;width:100%;height:1050px;border:0;background:#071019}iframe[hidden]{display:none}
</style></head><body><nav class="switcher" aria-label="Projection sessions" role="tablist">
"##,
    );
    for (index, session) in sessions.iter().enumerate() {
        write!(
            html,
            "<button type=\"button\" role=\"tab\" aria-selected=\"{}\" data-session=\"session-{index}\">{}</button>",
            index == 0,
            escape(&session.label)
        )
        .unwrap();
    }
    html.push_str("</nav><main>");
    for (index, session) in sessions.iter().enumerate() {
        let receipt = render_projection_receipt(&session.projection);
        write!(
            html,
            "<iframe id=\"session-{index}\" title=\"{}\" srcdoc=\"{}\"{}></iframe>",
            escape(&session.label),
            escape(&receipt),
            if index == 0 { "" } else { " hidden" }
        )
        .unwrap();
    }
    html.push_str(
        r##"</main><script>
const tabs=[...document.querySelectorAll('[role=tab]')];
for(const tab of tabs){tab.addEventListener('click',()=>{for(const item of tabs){const active=item===tab;item.setAttribute('aria-selected',active);document.getElementById(item.dataset.session).hidden=!active;}});}
</script></body></html>
"##,
    );
    html
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
    use crate::canary::run_loopback_canary;

    #[test]
    fn switcher_keeps_each_projection_in_a_separate_keyboard_tab() {
        let run = run_loopback_canary().unwrap();
        let session = || SessionProjectionView {
            label: "Fixture · Notes".into(),
            projection: ProjectionReceiptView {
                eyebrow: "Graphshell".into(),
                title: "Notes".into(),
                lede: "Fixture".into(),
                session: run.session.0.clone(),
                status: "Live".into(),
                presentations: run.rich.clone(),
                layout: None,
                intents: Vec::new(),
            },
        };
        let html = render_session_switch_receipt(&[session(), session()]);
        assert_eq!(html.matches("role=\"tab\"").count(), 2);
        assert_eq!(html.matches("<iframe").count(), 2);
        assert!(html.contains("data-session=\"session-1\""));
    }

    #[test]
    fn committed_g4_receipt_contains_both_products_and_all_three_sessions() {
        let html = include_str!("../../../docs/receipts/g4_session_switch.html");
        assert_eq!(html.matches("role=\"tab\"").count(), 3);
        assert_eq!(html.matches("<iframe").count(), 3);
        assert!(html.contains("Merecat · Browsing graph"));
        assert!(html.contains("Isometry · Player overmap"));
        assert!(html.contains("Isometry · Moor crossing"));
        assert!(html.contains("open-address requires an address"));
        assert!(html.contains("player projection grant is read-only for campaign travel"));
    }
}
