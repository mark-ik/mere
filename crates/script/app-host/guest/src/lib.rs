//! The `app-core` test guest: a component that acts on the host app through
//! the action ENVELOPE, and reports what came back.
//!
//! It deliberately tries things it should not be allowed to do — an
//! out-of-ring action, gate management (the self-escalation attempt), an
//! action name this host does not know, and a malformed payload — and logs
//! each typed refusal. That is the point of the contract: a denial is a value
//! the component can read and adapt to, not a trap.

wit_bindgen::generate!({
    path: "../../wit",
    world: "app-core",
});

use crate::mere::script::actions::{emit, ActionEnvelope, EmitError};
use crate::mere::script::caps::granted;
use crate::mere::script::log::log;

struct Component;

fn describe(error: &EmitError) -> String {
    match error {
        EmitError::Denied(why) => format!("denied({why})"),
        EmitError::Unknown(name) => format!("unknown({name})"),
        EmitError::Malformed(why) => format!("malformed({why})"),
    }
}

/// Emit one envelope and log the outcome either way.
fn try_emit(name: &str, payload: &str) {
    let envelope = ActionEnvelope {
        name: name.to_string(),
        payload: payload.to_string(),
    };
    match emit(&envelope) {
        Ok(()) => log(&format!("guest: {name} accepted")),
        Err(error) => log(&format!("guest: {name} refused: {}", describe(&error))),
    }
}

impl Guest for Component {
    fn activate() {
        let caps = granted();
        log(&format!("guest: activated; granted [{}]", caps.join(", ")));
    }

    fn deactivate() {
        log("guest: deactivated");
    }

    fn on_event(kind: String, payload: String) {
        log(&format!("guest: event '{kind}'"));
        match kind.as_str() {
            // The ordinary case: an action inside the grant.
            "browse" => {
                try_emit("open-address", &format!("{{\"url\": \"{payload}\"}}"));
                try_emit("fit-view", "");
            }
            // An action in a ring this denizen was not granted.
            "reach" => try_emit("close-session", ""),
            // Gate management: no grant can ever cover it. A component that
            // could confirm its own install review would be self-escalating.
            "escalate" => {
                try_emit("confirm-install-denizen", "");
                try_emit("install-denizen", "{\"path\": \"evil.lua\"}");
            }
            // Misfires must be loud, never silent no-ops.
            "misfire" => {
                try_emit("summon-the-kraken", "");
                try_emit("open-address", "{}");
            }
            other => log(&format!("guest: nothing to do for '{other}'")),
        }
    }
}

export!(Component);
