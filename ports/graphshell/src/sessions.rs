//! Product-neutral local session mounting for the G4 cross-product proof.

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fmt::Write;
use std::path::{Path, PathBuf};

use graphshell_client::{
    ClientState, PresentationResolution, ResolvedPresentation, ResumeApplication,
};
use graphshell_protocol::{
    AdvertisedAction, CapabilityProfile, CarrierNotice, CarrierRequestBody, CarrierResponseBody,
    EndpointDescriptor, IntentInvocation, IntentResult, PresentationCapability, ProjectionSession,
    ResumeRequest,
};
use graphshell_stdio::StdioCarrier;
use sceno::InstanceId;
use serde::Serialize;

use crate::view::{
    IntentReceiptView, ProjectionLayoutView, ProjectionReceiptView, render_projection_receipt,
};

/// One mounted endpoint projection and the label used by Graphshell's switcher.
pub struct SessionProjectionView {
    pub label: String,
    pub projection: ProjectionReceiptView,
}

/// One retained local endpoint process and its Graphshell client state.
///
/// Resource bytes remain in `ClientState` only for this object's lifetime.
/// `close` and `Drop` both release the child carrier and discard every mounted
/// session, including memory-only editable source.
pub struct RetainedEndpointSession {
    carrier: Option<StdioCarrier>,
    client: ClientState,
    profile: CapabilityProfile,
    descriptor: EndpointDescriptor,
    mounted: BTreeSet<ProjectionSession>,
}

impl RetainedEndpointSession {
    pub fn spawn(
        program: impl AsRef<OsStr>,
        args: impl IntoIterator<Item = impl AsRef<OsStr>>,
        profile: CapabilityProfile,
    ) -> Result<Self, String> {
        let mut carrier = StdioCarrier::spawn(program, args).map_err(|error| error.to_string())?;
        let descriptor = match carrier.request(CarrierRequestBody::Discover)? {
            CarrierResponseBody::Descriptor(descriptor) => descriptor,
            other => return Err(unexpected("descriptor", &other)),
        };
        Ok(Self {
            carrier: Some(carrier),
            client: ClientState::default(),
            profile,
            descriptor,
            mounted: BTreeSet::new(),
        })
    }

    pub fn descriptor(&self) -> &EndpointDescriptor {
        &self.descriptor
    }

    pub fn client(&self) -> &ClientState {
        &self.client
    }

    pub fn profile(&self) -> &CapabilityProfile {
        &self.profile
    }

    /// Forget one mounted projection and every resource cached beneath it.
    ///
    /// The endpoint process may remain alive for another document, but a
    /// closed editor must not leave its memory-only source in client state.
    pub fn forget(&mut self, session: &ProjectionSession) {
        self.mounted.remove(session);
        self.client.forget_session(session);
    }

    /// Mount one discovered projection without resolving resources or invoking
    /// any of its actions.
    pub fn mount(&mut self, offer_index: usize) -> Result<ProjectionSession, String> {
        let offer = self
            .descriptor
            .projections
            .get(offer_index)
            .cloned()
            .ok_or_else(|| format!("endpoint has no projection {offer_index}"))?;
        let snapshot = match self
            .carrier_mut()?
            .request(CarrierRequestBody::Snapshot(offer.request))?
        {
            CarrierResponseBody::Snapshot(snapshot) => *snapshot,
            other => return Err(unexpected("snapshot", &other)),
        };
        let session = snapshot.session.clone();
        self.client
            .apply_snapshot(snapshot)
            .map_err(|error| format!("Graphshell rejected {session:?}: {error:?}"))?;
        self.mounted.insert(session.clone());
        Ok(session)
    }

    /// Resolve one presentation on demand, fetching only the selected
    /// capability's resource.
    pub fn resolve(
        &mut self,
        session: &ProjectionSession,
        instance: InstanceId,
    ) -> Result<ResolvedPresentation, String> {
        loop {
            match self
                .client
                .resolve(session, instance, &self.profile)
                .map_err(|error| format!("could not resolve {}: {error:?}", session.0))?
            {
                PresentationResolution::Ready(presentation) => return Ok(presentation),
                PresentationResolution::NeedsResource(request) => {
                    let response = match self
                        .carrier_mut()?
                        .request(CarrierRequestBody::Resource(request))?
                    {
                        CarrierResponseBody::Resource(response) => response,
                        other => return Err(unexpected("resource", &other)),
                    };
                    self.client
                        .apply_resource(response)
                        .map_err(|error| format!("resource was rejected: {error:?}"))?;
                }
            }
        }
    }

    pub fn resolve_all(
        &mut self,
        session: &ProjectionSession,
    ) -> Result<Vec<(InstanceId, ResolvedPresentation)>, String> {
        let instances = self
            .client
            .mounted(session)
            .ok_or_else(|| format!("Graphshell did not mount {}", session.0))?
            .scene
            .active_items_in_order()
            .into_iter()
            .map(|(instance, _)| instance)
            .collect::<Vec<_>>();
        instances
            .into_iter()
            .map(|instance| {
                self.resolve(session, instance)
                    .map(|value| (instance, value))
            })
            .collect()
    }

    /// Invoke an action exactly as advertised, using the current client
    /// acknowledgement and a typed, versioned payload.
    pub fn invoke<T: Serialize>(
        &mut self,
        session: &ProjectionSession,
        target: InstanceId,
        action: &AdvertisedAction,
        payload: &T,
    ) -> Result<IntentResult, String> {
        let advertised = self
            .client
            .mounted(session)
            .and_then(|mounted| mounted.presentation.offers_for(target))
            .and_then(|offers| {
                offers
                    .iter()
                    .find(|offer| self.profile.supports(offer.requires))
            })
            .is_some_and(|offer| {
                offer
                    .semantics
                    .actions
                    .iter()
                    .any(|candidate| candidate == action)
            });
        if !advertised {
            return Err("intent was not advertised for the selected presentation".into());
        }
        let ack = self
            .client
            .acknowledgement(session)
            .ok_or_else(|| format!("Graphshell did not acknowledge {}", session.0))?;
        let payload = serde_json::to_vec(payload)
            .map_err(|error| format!("could not encode intent payload: {error}"))?;
        match self
            .carrier_mut()?
            .request(CarrierRequestBody::Intent(IntentInvocation {
                session: session.clone(),
                target,
                observed_epoch: ack.epoch,
                observed_revision: ack.revision,
                intent: action.intent.0.clone(),
                payload,
            }))? {
            CarrierResponseBody::Intent(result) => Ok(result),
            other => Err(unexpected("intent result", &other)),
        }
    }

    /// Block for one revision bell and recover through the ordinary resume
    /// path. Source bytes never travel in the notice.
    pub fn wait_for_change(&mut self) -> Result<bool, String> {
        let notice = self.carrier_mut()?.wait_for_notice()?;
        let carrier = self
            .carrier
            .as_mut()
            .ok_or_else(|| "endpoint carrier is closed".to_string())?;
        resume_after_notice(carrier, &mut self.client, &notice)
    }

    /// Pump the stdio carrier without waiting for a notice.
    ///
    /// The discovery request is a harmless round trip that lets the carrier
    /// collect any notices already written by the endpoint. This is intended
    /// for a background owner that polls on a short cadence while its UI
    /// remains entirely local.
    pub fn poll_for_change(&mut self) -> Result<bool, String> {
        match self.carrier_mut()?.request(CarrierRequestBody::Discover)? {
            CarrierResponseBody::Descriptor(_) => {}
            other => return Err(unexpected("descriptor", &other)),
        }
        let mut changed = false;
        loop {
            let notice = self.carrier.as_mut().and_then(StdioCarrier::take_notice);
            let Some(notice) = notice else {
                break;
            };
            let carrier = self
                .carrier
                .as_mut()
                .ok_or_else(|| "endpoint carrier is closed".to_string())?;
            changed |= resume_after_notice(carrier, &mut self.client, &notice)?;
        }
        Ok(changed)
    }

    pub fn close(mut self) -> Result<(), String> {
        let carrier_result = if let Some(mut carrier) = self.carrier.take() {
            let response = carrier.request(CarrierRequestBody::Close);
            let close = match response {
                Ok(CarrierResponseBody::Closed) => Ok(()),
                Ok(other) => Err(unexpected("session close", &other)),
                Err(error) => Err(error),
            };
            let shutdown = carrier
                .shutdown()
                .map_err(|error| format!("endpoint did not stop cleanly: {error}"));
            close.and(shutdown)
        } else {
            Ok(())
        };
        self.purge();
        carrier_result
    }

    fn carrier_mut(&mut self) -> Result<&mut StdioCarrier, String> {
        self.carrier
            .as_mut()
            .ok_or_else(|| "endpoint carrier is closed".to_string())
    }

    fn purge(&mut self) {
        for session in std::mem::take(&mut self.mounted) {
            self.client.forget_session(&session);
        }
    }
}

impl Drop for RetainedEndpointSession {
    fn drop(&mut self) {
        self.purge();
        // Dropping StdioCarrier closes stdin and waits for the child. Taking it
        // makes the order explicit: source bytes are purged before the process
        // boundary is released.
        drop(self.carrier.take());
    }
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
    let profile = CapabilityProfile::new([
        PresentationCapability::PortableCard,
        PresentationCapability::NativeGlyph,
        PresentationCapability::Image,
    ]);
    let mut retained = RetainedEndpointSession::spawn(program, std::iter::empty::<&str>(), profile)
        .map_err(|error| format!("could not start {}: {error}", program.display()))?;
    let views = mount_descriptor(&mut retained);
    let shutdown = retained.close();
    match (views, shutdown) {
        (Ok(views), Ok(())) => Ok(views),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

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

/// Block for one endpoint revision bell, mark the mounted scene stale, and
/// recover through the ordinary revision-addressed resume path.
pub fn wait_for_session_change(
    carrier: &mut StdioCarrier,
    client: &mut ClientState,
) -> Result<bool, String> {
    let notice = carrier.wait_for_notice()?;
    resume_after_notice(carrier, client, &notice)
}

pub fn resume_after_notice(
    carrier: &mut StdioCarrier,
    client: &mut ClientState,
    notice: &CarrierNotice,
) -> Result<bool, String> {
    let Some(mut request) = resume_request_for_notice(client, notice)? else {
        return Ok(false);
    };
    for _ in 0..4 {
        let reply = match carrier.request(CarrierRequestBody::Resume(request))? {
            CarrierResponseBody::Resume(reply) => reply,
            other => return Err(unexpected("resume reply", &other)),
        };
        match client
            .apply_resume(&notice.session, reply)
            .map_err(|error| {
                format!(
                    "Graphshell rejected resume for {}: {error:?}",
                    notice.session.0
                )
            })? {
            ResumeApplication::Current(_) | ResumeApplication::Applied(_) => return Ok(true),
            ResumeApplication::Resynchronize(next) => request = next,
        }
    }
    Err("endpoint did not produce an applicable resume after four attempts".into())
}

pub fn resume_request_for_notice(
    client: &mut ClientState,
    notice: &CarrierNotice,
) -> Result<Option<ResumeRequest>, String> {
    let acknowledged = client
        .acknowledgement(&notice.session)
        .ok_or_else(|| format!("revision notice names unknown session {}", notice.session.0))?;
    if notice.epoch == acknowledged.epoch && notice.revision <= acknowledged.revision {
        return Ok(None);
    }
    client.mark_stale(&notice.session);
    Ok(client.resume_request(&notice.session))
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
            CarrierResponseBody::Opened(_) => "an opened session",
            CarrierResponseBody::Closed => "a session close",
            CarrierResponseBody::Suspended => "a session suspend",
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
    use graphshell_protocol::{
        CachePolicy, PresentationManifest, ProjectionSnapshot, ProtocolVersion,
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
            graphshell_protocol::SessionStatus::Stale
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
