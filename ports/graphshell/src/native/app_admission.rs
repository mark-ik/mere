//! Admission for first-party native applications.
//!
//! The resident host already serves browsers, over an endpoint whose first
//! gate is an extension-id allowlist. A first-party application is a
//! different kind of caller and gets a **different endpoint**, rather than a
//! seat in that allowlist: the browser gate's whole job is to be narrow, and
//! widening it to admit a native app would weaken the one check keeping
//! arbitrary extensions out.
//!
//! **What this gate does and does not decide.** It is the first of two, the
//! same shape the browser path has: this says *a local first-party app of a
//! known name is talking*, and the ordinary session admission behind it still
//! decides what that app may see. Nothing here grants anything.
//!
//! **What actually proves "first-party" is the endpoint's own permissions.**
//! The socket lives in the user's runtime directory and the named pipe is
//! created for the current user, so reaching it at all means running as the
//! owner. The app id below is a *label*, not a credential: it tells the host
//! and the operator which application is connected, and lets an owner turn
//! one off. Treating a self-declared name as proof would be theatre, and it
//! is not treated as such.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// Environment override for the first-party application endpoint.
pub const APP_ENDPOINT_ENV: &str = "GRAPHSHELL_APP_ENDPOINT";

/// The hello schema a first-party application opens with.
///
/// Distinct from the browser broker's, so a client that reaches the wrong
/// endpoint is refused with a clear reason rather than half-speaking the
/// other protocol.
pub const APP_HELLO_SCHEMA: &str = "mere.graphshell/app-broker-hello/v1";

#[cfg(windows)]
const DEFAULT_WINDOWS_APP_ENDPOINT: &str = r"\\.\pipe\graphshell-device-app";

/// Which application is connecting.
///
/// A plain name rather than a signed identity: see the module note on why
/// this is a label. Lowercased on construction so `Turnstone` and `turnstone`
/// are one application rather than two, one of which is silently not allowed.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AppId(String);

impl AppId {
    /// Name an application.
    pub fn new(id: impl AsRef<str>) -> Self {
        Self(id.as_ref().trim().to_ascii_lowercase())
    }

    /// The name as matched.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for AppId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Why a first-party connection was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppAdmissionError {
    /// The hello was not this endpoint's protocol.
    WrongSchema(String),
    /// The application is not one this device admits.
    NotAllowed(AppId),
    /// The hello named no application.
    Unnamed,
}

impl std::fmt::Display for AppAdmissionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongSchema(found) => write!(
                f,
                "expected the first-party hello {APP_HELLO_SCHEMA}, got {found}; \
                 a browser belongs on the browser endpoint"
            ),
            Self::NotAllowed(app) => {
                write!(f, "{app} is not an application this device admits")
            }
            Self::Unnamed => f.write_str("the first-party hello named no application"),
        }
    }
}

impl std::error::Error for AppAdmissionError {}

/// First message on a first-party application connection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppHello {
    /// Always [`APP_HELLO_SCHEMA`].
    pub schema: String,
    /// Which application is speaking.
    pub app: AppId,
}

impl AppHello {
    /// Open a connection as `app`.
    pub fn new(app: AppId) -> Self {
        Self {
            schema: APP_HELLO_SCHEMA.to_string(),
            app,
        }
    }

    /// Check the protocol and yield the application named.
    pub fn accept(self) -> Result<AppId, AppAdmissionError> {
        if self.schema != APP_HELLO_SCHEMA {
            return Err(AppAdmissionError::WrongSchema(self.schema));
        }
        if self.app.as_str().is_empty() {
            return Err(AppAdmissionError::Unnamed);
        }
        Ok(self.app)
    }
}

/// The applications this device serves.
///
/// Default-deny with a named default set, the same posture as the browser
/// allowlist: an application the owner has not heard of does not get a
/// session by knowing the endpoint's name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AllowedApps {
    apps: BTreeSet<AppId>,
}

impl Default for AllowedApps {
    fn default() -> Self {
        Self {
            apps: [AppId::new("turnstone")].into_iter().collect(),
        }
    }
}

impl AllowedApps {
    /// Admit exactly these applications.
    pub fn new(apps: impl IntoIterator<Item = AppId>) -> Self {
        Self {
            apps: apps.into_iter().collect(),
        }
    }

    /// Admit none. A device that wants no first-party clients says so rather
    /// than leaving the endpoint unserved and looking broken.
    pub fn none() -> Self {
        Self {
            apps: BTreeSet::new(),
        }
    }

    /// Whether this application may open a session.
    pub fn admit(&self, app: &AppId) -> Result<(), AppAdmissionError> {
        self.apps
            .contains(app)
            .then_some(())
            .ok_or_else(|| AppAdmissionError::NotAllowed(app.clone()))
    }

    /// The applications admitted, for reporting.
    pub fn iter(&self) -> impl Iterator<Item = &AppId> {
        self.apps.iter()
    }
}

/// The per-user endpoint first-party applications connect to.
pub fn configured_app_endpoint() -> String {
    std::env::var(APP_ENDPOINT_ENV)
        .ok()
        .filter(|endpoint| !endpoint.trim().is_empty())
        .unwrap_or_else(default_app_endpoint)
}

fn default_app_endpoint() -> String {
    #[cfg(windows)]
    {
        DEFAULT_WINDOWS_APP_ENDPOINT.to_string()
    }
    #[cfg(not(windows))]
    {
        // The runtime directory, so reaching the socket means being the owner.
        // Falling back to the vault directory keeps a session possible on a
        // host without XDG, at the same user-only permissions.
        std::env::var_os("XDG_RUNTIME_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(personae::bootstrap::default_vault_dir)
            .join("graphshell-app.sock")
            .display()
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_ids_are_case_and_space_insensitive() {
        assert_eq!(AppId::new("  Turnstone "), AppId::new("turnstone"));
        assert!(
            AllowedApps::default()
                .admit(&AppId::new("TURNSTONE"))
                .is_ok()
        );
    }

    #[test]
    fn an_unknown_application_is_refused() {
        let error = AllowedApps::default()
            .admit(&AppId::new("someone-elses-app"))
            .unwrap_err();
        assert_eq!(
            error,
            AppAdmissionError::NotAllowed(AppId::new("someone-elses-app"))
        );
    }

    #[test]
    fn a_device_can_admit_none() {
        assert!(AllowedApps::none().admit(&AppId::new("turnstone")).is_err());
        assert_eq!(AllowedApps::none().iter().count(), 0);
    }

    /// A browser reaching this endpoint is refused with a reason that says
    /// where it should have gone, rather than failing deeper in as a
    /// malformed frame.
    #[test]
    fn the_browser_hello_is_refused_by_schema() {
        let wrong = AppHello {
            schema: "mere.graphshell/device-broker-hello/v1".to_string(),
            app: AppId::new("turnstone"),
        };
        let error = wrong.accept().unwrap_err();
        assert!(matches!(error, AppAdmissionError::WrongSchema(_)));
        assert!(
            error.to_string().contains("browser endpoint"),
            "the refusal points at the right endpoint, got: {error}",
        );
    }

    #[test]
    fn a_hello_naming_nothing_is_refused() {
        let unnamed = AppHello::new(AppId::new("   "));
        assert_eq!(unnamed.accept().unwrap_err(), AppAdmissionError::Unnamed);
    }

    #[test]
    fn a_well_formed_hello_yields_its_application() {
        let hello = AppHello::new(AppId::new("turnstone"));
        assert_eq!(hello.clone().accept().unwrap(), AppId::new("turnstone"));
        let json = serde_json::to_string(&hello).unwrap();
        assert_eq!(serde_json::from_str::<AppHello>(&json).unwrap(), hello);
    }

    #[test]
    fn the_endpoint_is_overridable_and_distinct_from_the_browser_one() {
        let default = default_app_endpoint();
        assert!(!default.is_empty());
        assert_ne!(
            default,
            crate::native::device_broker::configured_device_endpoint(),
            "a first-party app must not land on the browser endpoint",
        );
    }
}
