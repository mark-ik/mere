//! The G4 cross-product proof: mount several endpoints and render them behind
//! one switcher.
//!
//! The session state machine this used to hold now lives in
//! [`graphshell_client::RetainedEndpointSession`], which is where a product
//! reaching an endpoint should find it. What stays here is what needs the
//! port: the stdio deployment, the receipt views, and the harness that drives
//! every advertised action to prove a projection is live rather than rendered.

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
use std::collections::BTreeSet;
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
use std::ffi::OsStr;
use std::fmt::Write;
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
use std::path::{Path, PathBuf};

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
use graphshell_client::{ClientState, ResolvedPresentation, RetainedEndpointSession, unexpected};
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
use chirograph::{
    CapabilityProfile, Carrier, CarrierRequestBody, CarrierResponseBody, IntentInvocation,
    IntentResult, PresentationCapability, ProjectionSession,
};
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
use graphshell_stdio::StdioCarrier;

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
use crate::view::{IntentReceiptView, ProjectionLayoutView};
use crate::view::{ProjectionReceiptView, render_projection_receipt};

/// One mounted endpoint projection and the label used by Graphshell's switcher.
pub struct SessionProjectionView {
    pub label: String,
    pub projection: ProjectionReceiptView,
}

/// Mount an endpoint that runs as a child process.
///
/// A free function rather than a constructor, because spawning is a property
/// of one carrier rather than of a mounted session:
/// [`RetainedEndpointSession`] holds a `Box<dyn Carrier>` and has no business
/// knowing that processes exist. A host embedding an endpoint or dialling a
/// remote one builds its own carrier and calls `over` directly.
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub fn spawn_endpoint_session(
    program: impl AsRef<OsStr>,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    profile: CapabilityProfile,
) -> Result<RetainedEndpointSession, String> {
    let carrier = StdioCarrier::spawn(program, args).map_err(|error| error.to_string())?;
    RetainedEndpointSession::over(Box::new(carrier), profile)
}

/// Spawn endpoint processes, discover their projections, and mount each one
/// through the same Graphshell client state machine.
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub fn mount_endpoint_processes(
    programs: &[PathBuf],
) -> Result<Vec<SessionProjectionView>, String> {
    let mut sessions = Vec::new();
    for program in programs {
        sessions.extend(mount_endpoint_process(program)?);
    }
    Ok(sessions)
}

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
fn mount_endpoint_process(program: &Path) -> Result<Vec<SessionProjectionView>, String> {
    let profile = CapabilityProfile::new([
        PresentationCapability::PortableCard,
        PresentationCapability::NativeGlyph,
        PresentationCapability::Image,
    ]);
    let mut retained = spawn_endpoint_session(program, std::iter::empty::<&str>(), profile)
        .map_err(|error| format!("could not start {}: {error}", program.display()))?;
    let views = mount_descriptor(&mut retained);
    let shutdown = retained.close();
    match (views, shutdown) {
        (Ok(views), Ok(())) => Ok(views),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
fn mount_descriptor(
    retained: &mut RetainedEndpointSession,
) -> Result<Vec<SessionProjectionView>, String> {
    let descriptor = retained.descriptor().clone();
    let mut views = Vec::new();
    for (index, offer) in descriptor.projections.into_iter().enumerate() {
        let session = retained.mount(index)?;
        let mounted = retained.client().mounted(&session).unwrap();
        let layout = ProjectionLayoutView::from_scene(&mounted.scene);
        let item_count = mounted.scene.active_item_count();
        let resolved = retained.resolve_all(&session)?;
        let presentations = resolved
            .iter()
            .map(|(_, presentation)| presentation.clone())
            .collect::<Vec<_>>();
        let client = retained.client().clone();
        let intents =
            invoke_advertised_actions(retained.carrier_mut()?, &client, &session, &presentations)?;
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

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
fn invoke_advertised_actions(
    carrier: &mut (dyn Carrier + 'static),
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
            let result = match carrier
                .request(CarrierRequestBody::Intent(IntentInvocation {
                    session: session.clone(),
                    target,
                    observed_epoch: ack.epoch,
                    observed_revision: ack.revision,
                    intent: action.intent.0.clone(),
                    payload: Vec::new(),
                }))
                .map_err(|error| error.to_string())?
            {
                CarrierResponseBody::Intent(result) => result,
                other => return Err(unexpected("intent result", &other)),
            };
            receipts.push(intent_receipt(action.label.clone(), result));
        }
    }
    Ok(receipts)
}

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
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
    use graphshell_client::resume_request_for_notice;
    use chirograph::{
        CachePolicy, CarrierNotice, PresentationManifest, ProjectionSnapshot, ProtocolVersion,
    };
    use sceno::Scene;
    use scenotime::{Revision, SceneEpoch, SceneSnapshot};

    use super::*;
    use crate::canary::run_loopback_canary;

    #[test]
    fn a_revision_notice_marks_the_scene_stale_and_resumes_from_the_host_ack() {
        let session = ProjectionSession("knot:directory".into());
        let mut client = ClientState::default();
        client
            .apply_snapshot(ProjectionSnapshot {
                version: ProtocolVersion::V1,
                session: session.clone(),
                scene: SceneSnapshot::from_dense(SceneEpoch(1), Revision(3), Scene::new()).unwrap(),
                presentation: PresentationManifest::default(),
                cache_policy: CachePolicy::default(),
            })
            .unwrap();
        let request = resume_request_for_notice(
            &mut client,
            &CarrierNotice {
                session: session.clone(),
                epoch: SceneEpoch(1),
                revision: Revision(4),
            },
        )
        .unwrap()
        .unwrap();

        assert_eq!(request.session, session);
        assert_eq!(request.epoch, SceneEpoch(1));
        assert_eq!(request.revision, Revision(3));
        assert_eq!(
            client.mounted(&request.session).unwrap().status,
            chirograph::SessionStatus::Stale
        );
    }

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
        let html = include_str!("../docs/receipts/g4_session_switch.html");
        assert_eq!(html.matches("role=\"tab\"").count(), 3);
        assert_eq!(html.matches("<iframe").count(), 3);
        assert!(html.contains("Turnstone · Browsing graph"));
        assert!(html.contains("Isometry · Player overmap"));
        assert!(html.contains("Isometry · Moor crossing"));
        assert!(html.contains("open-address requires an address"));
        assert!(html.contains("player projection grant is read-only for campaign travel"));
    }
}
