// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Embeddable, seed-free OTP code tiles.
//!
//! A tile is a short-lived presentation result from the resident authority.
//! It holds the code a host is allowed to show, ordinary item metadata, and
//! enough integer timing information for any renderer to draw the remaining
//! time ring. It deliberately has no serialization or public constructor:
//! carriers do not get a new code wire before an actual carrier needs one.

use std::fmt;

use super::{OtpItem, OtpKind};
use zeroize::Zeroizing;

/// One code prepared for an admitted host to show.
///
/// The code is intentionally not part of [`fmt::Debug`]. It is not a seed,
/// but it is still a credential valid for a short interval and belongs on a
/// screen, not in diagnostic output.
pub struct OtpCodeTile {
    item: OtpItem,
    code: Zeroizing<String>,
    time_ring: Option<OtpTimeRing>,
}

impl fmt::Debug for OtpCodeTile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OtpCodeTile")
            .field("item", &self.item)
            .field("code", &"<redacted>")
            .field("time_ring", &self.time_ring)
            .finish()
    }
}

impl OtpCodeTile {
    pub(crate) fn new(item: OtpItem, code: String, unix_secs: u64) -> Self {
        let time_ring = match item.kind {
            OtpKind::Totp { period, t0 } => {
                let elapsed = unix_secs.saturating_sub(t0);
                let seconds_remaining = period - (elapsed % period);
                Some(OtpTimeRing {
                    period_seconds: period,
                    seconds_remaining,
                })
            }
            OtpKind::Hotp { .. } => None,
        };
        Self {
            item,
            code: Zeroizing::new(code),
            time_ring,
        }
    }

    /// Secret-free item metadata, suitable for the tile label.
    pub fn item(&self) -> &OtpItem {
        &self.item
    }

    /// The short-lived code the admitted host may display.
    pub fn code(&self) -> &str {
        self.code.as_str()
    }

    /// Remaining-time facts for a TOTP code, or `None` for HOTP.
    pub fn time_ring(&self) -> Option<OtpTimeRing> {
        self.time_ring
    }
}

/// Integer facts for a TOTP tile's remaining-time ring.
///
/// Renderers choose their own geometry and motion. The values are fixed for a
/// code at one instant, so a host can redraw without copying secret material
/// or deriving time from the seed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OtpTimeRing {
    /// The TOTP period selected by the issuer.
    pub period_seconds: u64,
    /// Whole seconds before this code is replaced. At a fresh step this equals
    /// `period_seconds`; immediately before rollover it is one.
    pub seconds_remaining: u64,
}

impl OtpTimeRing {
    /// Whole seconds elapsed within this code's period.
    pub fn elapsed_seconds(self) -> u64 {
        self.period_seconds.saturating_sub(self.seconds_remaining)
    }

    /// Completed ring fraction, quantized to 0 through 1000.
    pub fn completed_per_mille(self) -> u16 {
        if self.period_seconds == 0 {
            return 0;
        }
        let completed = self
            .elapsed_seconds()
            .saturating_mul(1_000)
            .checked_div(self.period_seconds)
            .unwrap_or(0);
        completed.min(1_000) as u16
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::otp::{OtpAlgorithm, OtpItemId};

    fn item(kind: OtpKind) -> OtpItem {
        OtpItem {
            id: OtpItemId::from_uuid(uuid::Uuid::from_u128(0x44)),
            account: "mark".to_string(),
            issuer: Some("Merely".to_string()),
            algorithm: OtpAlgorithm::Sha1,
            digits: 6,
            kind,
        }
    }

    #[test]
    fn totp_tile_describes_the_remaining_time_ring_without_exposing_its_code_in_debug() {
        let tile = OtpCodeTile::new(
            item(OtpKind::Totp { period: 30, t0: 0 }),
            "123456".to_string(),
            29,
        );

        assert_eq!(tile.code(), "123456");
        assert_eq!(
            tile.time_ring(),
            Some(OtpTimeRing {
                period_seconds: 30,
                seconds_remaining: 1,
            })
        );
        assert_eq!(tile.time_ring().unwrap().elapsed_seconds(), 29);
        assert_eq!(tile.time_ring().unwrap().completed_per_mille(), 966);
        assert!(!format!("{tile:?}").contains("123456"));
    }

    #[test]
    fn hotp_tile_has_no_countdown_ring() {
        let tile = OtpCodeTile::new(item(OtpKind::Hotp { counter: 7 }), "123456".to_string(), 59);

        assert_eq!(tile.time_ring(), None);
    }
}
