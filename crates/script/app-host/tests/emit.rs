// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The envelope lane end to end over a real component: a guest emits app
//! actions, the host's sink decides, and the guest reads each refusal.
//!
//! The sink here is a STUB of turnstone's ring gate (this crate is
//! app-agnostic — it has no action vocabulary of its own). What it proves is
//! the mechanism: an accepted emission is queued for the app to lower after
//! the turn, a refused one never enters the queue, and the guest learns
//! which kind of refusal it hit.

use std::path::PathBuf;

use app_host::{ActionSink, AppScript, Refusal};
use wasmtime::{Engine, StoreLimits};

/// Where build.rs left the guest component.
fn guest_component() -> PathBuf {
    if let Some(path) = std::env::var_os("APP_HOST_GUEST_WASM") {
        return PathBuf::from(path);
    }
    let path = PathBuf::from("guest/target/wasm32-wasip2/release/app_core_guest.wasm");
    assert!(
        path.exists(),
        "guest component missing at {}. Build it with:\n  \
         cd crates/script/app-host/guest && cargo build --target wasm32-wasip2 --release",
        path.display()
    );
    path
}

/// A stand-in for turnstone's ring gate: `granted` names pass, known-but-
/// ungranted names are denied by ring, unknown names are unknown, and
/// `open-address` needs a `url` in its payload.
struct StubGate {
    granted: Vec<&'static str>,
    accepted: Vec<(String, String)>,
    refusals: Vec<Refusal>,
}

impl StubGate {
    fn new(granted: Vec<&'static str>) -> Self {
        Self {
            granted,
            accepted: Vec::new(),
            refusals: Vec::new(),
        }
    }

    fn ring_of(name: &str) -> Option<&'static str> {
        match name {
            "open-address" | "nav-back" => Some("navigate"),
            "fit-view" | "toggle-physics" => Some("dispatch"),
            "close-session" | "fork-focused-node" => Some("session"),
            // Gate management: classified, but to a ring with no path.
            "confirm-install-denizen" | "install-denizen" => Some("host-only"),
            _ => None,
        }
    }
}

impl ActionSink for StubGate {
    fn emit(&mut self, name: &str, payload: &str) -> Result<(), Refusal> {
        let outcome = (|| {
            let Some(ring) = Self::ring_of(name) else {
                return Err(Refusal::Unknown(name.to_string()));
            };
            if name == "open-address" && !payload.contains("\"url\"") {
                return Err(Refusal::Malformed("missing field `url`".to_string()));
            }
            if ring == "host-only" {
                return Err(Refusal::Denied(
                    "host-only: gate management is never grantable".to_string(),
                ));
            }
            if !self.granted.contains(&ring) {
                return Err(Refusal::Denied(format!("{ring}: not covered by this grant")));
            }
            Ok(())
        })();
        match outcome {
            Ok(()) => {
                self.accepted.push((name.to_string(), payload.to_string()));
                Ok(())
            }
            Err(refusal) => {
                self.refusals.push(refusal.clone());
                Err(refusal)
            }
        }
    }
}

async fn attached(granted: Vec<&'static str>) -> AppScript<StubGate> {
    let engine = Engine::default();
    let mut script = AppScript::attach(
        &engine,
        &guest_component(),
        StubGate::new(granted),
        vec!["mere:script/actions".to_string()],
        StoreLimits::default(),
        None,
    )
    .await
    .expect("the guest instantiates");
    script.activate().await.expect("activate");
    script
}

#[tokio::test(flavor = "current_thread")]
async fn granted_emissions_queue_for_lowering() {
    let mut script = attached(vec!["navigate", "dispatch"]).await;
    script
        .on_event("browse", "https://example.test/a")
        .await
        .expect("turn runs");

    let accepted: Vec<&str> = script
        .sink()
        .accepted
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(accepted, ["open-address", "fit-view"]);
    assert!(
        script.sink().accepted[0].1.contains("https://example.test/a"),
        "the payload crossed intact: {:?}",
        script.sink().accepted[0].1
    );
    assert!(script.sink().refusals.is_empty());
    assert!(
        script.logs().iter().any(|l| l.contains("open-address accepted")),
        "the guest saw its own success: {:?}",
        script.logs()
    );
    script.deactivate().await.expect("deactivate");
}

#[tokio::test(flavor = "current_thread")]
async fn an_ungranted_ring_is_denied_and_never_queues() {
    let mut script = attached(vec!["navigate"]).await;
    script.on_event("reach", "").await.expect("turn runs");

    assert!(
        script.sink().accepted.is_empty(),
        "nothing outside the grant reaches the app"
    );
    assert_eq!(
        script.sink().refusals,
        vec![Refusal::Denied("session: not covered by this grant".to_string())]
    );
    assert!(
        script
            .logs()
            .iter()
            .any(|l| l.contains("close-session refused: denied(session")),
        "the guest reads the ring it lacked: {:?}",
        script.logs()
    );
}

#[tokio::test(flavor = "current_thread")]
async fn gate_management_is_refused_even_when_everything_else_is_granted() {
    // The self-escalation guard, over the real boundary: a component holding
    // every grantable ring still cannot confirm its own install review.
    let mut script = attached(vec!["navigate", "dispatch", "session", "panes"]).await;
    script.on_event("escalate", "").await.expect("turn runs");

    assert!(script.sink().accepted.is_empty(), "nothing escalated");
    assert_eq!(script.sink().refusals.len(), 2);
    assert!(
        script
            .sink()
            .refusals
            .iter()
            .all(|r| matches!(r, Refusal::Denied(why) if why.contains("host-only"))),
        "both attempts hit the structural floor: {:?}",
        script.sink().refusals
    );
}

#[tokio::test(flavor = "current_thread")]
async fn misfires_are_loud_to_the_guest() {
    let mut script = attached(vec!["navigate", "dispatch"]).await;
    script.on_event("misfire", "").await.expect("turn runs");

    assert!(script.sink().accepted.is_empty());
    assert_eq!(
        script.sink().refusals,
        vec![
            Refusal::Unknown("summon-the-kraken".to_string()),
            Refusal::Malformed("missing field `url`".to_string()),
        ],
        "an unknown name and a bad payload are distinct, named outcomes"
    );
    let logs = script.logs().join("\n");
    assert!(logs.contains("unknown(summon-the-kraken)"), "{logs}");
    assert!(logs.contains("malformed(missing field `url`)"), "{logs}");
}
