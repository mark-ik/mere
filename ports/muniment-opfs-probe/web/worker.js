// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//
// The dedicated worker: initializes the wasm module once and runs one probe
// command per message. State and progress messages come straight from Rust
// as JSON strings; results and errors are tagged with the caller's id.

import init, { build_info, export_file, opfs_capabilities, run_command } from "./pkg/muniment_opfs_probe.js";

let initialized;

self.addEventListener("message", async (event) => {
  const { id, command } = event.data ?? {};
  if (!command) return;
  try {
    initialized ??= init();
    await initialized;
    if (command.command === "build_info") {
      self.postMessage({ kind: "result", id, report: JSON.parse(build_info()) });
      return;
    }
    if (command.command === "capabilities") {
      // createSyncAccessHandle is exposed only in dedicated workers, so the
      // page cannot see it; this is the only honest place to record it.
      const fromRust = JSON.parse(await opfs_capabilities());
      self.postMessage({
        kind: "result",
        id,
        report: {
          ...fromRust,
          sync_access_handle: typeof FileSystemFileHandle?.prototype?.createSyncAccessHandle === "function",
          storage_manager: typeof navigator.storage?.getDirectory === "function",
          web_locks: typeof navigator.locks?.request === "function",
          user_agent: navigator.userAgent,
        },
      });
      return;
    }
    if (command.command === "export") {
      const report = JSON.parse(await run_command(JSON.stringify(command)));
      const bytes = await export_file(command.path);
      self.postMessage({ kind: "result", id, report, bytes }, [bytes.buffer]);
      return;
    }
    const report = JSON.parse(await run_command(JSON.stringify(command)));
    self.postMessage({ kind: "result", id, report });
  } catch (error) {
    self.postMessage({
      kind: "error",
      id,
      error: String(error),
      stack: error?.stack ?? null,
    });
  }
});
