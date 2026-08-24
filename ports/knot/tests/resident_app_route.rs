//! R1 receipt: a first-party Turnstone client opens Knot through the resident
//! application door, after both local admission and an app-to-route grant.

use std::sync::Arc;
use std::time::Duration;

use graphshell::identity::VaultProtectionView;
use graphshell::native::app_admission::{AllowedAppRoutes, AppId, AppRouteId};
use graphshell::native::app_broker::{AppEndpointCatalog, serve_app_broker};
use graphshell::native::app_client::AppBrokerClient;
use graphshell::native::endpoint_catalog::{ResidentEndpointCatalog, ResidentEndpointRoute};
use graphshell::native::personae_host::PersonaeHost;
use personae::{Ed25519Keypair, IdentityVault, InMemoryStorage, Profile, ProfileId};

fn resident_host() -> Arc<PersonaeHost<InMemoryStorage>> {
    let profile = Profile::new(
        ProfileId("default".into()),
        "Default",
        Ed25519Keypair::from_seed([0xA1; 32]),
    );
    Arc::new(PersonaeHost::new(
        IdentityVault::with_profile(InMemoryStorage::new(), profile),
        None,
        VaultProtectionView::Ephemeral,
    ))
}

#[tokio::test(flavor = "multi_thread")]
async fn turnstone_opens_the_in_memory_knot_route() {
    let mut catalog = ResidentEndpointCatalog::new();
    catalog
        .register("knot", "Knot fixture", |_| {
            Ok(knot_editor::KnotEndpoint::fixture())
        })
        .unwrap();
    let route = ResidentEndpointRoute::new("knot", Duration::from_millis(10)).unwrap();
    let grants = AllowedAppRoutes::new([(AppId::new("turnstone"), route)]);

    #[cfg(windows)]
    let endpoint = format!(r"\\.\pipe\graphshell-knot-route-{}", uuid::Uuid::new_v4());
    #[cfg(not(windows))]
    let endpoint = std::env::temp_dir()
        .join(format!(
            "graphshell-knot-route-{}.sock",
            uuid::Uuid::new_v4()
        ))
        .display()
        .to_string();

    let server_endpoint = endpoint.clone();
    let server = tokio::spawn(async move {
        let _ = serve_app_broker(
            &server_endpoint,
            resident_host(),
            grants,
            60_000,
            None,
            AppEndpointCatalog::new(catalog),
        )
        .await;
    });

    let mut client = None;
    let mut last_error = String::new();
    for _ in 0..50 {
        match AppBrokerClient::open_route_at(
            &endpoint,
            AppId::new("turnstone"),
            AppRouteId::new("knot").unwrap(),
        )
        .await
        {
            Ok(open) => {
                client = Some(open);
                break;
            }
            Err(error) => {
                last_error = error.to_string();
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
    }
    let mut client =
        client.unwrap_or_else(|| panic!("the Knot route never opened, last: {last_error}"));
    let opened = client.open_session().await.unwrap();
    let request = opened.descriptor.projections[0].request.clone();
    let snapshot = client.snapshot(request).await.unwrap();
    assert_eq!(
        snapshot.scene.active_item_count(),
        3,
        "the selected endpoint is Knot's deterministic fixture",
    );
    client.close().await.unwrap();
    server.abort();
}
