//! See Cargo.toml: the participant-gate B5 probe. Test-only crate.

#[cfg(test)]
mod tests {
    use eidetic::pack::{
        PackManifest, PackPart, PackPartRole, PackVerdict, sign_pack, verify_pack,
    };
    use eidetic::schema::{ManifestId, ModerationState, TrustLevel, TrustEnvelope};
    use identity::Ed25519Keypair;
    use retinue::destination::DestinationName;
    use retinue::identity::PrivateIdentity;
    use retinue::link::{LinkMode, LinkTrailer, PendingLink, accept};
    use retinue::resource::{Incoming, Outgoing, content, parse_hmu, parse_request};

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn signed_pack() -> (PackManifest, TrustEnvelope) {
        let author = Ed25519Keypair::from_seed([21u8; 32]);
        let source = b"mere.open('mere://kept/note')";
        let manifest = PackManifest {
            name: "trail-keeper".to_string(),
            version: "0.1.0".to_string(),
            author: hex(&author.public_key().to_bytes()),
            requested_scopes: vec!["scenario/".to_string(), "app/".to_string()],
            parts: vec![PackPart {
                name: "main.lua".to_string(),
                role: PackPartRole::ScenarioSource,
                blob: ManifestId::of_blob(source),
                bytes: source.len() as u64,
            }],
        };
        let envelope = TrustEnvelope {
            level: TrustLevel::SelfAsserted,
            signatures: vec![sign_pack(&manifest, &author)],
            moderation_state: ModerationState::Unreviewed,
        };
        (manifest, envelope)
    }

    /// Ferry `data` across a fresh in-process retinue link pair via the
    /// windowed resource path (advertise -> request/HMU -> serve -> recover).
    fn ferry_over_link(data: &[u8]) -> Vec<u8> {
        let dest_id = PrivateIdentity::from_secret_bytes(&[0x11; 64]);
        let (pending, req) = PendingLink::open(
            DestinationName::new("retinue", ["pack"]).destination_hash(dest_id.public()),
            *dest_id.public(),
            &[0x33; 64],
            LinkTrailer { mode: LinkMode::Aes256Cbc, mtu: 500 },
        );
        let (recv_link, proof_pkt) = accept(
            &req,
            &dest_id,
            &[0x99; 64],
            LinkTrailer { mode: LinkMode::Aes256Cbc, mtu: 500 },
        )
        .unwrap();
        let send_link = pending.prove(&proof_pkt).unwrap();

        let rh = [0x0B, 0x05, 0xB5, 0x01];
        let token = send_link.seal(&content(data, &rh), &[7u8; 16]);
        let mut out = Outgoing::new(data, &token, rh, false);
        let mut inc = Incoming::new(&out.advertisement()).unwrap();
        while !inc.is_complete() {
            let want = inc.missing_known();
            if !want.is_empty() {
                let req = parse_request(&inc.request(&want)).unwrap();
                for part in out.serve(&req) {
                    inc.accept_part(&part);
                }
            } else if inc.needs_hmu() {
                let solicit = parse_request(&inc.solicit_hmu()).unwrap();
                let last = solicit.last_map_hash.unwrap();
                let hmu = parse_hmu(&out.hmu_after(&last)).unwrap();
                assert!(inc.ingest_hmu(&hmu) > 0);
            } else {
                panic!("transfer stuck");
            }
        }
        let recovered = inc
            .recover(&recv_link.open(&inc.assemble_token().unwrap()).unwrap())
            .unwrap();
        assert_eq!(inc.proof(&recovered), out.expected_proof());
        recovered
    }

    /// B5's pair half, in-process: publish -> transfer -> verify Trusted on
    /// arrival; a tampered wire copy verifies Broken and is refused.
    #[test]
    fn a_signed_pack_survives_the_wire_and_a_tampered_one_is_refused() {
        let (manifest, envelope) = signed_pack();
        // The wire form: manifest + envelope, one JSON document.
        let wire = serde_json::to_vec(&(&manifest, &envelope)).unwrap();

        let received = ferry_over_link(&wire);
        let (got_manifest, got_envelope): (PackManifest, TrustEnvelope) =
            serde_json::from_slice(&received).unwrap();
        assert_eq!(got_manifest, manifest, "bit-equal across the transfer");
        assert_eq!(
            verify_pack(&got_manifest, &got_envelope),
            PackVerdict::Trusted,
            "the subscriber re-verifies the signature it received"
        );

        // A malicious relay widens the ask in flight: the signature breaks.
        let mut tampered = got_manifest.clone();
        tampered.requested_scopes.push("wallet/".to_string());
        let tampered_wire = serde_json::to_vec(&(&tampered, &got_envelope)).unwrap();
        let received = ferry_over_link(&tampered_wire);
        let (bad_manifest, bad_envelope): (PackManifest, TrustEnvelope) =
            serde_json::from_slice(&received).unwrap();
        assert_eq!(
            verify_pack(&bad_manifest, &bad_envelope),
            PackVerdict::Broken,
            "install refuses a Broken pack"
        );
    }
}
