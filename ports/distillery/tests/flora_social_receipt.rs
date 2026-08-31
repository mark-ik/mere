// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

#![cfg(feature = "flora")]

use std::collections::{BTreeMap, BTreeSet};

use distillery::flora::{
    FloraContribution, FloraRequest, FloraWeight as TensorWeight, aggregate_exact,
};
use eidetic::{AdapterRuntimeCompat, Hash, ManifestId, ModelAdapterManifest};
use esp::infer::decoder::PEFT_LORA_NDARRAY_LOADER;
use gemot::moot::standing::{
    ChainRoot, CommitmentId, Scope, StandingConfig, StandingEvent, StandingFileStore,
};
use gemot::moot::{
    ArtifactRef, FloraCandidateArtifact, FloraContributionReceipt, FloraEvent, FloraFileStore,
    FloraParticipant, FloraRoundId, FloraRoundSpec, FloraWeight as SocialWeight, TulpaEvent,
    TulpaFileStore, TulpaId, TulpaProposal, TulpaProposalId, TulpaVersion,
};
use identity::Ed25519Keypair;
use mooting::{ElectorateSnapshot, RecognitionContext, RecognitionPolicy};
use safetensors::tensor::{Dtype, TensorView};

const MOOT: [u8; 32] = [0x31; 32];
const ROUND: FloraRoundId = FloraRoundId([0x41; 32]);
const BASE_BYTES: &[u8] = b"flora integration base";
const TOKENIZER_BYTES: &[u8] = b"flora integration tokenizer";
const TEMPLATE_BYTES: &[u8] = b"{{ prompt }}";

#[derive(Clone)]
struct AdapterFixture {
    contribution: FloraContribution,
    manifest_bytes: Vec<u8>,
    a_bytes: Vec<u8>,
    b_bytes: Vec<u8>,
}

fn participant_id(bytes: [u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn tensor_weight(weight: SocialWeight) -> TensorWeight {
    TensorWeight {
        numerator: weight.numerator,
        denominator: weight.denominator,
    }
}

fn f32_bytes(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn adapter_fixture(
    participant: [u8; 32],
    rank: u16,
    alpha: f32,
    weight: SocialWeight,
    start: f32,
) -> AdapterFixture {
    let id = participant_id(participant);
    let config = serde_json::to_vec(&serde_json::json!({
        "base_model_name_or_path": "fixture/flora-social-receipt",
        "peft_type": "LORA",
        "peft_version": "esp-trainer-v0",
        "r": rank,
        "lora_alpha": alpha,
        "target_modules": ["q_proj"],
        "bias": "none",
    }))
    .unwrap();

    let a_values = (0..usize::from(rank) * 2)
        .map(|index| start + index as f32)
        .collect::<Vec<_>>();
    let b_values = (0..3 * usize::from(rank))
        .map(|index| start + 20.0 + index as f32)
        .collect::<Vec<_>>();
    let a_bytes = f32_bytes(&a_values);
    let b_bytes = f32_bytes(&b_values);
    let a = TensorView::new(Dtype::F32, vec![usize::from(rank), 2], &a_bytes).unwrap();
    let b = TensorView::new(Dtype::F32, vec![3, usize::from(rank)], &b_bytes).unwrap();
    let prefix = "base_model.model.model.layers.0.self_attn.q_proj";
    let adapter = safetensors::serialize(
        vec![
            (format!("{prefix}.lora_A.weight"), a),
            (format!("{prefix}.lora_B.weight"), b),
        ],
        &None,
    )
    .unwrap();
    let manifest = ModelAdapterManifest {
        name: id.clone(),
        base_model_ref: ManifestId::of_blob(BASE_BYTES),
        adapter_blob: ManifestId::of_blob(&adapter),
        adapter_config_blob: ManifestId::of_blob(&config),
        adapter_format: "peft-lora".into(),
        adapter_format_version: "peft-esp-trainer-v0".into(),
        runtime_compat: AdapterRuntimeCompat {
            minimum_capabilities: vec!["peft-lora".into()],
            known_loaders: vec![PEFT_LORA_NDARRAY_LOADER.into()],
            converter_lineage: vec![],
        },
        rank,
        alpha,
        target_modules: vec!["q_proj".into()],
        tokenizer_ref: ManifestId::of_blob(TOKENIZER_BYTES),
        prompt_template_hash: Hash::of(TEMPLATE_BYTES),
        quantization_assumption: None,
        training_corpus_root: None,
        training_method: serde_json::json!({"method": "fixture"}),
        eval_results: None,
    };
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
    AdapterFixture {
        contribution: FloraContribution {
            contribution_id: id,
            manifest_ref: ManifestId::of_blob(&manifest_bytes),
            manifest,
            adapter_config_json: config,
            adapter_safetensors: adapter,
            weight: tensor_weight(weight),
        },
        manifest_bytes,
        a_bytes,
        b_bytes,
    }
}

fn social_receipt(participant: [u8; 32], fixture: &AdapterFixture) -> FloraContributionReceipt {
    FloraContributionReceipt {
        round: ROUND,
        participant,
        a_factor: ArtifactRef::blake3(&fixture.a_bytes),
        b_factor: ArtifactRef::blake3(&fixture.b_bytes),
        receipt: ArtifactRef::blake3(&fixture.manifest_bytes),
    }
}

#[tokio::test]
async fn signed_round_aggregates_publishes_adopts_and_replays_after_restart() {
    let directory = tempfile::tempdir().unwrap();
    let standing_source_path = directory.path().join("standing-source.redb");
    let standing_peer_path = directory.path().join("standing-peer.redb");
    let flora_source_path = directory.path().join("flora-source.redb");
    let flora_peer_path = directory.path().join("flora-peer.redb");
    let tulpa_source_path = directory.path().join("tulpa-source.redb");
    let tulpa_peer_path = directory.path().join("tulpa-peer.redb");

    let owner_seed = [0x19; 32];
    let alice_seed = [0x1a; 32];
    let bob_seed = [0x1b; 32];
    let alice = Ed25519Keypair::from_seed(alice_seed)
        .public_key()
        .to_bytes();
    let bob = Ed25519Keypair::from_seed(bob_seed).public_key().to_bytes();
    let weights = BTreeMap::from([
        (
            alice,
            FloraParticipant {
                rank: 1,
                weight: SocialWeight {
                    numerator: 1,
                    denominator: 2,
                },
            },
        ),
        (
            bob,
            FloraParticipant {
                rank: 2,
                weight: SocialWeight {
                    numerator: 1,
                    denominator: 4,
                },
            },
        ),
    ]);
    let round = FloraRoundSpec {
        round: ROUND,
        base_model: ArtifactRef::blake3(BASE_BYTES),
        rank_budget: 3,
        participants: weights,
    };
    assert_eq!(round.validate(), Ok(3));

    let alice_fixture = adapter_fixture(alice, 1, 2.0, round.participants[&alice].weight, 1.0);
    let bob_fixture = adapter_fixture(bob, 2, 6.0, round.participants[&bob].weight, 6.0);
    assert_eq!(
        round.participants[&alice].rank,
        alice_fixture.contribution.manifest.rank
    );
    assert_eq!(
        round.participants[&bob].rank,
        bob_fixture.contribution.manifest.rank
    );
    let alice_receipt = social_receipt(alice, &alice_fixture);
    let bob_receipt = social_receipt(bob, &bob_fixture);
    assert!(round.base_model.verifies(BASE_BYTES));
    assert!(alice_receipt.a_factor.verifies(&alice_fixture.a_bytes));
    assert!(alice_receipt.b_factor.verifies(&alice_fixture.b_bytes));
    assert!(
        alice_receipt
            .receipt
            .verifies(&alice_fixture.manifest_bytes)
    );
    assert!(bob_receipt.a_factor.verifies(&bob_fixture.a_bytes));
    assert!(bob_receipt.b_factor.verifies(&bob_fixture.b_bytes));
    assert!(bob_receipt.receipt.verifies(&bob_fixture.manifest_bytes));

    let flora_source = FloraFileStore::open(&flora_source_path).unwrap();
    let flora_peer = FloraFileStore::open(&flora_peer_path).unwrap();
    let proposed = flora_source
        .author_seed(
            owner_seed,
            MOOT,
            &FloraEvent::RoundProposed {
                spec: round.clone(),
                at_ms: 1,
            },
        )
        .await
        .unwrap();
    assert!(flora_peer.accept(MOOT, &proposed).await.unwrap());
    let alice_contributed = flora_source
        .author_seed(
            alice_seed,
            MOOT,
            &FloraEvent::ContributionReceived {
                contribution: alice_receipt,
                at_ms: 2,
            },
        )
        .await
        .unwrap();
    assert!(flora_peer.accept(MOOT, &alice_contributed).await.unwrap());
    let bob_contributed = flora_source
        .author_seed(
            bob_seed,
            MOOT,
            &FloraEvent::ContributionReceived {
                contribution: bob_receipt,
                at_ms: 3,
            },
        )
        .await
        .unwrap();
    assert!(flora_peer.accept(MOOT, &bob_contributed).await.unwrap());

    let aggregate = aggregate_exact(FloraRequest {
        output_name: "community aggregate".into(),
        rank_budget: round.rank_budget,
        // Reverse arrival order deliberately; Distillery must canonicalize by
        // the stable participant ids derived above.
        contributions: vec![bob_fixture.contribution, alice_fixture.contribution],
    })
    .unwrap();
    assert_eq!(
        aggregate.receipt.aggregate_rank,
        round.global_rank().unwrap()
    );
    for item in &aggregate.receipt.contribution_order {
        let participant = if item.contribution_id == participant_id(alice) {
            alice
        } else {
            bob
        };
        let social = round.participants[&participant].weight;
        assert_eq!(item.weight_numerator, social.numerator);
        assert_eq!(item.weight_denominator, social.denominator);
    }
    let output_manifest = serde_json::to_vec(&aggregate.manifest).unwrap();
    let candidate_ref = ArtifactRef::blake3(&output_manifest);
    assert!(candidate_ref.verifies(&output_manifest));
    let contributors = BTreeSet::from([alice, bob]);
    let published = flora_source
        .author_seed(
            owner_seed,
            MOOT,
            &FloraEvent::CandidatePublished {
                candidate: FloraCandidateArtifact {
                    round: ROUND,
                    adapter: candidate_ref.clone(),
                    global_rank: aggregate.receipt.aggregate_rank,
                    contributors: contributors.clone(),
                },
                at_ms: 4,
            },
        )
        .await
        .unwrap();
    assert!(flora_peer.accept(MOOT, &published).await.unwrap());
    let flora_before_restart = flora_peer.fold_moot(MOOT).await.unwrap();
    assert_eq!(flora_before_restart.facts.len(), 4);
    assert_eq!(
        flora_before_restart.rounds[&ROUND].viable_candidates.len(),
        1
    );

    let tulpa = TulpaId([0x51; 32]);
    let version = TulpaVersion(candidate_ref.digest.as_32().unwrap());
    let proposal = TulpaProposalId([0x52; 32]);
    let recognition = RecognitionContext::new(
        RecognitionPolicy::Unanimous,
        ElectorateSnapshot::new(MOOT, [0x53; 32], contributors.clone()),
    );
    let tulpa_source = TulpaFileStore::open(&tulpa_source_path).unwrap();
    let tulpa_peer = TulpaFileStore::open(&tulpa_peer_path).unwrap();
    let offered = tulpa_source
        .author_seed(
            owner_seed,
            MOOT,
            &TulpaEvent::Proposed {
                proposal,
                action: TulpaProposal::Adopt {
                    tulpa,
                    version,
                    artifact: candidate_ref.clone(),
                },
                recognition,
                at_ms: 5,
            },
        )
        .await
        .unwrap();
    assert!(tulpa_peer.accept(MOOT, &offered).await.unwrap());
    let alice_endorsed = tulpa_source
        .author_seed(
            alice_seed,
            MOOT,
            &TulpaEvent::Endorsed { proposal, at_ms: 6 },
        )
        .await
        .unwrap();
    assert!(tulpa_peer.accept(MOOT, &alice_endorsed).await.unwrap());
    let bob_endorsed = tulpa_source
        .author_seed(bob_seed, MOOT, &TulpaEvent::Endorsed { proposal, at_ms: 7 })
        .await
        .unwrap();
    assert!(tulpa_peer.accept(MOOT, &bob_endorsed).await.unwrap());
    let tulpa_before_restart = tulpa_peer.fold_moot(MOOT).await.unwrap();
    assert_eq!(tulpa_before_restart.facts.len(), 3);
    assert_eq!(tulpa_before_restart.adopted[&tulpa].artifact, candidate_ref);

    let standing_source = StandingFileStore::open(&standing_source_path).unwrap();
    let standing_peer = StandingFileStore::open(&standing_peer_path).unwrap();
    let root = ChainRoot(alice);
    let commitment = CommitmentId([0x61; 32]);
    let pledged = standing_source
        .author_seed(
            alice_seed,
            MOOT,
            &StandingEvent::CommitmentMade {
                by: root,
                commitment,
                scope: Scope("flora/round/41".into()),
                cadence_ms: 10,
                duration_ms: Some(5),
                at_ms: 8,
            },
        )
        .await
        .unwrap();
    assert!(standing_peer.accept(MOOT, &pledged).await.unwrap());
    let fulfilled = standing_source
        .author_seed(
            alice_seed,
            MOOT,
            &StandingEvent::CommitmentFulfilled {
                by: root,
                commitment,
                at_ms: 9,
            },
        )
        .await
        .unwrap();
    assert!(standing_peer.accept(MOOT, &fulfilled).await.unwrap());
    let standing_before_restart = standing_peer
        .fold_moot(MOOT, StandingConfig::default())
        .await
        .unwrap()
        .score(&root, 10);
    assert_eq!(standing_before_restart, 10);

    drop(flora_source);
    drop(flora_peer);
    drop(tulpa_source);
    drop(tulpa_peer);
    drop(standing_source);
    drop(standing_peer);

    let flora_after_restart = FloraFileStore::open(&flora_peer_path)
        .unwrap()
        .fold_moot(MOOT)
        .await
        .unwrap();
    let tulpa_after_restart = TulpaFileStore::open(&tulpa_peer_path)
        .unwrap()
        .fold_moot(MOOT)
        .await
        .unwrap();
    let standing_after_restart = StandingFileStore::open(&standing_peer_path)
        .unwrap()
        .fold_moot(MOOT, StandingConfig::default())
        .await
        .unwrap()
        .score(&root, 10);

    assert_eq!(flora_after_restart, flora_before_restart);
    assert_eq!(tulpa_after_restart, tulpa_before_restart);
    assert_eq!(standing_after_restart, standing_before_restart);
    assert_eq!(
        tulpa_after_restart.adopted[&tulpa].artifact,
        flora_after_restart.rounds[&ROUND].viable_candidates[0].adapter
    );
}
