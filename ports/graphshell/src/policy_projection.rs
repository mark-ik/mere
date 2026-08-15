//! N4: host-facing projection of Notochord owner policy.
//!
//! The editable model stays in Notochord and persistence stays in
//! pandect. This module turns that model into plain settings rows and
//! a headed receipt. It never serializes carrier facts or an admitted
//! principal.

use std::fmt::Write;
use std::io;
use std::path::Path;

use notochord::{
    NetworkId, OwnerNetworkPolicy, OwnerPolicyEdit, OwnerPolicySet, ProfileRef, ServiceAccess,
    ServiceRule,
};
use pandect::{PersonaId, load_notochord_policy, save_notochord_policy};

use crate::admission::PROJECTION_SERVICE;

/// Murm's point-to-point service path.
pub const MURM_SERVICE: &str = "/services/murm";

/// One service row shown to the owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServicePolicyView {
    /// Structural service path.
    pub path: String,
    /// Plain access label.
    pub access: &'static str,
    /// Admission domain offered by the owner.
    pub domain: String,
    /// Admission actions offered by the owner.
    pub actions: Vec<String>,
    /// Whether this service insists on carrier-authenticated identity.
    pub requires_transport_identity: bool,
    /// Owner-selected concurrent-session ceiling.
    pub max_sessions: Option<u32>,
}

/// Host-neutral settings view for one network.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicySettingsView {
    /// Human-readable network id prefix.
    pub network: String,
    /// Accepted vocabulary revisions.
    pub profiles: Vec<String>,
    /// Number of accepted Personae roots.
    pub trusted_roots: usize,
    /// Service settings, sorted by path.
    pub services: Vec<ServicePolicyView>,
    /// Discovery is an independent setting.
    pub discovery: bool,
    /// Transit is an independent setting.
    pub transit: bool,
    /// Verified revocations retained for restart.
    pub revocations: usize,
    /// Maximum hello bytes.
    pub max_hello_bytes: u32,
    /// Handshake deadline.
    pub handshake_deadline_ms: u32,
}

impl From<&OwnerNetworkPolicy> for PolicySettingsView {
    fn from(settings: &OwnerNetworkPolicy) -> Self {
        let network = settings.policy.network.0;
        Self {
            network: format!(
                "{:02x}{:02x}{:02x}{:02x}…",
                network[0], network[1], network[2], network[3]
            ),
            profiles: settings
                .policy
                .accepted_profiles
                .iter()
                .map(|profile| format!("{} r{}", profile.id, profile.revision))
                .collect(),
            trusted_roots: settings.policy.trusted_roots.len(),
            services: settings
                .policy
                .services
                .iter()
                .map(|(path, rule)| ServicePolicyView {
                    path: path.clone(),
                    access: access_label(rule.access),
                    domain: rule.domain.clone(),
                    actions: rule.actions.iter().cloned().collect(),
                    requires_transport_identity: rule.require_transport_identity,
                    max_sessions: rule.max_sessions,
                })
                .collect(),
            discovery: settings.policy.permits_discovery(),
            transit: settings.policy.permits_transit(),
            revocations: settings.revocations.len(),
            max_hello_bytes: settings.policy.limits.max_hello_bytes,
            handshake_deadline_ms: settings.policy.limits.deadline_ms,
        }
    }
}

fn access_label(access: ServiceAccess) -> &'static str {
    match access {
        ServiceAccess::Disabled => "Disabled",
        ServiceAccess::Public => "Public",
        ServiceAccess::MemberOnly => "Members",
    }
}

/// One stage in the headed independence receipt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyReceiptStage {
    /// What the owner did.
    pub title: String,
    /// What remained true afterwards.
    pub note: String,
    /// Projected settings.
    pub view: PolicySettingsView,
}

/// Complete N4 headed receipt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyReceipt {
    /// Ordered owner edits and restart state.
    pub stages: Vec<PolicyReceiptStage>,
}

/// Run N4's owner-policy scenario through the real persistence seam.
pub fn run_n4_policy_scenario(data_root: &Path, persona: PersonaId) -> io::Result<PolicyReceipt> {
    let network_id = NetworkId([9; 32]);
    let mut network = OwnerNetworkPolicy::closed(network_id);
    network.apply(OwnerPolicyEdit::AcceptedProfiles(vec![ProfileRef {
        id: "mere.base".to_string(),
        revision: 1,
    }]));
    network.apply(OwnerPolicyEdit::Service {
        path: MURM_SERVICE.to_string(),
        rule: rule(ServiceAccess::Public, Some(4)),
    });
    network.apply(OwnerPolicyEdit::Service {
        path: PROJECTION_SERVICE.to_string(),
        rule: rule(ServiceAccess::Public, Some(2)),
    });
    network.apply(OwnerPolicyEdit::Transit(true));

    let mut stages = vec![stage(
        "Starting policy",
        "Murm and Graphshell are offered; transit is independently enabled.",
        &network,
    )];

    network.apply(OwnerPolicyEdit::Service {
        path: MURM_SERVICE.to_string(),
        rule: rule(ServiceAccess::Disabled, Some(4)),
    });
    stages.push(stage(
        "Murm disabled",
        "Graphshell remains open and transit remains enabled.",
        &network,
    ));

    network.apply(OwnerPolicyEdit::Transit(false));
    stages.push(stage(
        "Transit disabled",
        "Both service rules are byte-for-byte unchanged.",
        &network,
    ));

    let mut policies = OwnerPolicySet::new();
    policies.upsert(network);
    save_notochord_policy(data_root, persona, &policies)?;
    let restarted = load_notochord_policy(data_root, persona)?
        .and_then(|restored| restored.network(network_id).cloned())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "restored policy is absent"))?;
    stages.push(stage(
        "Restarted",
        "Owner rules and revocations return; live session conclusions do not.",
        &restarted,
    ));

    Ok(PolicyReceipt { stages })
}

fn rule(access: ServiceAccess, max_sessions: Option<u32>) -> ServiceRule {
    ServiceRule::new(access, "mere.network", ["connect"], false, max_sessions)
}

fn stage(title: &str, note: &str, settings: &OwnerNetworkPolicy) -> PolicyReceiptStage {
    PolicyReceiptStage {
        title: title.to_string(),
        note: note.to_string(),
        view: PolicySettingsView::from(settings),
    }
}

/// Render the N4 scenario as responsive semantic HTML.
pub fn render_n4_policy_receipt(receipt: &PolicyReceipt) -> String {
    let mut html = String::from(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Notochord owner policy receipt</title>
<style>
:root{color-scheme:dark;--ink:#edf3ee;--muted:#9eb1a6;--line:#31493d;--panel:#102019;--deep:#07100c;--accent:#8fcf9b;--on:#71c58a;--off:#dc8b77}*{box-sizing:border-box}body{margin:0;background:radial-gradient(circle at 18% 0,#173628 0,transparent 38%),linear-gradient(145deg,#06100b,#0a1711 55%,#0c1c14);color:var(--ink);font:15px/1.5 Inter,ui-sans-serif,system-ui,-apple-system,"Segoe UI",sans-serif}.shell{width:min(1160px,calc(100% - 32px));margin:auto;padding:48px 0 64px}.eyebrow{color:var(--accent);font-size:12px;font-weight:800;letter-spacing:.16em;text-transform:uppercase}h1{max-width:780px;margin:8px 0 12px;font-size:clamp(34px,5vw,58px);line-height:1.02;letter-spacing:-.04em}.lede{max-width:760px;color:#bdd0c4;font-size:17px}.stages{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:16px;margin-top:30px}.stage{border:1px solid var(--line);border-radius:18px;background:rgba(14,31,23,.95);overflow:hidden}.stage header{padding:18px 20px;border-bottom:1px solid var(--line)}h2{margin:0;font-size:21px}.note{margin:5px 0 0;color:var(--muted)}.axes{display:grid;grid-template-columns:repeat(3,1fr);gap:1px;background:var(--line);border-bottom:1px solid var(--line)}.axis{padding:13px;background:#0d1b15}.axis span{display:block;color:var(--muted);font-size:11px;text-transform:uppercase}.axis strong.on{color:var(--on)}.axis strong.off{color:var(--off)}.services{padding:16px 20px}.service{display:grid;grid-template-columns:1fr auto;gap:12px;padding:11px 0;border-bottom:1px solid #273c32}.service:last-child{border-bottom:0}.path{font:12px ui-monospace,SFMono-Regular,Consolas,monospace;color:#c9d8ce}.meta{color:var(--muted);font-size:12px}.badge{align-self:start;border:1px solid #446052;border-radius:999px;padding:4px 9px;font-size:11px;font-weight:750}.badge.Disabled{color:#f0aa98;border-color:#71463c}.footer{display:flex;flex-wrap:wrap;gap:8px;padding:0 20px 18px;color:var(--muted);font-size:12px}.pill{border:1px solid #30493b;border-radius:999px;padding:5px 8px;background:#0c1812}@media(max-width:760px){.shell{padding-top:28px}.stages{grid-template-columns:1fr}.axes{grid-template-columns:1fr 1fr}.axis:last-child{grid-column:1/-1}}
</style>
</head>
<body>
<main class="shell">
<p class="eyebrow">Notochord N4 · headed receipt</p>
<h1>One owner policy, independent controls.</h1>
<p class="lede">A service edit does not rewrite another service or transit. A transit edit does not expose a service. Restart restores chosen rules while live admission conclusions stay gone.</p>
<section class="stages">
"#,
    );

    for stage in &receipt.stages {
        write!(
            html,
            "<article class=\"stage\"><header><h2>{}</h2><p class=\"note\">{}</p></header>",
            escape(&stage.title),
            escape(&stage.note)
        )
        .unwrap();
        write!(
            html,
            "<div class=\"axes\"><div class=\"axis\"><span>Discovery</span><strong class=\"{}\">{}</strong></div><div class=\"axis\"><span>Transit</span><strong class=\"{}\">{}</strong></div><div class=\"axis\"><span>Revocations</span><strong>{}</strong></div></div>",
            on_off_class(stage.view.discovery),
            on_off_label(stage.view.discovery),
            on_off_class(stage.view.transit),
            on_off_label(stage.view.transit),
            stage.view.revocations,
        )
        .unwrap();
        html.push_str("<div class=\"services\">");
        for service in &stage.view.services {
            write!(
                html,
                "<div class=\"service\"><div><div class=\"path\">{}</div><div class=\"meta\">{} · {} · transport identity: {} · capacity: {}</div></div><span class=\"badge {}\">{}</span></div>",
                escape(&service.path),
                escape(&service.domain),
                escape(&service.actions.join(", ")),
                if service.requires_transport_identity {
                    "required"
                } else {
                    "optional"
                },
                service
                    .max_sessions
                    .map_or_else(|| "owner default".to_string(), |value| value.to_string()),
                service.access,
                service.access,
            )
            .unwrap();
        }
        html.push_str("</div><div class=\"footer\">");
        write!(
            html,
            "<span class=\"pill\">network {}</span><span class=\"pill\">profiles {}</span><span class=\"pill\">roots {}</span><span class=\"pill\">hello ≤ {} B</span><span class=\"pill\">deadline {} ms</span>",
            escape(&stage.view.network),
            escape(&stage.view.profiles.join(", ")),
            stage.view.trusted_roots,
            stage.view.max_hello_bytes,
            stage.view.handshake_deadline_ms,
        )
        .unwrap();
        html.push_str("</div></article>");
    }
    html.push_str("</section></main></body></html>\n");
    html
}

fn on_off_class(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

fn on_off_label(value: bool) -> &'static str {
    if value { "Enabled" } else { "Disabled" }
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use notochord::{
        CarrierKind, DenyReason, RequestedAction, RevocationLedger, SessionClaims, SessionDecision,
        SessionFacts, TrafficClass,
    };

    use super::*;

    fn claims(path: &str) -> SessionClaims {
        SessionClaims {
            wire_version: 1,
            network: NetworkId([9; 32]),
            profile: ProfileRef {
                id: "mere.base".to_string(),
                revision: 1,
            },
            action: RequestedAction {
                domain: "mere.network".to_string(),
                path: path.to_string(),
                action: "connect".to_string(),
            },
            class: TrafficClass::Interactive,
            subject: [4; 32],
            delegations: Vec::new(),
        }
    }

    fn decision(settings: &OwnerNetworkPolicy, path: &str) -> SessionDecision {
        settings.policy.evaluate(
            &SessionFacts::new(b"receipt", CarrierKind::Memory),
            &claims(path),
            &RevocationLedger::new(),
            10,
            0,
        )
    }

    #[test]
    fn murm_and_transit_edits_do_not_leak_across_axes() {
        let root =
            std::env::temp_dir().join(format!("graphshell-n4-independence-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let receipt =
            run_n4_policy_scenario(&root, PersonaId::default_persona()).expect("scenario");

        let initial = &receipt.stages[0].view;
        let murm_disabled = &receipt.stages[1].view;
        let transit_disabled = &receipt.stages[2].view;
        assert!(initial.transit);
        assert!(murm_disabled.transit);
        assert_eq!(
            initial
                .services
                .iter()
                .find(|service| service.path == PROJECTION_SERVICE),
            murm_disabled
                .services
                .iter()
                .find(|service| service.path == PROJECTION_SERVICE)
        );
        assert_eq!(murm_disabled.services, transit_disabled.services);
        assert!(!transit_disabled.transit);

        let restored = load_notochord_policy(&root, PersonaId::default_persona())
            .expect("load")
            .expect("policy")
            .network(NetworkId([9; 32]))
            .expect("network")
            .clone();
        assert_eq!(
            decision(&restored, MURM_SERVICE),
            SessionDecision::Deny {
                reason: DenyReason::ServiceNotOffered
            }
        );
        assert!(decision(&restored, PROJECTION_SERVICE).is_accept());
        assert!(!restored.policy.permits_transit());

        std::fs::remove_dir_all(root).expect("remove scratch");
    }

    #[test]
    fn committed_n4_receipt_matches_the_live_policy_scenario() {
        let root =
            std::env::temp_dir().join(format!("graphshell-n4-receipt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let receipt =
            run_n4_policy_scenario(&root, PersonaId::default_persona()).expect("scenario");
        let rendered = render_n4_policy_receipt(&receipt);

        assert_eq!(
            rendered,
            include_str!("../docs/receipts/n4_owner_policy.html")
        );
        std::fs::remove_dir_all(root).expect("remove scratch");
    }
}
