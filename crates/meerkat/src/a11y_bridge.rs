/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Platform AccessKit bridge for the host-local uxtree snapshot.

use accesskit::{Action, NodeId as AccessNodeId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BridgeStatus {
    Unavailable,
    Installed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct A11yActionRequest {
    pub action: Action,
    pub target_node: AccessNodeId,
}

#[cfg(target_os = "windows")]
mod imp {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use accesskit::{ActionHandler, ActionRequest, ActivationHandler, TreeUpdate};
    use accesskit_windows::{HWND, SubclassingAdapter};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winit::window::Window;

    use super::{A11yActionRequest, BridgeStatus};

    pub(crate) struct AccessKitBridge {
        adapter: Option<SubclassingAdapter>,
        latest: Arc<Mutex<Option<TreeUpdate>>>,
        actions: Arc<Mutex<VecDeque<A11yActionRequest>>>,
        wake: Arc<dyn Fn() + Send + Sync>,
    }

    impl AccessKitBridge {
        pub(crate) fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
            Self {
                adapter: None,
                latest: Arc::new(Mutex::new(None)),
                actions: Arc::new(Mutex::new(VecDeque::new())),
                wake: Arc::new(wake),
            }
        }

        pub(crate) fn status(&self) -> BridgeStatus {
            if self.adapter.is_some() {
                BridgeStatus::Installed
            } else {
                BridgeStatus::Unavailable
            }
        }

        pub(crate) fn install(
            &mut self,
            window: &Window,
            initial: TreeUpdate,
        ) -> Result<(), String> {
            *self.latest.lock().map_err(|err| err.to_string())? = Some(initial);
            if self.adapter.is_some() {
                return Ok(());
            }
            let hwnd = match window
                .window_handle()
                .map_err(|err| err.to_string())?
                .as_raw()
            {
                RawWindowHandle::Win32(handle) => HWND(handle.hwnd.get() as *mut _),
                RawWindowHandle::WinRt(_) => {
                    return Err("WinRT window handles are not supported".to_string());
                }
                _ => return Err("window is not backed by a Win32 HWND".to_string()),
            };
            let adapter = SubclassingAdapter::new(
                hwnd,
                Activation {
                    latest: Arc::clone(&self.latest),
                },
                QueuedActions {
                    actions: Arc::clone(&self.actions),
                    wake: Arc::clone(&self.wake),
                },
            );
            self.adapter = Some(adapter);
            Ok(())
        }

        pub(crate) fn update(&mut self, update: TreeUpdate) {
            if let Ok(mut latest) = self.latest.lock() {
                *latest = Some(update.clone());
            }
            if let Some(adapter) = self.adapter.as_mut() {
                if let Some(events) = adapter.update_if_active(|| update) {
                    events.raise();
                }
            }
        }

        pub(crate) fn drain_actions(&mut self) -> Vec<A11yActionRequest> {
            let Ok(mut actions) = self.actions.lock() else {
                return Vec::new();
            };
            actions.drain(..).collect()
        }
    }

    struct Activation {
        latest: Arc<Mutex<Option<TreeUpdate>>>,
    }

    impl ActivationHandler for Activation {
        fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
            self.latest.lock().ok().and_then(|latest| latest.clone())
        }
    }

    struct QueuedActions {
        actions: Arc<Mutex<VecDeque<A11yActionRequest>>>,
        wake: Arc<dyn Fn() + Send + Sync>,
    }

    impl ActionHandler for QueuedActions {
        fn do_action(&mut self, request: ActionRequest) {
            if let Ok(mut actions) = self.actions.lock() {
                actions.push_back(A11yActionRequest {
                    action: request.action,
                    target_node: request.target_node,
                });
            }
            (self.wake)();
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    use accesskit::TreeUpdate;
    use winit::window::Window;

    use super::{A11yActionRequest, BridgeStatus};

    pub(crate) struct AccessKitBridge;

    impl AccessKitBridge {
        pub(crate) fn new(_wake: impl Fn() + Send + Sync + 'static) -> Self {
            Self
        }

        pub(crate) fn status(&self) -> BridgeStatus {
            BridgeStatus::Unavailable
        }

        pub(crate) fn install(
            &mut self,
            _window: &Window,
            _initial: TreeUpdate,
        ) -> Result<(), String> {
            Err("AccessKit OS bridge is only wired for Windows in this slice".to_string())
        }

        pub(crate) fn update(&mut self, _update: TreeUpdate) {}

        pub(crate) fn drain_actions(&mut self) -> Vec<A11yActionRequest> {
            Vec::new()
        }
    }
}

pub(crate) use imp::AccessKitBridge;
