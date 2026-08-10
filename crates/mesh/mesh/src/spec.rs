// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The V2 job vocabulary: what a job asks for, and what its result claims.
//!
//! A [`JobSpec`] is a *manifest*, not an authorization. It names blobs by
//! content address; only the local host decides whether this device will hand a
//! resource a reader for them (see [`crate::namespace`]). Keeping those two
//! apart is the difference between a namespace-shaped record and an enforced
//! namespace.
//!
//! Everything here rides inside a signed CBOR body, so every bound is checked
//! on decode — [`JobSpec::validate`] runs before any store mutation.

use std::collections::BTreeSet;

use proofs::BlobRef;
use serde::{Deserialize, Serialize};

use crate::ident::{IdentError, ImplementationId, ResourceId};
use crate::lease::{LeaseTerms, LeaseTermsError};

/// Most named inputs one job may carry.
pub const MAX_INPUTS: usize = 32;
/// Longest input/output name, in bytes.
pub const MAX_NAME_LEN: usize = 64;
/// Largest output a single grant may promise (64 MiB).
pub const MAX_OUTPUT_BYTES: u64 = 64 * 1024 * 1024;

/// One named input: a slot name plus the content address that fills it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobInput {
    pub name: String,
    pub blob: BlobRef,
}

/// The single output slot a job is allowed to fill, and its ceiling.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputGrant {
    pub name: String,
    pub max_bytes: u64,
}

/// Compute class a resource must run on. Device *discovery* is a later
/// milestone; this is only the vocabulary a host matches its own facts against.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComputeClass {
    Cpu,
    Gpu,
}

/// What a job asks of whichever device runs it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceRequirements {
    /// Peak working memory, in MiB. `0` means "small enough not to say".
    pub memory_mib: u32,
    pub compute: ComputeClass,
}

impl ResourceRequirements {
    /// A small CPU job.
    pub const fn cpu() -> Self {
        Self {
            memory_mib: 0,
            compute: ComputeClass::Cpu,
        }
    }

    pub fn satisfied_by(&self, host: &HostFacts) -> bool {
        self.memory_mib <= host.memory_mib
            && match self.compute {
                ComputeClass::Cpu => true,
                ComputeClass::Gpu => host.gpu,
            }
    }
}

/// What this device advertises about itself. Host-supplied: the mesh never
/// inspects the OS.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostFacts {
    pub memory_mib: u32,
    pub gpu: bool,
}

impl HostFacts {
    /// A CPU-only device with `memory_mib` to spare.
    pub const fn cpu(memory_mib: u32) -> Self {
        Self {
            memory_mib,
            gpu: false,
        }
    }
}

/// What a poster may claim about re-running this job elsewhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeterminismClass {
    /// Same inputs, same output bytes, on any conforming device.
    Exact,
    /// Same inputs, numerically close output; compare under a tolerance.
    Tolerant,
    /// No cross-device claim at all.
    Observed,
}

/// What happens to the work when a run is interrupted. The M3 plan called these
/// `Interruptible` / `Checkpointable` / `NonInterruptible`; the two M2 names are
/// kept because renaming them would change the encoded variant strings inside
/// already-signed V2 specs, and they say the same thing from the work's side.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CheckpointClass {
    /// Interruptible: re-dispatch starts again from nothing.
    Restart,
    /// Checkpointable: the resource can stop at a boundary and report how far
    /// it got. (Resuming *elsewhere* additionally needs a blob lane the mesh
    /// does not have yet — see the lease plan's carried-forward section.)
    Resumable,
    /// Not interruptible: the device finishes, or fails inside the owner's
    /// configured grace window. Owner reclaim still wins; the grace window
    /// changes how the handoff happens, not who has authority.
    NonInterruptible,
}

/// The V2 job manifest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobSpec {
    pub resource: ResourceId,
    pub inputs: Vec<JobInput>,
    pub output: OutputGrant,
    pub requirements: ResourceRequirements,
    pub determinism: DeterminismClass,
    pub checkpoint: CheckpointClass,
    /// The lease envelope the author signs once, at post time. `None` keeps M2
    /// semantics: claim, run, commit, with no lending contract. Skipped when
    /// absent so a pre-M3 spec still encodes to its original bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease: Option<LeaseTerms>,
}

impl JobSpec {
    /// A single-input, single-output CPU job — the common shape.
    pub fn simple(
        resource: ResourceId,
        input_name: &str,
        blob: BlobRef,
        output_name: &str,
        max_bytes: u64,
        determinism: DeterminismClass,
    ) -> Self {
        Self {
            resource,
            inputs: vec![JobInput {
                name: input_name.to_string(),
                blob,
            }],
            output: OutputGrant {
                name: output_name.to_string(),
                max_bytes,
            },
            requirements: ResourceRequirements::cpu(),
            determinism,
            checkpoint: CheckpointClass::Restart,
            lease: None,
        }
    }

    /// Sign a lease envelope into this spec, making it a lendable job.
    pub fn leased(mut self, terms: LeaseTerms) -> Self {
        self.lease = Some(terms);
        self
    }

    /// Every bound this spec must satisfy before it may mutate anything.
    pub fn validate(&self) -> Result<(), SpecError> {
        self.resource.validate()?;
        if let Some(terms) = &self.lease {
            terms.validate()?;
        }
        if self.inputs.len() > MAX_INPUTS {
            return Err(SpecError::TooManyInputs(self.inputs.len()));
        }
        let mut seen = BTreeSet::new();
        for input in &self.inputs {
            validate_name(&input.name)?;
            if !seen.insert(input.name.as_str()) {
                return Err(SpecError::DuplicateInput(input.name.clone()));
            }
            if input.blob.digest.as_32().is_err() {
                return Err(SpecError::MalformedInputDigest(input.name.clone()));
            }
        }
        validate_name(&self.output.name)?;
        if self.output.max_bytes == 0 || self.output.max_bytes > MAX_OUTPUT_BYTES {
            return Err(SpecError::OversizedGrant(self.output.max_bytes));
        }
        Ok(())
    }

    /// The content address bound to `name`, if the spec grants it.
    pub fn input(&self, name: &str) -> Option<&BlobRef> {
        self.inputs
            .iter()
            .find(|input| input.name == name)
            .map(|input| &input.blob)
    }
}

/// Why a spec was refused, before mutation.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SpecError {
    #[error("job spec resource id: {0}")]
    Resource(#[from] IdentError),
    #[error("job spec carries {0} inputs (max {MAX_INPUTS})")]
    TooManyInputs(usize),
    #[error("job spec names input {0:?} twice")]
    DuplicateInput(String),
    #[error("job spec slot name {0:?} is malformed")]
    MalformedName(String),
    #[error("job spec input {0:?} carries a malformed content digest")]
    MalformedInputDigest(String),
    #[error("job spec grants {0} output bytes (max {MAX_OUTPUT_BYTES})")]
    OversizedGrant(u64),
    #[error("job spec lease terms: {0}")]
    Lease(#[from] LeaseTermsError),
}

/// Slot names are lowercase, start alphanumeric, and stay short: they are map
/// keys a resource matches on, not free text.
fn validate_name(name: &str) -> Result<(), SpecError> {
    let bad = || SpecError::MalformedName(name.to_string());
    let bytes = name.as_bytes();
    if bytes.is_empty() || bytes.len() > MAX_NAME_LEN {
        return Err(bad());
    }
    if !(bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit()) {
        return Err(bad());
    }
    if !bytes
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_' | b'.'))
    {
        return Err(bad());
    }
    Ok(())
}

/// What a *resource* declares its output can claim — the earned counterpart of
/// the spec's requested [`DeterminismClass`]. Only the resource knows the
/// concrete tolerance, so the detail lives here rather than in the ask.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerificationClass {
    /// A re-run on any conforming device produces byte-identical output.
    ExactBytes,
    /// A re-run produces the same shape within `max_abs_error_ppm` parts per
    /// million, element-wise.
    Tolerance { max_abs_error_ppm: u32 },
    /// Nothing is claimed beyond "these are the bytes this device stored".
    ProducerOnly,
}

impl VerificationClass {
    pub fn class(&self) -> DeterminismClass {
        match self {
            Self::ExactBytes => DeterminismClass::Exact,
            Self::Tolerance { .. } => DeterminismClass::Tolerant,
            Self::ProducerOnly => DeterminismClass::Observed,
        }
    }

    /// Whether a resource declaring this class may answer a job asking for
    /// `requested`. Exact answers everything; observed answers only observed.
    pub fn satisfies(&self, requested: DeterminismClass) -> bool {
        matches!(
            (self.class(), requested),
            (DeterminismClass::Exact, _)
                | (
                    DeterminismClass::Tolerant,
                    DeterminismClass::Tolerant | DeterminismClass::Observed
                )
                | (DeterminismClass::Observed, DeterminismClass::Observed)
        )
    }
}

/// The committed result of a V2 job: an address plus the identities a verifier
/// needs. Result bytes never return inline.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobOutput {
    /// The granted slot this fills.
    pub name: String,
    /// Content address and length of the committed bytes.
    pub blob: BlobRef,
    pub resource: ResourceId,
    pub implementation: ImplementationId,
    pub verification: VerificationClass,
}

impl JobOutput {
    /// The checks that need no job in hand: identities parse, the slot name is
    /// well formed, the digest is a real digest. This is what an operation must
    /// pass before it may be stored, where the job it answers may not be known.
    pub fn validate_self(&self) -> Result<(), OutputError> {
        self.resource.validate().map_err(OutputError::Resource)?;
        self.implementation
            .validate()
            .map_err(OutputError::Implementation)?;
        validate_name(&self.name).map_err(|_| OutputError::MalformedName(self.name.clone()))?;
        if self.blob.digest.as_32().is_err() {
            return Err(OutputError::MalformedDigest);
        }
        Ok(())
    }

    /// Whether this result honours the signed grant it answers. A result that
    /// renames the slot, overflows the ceiling, or swaps the resource is not a
    /// result for this job.
    pub fn validate_against(&self, spec: &JobSpec) -> Result<(), OutputError> {
        self.validate_self()?;
        if self.name != spec.output.name {
            return Err(OutputError::UngrantedName(self.name.clone()));
        }
        if self.resource != spec.resource {
            return Err(OutputError::WrongResource(self.resource.clone()));
        }
        if self.blob.byte_len > spec.output.max_bytes {
            return Err(OutputError::OverGrant {
                bytes: self.blob.byte_len,
                max_bytes: spec.output.max_bytes,
            });
        }
        if !self.verification.satisfies(spec.determinism) {
            return Err(OutputError::WeakerThanAsked {
                declared: self.verification.class(),
                asked: spec.determinism,
            });
        }
        Ok(())
    }
}

/// Why a committed result does not answer its job.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum OutputError {
    #[error("job result resource id: {0}")]
    Resource(IdentError),
    #[error("job result implementation id: {0}")]
    Implementation(IdentError),
    #[error("job result slot name {0:?} is malformed")]
    MalformedName(String),
    #[error("job result fills ungranted slot {0:?}")]
    UngrantedName(String),
    #[error("job result names resource {0}, which the job did not ask for")]
    WrongResource(ResourceId),
    #[error("job result is {bytes} bytes against a {max_bytes}-byte grant")]
    OverGrant { bytes: u64, max_bytes: u64 },
    #[error("job result carries a malformed content digest")]
    MalformedDigest,
    #[error("job result claims {declared:?} against a {asked:?} ask")]
    WeakerThanAsked {
        declared: DeterminismClass,
        asked: DeterminismClass,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resource() -> ResourceId {
        ResourceId::parse("esp.embed.lexical/v1").unwrap()
    }

    fn spec() -> JobSpec {
        JobSpec::simple(
            resource(),
            "texts",
            BlobRef::blake3(b"batch"),
            "vectors",
            4096,
            DeterminismClass::Exact,
        )
    }

    fn output() -> JobOutput {
        JobOutput {
            name: "vectors".to_string(),
            blob: BlobRef::blake3(b"result"),
            resource: resource(),
            implementation: ImplementationId::parse("mesh.lexical.fnv1a/v1").unwrap(),
            verification: VerificationClass::ExactBytes,
        }
    }

    #[test]
    fn a_well_formed_spec_validates_and_resolves_its_inputs() {
        let spec = spec();
        assert!(spec.validate().is_ok());
        assert_eq!(spec.input("texts"), Some(&BlobRef::blake3(b"batch")));
        assert_eq!(spec.input("weights"), None);
    }

    #[test]
    fn duplicate_input_names_are_refused() {
        let mut spec = spec();
        spec.inputs.push(JobInput {
            name: "texts".to_string(),
            blob: BlobRef::blake3(b"other"),
        });
        assert_eq!(
            spec.validate(),
            Err(SpecError::DuplicateInput("texts".to_string()))
        );
    }

    #[test]
    fn malformed_names_and_counts_are_refused() {
        for name in [
            "",
            "Texts",
            "-texts",
            "text s",
            &"t".repeat(MAX_NAME_LEN + 1),
        ] {
            let mut spec = spec();
            spec.inputs[0].name = name.to_string();
            assert!(spec.validate().is_err(), "{name:?} should be refused");
        }
        let mut spec = spec();
        spec.inputs = (0..=MAX_INPUTS)
            .map(|i| JobInput {
                name: format!("input{i}"),
                blob: BlobRef::blake3(b"x"),
            })
            .collect();
        assert_eq!(
            spec.validate(),
            Err(SpecError::TooManyInputs(MAX_INPUTS + 1))
        );
    }

    #[test]
    fn oversized_and_empty_grants_are_refused() {
        for max_bytes in [0, MAX_OUTPUT_BYTES + 1, u64::MAX] {
            let mut spec = spec();
            spec.output.max_bytes = max_bytes;
            assert_eq!(spec.validate(), Err(SpecError::OversizedGrant(max_bytes)));
        }
    }

    #[test]
    fn an_invalid_resource_id_fails_the_whole_spec() {
        let mut spec = spec();
        spec.resource = super::super::ident::ResourceId::parse("mesh.echo/v1").unwrap();
        assert!(spec.validate().is_ok());
        // Only a decoded (unvalidated) id can be malformed, so round-trip one in.
        let forged: ResourceId = p2panda_core::cbor::decode_cbor(
            p2panda_core::cbor::encode_cbor(&"nope").unwrap().as_slice(),
        )
        .unwrap();
        spec.resource = forged;
        assert!(matches!(spec.validate(), Err(SpecError::Resource(_))));
    }

    #[test]
    fn a_result_must_honour_the_grant_it_answers() {
        let spec = spec();
        assert!(output().validate_against(&spec).is_ok());

        let mut renamed = output();
        renamed.name = "elsewhere".to_string();
        assert!(matches!(
            renamed.validate_against(&spec),
            Err(OutputError::UngrantedName(_))
        ));

        let mut swapped = output();
        swapped.resource = ResourceId::parse("mesh.echo/v1").unwrap();
        assert!(matches!(
            swapped.validate_against(&spec),
            Err(OutputError::WrongResource(_))
        ));

        let mut oversize = output();
        oversize.blob.byte_len = spec.output.max_bytes + 1;
        assert!(matches!(
            oversize.validate_against(&spec),
            Err(OutputError::OverGrant { .. })
        ));

        let mut weak = output();
        weak.verification = VerificationClass::ProducerOnly;
        assert!(matches!(
            weak.validate_against(&spec),
            Err(OutputError::WeakerThanAsked { .. })
        ));
    }

    #[test]
    fn verification_classes_order_from_exact_down_to_observed() {
        let exact = VerificationClass::ExactBytes;
        let tolerant = VerificationClass::Tolerance {
            max_abs_error_ppm: 10,
        };
        let observed = VerificationClass::ProducerOnly;
        assert!(exact.satisfies(DeterminismClass::Exact));
        assert!(exact.satisfies(DeterminismClass::Observed));
        assert!(!tolerant.satisfies(DeterminismClass::Exact));
        assert!(tolerant.satisfies(DeterminismClass::Tolerant));
        assert!(!observed.satisfies(DeterminismClass::Tolerant));
        assert!(observed.satisfies(DeterminismClass::Observed));
    }

    #[test]
    fn requirements_match_host_facts() {
        let cpu = HostFacts::cpu(4096);
        let gpu = HostFacts {
            memory_mib: 4096,
            gpu: true,
        };
        assert!(ResourceRequirements::cpu().satisfied_by(&cpu));
        let heavy = ResourceRequirements {
            memory_mib: 8192,
            compute: ComputeClass::Cpu,
        };
        assert!(!heavy.satisfied_by(&cpu));
        let needs_gpu = ResourceRequirements {
            memory_mib: 0,
            compute: ComputeClass::Gpu,
        };
        assert!(!needs_gpu.satisfied_by(&cpu));
        assert!(needs_gpu.satisfied_by(&gpu));
    }
}
