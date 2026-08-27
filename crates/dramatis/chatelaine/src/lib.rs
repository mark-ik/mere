// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Name reservation for **chatelaine**, the secret half of the Mere
//! platform's credential model.
//!
//! Named for the waist-worn chain that held the household's keys, and by
//! extension the keeper of them. The chatelaine holds what must never be
//! shown: passwords, 2FA seeds, tokens, foreign key material. Everything here
//! is *damaged by disclosure*: a password shown is burned, a TOTP seed shown
//! is cloned, a bearer token shown is stolen. Chatelaine items are exercised
//! (filled, generated, released through the gate), never presented.
//!
//! The boundaries are the point:
//!
//! - **Not the proofs.** That is `insigne`: public-key artifacts made to be
//!   shown. The insigne/chatelaine boundary is cryptographic, not filing.
//! - **Not the keeper.** That is `castellan`, which exercises the chatelaine
//!   behind gate petitions; apps talk to a pipe and never see the key.
//! - **Not the substrate.** Storage and sealing are personae's vault; the
//!   chatelaine is the item taxonomy kept there.
//!
//! No implementation yet.

#![doc(html_no_source)]
