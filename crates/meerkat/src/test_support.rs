/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Shared test harness helpers for process-global host resources.

use std::sync::OnceLock;

use winit::event_loop::EventLoopProxy;

pub(crate) fn event_loop_proxy() -> EventLoopProxy<()> {
    static PROXY: OnceLock<EventLoopProxy<()>> = OnceLock::new();
    PROXY
        .get_or_init(|| {
            let mut builder = winit::event_loop::EventLoop::builder();
            #[cfg(target_os = "windows")]
            {
                use winit::platform::windows::EventLoopBuilderExtWindows;
                builder.with_any_thread(true);
            }
            let event_loop = builder.build().expect("event loop");
            let proxy = event_loop.create_proxy();
            Box::leak(Box::new(event_loop));
            proxy
        })
        .clone()
}

pub(crate) fn temp_session_dir(group: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(group).join(format!(
        "{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}
