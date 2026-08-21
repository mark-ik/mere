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
//! known name is talking*, the host-owned route table grants a local route,
//! and the ordinary session admission behind both still decides what that app
//! may see. The self-declared hello grants nothing by itself.
//!
//! **What actually proves "first-party" is the endpoint's own permissions.**
//! The socket lives in the user's runtime directory and the named pipe is
//! created for the current user, so reaching it at all means running as the
//! owner. The app id below is a *label*, not a credential: it tells the host
//! and the operator which application is connected, and lets an owner turn
//! one off. Treating a self-declared name as proof would be theatre, and it
//! is not treated as such.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::endpoint_catalog::ResidentEndpointRoute;

/// Environment override for the first-party application endpoint.
pub const APP_ENDPOINT_ENV: &str = "GRAPHSHELL_APP_ENDPOINT";

/// The hello schema a first-party application opens with.
///
/// Distinct from the browser broker's, so a client that reaches the wrong
/// endpoint is refused with a clear reason rather than half-speaking the
/// other protocol.
pub const APP_HELLO_SCHEMA: &str = "mere.graphshell/app-broker-hello/v1";

/// Route-aware hello schema. Version one remains the installed-client
/// identity default; version two names one host-configured resident route.
pub const APP_ROUTE_HELLO_SCHEMA: &str = "mere.graphshell/app-broker-hello/v2";

/// The route selected by the version-one hello.
pub const APP_IDENTITY_ROUTE: &str = "identity";

/// Default notice cadence for the legacy identity grant. The cadence is
/// ignored by that route, while callers granting catalog routes supply their
/// own value through [`ResidentEndpointRoute`].
const DEFAULT_IDENTITY_NOTICE_POLL: Duration = Duration::from_millis(50);

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

/// One host-local resident route key.
///
/// This is a routing label, not authority. Authority comes from the
/// host-owned [`AllowedAppRoutes`] entry selected after the owner-only door is
/// reached.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AppRouteId(String);

impl AppRouteId {
    /// Validate and name a host-local resident route.
    pub fn new(id: impl Into<String>) -> Result<Self, AppAdmissionError> {
        let id = id.into();
        if id.is_empty() || id.chars().any(char::is_whitespace) {
            return Err(AppAdmissionError::InvalidRoute(id));
        }
        Ok(Self(id))
    }

    /// The route selected by an installed version-one client.
    pub fn identity() -> Self {
        Self(APP_IDENTITY_ROUTE.to_string())
    }

    /// The exact host-local routing key.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for AppRouteId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
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
    /// The application is known, but this route is not in its host grant.
    RouteNotGranted { app: AppId, route: AppRouteId },
    /// The hello named no application.
    Unnamed,
    /// The route key was empty or ambiguous.
    InvalidRoute(String),
    /// A version-one hello attempted to smuggle a route selection.
    RouteOnLegacyHello,
}

impl std::fmt::Display for AppAdmissionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongSchema(found) => write!(
                f,
                "expected the first-party hello {APP_HELLO_SCHEMA} or \
                 {APP_ROUTE_HELLO_SCHEMA}, got {found}; \
                 a browser belongs on the browser endpoint"
            ),
            Self::NotAllowed(app) => {
                write!(f, "{app} is not an application this device admits")
            }
            Self::RouteNotGranted { app, route } => {
                write!(f, "{app} is not granted the resident route {route}")
            }
            Self::Unnamed => f.write_str("the first-party hello named no application"),
            Self::InvalidRoute(route) => write!(
                f,
                "the first-party hello named an invalid resident route {route:?}"
            ),
            Self::RouteOnLegacyHello => {
                f.write_str("the version-one first-party hello cannot select a resident route")
            }
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
    /// Version two's requested resident route. Absent on the installed
    /// version-one hello, which always means [`APP_IDENTITY_ROUTE`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<AppRouteId>,
}

impl AppHello {
    /// Open a connection as `app`.
    pub fn new(app: AppId) -> Self {
        Self {
            schema: APP_HELLO_SCHEMA.to_string(),
            app,
            route: None,
        }
    }

    /// Open a route-aware connection as `app`.
    pub fn for_route(app: AppId, route: AppRouteId) -> Self {
        Self {
            schema: APP_ROUTE_HELLO_SCHEMA.to_string(),
            app,
            route: Some(route),
        }
    }

    /// Check the protocol and yield the application and route requested.
    pub fn accept(self) -> Result<AppRequest, AppAdmissionError> {
        // Transparent serde can bypass `AppId::new`; normalize the wire value
        // so case and surrounding spaces cannot create a second identity.
        let app = AppId::new(self.app.as_str());
        if app.as_str().is_empty() {
            return Err(AppAdmissionError::Unnamed);
        }
        let route = match self.schema.as_str() {
            APP_HELLO_SCHEMA => {
                if self.route.is_some() {
                    return Err(AppAdmissionError::RouteOnLegacyHello);
                }
                AppRouteId::identity()
            }
            APP_ROUTE_HELLO_SCHEMA => self
                .route
                .ok_or_else(|| AppAdmissionError::InvalidRoute(String::new()))?,
            _ => return Err(AppAdmissionError::WrongSchema(self.schema)),
        };
        // Transparent serde can construct the type without calling `new`, so
        // validate the wire value again at admission.
        let route = AppRouteId::new(route.0)?;
        Ok(AppRequest { app, route })
    }
}

/// The host-relevant facts from one accepted hello.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppRequest {
    pub app: AppId,
    pub route: AppRouteId,
}

/// The applications this device serves.
///
/// Default-deny with a named default set, the same posture as the browser
/// allowlist: an application the owner has not heard of does not get a
/// session by knowing the endpoint's name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AllowedAppRoutes {
    routes: BTreeMap<AppId, BTreeMap<AppRouteId, ResidentEndpointRoute>>,
}

impl Default for AllowedAppRoutes {
    fn default() -> Self {
        Self::new([(
            AppId::new("turnstone"),
            ResidentEndpointRoute::new(APP_IDENTITY_ROUTE, DEFAULT_IDENTITY_NOTICE_POLL)
                .expect("the identity route is valid"),
        )])
    }
}

impl AllowedAppRoutes {
    /// Admit exactly these application-to-route pairs. Repeating an
    /// application grants it more than one route.
    pub fn new(grants: impl IntoIterator<Item = (AppId, ResidentEndpointRoute)>) -> Self {
        let mut routes = BTreeMap::<AppId, BTreeMap<AppRouteId, ResidentEndpointRoute>>::new();
        for (app, route) in grants {
            let id = AppRouteId::new(route.id())
                .expect("ResidentEndpointRoute has already validated its id");
            routes.entry(app).or_default().insert(id, route);
        }
        Self { routes }
    }

    /// Admit none. A device that wants no first-party clients says so rather
    /// than leaving the endpoint unserved and looking broken.
    pub fn none() -> Self {
        Self {
            routes: BTreeMap::new(),
        }
    }

    /// Select the host-configured route granted to this request.
    pub fn admit(&self, request: &AppRequest) -> Result<ResidentEndpointRoute, AppAdmissionError> {
        let Some(routes) = self.routes.get(&request.app) else {
            return Err(AppAdmissionError::NotAllowed(request.app.clone()));
        };
        routes
            .get(&request.route)
            .cloned()
            .ok_or_else(|| AppAdmissionError::RouteNotGranted {
                app: request.app.clone(),
                route: request.route.clone(),
            })
    }

    /// The application and route pairs admitted, for reporting.
    pub fn iter(&self) -> impl Iterator<Item = (&AppId, &AppRouteId)> {
        self.routes
            .iter()
            .flat_map(|(app, routes)| routes.keys().map(move |route| (app, route)))
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
        let request = AppHello::new(AppId::new("TURNSTONE")).accept().unwrap();
        assert!(AllowedAppRoutes::default().admit(&request).is_ok());
    }

    #[test]
    fn an_unknown_application_is_refused() {
        let request = AppHello::new(AppId::new("someone-elses-app"))
            .accept()
            .unwrap();
        let error = AllowedAppRoutes::default().admit(&request).unwrap_err();
        assert_eq!(
            error,
            AppAdmissionError::NotAllowed(AppId::new("someone-elses-app"))
        );
    }

    #[test]
    fn a_device_can_admit_none() {
        let request = AppHello::new(AppId::new("turnstone")).accept().unwrap();
        assert!(AllowedAppRoutes::none().admit(&request).is_err());
        assert_eq!(AllowedAppRoutes::none().iter().count(), 0);
    }

    /// A browser reaching this endpoint is refused with a reason that says
    /// where it should have gone, rather than failing deeper in as a
    /// malformed frame.
    #[test]
    fn the_browser_hello_is_refused_by_schema() {
        let wrong = AppHello {
            schema: "mere.graphshell/device-broker-hello/v1".to_string(),
            app: AppId::new("turnstone"),
            route: None,
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
        assert_eq!(
            hello.clone().accept().unwrap(),
            AppRequest {
                app: AppId::new("turnstone"),
                route: AppRouteId::identity(),
            }
        );
        let json = serde_json::to_string(&hello).unwrap();
        assert!(
            !json.contains("route"),
            "the legacy hello stays byte-shaped"
        );
        assert_eq!(serde_json::from_str::<AppHello>(&json).unwrap(), hello);
    }

    #[test]
    fn a_version_two_hello_requests_one_granted_route() {
        let knot = AppRouteId::new("knot").unwrap();
        let hello = AppHello::for_route(AppId::new("turnstone"), knot.clone());
        assert_eq!(
            hello.clone().accept().unwrap(),
            AppRequest {
                app: AppId::new("turnstone"),
                route: knot.clone(),
            }
        );
        let grants = AllowedAppRoutes::new([(
            AppId::new("turnstone"),
            ResidentEndpointRoute::new("knot", Duration::from_millis(7)).unwrap(),
        )]);
        let selected = grants.admit(&hello.accept().unwrap()).unwrap();
        assert_eq!(selected.id(), "knot");
        assert_eq!(selected.notice_poll_interval(), Duration::from_millis(7));
    }

    #[test]
    fn a_legacy_hello_cannot_smuggle_a_route() {
        let hello = AppHello {
            schema: APP_HELLO_SCHEMA.to_string(),
            app: AppId::new("turnstone"),
            route: Some(AppRouteId::new("knot").unwrap()),
        };
        assert_eq!(
            hello.accept().unwrap_err(),
            AppAdmissionError::RouteOnLegacyHello
        );
    }

    #[test]
    fn an_ungranted_route_is_distinct_from_an_unknown_application() {
        let request =
            AppHello::for_route(AppId::new("turnstone"), AppRouteId::new("knot").unwrap())
                .accept()
                .unwrap();
        assert_eq!(
            AllowedAppRoutes::default().admit(&request).unwrap_err(),
            AppAdmissionError::RouteNotGranted {
                app: AppId::new("turnstone"),
                route: AppRouteId::new("knot").unwrap(),
            }
        );
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
