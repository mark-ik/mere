/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Context and shellbar menus for a window: opening the right-click context menu
//! over the selection working set (or Add node / Add field on empty canvas),
//! opening the shellbar move menu, dismissing the menu, and draining the captured
//! menu action (open as splits / stack, relate, add node / tile / field / session,
//! move the shellbar). The chrome renders the rows; the host owns the set and runs
//! the action. Factored out of `frame_ops.rs` to keep files under the 600-LOC
//! ceiling.

use mere::forme::GraphMemberId;
use mere::kernel::graph::SemanticSubKind;
use meerkat::{Chrome, ContextAction, ContextItem};
use mere::canvas::Face;
use session_runtime::ShellbarEdge;

use super::WindowCtx;
use super::observability::Severity;

mod actions;
mod build;
