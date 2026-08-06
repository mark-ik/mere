//! Product endpoints selected by one resident Graphshell host.
//!
//! A catalog is an in-process composition seam. The host admits the carrier,
//! derives an [`AdmittedEndpointContext`], selects one registered endpoint,
//! and keeps its authority loop around that endpoint. Registrations receive no
//! vault, delegation, or transport: only the already-admitted session and
//! subject they must bind into product truth.

use std::collections::BTreeMap;
use std::fmt::Display;

use graphshell_endpoint::{
    IntentSink, PresentationSource, ProjectionCatalog, ProjectionNoticeSource, ProjectionSource,
};
use graphshell_protocol::{
    CarrierNotice, EndpointDescriptor, IntentInvocation, IntentResult, ProjectionRequest,
    ProjectionSnapshot, ResourceChunkRequest, ResourceChunkResponse, ResourceRequest,
    ResourceResponse, ResumeReply, ResumeRequest,
};

use crate::lifecycle::AdmittedEndpointContext;

/// One endpoint prepared for a resident admitted session.
///
/// This object-safe boundary is the catalog's type erasure. Product adapters
/// ordinarily enter through [`ResidentEndpointCatalog::register`] or
/// [`ResidentEndpointCatalog::register_notifying`], which preserve their
/// ordinary Graphshell endpoint implementations without exposing error types
/// across product registrations.
pub trait ResidentEndpoint: Send {
    fn describe(&self) -> EndpointDescriptor;
    fn snapshot(&mut self, request: ProjectionRequest) -> Result<ProjectionSnapshot, String>;
    fn resource(&mut self, request: ResourceRequest) -> Result<ResourceResponse, String>;

    fn resource_chunk(
        &mut self,
        request: ResourceChunkRequest,
    ) -> Result<ResourceChunkResponse, String> {
        let whole = self.resource(ResourceRequest {
            session: request.session,
            resource: request.resource,
        })?;
        Ok(ResourceChunkResponse::slice(
            &whole,
            request.offset,
            request.length,
        ))
    }

    fn invoke(&mut self, intent: IntentInvocation) -> Result<IntentResult, String>;

    /// A product that does not emit revision bells remains a valid resident
    /// endpoint. A notifying registration overrides this through the typed
    /// adapter below.
    fn poll_notice(&mut self) -> Result<Option<CarrierNotice>, String> {
        Ok(None)
    }

    /// Resume is product-specific. Registrations that need it can supply a
    /// hand-written [`ResidentEndpoint`] rather than make every endpoint claim
    /// a diff history it does not hold.
    fn resume(&mut self, _request: ResumeRequest) -> Result<ResumeReply, String> {
        Err("endpoint does not support projection resume".to_string())
    }
}

type Factory =
    dyn FnMut(&AdmittedEndpointContext) -> Result<Box<dyn ResidentEndpoint>, String> + Send;

struct Registration {
    label: String,
    factory: Box<Factory>,
}

/// A host-visible registered endpoint. The id is a local routing key, not a
/// browser-supplied authority claim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResidentEndpointOffer {
    pub id: String,
    pub label: String,
}

/// Why resident endpoint selection failed before the session loop started.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ResidentEndpointCatalogError {
    #[error("resident endpoint id is empty or contains whitespace: {id:?}")]
    InvalidId { id: String },
    #[error("resident endpoint {id:?} is already registered")]
    Duplicate { id: String },
    #[error("resident endpoint {id:?} is not registered")]
    Unknown { id: String },
    #[error("resident endpoint {id:?} could not open: {reason}")]
    Open { id: String, reason: String },
}

/// Host-owned registrations for the endpoints it can compose.
#[derive(Default)]
pub struct ResidentEndpointCatalog {
    registrations: BTreeMap<String, Registration>,
}

impl ResidentEndpointCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    /// List local route keys and labels without opening product state.
    pub fn offers(&self) -> Vec<ResidentEndpointOffer> {
        self.registrations
            .iter()
            .map(|(id, registration)| ResidentEndpointOffer {
                id: id.clone(),
                label: registration.label.clone(),
            })
            .collect()
    }

    /// Register a silent endpoint factory. The factory runs once when this
    /// catalog selects the route for an already-admitted session.
    pub fn register<E, F>(
        &mut self,
        id: impl Into<String>,
        label: impl Into<String>,
        mut factory: F,
    ) -> Result<(), ResidentEndpointCatalogError>
    where
        E: ProjectionCatalog + ProjectionSource + PresentationSource + IntentSink + Send + 'static,
        <E as ProjectionSource>::Error: Display,
        <E as PresentationSource>::Error: Display,
        <E as IntentSink>::Error: Display,
        F: FnMut(&AdmittedEndpointContext) -> Result<E, String> + Send + 'static,
    {
        self.insert(id.into(), label.into(), move |context| {
            let endpoint = factory(context)?;
            Ok(Box::new(TypedResidentEndpoint::silent(endpoint)))
        })
    }

    /// Register an endpoint that supplies revision notices. It is otherwise
    /// the same composition contract as [`Self::register`].
    pub fn register_notifying<E, F>(
        &mut self,
        id: impl Into<String>,
        label: impl Into<String>,
        mut factory: F,
    ) -> Result<(), ResidentEndpointCatalogError>
    where
        E: ProjectionCatalog
            + ProjectionSource
            + PresentationSource
            + IntentSink
            + ProjectionNoticeSource
            + Send
            + 'static,
        <E as ProjectionSource>::Error: Display,
        <E as PresentationSource>::Error: Display,
        <E as IntentSink>::Error: Display,
        <E as ProjectionNoticeSource>::Error: Display,
        F: FnMut(&AdmittedEndpointContext) -> Result<E, String> + Send + 'static,
    {
        self.insert(id.into(), label.into(), move |context| {
            let endpoint = factory(context)?;
            Ok(Box::new(TypedResidentEndpoint::notifying(endpoint)))
        })
    }

    /// Open exactly one endpoint under an already-admitted context.
    pub fn open(
        &mut self,
        id: &str,
        context: &AdmittedEndpointContext,
    ) -> Result<ResidentEndpointSession, ResidentEndpointCatalogError> {
        let registration = self
            .registrations
            .get_mut(id)
            .ok_or_else(|| ResidentEndpointCatalogError::Unknown { id: id.to_string() })?;
        let endpoint = (registration.factory)(context).map_err(|reason| {
            ResidentEndpointCatalogError::Open {
                id: id.to_string(),
                reason,
            }
        })?;
        Ok(ResidentEndpointSession { endpoint })
    }

    fn insert(
        &mut self,
        id: String,
        label: String,
        factory: impl FnMut(&AdmittedEndpointContext) -> Result<Box<dyn ResidentEndpoint>, String>
        + Send
        + 'static,
    ) -> Result<(), ResidentEndpointCatalogError> {
        if id.is_empty() || id.chars().any(char::is_whitespace) {
            return Err(ResidentEndpointCatalogError::InvalidId { id });
        }
        if self.registrations.contains_key(&id) {
            return Err(ResidentEndpointCatalogError::Duplicate { id });
        }
        self.registrations.insert(
            id,
            Registration {
                label,
                factory: Box::new(factory),
            },
        );
        Ok(())
    }
}

/// The erased endpoint selected for one admitted session.
pub struct ResidentEndpointSession {
    endpoint: Box<dyn ResidentEndpoint>,
}

impl ResidentEndpointSession {
    pub fn resume(&mut self, request: ResumeRequest) -> Result<ResumeReply, String> {
        self.endpoint.resume(request)
    }
}

impl ProjectionCatalog for ResidentEndpointSession {
    fn describe(&self) -> EndpointDescriptor {
        self.endpoint.describe()
    }
}

impl ProjectionSource for ResidentEndpointSession {
    type Error = String;

    fn snapshot(&mut self, request: ProjectionRequest) -> Result<ProjectionSnapshot, Self::Error> {
        self.endpoint.snapshot(request)
    }
}

impl PresentationSource for ResidentEndpointSession {
    type Error = String;

    fn resource(&mut self, request: ResourceRequest) -> Result<ResourceResponse, Self::Error> {
        self.endpoint.resource(request)
    }

    fn resource_chunk(
        &mut self,
        request: ResourceChunkRequest,
    ) -> Result<ResourceChunkResponse, Self::Error> {
        self.endpoint.resource_chunk(request)
    }
}

impl IntentSink for ResidentEndpointSession {
    type Error = String;

    fn invoke(&mut self, intent: IntentInvocation) -> Result<IntentResult, Self::Error> {
        self.endpoint.invoke(intent)
    }
}

impl ProjectionNoticeSource for ResidentEndpointSession {
    type Error = String;

    fn poll_notice(&mut self) -> Result<Option<CarrierNotice>, Self::Error> {
        self.endpoint.poll_notice()
    }
}

enum NoticeMode<E> {
    Silent,
    Poll(fn(&mut E) -> Result<Option<CarrierNotice>, String>),
}

struct TypedResidentEndpoint<E> {
    endpoint: E,
    notices: NoticeMode<E>,
}

impl<E> TypedResidentEndpoint<E> {
    fn silent(endpoint: E) -> Self {
        Self {
            endpoint,
            notices: NoticeMode::Silent,
        }
    }
}

impl<E> TypedResidentEndpoint<E>
where
    E: ProjectionNoticeSource,
    E::Error: Display,
{
    fn notifying(endpoint: E) -> Self {
        Self {
            endpoint,
            notices: NoticeMode::Poll(poll_notice::<E>),
        }
    }
}

fn poll_notice<E>(endpoint: &mut E) -> Result<Option<CarrierNotice>, String>
where
    E: ProjectionNoticeSource,
    E::Error: Display,
{
    endpoint.poll_notice().map_err(|error| error.to_string())
}

impl<E> ResidentEndpoint for TypedResidentEndpoint<E>
where
    E: ProjectionCatalog + ProjectionSource + PresentationSource + IntentSink + Send,
    <E as ProjectionSource>::Error: Display,
    <E as PresentationSource>::Error: Display,
    <E as IntentSink>::Error: Display,
{
    fn describe(&self) -> EndpointDescriptor {
        self.endpoint.describe()
    }

    fn snapshot(&mut self, request: ProjectionRequest) -> Result<ProjectionSnapshot, String> {
        self.endpoint
            .snapshot(request)
            .map_err(|error| error.to_string())
    }

    fn resource(&mut self, request: ResourceRequest) -> Result<ResourceResponse, String> {
        self.endpoint
            .resource(request)
            .map_err(|error| error.to_string())
    }

    fn resource_chunk(
        &mut self,
        request: ResourceChunkRequest,
    ) -> Result<ResourceChunkResponse, String> {
        self.endpoint
            .resource_chunk(request)
            .map_err(|error| error.to_string())
    }

    fn invoke(&mut self, intent: IntentInvocation) -> Result<IntentResult, String> {
        self.endpoint
            .invoke(intent)
            .map_err(|error| error.to_string())
    }

    fn poll_notice(&mut self) -> Result<Option<CarrierNotice>, String> {
        match &self.notices {
            NoticeMode::Silent => Ok(None),
            NoticeMode::Poll(poll) => poll(&mut self.endpoint),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use graphshell_protocol::ProjectionSession;

    #[derive(Default)]
    struct FixtureEndpoint {
        polls: Arc<Mutex<u32>>,
    }

    impl ProjectionCatalog for FixtureEndpoint {
        fn describe(&self) -> EndpointDescriptor {
            EndpointDescriptor {
                label: "Fixture endpoint".to_string(),
                projections: Vec::new(),
            }
        }
    }

    impl ProjectionSource for FixtureEndpoint {
        type Error = String;

        fn snapshot(&mut self, _: ProjectionRequest) -> Result<ProjectionSnapshot, Self::Error> {
            Err("fixture has no snapshot".to_string())
        }
    }

    impl PresentationSource for FixtureEndpoint {
        type Error = String;

        fn resource(&mut self, _: ResourceRequest) -> Result<ResourceResponse, Self::Error> {
            Err("fixture has no resource".to_string())
        }
    }

    impl IntentSink for FixtureEndpoint {
        type Error = String;

        fn invoke(&mut self, _: IntentInvocation) -> Result<IntentResult, Self::Error> {
            Err("fixture has no intent".to_string())
        }
    }

    impl ProjectionNoticeSource for FixtureEndpoint {
        type Error = String;

        fn poll_notice(&mut self) -> Result<Option<CarrierNotice>, Self::Error> {
            *self.polls.lock().unwrap() += 1;
            Ok(None)
        }
    }

    #[test]
    fn catalog_binds_one_named_notifying_endpoint_to_the_admitted_context() {
        let context = AdmittedEndpointContext::new(
            ProjectionSession("admitted:catalog".to_string()),
            [0xC0; 32],
        );
        let seen = Arc::new(Mutex::new(None));
        let polls = Arc::new(Mutex::new(0));
        let mut catalog = ResidentEndpointCatalog::new();
        let factory_seen = Arc::clone(&seen);
        let factory_polls = Arc::clone(&polls);
        catalog
            .register_notifying("cleromancy", "Local Cleromancy readings", move |context| {
                *factory_seen.lock().unwrap() = Some(context.clone());
                Ok(FixtureEndpoint {
                    polls: Arc::clone(&factory_polls),
                })
            })
            .unwrap();

        assert_eq!(
            catalog.offers(),
            vec![ResidentEndpointOffer {
                id: "cleromancy".to_string(),
                label: "Local Cleromancy readings".to_string(),
            }]
        );
        let mut endpoint = catalog.open("cleromancy", &context).unwrap();
        assert_eq!(*seen.lock().unwrap(), Some(context));
        assert_eq!(endpoint.describe().label, "Fixture endpoint");
        assert_eq!(endpoint.poll_notice().unwrap(), None);
        assert_eq!(*polls.lock().unwrap(), 1);
    }

    #[test]
    fn catalog_refuses_ambiguous_or_unknown_routes() {
        let mut catalog = ResidentEndpointCatalog::new();
        catalog
            .register("identity", "Identity", |_| Ok(FixtureEndpoint::default()))
            .unwrap();
        assert!(matches!(
            catalog.register("identity", "Duplicate", |_| Ok(FixtureEndpoint::default())),
            Err(ResidentEndpointCatalogError::Duplicate { .. })
        ));
        assert!(matches!(
            catalog.register("not a route", "Invalid", |_| Ok(FixtureEndpoint::default())),
            Err(ResidentEndpointCatalogError::InvalidId { .. })
        ));
        assert!(matches!(
            catalog.open(
                "missing",
                &AdmittedEndpointContext::new(
                    ProjectionSession("admitted:missing".to_string()),
                    [0; 32]
                ),
            ),
            Err(ResidentEndpointCatalogError::Unknown { .. })
        ));
    }
}
