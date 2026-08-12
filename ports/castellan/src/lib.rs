//! Name reservation for **castellan**, the credential-keeper port of the Mere
//! platform.
//!
//! A castellan holds a keep in trust for its lord: custody without ownership,
//! and the office of the gate. This port is that keeper for your credentials.
//! It splits in two:
//!
//! - an **embeddable half** any host app composes: vault browse, credential
//!   status, code tiles. These views render *about* secrets and never contain
//!   them.
//! - an **authority half** that lives with the resident: credential release,
//!   signing, presentation. Requests arrive as participant-gate petitions and
//!   are answered over an agent-style channel, the way the personae ssh-agent
//!   already works. Apps talk to a pipe; apps never see the key.
//!
//! The vocabulary it keeps (per the dramatis tier model):
//!
//! - **chatelaine**: the secrets. Passwords, 2FA seeds, tokens, foreign key
//!   material. Never presented, only exercised.
//! - **emblem**: the proofs. Graded presentations of identity a persona hands
//!   out, from a bare handle to signed cross-attestations. Made to be shown;
//!   what lands in someone else's gaz.
//!
//! The boundaries are the point:
//!
//! - **Not personae.** The faces, their derivation roots, and the vault
//!   substrate live in `personae`; castellan is the keeper who serves them.
//! - **Not gaz or gazette.** Those keep and find the other players; castellan
//!   guards and presents you.
//!
//! [`otp`] is the algorithm half of the chatelaine's 2FA codes: given a secret
//! and a clock, the digits; given an `otpauth://` URI, the configured
//! generator. [`reticulum`] is Castellan's first device-identity issue seam:
//! it derives a radio credential from a supplied Persona provider without
//! creating a device-local account file. Storage, presentation, and
//! gate-mediated release remain later slices; see the castellan OTP plan in
//! mere's design docs.

#![doc(html_no_source)]
#![warn(missing_docs)]

pub mod otp;
pub mod reticulum;
