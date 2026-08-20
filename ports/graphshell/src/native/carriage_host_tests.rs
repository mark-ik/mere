//! Proof tests for the carriage host, split out for the file ceiling.
//!
//! A `#[path]` child of `carriage_host`, so private internals (`scan_held`,
//! the store field) stay reachable without widening their visibility.

use super::*;

mod lane {
    use super::*;
    use personae::InMemoryProvider;

    const GRAPH: [u8; 32] = [0x81; 32];

    fn issuer() -> Ed25519Keypair {
        Ed25519Keypair::from_seed([0x82; 32])
    }

    fn slot() -> BlindedSlotId {
        pandect::blinded_slot_id(personae::delegation::DelegationId([0x83; 32]), [0x84; 32])
    }

    fn config(path: PathBuf, tickets: Vec<String>) -> CarriageHostConfig {
        CarriageHostConfig {
            graph: GRAPH,
            store_path: path,
            trusted_roots: vec![issuer().public_key().to_bytes()],
            ceilings: CarriageCeilings::default(),
            peer_tickets: tickets,
            paired_nodes: Vec::new(),
        }
    }

    /// The lane's whole point, demonstrated end to end: a peer that never
    /// held the record recovers it over the wire while the lease is live,
    /// without re-pairing, and a superseded version is replaced rather than
    /// accumulated.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_peer_recovers_a_live_slot_and_supersession_replaces_it() {
        let directory = tempfile::tempdir().unwrap();
        let wallet_device = InMemoryProvider::from_seed([0x85; 32]);
        let replica_device = InMemoryProvider::from_seed([0x86; 32]);

        let wallet = CarriageHost::open(
            &wallet_device,
            config(directory.path().join("wallet.redb"), Vec::new()),
        )
        .await
        .unwrap();
        let replica = CarriageHost::open(
            &replica_device,
            config(
                directory.path().join("replica.redb"),
                vec![wallet.ticket().await.unwrap()],
            ),
        )
        .await
        .unwrap();

        let lease_expiry = now_ms() + 60_000;
        wallet
            .publish_slot(
                &issuer(),
                slot(),
                1,
                lease_expiry,
                b"wrapped-record-v1".to_vec(),
                CarriageCeilings::default(),
            )
            .await
            .unwrap();

        // The replica learns the slot from sync alone; nothing hands it over.
        let mut recovered = None;
        for _ in 0..100 {
            recovered = replica.recover(slot()).await.unwrap();
            if recovered.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        assert_eq!(
            recovered.as_deref(),
            Some(b"wrapped-record-v1".as_slice()),
            "the replica must serve back exactly what the wallet published"
        );

        // Supersession: issue 2 replaces issue 1 on the replica, and the
        // replica never accumulates history it could be harvested for.
        wallet
            .publish_slot(
                &issuer(),
                slot(),
                2,
                lease_expiry,
                b"wrapped-record-v2".to_vec(),
                CarriageCeilings::default(),
            )
            .await
            .unwrap();
        let mut superseded = None;
        for _ in 0..100 {
            superseded = replica.recover(slot()).await.unwrap();
            if superseded.as_deref() == Some(b"wrapped-record-v2".as_slice()) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        assert_eq!(superseded.as_deref(), Some(b"wrapped-record-v2".as_slice()));
        let stale = scan_held(&replica.store, GRAPH).await.unwrap();
        assert_eq!(
            stale.get(&slot()).map(|lease| lease.issue),
            Some(2),
            "the store holds the head version only"
        );

        wallet.close().await.unwrap();
        replica.close().await.unwrap();
    }

    /// Ruling 4's two enforcement points, on one host: an expired lease is
    /// refused on read, and the purge pass removes it from the store.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_expired_lease_is_refused_on_read_and_purged_on_schedule() {
        let directory = tempfile::tempdir().unwrap();
        let device = InMemoryProvider::from_seed([0x87; 32]);
        let host = CarriageHost::open(
            &device,
            config(directory.path().join("solo.redb"), Vec::new()),
        )
        .await
        .unwrap();

        host.publish_slot(
            &issuer(),
            slot(),
            1,
            now_ms() + 150,
            b"short-lease".to_vec(),
            CarriageCeilings::default(),
        )
        .await
        .unwrap();
        assert!(host.recover(slot()).await.unwrap().is_some());

        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        assert!(
            host.recover(slot()).await.unwrap().is_none(),
            "an expired lease must be refused on read"
        );

        let proposal = host.propose_purge().await;
        assert!(proposal.is_executable());
        assert_eq!(proposal.expired, vec![slot()]);
        let purged = host.execute_purge(&proposal).await.unwrap();
        assert!(purged >= 1, "the purge must delete the expired operation");
        assert_eq!(host.held_count().await, 0);

        host.close().await.unwrap();
    }

    /// Issue-side loudness: a lease violating a knowable ceiling is refused
    /// at the issuer, not silently dropped by every peer.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_ceiling_violation_is_refused_at_the_issuer() {
        let directory = tempfile::tempdir().unwrap();
        let device = InMemoryProvider::from_seed([0x88; 32]);
        let host = CarriageHost::open(
            &device,
            config(directory.path().join("ceiling.redb"), Vec::new()),
        )
        .await
        .unwrap();

        let refused = host
            .publish_slot(
                &issuer(),
                slot(),
                1,
                now_ms() + 60_000,
                b"too-long".to_vec(),
                CarriageCeilings {
                    device_max_ttl_ms: Some(1_000),
                    grant_expires_at_ms: None,
                },
            )
            .await;
        assert!(
            matches!(refused, Err(CarriageHostError::Refused(_))),
            "a lease over the device TTL must be refused at issue: {refused:?}"
        );
        host.close().await.unwrap();
    }
}

/// The commissioning story end to end, through the real machinery at every
/// step: a wallet pairs a device with a private epoch, leases it onto a
/// graph, publishes through the roster-driven path, and a peer replica
/// serves back a record the pairing key can actually open.
mod commissioning {
    use super::*;
    use pandect::{
        CarriagePolicy, DeviceExposure, DeviceId, DevicePublicKey, KeyEpochId,
        PairedRemoteAuthGrantSpec, PersonaId, PrivateEpochPlaintext, blinded_slot_id,
        decode_epoch_record, ensure_wallet_state, issue_remote_auth_device_grant_from_pairing,
        load_device_grant_set, load_device_roster, load_identity_seed, save_device_roster,
        unwrap_private_epoch_material,
    };
    use personae::InMemoryProvider;
    use personae::carry::derive_persona_chain_root;
    use uuid::Uuid;

    const GRAPH: [u8; 32] = [0x91; 32];

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_paired_leased_device_recovers_its_epoch_through_a_peer() {
        let directory = tempfile::tempdir().unwrap();
        let carry_root = directory.path().join("wallet-root");
        let persona = PersonaId::new();
        ensure_wallet_state(&carry_root, persona, "Graphshell workstation").unwrap();

        // Pair a device carrying one private epoch. The pairing path is the
        // one that retains a wrapping key, which is what makes the slot
        // addressable at publish time.
        let device_id = DeviceId::new();
        let epoch = KeyEpochId(Uuid::from_u128(0x92));
        let delegatee = Ed25519Keypair::from_seed([0x93; 32]);
        let now = now_ms();
        let (_grant, pairing) = issue_remote_auth_device_grant_from_pairing(
            &carry_root,
            &PairedRemoteAuthGrantSpec {
                device_id,
                delegatee_pubkey: DevicePublicKey::from(delegatee.public_key()),
                label: "Pocket relay".into(),
                exposure: DeviceExposure::HiddenClient,
                issued_at_ms: now,
                expires_at_ms: Some(now + 7 * 24 * 60 * 60 * 1000),
                personas: vec![persona],
                scopes: vec!["identity.act".into(), "private.read".into()],
                attenuations: vec!["no-subdelegation".into()],
                pairing_secret: b"qr-code-derived-shared-secret".to_vec(),
                private_epochs: vec![PrivateEpochPlaintext {
                    persona_id: persona,
                    epoch_id: epoch,
                    epoch_secret: b"commissioned-epoch-secret".to_vec(),
                }],
            },
        )
        .unwrap();

        // Lease the device onto this graph: the ruled layering's "whether",
        // decided on the roster where the other posture fields live.
        let mut roster = load_device_roster(&carry_root).unwrap().unwrap();
        let record = roster
            .devices
            .iter_mut()
            .find(|record| record.device_id == device_id)
            .unwrap();
        record.carriage = CarriagePolicy::Leased {
            max_ttl_ms: 60_000,
            graph: GRAPH,
        };
        save_device_roster(&carry_root, &roster).unwrap();

        // Verifiers hold one trusted root per persona: its chain root.
        let master_seed = load_identity_seed(&carry_root).unwrap().unwrap();
        let chain_root = derive_persona_chain_root(master_seed, persona).unwrap();
        let trusted = vec![chain_root.0];

        let wallet_device = InMemoryProvider::from_seed([0x94; 32]);
        let replica_device = InMemoryProvider::from_seed([0x95; 32]);
        let host_config = |path: PathBuf, tickets: Vec<String>| CarriageHostConfig {
            graph: GRAPH,
            store_path: path,
            trusted_roots: trusted.clone(),
            ceilings: CarriageCeilings::default(),
            peer_tickets: tickets,
            paired_nodes: Vec::new(),
        };
        let wallet_host = CarriageHost::open(
            &wallet_device,
            host_config(directory.path().join("wallet.redb"), Vec::new()),
        )
        .await
        .unwrap();
        let replica_host = CarriageHost::open(
            &replica_device,
            host_config(
                directory.path().join("replica.redb"),
                vec![wallet_host.ticket().await.unwrap()],
            ),
        )
        .await
        .unwrap();

        // The roster-driven publish. One persona certificate carries epoch
        // material, so exactly one slot goes onto the lane.
        let report = wallet_host
            .publish_grant_carriage(&carry_root)
            .await
            .unwrap();
        assert_eq!(report.published, vec![(device_id, persona)]);
        assert!(report.skipped_no_wrapping_key.is_empty());
        assert_eq!(report.skipped_no_record, 0);

        // The device's side of the story: it knows its certificate and its
        // pairing key, so it can compute its slot and ask a peer.
        let set = load_device_grant_set(&carry_root, device_id).unwrap();
        let certificate_id = set.personas.get(&persona).unwrap().certificate.id();
        let slot = blinded_slot_id(certificate_id, pairing.wrapping_key);

        let mut recovered = None;
        for _ in 0..100 {
            recovered = replica_host.recover(slot).await.unwrap();
            if recovered.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        let bytes = recovered.expect("the replica must serve the commissioned slot");

        // The recovered record is not just bytes: the pairing key opens it,
        // which is recovery without re-pairing, the lane's whole point.
        let record = decode_epoch_record(&bytes).unwrap();
        assert_eq!(record.certificate, certificate_id);
        let secret =
            unwrap_private_epoch_material(&record.epochs[0], persona, epoch, pairing.wrapping_key)
                .unwrap();
        assert_eq!(secret, b"commissioned-epoch-secret".to_vec());

        wallet_host.close().await.unwrap();
        replica_host.close().await.unwrap();
    }
}

/// The revocation fast path, end to end through real revocation: material a
/// peer holds is destroyed by retraction now, not at expiry.
mod retraction {
    use super::*;
    use pandect::{
        CarriagePolicy, DeviceExposure, DeviceId, DevicePublicKey, KeyEpochId,
        PairedRemoteAuthGrantSpec, PersonaId, PrivateEpochPlaintext, blinded_slot_id,
        decode_epoch_record, ensure_wallet_state, issue_remote_auth_device_grant_from_pairing,
        load_device_grant_set, load_device_roster, load_identity_seed, revoke_remote_auth_device,
        save_device_roster,
    };
    use personae::InMemoryProvider;
    use personae::carry::derive_persona_chain_root;
    use uuid::Uuid;

    const GRAPH: [u8; 32] = [0xA1; 32];

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn revoking_a_device_destroys_its_carriage_on_a_peer_before_expiry() {
        let directory = tempfile::tempdir().unwrap();
        let carry_root = directory.path().join("wallet-root");
        let persona = PersonaId::new();
        ensure_wallet_state(&carry_root, persona, "Graphshell workstation").unwrap();

        let device_id = DeviceId::new();
        let now = now_ms();
        let (_grant, pairing) = issue_remote_auth_device_grant_from_pairing(
            &carry_root,
            &PairedRemoteAuthGrantSpec {
                device_id,
                delegatee_pubkey: DevicePublicKey::from(
                    Ed25519Keypair::from_seed([0xA2; 32]).public_key(),
                ),
                label: "Doomed relay".into(),
                exposure: DeviceExposure::HiddenClient,
                issued_at_ms: now,
                expires_at_ms: Some(now + 7 * 24 * 60 * 60 * 1000),
                personas: vec![persona],
                scopes: vec!["identity.act".into(), "private.read".into()],
                attenuations: vec!["no-subdelegation".into()],
                pairing_secret: b"qr-code-derived-shared-secret".to_vec(),
                private_epochs: vec![PrivateEpochPlaintext {
                    persona_id: persona,
                    epoch_id: KeyEpochId(Uuid::from_u128(0xA3)),
                    epoch_secret: b"soon-to-be-retracted".to_vec(),
                }],
            },
        )
        .unwrap();
        let mut roster = load_device_roster(&carry_root).unwrap().unwrap();
        roster
            .devices
            .iter_mut()
            .find(|record| record.device_id == device_id)
            .unwrap()
            .carriage = CarriagePolicy::Leased {
            // A long lease on purpose: if expiry did the destroying, this
            // test could not tell retraction from waiting.
            max_ttl_ms: 60 * 60 * 1000,
            graph: GRAPH,
        };
        save_device_roster(&carry_root, &roster).unwrap();

        let master_seed = load_identity_seed(&carry_root).unwrap().unwrap();
        let trusted = vec![derive_persona_chain_root(master_seed, persona).unwrap().0];
        let host_config = |path: PathBuf, tickets: Vec<String>| CarriageHostConfig {
            graph: GRAPH,
            store_path: path,
            trusted_roots: trusted.clone(),
            ceilings: CarriageCeilings::default(),
            peer_tickets: tickets,
            paired_nodes: Vec::new(),
        };
        let wallet_host = CarriageHost::open(
            &InMemoryProvider::from_seed([0xA4; 32]),
            host_config(directory.path().join("wallet.redb"), Vec::new()),
        )
        .await
        .unwrap();
        let replica_host = CarriageHost::open(
            &InMemoryProvider::from_seed([0xA5; 32]),
            host_config(
                directory.path().join("replica.redb"),
                vec![wallet_host.ticket().await.unwrap()],
            ),
        )
        .await
        .unwrap();

        // Slot computed while the wrapping key exists; revocation deletes it.
        let set = load_device_grant_set(&carry_root, device_id).unwrap();
        let certificate_id = set.personas.get(&persona).unwrap().certificate.id();
        let slot = blinded_slot_id(certificate_id, pairing.wrapping_key);

        let report = wallet_host
            .publish_grant_carriage(&carry_root)
            .await
            .unwrap();
        assert_eq!(report.published.len(), 1);
        let mut held = None;
        for _ in 0..100 {
            held = replica_host.recover(slot).await.unwrap();
            if held.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        let before = decode_epoch_record(&held.expect("replica must hold the slot")).unwrap();
        assert!(!before.epochs.is_empty(), "the lease carries real material");

        // The real revocation, wrapping-key deletion and all. Then the fast
        // path, addressable only because the index was written at publish.
        revoke_remote_auth_device(&carry_root, device_id).unwrap();
        let retract = wallet_host
            .retract_device_carriage(device_id, master_seed)
            .await
            .unwrap();
        assert_eq!(retract.retracted, vec![slot.0]);
        assert!(retract.skipped_not_held.is_empty());

        // The peer's copy is destroyed by supersession, with the lease still
        // hours from expiry: the recovered record is now the empty shell.
        let mut after = None;
        for _ in 0..100 {
            if let Some(bytes) = replica_host.recover(slot).await.unwrap() {
                let record = decode_epoch_record(&bytes).unwrap();
                if record.epochs.is_empty() {
                    after = Some(record);
                    break;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        let shell = after.expect("the retraction must reach the peer");
        assert_eq!(shell.certificate, certificate_id);

        // Retraction is once: the index is consumed with it.
        let again = wallet_host
            .retract_device_carriage(device_id, master_seed)
            .await
            .unwrap();
        assert!(again.retracted.is_empty());

        wallet_host.close().await.unwrap();
        replica_host.close().await.unwrap();
    }
}

/// The endpoint fold: carriage attached to the personal sync host's bound
/// endpoint, both lanes converging over one connection per device.
mod attached {
    use super::*;
    use crate::native::personal_sync_host::{PersonalSyncHost, PersonalSyncHostConfig};
    use crate::personal_sync::{PersonalGraphEvent, SyncRoster, SyncSelection};
    use personae::{IdentityProvider, InMemoryProvider};
    use uuid::Uuid;

    const GRAPH: [u8; 32] = [0xB1; 32];

    fn issuer() -> Ed25519Keypair {
        Ed25519Keypair::from_seed([0xB2; 32])
    }

    fn slot() -> BlindedSlotId {
        pandect::blinded_slot_id(
            personae::delegation::DelegationId([0xB3; 32]),
            [0xB4; 32],
        )
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn both_lanes_converge_over_one_endpoint_per_device() {
        let directory = tempfile::tempdir().unwrap();
        let owner = InMemoryProvider::from_seed([0xB5; 32]);
        let sibling = InMemoryProvider::from_seed([0xB6; 32]);
        let roster = SyncRoster::new([
            owner.master_public_key().to_bytes(),
            sibling.master_public_key().to_bytes(),
        ]);
        let sync_config = |path: std::path::PathBuf, tickets: Vec<String>| PersonalSyncHostConfig {
            graph: GRAPH,
            store_path: path,
            roster: roster.clone(),
            selection: SyncSelection::default(),
            peer_tickets: tickets,
            peer_hints: Vec::new(),
            paired_nodes: Vec::new(),
            relay_urls: Vec::new(),
        };
        let owner_sync = PersonalSyncHost::open(
            &owner,
            sync_config(directory.path().join("owner.redb"), Vec::new()),
        )
        .await
        .unwrap();
        let sibling_sync = PersonalSyncHost::open(
            &sibling,
            sync_config(
                directory.path().join("sibling.redb"),
                vec![owner_sync.ticket().await.unwrap()],
            ),
        )
        .await
        .unwrap();
        owner_sync.pair_node(sibling_sync.node_id()).await.unwrap();

        // Attach carriage to BOTH existing endpoints: no second bind, and the
        // append-form overlay tag must leave the graph lane's tag standing.
        let trusted = vec![issuer().public_key().to_bytes()];
        let owner_carriage = CarriageHost::attach(
            &owner,
            &owner_sync,
            directory.path().join("owner-carriage.redb"),
            trusted.clone(),
            CarriageCeilings::default(),
            &[sibling_sync.node_id()],
        )
        .await
        .unwrap();
        let sibling_carriage = CarriageHost::attach(
            &sibling,
            &sibling_sync,
            directory.path().join("sibling-carriage.redb"),
            trusted,
            CarriageCeilings::default(),
            &[owner_sync.node_id()],
        )
        .await
        .unwrap();
        assert_eq!(
            owner_carriage.node_id(),
            owner_sync.node_id(),
            "an attached lane is the same endpoint, not a second identity"
        );

        // Carriage lane converges...
        owner_carriage
            .publish_slot(
                &issuer(),
                slot(),
                1,
                now_ms() + 60_000,
                b"folded-lane-record".to_vec(),
                CarriageCeilings::default(),
            )
            .await
            .unwrap();
        let mut recovered = None;
        for _ in 0..100 {
            recovered = sibling_carriage.recover(slot()).await.unwrap();
            if recovered.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        assert_eq!(
            recovered.as_deref(),
            Some(b"folded-lane-record".as_slice()),
            "the carriage lane must converge over the shared endpoint"
        );

        // ...and the graph lane still does, which is what proves the overlay
        // tags composed instead of clobbering.
        owner_sync
            .author(vec![PersonalGraphEvent::AddNode {
                id: Uuid::from_u128(0xB7),
                address: "https://folded.test/".into(),
                title: "Folded-lane node".into(),
            }])
            .await
            .unwrap();
        let mut graph_converged = false;
        for _ in 0..100 {
            let cards = sibling_sync.supplemental_cards().await.unwrap();
            if cards.iter().any(|card| card.card.title == "Folded-lane node") {
                graph_converged = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        assert!(
            graph_converged,
            "the graph lane must still converge beside carriage"
        );

        owner_carriage.close().await.unwrap();
        sibling_carriage.close().await.unwrap();
        owner_sync.close().await.unwrap();
        sibling_sync.close().await.unwrap();
    }
}
