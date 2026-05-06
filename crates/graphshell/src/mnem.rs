/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Private local-memory vocabulary for Graphshell.
//!
//! Mnem is the owner-scoped lane for local blobs, caches, and browsing memory.
//! It is distinct from Mere transport state and Moothold community state.

use serde::{Deserialize, Serialize};

use crate::app_state::WorkspaceServiceResult;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MnemRequest {
    LoadBlob { key: String },
    SaveBlob { key: String, value: Vec<u8> },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MnemResponse {
    BlobLoaded { key: String, value: Option<Vec<u8>> },
    BlobSaved { key: String },
}

pub trait MnemStore {
    fn load_blob(&mut self, key: &str) -> WorkspaceServiceResult<Option<Vec<u8>>>;

    fn save_blob(&mut self, key: &str, value: &[u8]) -> WorkspaceServiceResult<()>;
}

pub fn dispatch_mnem_request(
    store: &mut dyn MnemStore,
    request: &MnemRequest,
) -> WorkspaceServiceResult<MnemResponse> {
    match request {
        MnemRequest::LoadBlob { key } => Ok(MnemResponse::BlobLoaded {
            key: key.clone(),
            value: store.load_blob(key)?,
        }),
        MnemRequest::SaveBlob { key, value } => {
            store.save_blob(key, value)?;
            Ok(MnemResponse::BlobSaved { key: key.clone() })
        }
    }
}
