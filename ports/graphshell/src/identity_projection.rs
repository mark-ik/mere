//! Graphshell presentation of the resident identity read model.
//!
//! The cards are portable resources. Actions carry only public, typed fields;
//! authorization and all private key handling remain in
//! [`crate::native::personae_host::PersonaeHost`].

use std::fmt::Write;

use graphshell_protocol::{ActionFormV1, CardValueV1, PortableCardV1};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::identity::{
    AgentListenerView, IdentitySurfaceSnapshot, VaultLockView, VaultProtectionView,
};

pub const SIGNING_APPROVE_ONCE_INTENT: &str = "graphshell.identity.signing.approve-once";
pub const SIGNING_APPROVE_IDLE_INTENT: &str = "graphshell.identity.signing.approve-until-idle";
pub const SIGNING_DENY_INTENT: &str = "graphshell.identity.signing.deny";
pub const SIGNING_DECISION_SCHEMA: &str = "graphshell.identity.signing-decision/v1";
pub const SSH_GENERATE_INTENT: &str = "graphshell.identity.ssh.generate";
pub const SSH_GENERATE_SCHEMA: &str = "graphshell.identity.ssh.generate/v1";
pub const SSH_IMPORT_NATIVE_INTENT: &str = "graphshell.identity.ssh.import-native";
pub const SSH_IMPORT_NATIVE_SCHEMA: &str = "graphshell.identity.ssh.import-native/v1";
pub const SSH_REMOVE_INTENT: &str = "graphshell.identity.ssh.remove";
pub const SSH_REMOVE_SCHEMA: &str = "graphshell.identity.ssh.remove/v1";
pub const DEVICE_REVOKE_INTENT: &str = "graphshell.identity.device.revoke";
pub const DEVICE_REVOKE_SCHEMA: &str = "graphshell.identity.device.revoke/v1";
pub const PROFILE_SWITCH_INTENT: &str = "graphshell.identity.profile.switch";
pub const PROFILE_SWITCH_SCHEMA: &str = "graphshell.identity.profile.switch/v1";

/// Typed, secret-free signing decision payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SigningDecisionIntentV1 {
    pub request_id: Uuid,
}

/// User-selected unlock policy for a generated or natively imported key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SshUnlockPolicyIntentV1 {
    Session,
    ShortTtl { idle_seconds: u32 },
    PerUse,
}

/// Safe input for generating a key entirely inside the resident host.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerateSshKeyIntentV1 {
    pub comment: String,
    pub unlock_policy: SshUnlockPolicyIntentV1,
}

/// Options accompanying a native file-picker handoff.
///
/// The private key is intentionally absent. The native picker passes the
/// parsed key object directly to `PersonaeHost::import_ssh_private`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportSshKeyNativeIntentV1 {
    pub unlock_policy: SshUnlockPolicyIntentV1,
}

/// Explicit removal of one public SSH fingerprint.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoveSshKeyIntentV1 {
    pub fingerprint: String,
    pub confirmed: bool,
}

/// Explicit revocation of one delegated device at the live carry authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevokeDeviceIntentV1 {
    pub device_id: Uuid,
    pub confirmed: bool,
}

/// Switch the resident host to another persona, live.
///
/// The one identity mutation that was missing: the projection could show which
/// persona the host speaks as and could not change it. No confirmation field —
/// switching is not destructive (the previous persona keeps everything and is
/// one switch away), and a wrong guess corrects itself the same way it was
/// made.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SwitchProfileIntentV1 {
    /// The profile id, as carried by the profile card this action rides on.
    pub profile: String,
}

/// One visible local-authority action attached to a portable card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdentityProjectionAction {
    pub intent: &'static str,
    pub schema: &'static str,
    pub label: &'static str,
    /// Pre-bound public fields. `None` means the native host must collect the
    /// schema's user-configurable fields before invocation.
    pub payload: Option<Value>,
    /// H4b actions are local authority controls, not remote projection grants.
    pub native_only: bool,
    /// A bounded input contract the admitted browser can render and compose
    /// against. `None` keeps the opaque-payload path an identity control uses,
    /// where the native host collects the fields itself.
    pub input_form: Option<ActionFormV1>,
}

/// One portable card plus the actions Graphshell may place beside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdentityProjectionCard {
    pub key: String,
    pub card: PortableCardV1,
    pub actions: Vec<IdentityProjectionAction>,
}

/// Build the portable identity surface from the secret-free host snapshot.
pub fn project_identity(snapshot: &IdentitySurfaceSnapshot) -> Vec<IdentityProjectionCard> {
    let mut cards = vec![IdentityProjectionCard {
        key: "identity:vault".to_string(),
        card: PortableCardV1 {
            title: "Identity vault".to_string(),
            values: vec![
                value("Protection", protection_label(snapshot.vault.protection)),
                value("State", lock_label(snapshot.vault.lock)),
                value("SSH agent", listener_label(&snapshot.vault.agent)),
            ],
            badges: vec!["Personae".to_string(), "native authority".to_string()],
            media: Vec::new(),
        },
        actions: vec![
            IdentityProjectionAction {
                intent: SSH_GENERATE_INTENT,
                schema: SSH_GENERATE_SCHEMA,
                label: "Generate SSH key…",
                payload: None,
                native_only: true,
                input_form: None,
            },
            IdentityProjectionAction {
                intent: SSH_IMPORT_NATIVE_INTENT,
                schema: SSH_IMPORT_NATIVE_SCHEMA,
                label: "Import SSH key…",
                payload: None,
                native_only: true,
                input_form: None,
            },
        ],
    }];

    cards.push(IdentityProjectionCard {
        key: "identity:carry".to_string(),
        card: PortableCardV1 {
            title: "Device access".to_string(),
            values: vec![
                value(
                    "Recovery",
                    snapshot
                        .carry
                        .recovery_policy
                        .as_deref()
                        .unwrap_or("not configured"),
                ),
                value("Personae", joined_or_unknown(&snapshot.carry.personas)),
                value(
                    "Authority",
                    if snapshot.carry.unavailable.is_empty() {
                        "session-runtime ready".to_string()
                    } else {
                        snapshot.carry.unavailable.join("; ")
                    },
                ),
            ],
            badges: vec!["carry".to_string(), "device authority".to_string()],
            media: Vec::new(),
        },
        actions: Vec::new(),
    });

    cards.extend(
        snapshot
            .profiles
            .iter()
            .map(|profile| IdentityProjectionCard {
                key: format!("identity:profile:{}", profile.id),
                card: PortableCardV1 {
                    title: profile.display_name.clone(),
                    values: vec![
                        value("Profile", &profile.id),
                        value("Master public key", &profile.master_public_fingerprint),
                        value("Slots", profile.slot_count.to_string()),
                    ],
                    badges: if profile.selected {
                        vec!["selected profile".to_string()]
                    } else {
                        vec!["profile".to_string()]
                    },
                    media: Vec::new(),
                },
                // The selected persona gets no action: the only mutation a
                // profile card offers is becoming the selected one.
                actions: if profile.selected {
                    Vec::new()
                } else {
                    vec![IdentityProjectionAction {
                        intent: PROFILE_SWITCH_INTENT,
                        schema: PROFILE_SWITCH_SCHEMA,
                        label: "Speak as this persona",
                        payload: Some(
                            serde_json::to_value(SwitchProfileIntentV1 {
                                profile: profile.id.clone(),
                            })
                            .expect("profile switch payload is always serializable"),
                        ),
                        native_only: true,
                        input_form: None,
                    }]
                },
            }),
    );

    cards.extend(snapshot.ssh_keys.iter().map(|key| IdentityProjectionCard {
        key: format!("identity:ssh:{}", key.fingerprint),
        card: PortableCardV1 {
            title: if key.comment.is_empty() {
                "SSH key".to_string()
            } else {
                key.comment.clone()
            },
            values: vec![
                value("Fingerprint", &key.fingerprint),
                value("Unlock", &key.unlock_policy),
                value("Lineage", &key.lineage),
                value("Device loss", &key.device_loss_note),
                value("Public key", &key.public_openssh),
            ],
            badges: vec!["SSH".to_string(), "public export".to_string()],
            media: Vec::new(),
        },
        actions: vec![IdentityProjectionAction {
            intent: SSH_REMOVE_INTENT,
            schema: SSH_REMOVE_SCHEMA,
            label: "Remove key…",
            payload: Some(
                serde_json::to_value(RemoveSshKeyIntentV1 {
                    fingerprint: key.fingerprint.clone(),
                    confirmed: false,
                })
                .expect("SSH removal payload is always serializable"),
            ),
            native_only: true,
            input_form: None,
        }],
    }));

    cards.extend(snapshot.carry.devices.iter().map(|device| {
        let mut badges = vec![device.mode.clone(), device.exposure.clone()];
        if device.revoked {
            badges.push("revoked".to_string());
        }
        let actions = if device.revoked {
            Vec::new()
        } else {
            vec![IdentityProjectionAction {
                intent: DEVICE_REVOKE_INTENT,
                schema: DEVICE_REVOKE_SCHEMA,
                label: "Revoke device…",
                payload: Some(
                    serde_json::to_value(RevokeDeviceIntentV1 {
                        device_id: Uuid::parse_str(&device.device_id)
                            .expect("carry device ids are UUIDs"),
                        confirmed: false,
                    })
                    .expect("device revocation payload is always serializable"),
                ),
                native_only: true,
                input_form: None,
            }]
        };
        IdentityProjectionCard {
            key: format!("identity:device:{}", device.device_id),
            card: PortableCardV1 {
                title: device.label.clone(),
                values: vec![
                    value("Device", &device.device_id),
                    value("Public key", &device.public_key_fingerprint),
                    value(
                        "Grant",
                        device.grant_ref.as_deref().unwrap_or("not granted"),
                    ),
                ],
                badges,
                media: Vec::new(),
            },
            actions,
        }
    }));

    cards.extend(snapshot.carry.grants.iter().map(|grant| {
        let mut badges = vec!["delegation".to_string()];
        if snapshot
            .carry
            .devices
            .iter()
            .any(|device| device.device_id == grant.device_id && device.revoked)
        {
            badges.push("revoked".to_string());
        }
        IdentityProjectionCard {
            key: format!("identity:grant:{}", grant.device_id),
            card: PortableCardV1 {
                title: "Device grant".to_string(),
                values: vec![
                    value("Device", &grant.device_id),
                    value(
                        "Signature",
                        match grant.signature_valid {
                            Some(true) => "verified",
                            Some(false) => "invalid",
                            None => "unknown",
                        },
                    ),
                    value("Issued", grant.issued_at_ms.to_string()),
                    value(
                        "Expires",
                        grant
                            .expires_at_ms
                            .map_or_else(|| "never".to_string(), |expires| expires.to_string()),
                    ),
                    value("Scopes", joined_or_unknown(&grant.scopes)),
                    value("Attenuations", joined_or_unknown(&grant.attenuations)),
                    value("Personae", joined_or_unknown(&grant.personas)),
                    value("Wrapped epochs", grant.wrapped_epoch_count.to_string()),
                ],
                badges,
                media: Vec::new(),
            },
            actions: Vec::new(),
        }
    }));

    cards.extend(snapshot.pending_signing.iter().map(|pending| {
        let payload = SigningDecisionIntentV1 {
            request_id: pending.request.request_id,
        };
        let mut actions = vec![
            IdentityProjectionAction {
                intent: SIGNING_APPROVE_ONCE_INTENT,
                schema: SIGNING_DECISION_SCHEMA,
                label: "Approve once",
                payload: Some(
                    serde_json::to_value(&payload)
                        .expect("signing decision payload is always serializable"),
                ),
                native_only: true,
                input_form: None,
            },
            IdentityProjectionAction {
                intent: SIGNING_DENY_INTENT,
                schema: SIGNING_DECISION_SCHEMA,
                label: "Deny",
                payload: Some(
                    serde_json::to_value(&payload)
                        .expect("signing decision payload is always serializable"),
                ),
                native_only: true,
                input_form: None,
            },
        ];
        if matches!(
            pending.policy,
            personae::signing::SigningPolicy::ShortTtl { .. }
        ) {
            actions.insert(
                1,
                IdentityProjectionAction {
                    intent: SIGNING_APPROVE_IDLE_INTENT,
                    schema: SIGNING_DECISION_SCHEMA,
                    label: "Approve until idle",
                    payload: Some(
                        serde_json::to_value(payload)
                            .expect("signing decision payload is always serializable"),
                    ),
                    native_only: true,
                    input_form: None,
                },
            );
        }
        IdentityProjectionCard {
            key: format!("identity:pending:{}", pending.request.request_id),
            card: PortableCardV1 {
                title: "Signing approval".to_string(),
                values: vec![
                    value("Request", pending.request.request_id.to_string()),
                    value("Profile", &pending.request.profile),
                    value("Key", &pending.request.public_key_fingerprint),
                    value("Operation", &pending.request.operation),
                    value(
                        "Requester",
                        pending
                            .request
                            .authenticated_requester
                            .as_deref()
                            .unwrap_or("unknown"),
                    ),
                    value(
                        "Process",
                        pending
                            .request
                            .authenticated_process
                            .as_deref()
                            .unwrap_or("unknown"),
                    ),
                    value(
                        "Target",
                        pending
                            .request
                            .authenticated_target
                            .as_deref()
                            .unwrap_or("unknown"),
                    ),
                    value("Payload digest", &pending.request.payload_digest),
                ],
                badges: vec!["pending".to_string(), policy_label(pending.policy)],
                media: Vec::new(),
            },
            actions,
        }
    }));

    cards.extend(
        snapshot
            .signing_history
            .iter()
            .rev()
            .take(20)
            .map(|record| IdentityProjectionCard {
                key: format!(
                    "identity:history:{}:{}",
                    record.request.request_id, record.completed_at_ms
                ),
                card: PortableCardV1 {
                    title: "Signing history".to_string(),
                    values: vec![
                        value("Key", &record.request.public_key_fingerprint),
                        value("Operation", &record.request.operation),
                        value("Payload digest", &record.request.payload_digest),
                        value("Result", format!("{:?}", record.result)),
                    ],
                    badges: vec!["completed".to_string()],
                    media: Vec::new(),
                },
                actions: Vec::new(),
            }),
    );

    cards
}

/// Render a responsive headed receipt from the same portable card model.
pub fn render_identity_surface(snapshot: &IdentitySurfaceSnapshot) -> String {
    let cards = project_identity(snapshot);
    let mut html = String::from(
        r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Graphshell identity surface</title>
<style>
:root{color-scheme:dark;--bg:#090b12;--panel:#131827;--line:#2b3349;--ink:#f2f5ff;--muted:#a7afc2;--accent:#b9a4ff;--warn:#ffbc7a}*{box-sizing:border-box}body{margin:0;background:radial-gradient(circle at 15% 0,#272146 0,transparent 35%),var(--bg);color:var(--ink);font:15px/1.45 Inter,ui-sans-serif,system-ui,-apple-system,"Segoe UI",sans-serif}.shell{width:min(1180px,calc(100% - 30px));margin:auto;padding:44px 0 64px}.eyebrow{color:var(--accent);font-size:12px;font-weight:800;letter-spacing:.16em;text-transform:uppercase}h1{margin:6px 0 10px;font-size:clamp(34px,5vw,58px);line-height:1.02;letter-spacing:-.04em}.lede{max-width:760px;color:var(--muted);font-size:17px}.grid{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:14px;margin-top:28px}.card{display:flex;flex-direction:column;border:1px solid var(--line);border-radius:17px;background:rgba(19,24,39,.95);overflow:hidden}.card header{padding:17px 18px;border-bottom:1px solid var(--line)}h2{margin:0;font-size:19px}.badges{display:flex;flex-wrap:wrap;gap:6px;margin-top:9px}.badge{padding:3px 8px;border:1px solid #4a5270;border-radius:999px;color:#cbd2e4;font-size:11px}.values{margin:0;padding:13px 18px}.row{padding:8px 0;border-bottom:1px solid #252c3e}.row:last-child{border:0}dt{color:var(--muted);font-size:11px;text-transform:uppercase;letter-spacing:.06em}dd{margin:2px 0 0;overflow-wrap:anywhere}.actions{display:flex;flex-wrap:wrap;gap:8px;margin-top:auto;padding:0 18px 18px}.actions button{border:1px solid #66579d;border-radius:9px;background:#251f3b;color:#f4efff;padding:8px 11px;font:inherit;font-weight:700}.actions button.deny{border-color:#7a4a4a;background:#301d24;color:#ffd6d2}@media(max-width:900px){.grid{grid-template-columns:repeat(2,minmax(0,1fr))}}@media(max-width:610px){.shell{padding-top:26px}.grid{grid-template-columns:1fr}}
</style></head><body><main class="shell">
<p class="eyebrow">Graphshell H4 · Personae</p>
<h1>Your identities are graph objects.</h1>
<p class="lede">Public keys, devices, grants, vault posture, and real signing requests share one portable surface. Private material remains inside the native authority.</p>
<section class="grid">
"#,
    );
    for projected in cards {
        write!(
            html,
            "<article class=\"card\" data-key=\"{}\"><header><h2>{}</h2><div class=\"badges\">",
            escape(&projected.key),
            escape(&projected.card.title),
        )
        .unwrap();
        for badge in projected.card.badges {
            write!(html, "<span class=\"badge\">{}</span>", escape(&badge)).unwrap();
        }
        html.push_str("</div></header><dl class=\"values\">");
        for row in projected.card.values {
            write!(
                html,
                "<div class=\"row\"><dt>{}</dt><dd>{}</dd></div>",
                escape(&row.label),
                escape(&row.value),
            )
            .unwrap();
        }
        html.push_str("</dl>");
        if !projected.actions.is_empty() {
            html.push_str("<div class=\"actions\">");
            for action in projected.actions {
                let request_id = action
                    .payload
                    .as_ref()
                    .and_then(|payload| payload.get("request_id"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let payload = action
                    .payload
                    .as_ref()
                    .map(Value::to_string)
                    .unwrap_or_default();
                write!(
                    html,
                    "<button class=\"{}\" data-intent=\"{}\" data-schema=\"{}\" data-request-id=\"{}\" data-payload=\"{}\" data-native-only=\"{}\">{}</button>",
                    if action.intent == SIGNING_DENY_INTENT { "deny" } else { "approve" },
                    action.intent,
                    action.schema,
                    request_id,
                    escape(&payload),
                    action.native_only,
                    escape(action.label),
                )
                .unwrap();
            }
            html.push_str("</div>");
        }
        html.push_str("</article>");
    }
    html.push_str("</section></main></body></html>\n");
    html
}

fn value(label: impl Into<String>, value: impl Into<String>) -> CardValueV1 {
    CardValueV1 {
        label: label.into(),
        value: value.into(),
    }
}

fn joined_or_unknown(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(", ")
    }
}

fn protection_label(protection: VaultProtectionView) -> &'static str {
    match protection {
        VaultProtectionView::OsProtected => "OS protected",
        VaultProtectionView::Passphrase => "passphrase",
        VaultProtectionView::Ephemeral => "ephemeral",
    }
}

fn lock_label(lock: VaultLockView) -> &'static str {
    match lock {
        VaultLockView::Locked => "locked",
        VaultLockView::Unlocked => "unlocked",
    }
}

fn listener_label(listener: &AgentListenerView) -> &str {
    match listener {
        AgentListenerView::StandaloneRetained => "standalone agent retained",
        AgentListenerView::ReceiptEndpoint { endpoint }
        | AgentListenerView::StandardEndpoint { endpoint } => endpoint,
    }
}

fn policy_label(policy: personae::signing::SigningPolicy) -> String {
    match policy {
        personae::signing::SigningPolicy::Session => "session".to_string(),
        personae::signing::SigningPolicy::ShortTtl { idle_seconds } => {
            format!("{idle_seconds}s idle")
        }
        personae::signing::SigningPolicy::PerUse => "every use".to_string(),
    }
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
    use personae::signing::{PendingSigningRequest, SigningPolicy, SigningRequest};

    use super::*;
    use crate::identity::{
        CarryView, DeviceGrantView, DeviceView, IdentitySurfaceSnapshot, SshKeyView, VaultView,
    };

    fn snapshot(policy: SigningPolicy) -> IdentitySurfaceSnapshot {
        let request = SigningRequest::new(
            "research",
            "SHA256:public",
            "ssh.sign",
            b"cleartext-never-projects",
            "graphshell.ssh",
        );
        IdentitySurfaceSnapshot {
            vault: VaultView {
                protection: VaultProtectionView::OsProtected,
                lock: VaultLockView::Unlocked,
                agent: AgentListenerView::StandaloneRetained,
            },
            profiles: Vec::new(),
            ssh_keys: Vec::new(),
            carry: CarryView::default(),
            pending_signing: vec![PendingSigningRequest {
                request,
                policy,
                expires_at_ms: 42,
            }],
            signing_history: Vec::new(),
        }
    }

    #[test]
    fn per_use_card_exposes_once_and_deny_but_cannot_widen_approval() {
        let cards = project_identity(&snapshot(SigningPolicy::PerUse));
        let pending = cards
            .iter()
            .find(|card| card.key.starts_with("identity:pending:"))
            .unwrap();
        assert_eq!(pending.actions.len(), 2);
        assert!(
            pending
                .actions
                .iter()
                .any(|action| action.intent == SIGNING_APPROVE_ONCE_INTENT)
        );
        assert!(
            pending
                .actions
                .iter()
                .any(|action| action.intent == SIGNING_DENY_INTENT)
        );
        assert!(
            !pending
                .actions
                .iter()
                .any(|action| action.intent == SIGNING_APPROVE_IDLE_INTENT)
        );
    }

    #[test]
    fn short_ttl_card_exposes_the_configured_idle_decision() {
        let cards = project_identity(&snapshot(SigningPolicy::ShortTtl { idle_seconds: 30 }));
        let pending = cards
            .iter()
            .find(|card| card.key.starts_with("identity:pending:"))
            .unwrap();
        assert!(
            pending
                .actions
                .iter()
                .any(|action| action.intent == SIGNING_APPROVE_IDLE_INTENT)
        );
    }

    #[test]
    fn headed_surface_contains_only_the_payload_digest() {
        let html = render_identity_surface(&snapshot(SigningPolicy::PerUse));
        assert!(html.contains("Payload digest"));
        assert!(html.contains("Approve once"));
        assert!(html.contains("Deny"));
        assert!(!html.contains("cleartext-never-projects"));
    }

    #[test]
    fn key_controls_are_native_only_and_bind_no_private_payload() {
        let mut snapshot = snapshot(SigningPolicy::PerUse);
        snapshot.ssh_keys.push(SshKeyView {
            profile: "research".to_string(),
            fingerprint: "SHA256:public".to_string(),
            comment: "workstation".to_string(),
            public_openssh: "ssh-ed25519 AAAA-public workstation".to_string(),
            lineage: "locally generated, externally registered".to_string(),
            device_loss_note: "register a replacement".to_string(),
            unlock_policy: "every use".to_string(),
        });
        let cards = project_identity(&snapshot);
        let vault = cards
            .iter()
            .find(|card| card.key == "identity:vault")
            .unwrap();
        assert_eq!(vault.actions.len(), 2);
        assert!(vault.actions.iter().all(|action| action.native_only));
        assert!(vault.actions.iter().all(|action| action.payload.is_none()));

        let key = cards
            .iter()
            .find(|card| card.key == "identity:ssh:SHA256:public")
            .unwrap();
        let remove = key
            .actions
            .iter()
            .find(|action| action.intent == SSH_REMOVE_INTENT)
            .unwrap();
        let payload: RemoveSshKeyIntentV1 =
            serde_json::from_value(remove.payload.clone().unwrap()).unwrap();
        assert_eq!(payload.fingerprint, "SHA256:public");
        assert!(!payload.confirmed);
        assert!(
            !serde_json::to_string(remove.payload.as_ref().unwrap())
                .unwrap()
                .contains("private")
        );
    }

    #[test]
    fn every_persona_but_the_selected_one_offers_the_switch() {
        use crate::identity::ProfileView;
        let mut snapshot = snapshot(SigningPolicy::PerUse);
        for (id, selected) in [("work", true), ("personal", false)] {
            snapshot.profiles.push(ProfileView {
                id: id.to_string(),
                display_name: id.to_string(),
                selected,
                slot_count: 0,
                master_public_fingerprint: "blake3:test".to_string(),
            });
        }
        let cards = project_identity(&snapshot);

        let selected = cards
            .iter()
            .find(|card| card.key == "identity:profile:work")
            .unwrap();
        assert!(
            selected.actions.is_empty(),
            "the persona in use has nothing to become"
        );

        let other = cards
            .iter()
            .find(|card| card.key == "identity:profile:personal")
            .unwrap();
        let switch = other
            .actions
            .iter()
            .find(|action| action.intent == PROFILE_SWITCH_INTENT)
            .unwrap();
        assert!(switch.native_only, "switching mutates the resident vault");
        let payload: SwitchProfileIntentV1 =
            serde_json::from_value(switch.payload.clone().unwrap()).unwrap();
        assert_eq!(
            payload.profile, "personal",
            "activation resolves by identity, not by row position"
        );
    }

    #[test]
    fn active_device_advertises_confirmed_revocation_and_revoked_grant_is_inert() {
        let device_id = Uuid::from_u128(0x1234);
        let mut snapshot = snapshot(SigningPolicy::PerUse);
        snapshot.carry.devices.push(DeviceView {
            device_id: device_id.to_string(),
            label: "Pocket relay".to_string(),
            mode: "delegated".to_string(),
            exposure: "hidden client".to_string(),
            public_key_fingerprint: "blake3:public".to_string(),
            revoked: false,
            grant_ref: Some("grant:public".to_string()),
        });
        snapshot.carry.grants.push(DeviceGrantView {
            device_id: device_id.to_string(),
            grant_ref: Some("grant:public".to_string()),
            signature_valid: Some(true),
            issued_at_ms: 1,
            expires_at_ms: Some(2),
            personas: vec!["persona:public".to_string()],
            scopes: vec!["identity.act".to_string()],
            attenuations: vec!["no-subdelegation".to_string()],
            wrapped_epoch_count: 0,
        });

        let active = project_identity(&snapshot);
        let device = active
            .iter()
            .find(|card| card.key == format!("identity:device:{device_id}"))
            .unwrap();
        let revoke = device
            .actions
            .iter()
            .find(|action| action.intent == DEVICE_REVOKE_INTENT)
            .unwrap();
        assert!(revoke.native_only);
        let payload: RevokeDeviceIntentV1 =
            serde_json::from_value(revoke.payload.clone().unwrap()).unwrap();
        assert_eq!(payload.device_id, device_id);
        assert!(!payload.confirmed);

        snapshot.carry.devices[0].revoked = true;
        let revoked = project_identity(&snapshot);
        let device = revoked
            .iter()
            .find(|card| card.key == format!("identity:device:{device_id}"))
            .unwrap();
        assert!(device.actions.is_empty());
        assert!(device.card.badges.iter().any(|badge| badge == "revoked"));
        let grant = revoked
            .iter()
            .find(|card| card.key == format!("identity:grant:{device_id}"))
            .unwrap();
        assert!(grant.actions.is_empty());
        assert!(grant.card.badges.iter().any(|badge| badge == "revoked"));
    }
}
