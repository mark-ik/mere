// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! **Castellan**, the credential-keeper port of the Mere platform.
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
//! - **insigne**: the proofs. Graded presentations of identity a persona hands
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
//! creating a device-local account file.
//!
//! The **keeper surface** (feature `keeper`, founded 2026-08-14) is the two
//! halves made real, moved home from graphshell where they first grew:
//!
//! - [`view`] — the secret-free read model. What a host may know.
//! - [`projection`] — the cards and typed intents. What a host may show and
//!   offer.
//! - [`authority`] — [`authority::PersonaeHost`], the resident keeper. What
//!   only the castellan does.
//!
//! Graphshell composes all three (and re-exports them at their old paths);
//! any other host embeds the subset it needs without inheriting graphshell.
//! OTP items seal their imported configuration under a Persona through
//! [`otp::OtpItemStore`]. [`otp::OtpReleaseGate`] returns an
//! [`otp::OtpCodeTile`] only after a participant-bound petition receives a
//! resident approval. [`otp::OtpAdmittedSession`] binds remote petitions to one
//! exact item and the Notochord transcript that admitted their carrier.
//! [`resident::CastellanResident`] retains the process-wide sealed-record
//! authority. Feature `secret-service` adds the Linux desktop adapter, and
//! [`otp::SteamGuard`] is an explicitly nonstandard Valve compatibility shape.
//! CXF import remains follow-on work; see the castellan OTP plan and the keeper
//! founding plan in mere's design docs.

#![doc(html_no_source)]
#![warn(missing_docs)]

#[cfg(feature = "keeper")]
pub mod authority;
pub mod otp;
#[cfg(feature = "keeper")]
pub mod projection;
pub mod resident;
pub mod reticulum;
#[cfg(feature = "secret-service")]
pub mod secret_service;
#[cfg(feature = "keeper")]
pub mod view;
