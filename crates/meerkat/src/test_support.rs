/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Shared test harness helpers for process-global host resources.

use std::{
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
    sync::OnceLock,
    time::Duration,
};

use winit::event_loop::EventLoopProxy;

use crate::Shell;

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

pub(crate) struct TempSessionDir {
    path: PathBuf,
}

impl TempSessionDir {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempSessionDir {
    fn drop(&mut self) {
        for attempt in 0..5 {
            match std::fs::remove_dir_all(&self.path) {
                Ok(()) => return,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => return,
                Err(_) if attempt < 4 => std::thread::sleep(Duration::from_millis(50)),
                Err(_) => return,
            }
        }
    }
}

pub(crate) struct TestShell {
    shell: Shell,
    _temp: TempSessionDir,
}

impl TestShell {
    pub(crate) fn new(shell: Shell, temp: TempSessionDir) -> Self {
        Self { shell, _temp: temp }
    }
}

impl Deref for TestShell {
    type Target = Shell;

    fn deref(&self) -> &Self::Target {
        &self.shell
    }
}

impl DerefMut for TestShell {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.shell
    }
}

pub(crate) fn temp_session_dir(group: &str) -> TempSessionDir {
    let dir = std::env::temp_dir().join(group).join(format!(
        "{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    TempSessionDir { path: dir }
}
