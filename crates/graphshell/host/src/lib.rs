/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Host-side adapters for Graphshell surface lifecycle seams.

pub mod host_toolkit;
pub mod viewer_surface_host;

pub use host_toolkit::{HostToolkit, NEW_HOST_TOOLKIT_ORDER, preferred_host_toolkits};
pub use viewer_surface_host::{SurfaceFactoryHost, ViewerSurfaceRegistryHost};
