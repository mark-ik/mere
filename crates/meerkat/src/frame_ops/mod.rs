/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Navigation, content, session, and chrome-drain operations for
//! [`Shell`](super::Shell). Factored from `main.rs` to keep files under the
//! workspace 600-LOC ceiling.

use frisket::{GraphId, InsertSide, PaneContent, PaneId, PaneNode};
use meerkat::Chrome;
use mere::canvas::Canvas;
use mere::forme::GraphMemberId;
use session_runtime::{PersistedSettings, settings_store};

use super::observability::ObservabilitySnapshot;
use super::{GRAPH_PANE, WindowCtx, fetch, frame_view};

mod config;
mod panes;
