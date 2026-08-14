//! The producing machine's claim: `manifest.json` and the types it parses to.
//!
//! Separated from [`ingest`](super::ingest) because reading what another
//! machine recorded is a different concern from turning it into graph facts,
//! and because this half needs no store, no async, and no graph vocabulary.

use std::collections::BTreeMap;
use std::path::PathBuf;

use muniment::StoreError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// Namespace for deriving a receipt's node id from its address.
///
/// A v5 (name-based) UUID, not v4: the id has to be a function of the receipt
/// so that ingesting the same directory twice reaches the same node instead of
/// minting a second one.
pub(crate) const RECEIPT_NAMESPACE: Uuid = Uuid::from_u128(0x8f2c_41d7_5e3b_4a90_9c17_6de2_0b84_f5a3);

/// The facet carrying a run's provenance.
pub const FACET_RUN: &str = "receipt.run";
/// The facet listing a run's artifacts.
pub const FACET_ARTIFACTS: &str = "receipt.artifacts";
/// Address prefix for every receipt node.
pub const ADDRESS_PREFIX: &str = "receipt:";

/// What went wrong reading or ingesting a receipt directory.
#[derive(Debug)]
pub enum ReceiptError {
    /// The directory has no `manifest.json`, so nothing here has provenance.
    NoManifest(PathBuf),
    /// The manifest did not parse.
    Manifest(serde_json::Error),
    /// An artifact named in the manifest is missing from the directory.
    MissingArtifact(String),
    /// An artifact's bytes do not match the digest the producing machine
    /// recorded. The transfer corrupted it, or the manifest is not this
    /// directory's.
    DigestMismatch {
        /// The artifact's file name.
        name: String,
        /// What the manifest claimed.
        expected: String,
        /// What the bytes here actually hash to.
        found: String,
    },
    /// Reading a file failed.
    Io(std::io::Error),
    /// Writing a blob failed.
    Store(StoreError),
}

impl std::fmt::Display for ReceiptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoManifest(path) => {
                write!(f, "no manifest.json in {}", path.display())
            }
            Self::Manifest(error) => write!(f, "manifest.json did not parse: {error}"),
            Self::MissingArtifact(name) => {
                write!(f, "manifest names `{name}`, which is not in the directory")
            }
            Self::DigestMismatch {
                name,
                expected,
                found,
            } => write!(
                f,
                "`{name}` does not match its recorded digest \
                 (expected {expected}, found {found})"
            ),
            Self::Io(error) => write!(f, "reading the receipt failed: {error}"),
            Self::Store(error) => write!(f, "storing a blob failed: {error:?}"),
        }
    }
}

impl std::error::Error for ReceiptError {}

impl From<std::io::Error> for ReceiptError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<StoreError> for ReceiptError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

/// One artifact as the producing machine recorded it.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ManifestArtifact {
    /// File name within the receipt directory.
    pub name: String,
    /// Size in bytes.
    pub bytes: u64,
    /// Lowercase hex SHA-256, computed where the artifact was produced.
    pub sha256: String,
}

/// `manifest.json`, as written by the remote-receipt lane.
///
/// Unknown fields are kept rather than rejected: a newer lane may record more
/// provenance than this build knows about, and dropping it on ingest would
/// lose exactly the context the receipt exists to carry.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ReceiptManifest {
    /// Repository the scenario belongs to.
    pub repo: String,
    /// Cargo package that ran.
    pub package: String,
    /// Scenario path, relative to the checkout.
    pub scenario: String,
    /// SSH destination the run happened on.
    pub target: String,
    /// `linux` / `macos` / `windows`.
    pub platform: String,
    /// `uname -sr` (or equivalent) of the producing machine.
    pub remote_os: String,
    /// Commit the checkout was on.
    pub remote_commit: String,
    /// How many files were dirty. Non-zero means the receipt is not
    /// attributable to a commit alone.
    #[serde(default)]
    pub remote_dirty: u32,
    /// `wayland (…)`, `x11 (…)`, `aqua`.
    #[serde(default)]
    pub session: String,
    /// When the run happened, RFC 3339.
    pub ran_at_utc: String,
    /// Process exit code.
    pub exit_code: i32,
    /// The artifacts, with the digests to verify them against.
    #[serde(default)]
    pub artifacts: Vec<ManifestArtifact>,
    /// Everything else the lane recorded, preserved untouched.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl ReceiptManifest {
    /// Parse `manifest.json`.
    pub fn parse(json: &str) -> Result<Self, ReceiptError> {
        serde_json::from_str(json).map_err(ReceiptError::Manifest)
    }

    /// The receipt's stable address: what identifies this run forever.
    ///
    /// Built from the run's own facts rather than the directory name, so the
    /// same run ingested from a copied or renamed directory is still the same
    /// receipt.
    pub fn address(&self) -> String {
        format!(
            "receipt:{}/{}/{}/{}",
            self.repo, self.host(), self.scenario, self.ran_at_utc
        )
    }

    /// The machine part of `target`.
    pub fn host(&self) -> &str {
        self.target.rsplit('@').next().unwrap_or(&self.target)
    }

    /// The node id this receipt always lands on.
    pub fn node_id(&self) -> Uuid {
        Uuid::new_v5(&RECEIPT_NAMESPACE, self.address().as_bytes())
    }

    /// A one-line title for a list.
    pub fn title(&self) -> String {
        let verdict = if self.exit_code == 0 { "ok" } else { "failed" };
        format!(
            "{} · {} on {} · {verdict}",
            self.repo,
            scenario_name(&self.scenario),
            self.host()
        )
    }

    /// Whether the run passed.
    pub fn passed(&self) -> bool {
        self.exit_code == 0
    }

    /// The run's timestamp in unix milliseconds, for the facts that need one.
    /// `0` when the manifest's timestamp does not parse, which keeps ingest
    /// total rather than making a clock the failure path.
    pub fn ran_at_ms(&self) -> u64 {
        parse_rfc3339_ms(&self.ran_at_utc).unwrap_or(0)
    }
}

/// The bare scenario file name, for titles and tags.
fn scenario_name(scenario: &str) -> &str {
    scenario
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(scenario)
        .trim_end_matches(".scn")
}

/// Minimal RFC 3339 → unix milliseconds.
///
/// Hand-rolled because this crate has no date dependency and needs exactly one
/// direction of one format: the lane writes `o`-round-trip UTC. Anything else
/// returns `None` and the caller treats the time as unknown rather than
/// failing an otherwise-good ingest.
pub(crate) fn parse_rfc3339_ms(text: &str) -> Option<u64> {
    let text = text.trim();
    let (date, rest) = text.split_once('T')?;
    let mut date_parts = date.split('-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: i64 = date_parts.next()?.parse().ok()?;
    let day: i64 = date_parts.next()?.parse().ok()?;

    let time = rest
        .trim_end_matches('Z')
        .split_once('+')
        .map(|(t, _)| t)
        .unwrap_or_else(|| rest.trim_end_matches('Z'));
    let mut time_parts = time.split(':');
    let hour: i64 = time_parts.next()?.parse().ok()?;
    let minute: i64 = time_parts.next()?.parse().ok()?;
    let seconds_field = time_parts.next()?;
    let (secs, frac) = seconds_field
        .split_once('.')
        .unwrap_or((seconds_field, "0"));
    let second: i64 = secs.parse().ok()?;
    let millis: i64 = format!("{frac:0<3}")[..3].parse().ok()?;

    // Days from the civil calendar (Howard Hinnant's algorithm), which is
    // exact and needs no table.
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;

    let total = ((days * 86_400 + hour * 3_600 + minute * 60 + second) * 1_000) + millis;
    u64::try_from(total).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3339_parses_to_unix_millis() {
        // 2026-08-10T14:31:05.123Z. Derived by hand so the constant is an
        // independent check on the algorithm rather than a restatement of it:
        // 20675 days from the epoch (20454 to 2026-01-01, of which 14 are leap
        // days, plus 221 into the year), 20675 * 86400 = 1_786_320_000s, plus
        // 14:31:05 = 52_265s, plus 123ms.
        assert_eq!(
            parse_rfc3339_ms("2026-08-10T14:31:05.1234567Z"),
            Some(1_786_372_265_123),
        );
        assert_eq!(parse_rfc3339_ms("1970-01-01T00:00:00.000Z"), Some(0));
        // Sub-second precision is optional and over-long fractions truncate
        // rather than fail; the lane writes seven digits.
        assert_eq!(
            parse_rfc3339_ms("2026-08-10T14:31:05Z"),
            Some(1_786_372_265_000),
        );
        assert_eq!(parse_rfc3339_ms("not a time"), None);
    }
}
