// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//
// Drives the probe page in a real browser engine via Playwright, so the
// browser lane is not one host. Chromium and WebKit are also reachable here,
// but the receipt that matters is Firefox: a second engine with an
// independent OPFS and Web Locks implementation.
//
//   node run-browser.mjs --engine firefox --lanes 1,2,4,5,6 --out receipts/<name>.json
//
// Requires: the static server on --port (run-probe.ps1 or `python -m
// http.server`), and `npm i playwright` somewhere resolvable via NODE_PATH.

import { spawnSync } from "node:child_process";
import { writeFile } from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";

const args = Object.fromEntries(
  process.argv.slice(2).join(" ").split("--").filter(Boolean)
    .map((chunk) => { const [k, ...v] = chunk.trim().split(/\s+/); return [k, v.join(" ") || "true"]; }),
);
const ENGINE = args.engine ?? "firefox";
const PORT = Number(args.port ?? 8733);
const LANES = (args.lanes ?? "1,2,4,5,6").split(",").map((s) => s.trim());
const OUT = args.out ?? `receipts/${new Date().toISOString().slice(0, 10)}_${ENGINE}.json`;
const HEADLESS = args.headed !== "true";
const URL_BASE = `http://127.0.0.1:${PORT}/ports/muniment-opfs-probe/web/`;

// ESM ignores NODE_PATH, so a Playwright installed outside this tree (which
// is where it belongs — the probe does not carry node_modules) is named by
// PLAYWRIGHT_MODULE, an absolute path to the package directory.
const specifier = process.env.PLAYWRIGHT_MODULE
  ? pathToFileURL(path.join(process.env.PLAYWRIGHT_MODULE, "index.js")).href
  : "playwright";
// Playwright is CommonJS; imported by file URL its named exports may only be
// reachable through `default`.
const module_ = await import(specifier);
const playwright = module_.default ?? module_;
const browserType = playwright[ENGINE] ?? module_[ENGINE];
if (!browserType) {
  throw new Error(`unknown engine ${ENGINE}; available: ${Object.keys(playwright).join(", ")}`);
}

console.log(`[${ENGINE}] launching (headless=${HEADLESS})`);
const browser = await browserType.launch({ headless: HEADLESS });
const context = await browser.newContext();
const page = await context.newPage();
page.on("console", (m) => { if (m.type() === "error") console.log(`  [page error] ${m.text()}`); });
page.on("pageerror", (e) => console.log(`  [pageerror] ${e.message}`));

await page.goto(URL_BASE, { waitUntil: "load" });
await page.waitForFunction(() => !!window.munimentOpfsProbe, null, { timeout: 30_000 });
console.log(`[${ENGINE}] harness ready`);

// Lane 4's two-tab half needs a real second tab; open one on demand when the
// page announces it wants a holder.
let holderPage = null;
await page.exposeFunction("__openHolder", async (url) => {
  if (holderPage && !holderPage.isClosed()) return true;
  holderPage = await context.newPage();
  await holderPage.goto(url, { waitUntil: "load" });
  return true;
});
await page.evaluate(() => {
  const channel = new BroadcastChannel("muniment-opfs-probe");
  channel.addEventListener("message", (event) => {
    if (event.data?.type === "holder-wanted") {
      const u = new URL(location.href);
      u.searchParams.set("role", "holder");
      u.searchParams.set("path", event.data.path);
      window.__openHolder(u.toString());
    }
  });
});

const results = {};
for (const lane of LANES) {
  const label = `lane ${lane}`;
  console.log(`[${ENGINE}] ${label} …`);
  const started = Date.now();
  try {
    // Lane 4 reloads the page mid-lane; the harness resumes from
    // sessionStorage on load, so wait for the lane to land in the receipt
    // rather than for the runLane promise (which the reload destroys).
    if (lane === "4") {
      await page.evaluate((n) => { window.munimentOpfsProbe.runLane(n).catch(() => {}); }, Number(lane));
      await page.waitForFunction(
        () => window.munimentOpfsProbe.receipt()?.lanes?.lane4?.reload !== undefined
          || window.munimentOpfsProbe.receipt()?.lanes?.lane4?.failed,
        null,
        { timeout: 180_000, polling: 500 },
      );
    } else {
      await page.evaluate(
        async (n) => { await window.munimentOpfsProbe.runLane(n); },
        Number.isNaN(Number(lane)) ? lane : Number(lane),
      );
    }
    const key = `lane${String(lane).replace(/[ab]$/, "")}`;
    const outcome = await page.evaluate((k) => {
      const l = window.munimentOpfsProbe.receipt()?.lanes?.[k];
      return { ok: l?.ok ?? null, failed: l?.failed ?? false, error: l?.error ?? null };
    }, key);
    results[label] = { ...outcome, seconds: Math.round((Date.now() - started) / 1000) };
    console.log(`[${ENGINE}] ${label}: ok=${outcome.ok} failed=${outcome.failed} (${results[label].seconds}s)`);
    if (outcome.error) console.log(`  ${outcome.error.slice(0, 200)}`);
  } catch (error) {
    results[label] = { ok: false, error: String(error).slice(0, 300) };
    console.log(`[${ENGINE}] ${label} threw: ${String(error).slice(0, 200)}`);
  }
}

// An engine can fail so early that the page never builds a receipt at all —
// WebKit does exactly this, having no `navigator.storage` for the harness to
// interrogate. That is a RESULT and must be recorded, so synthesize a
// receipt rather than crashing on a null (which is what this did before).
let receipt = await page.evaluate(() => window.munimentOpfsProbe.receipt() ?? null);
if (!receipt) {
  const probe = await page.evaluate(async () => ({
    user_agent: navigator.userAgent,
    secure_context: isSecureContext,
    storage_manager: !!navigator.storage,
    get_directory: typeof navigator.storage?.getDirectory,
    file_system_file_handle: typeof self.FileSystemFileHandle,
    sync_access_handle: typeof self.FileSystemFileHandle?.prototype?.createSyncAccessHandle,
    web_locks: typeof navigator.locks?.request,
    indexed_db: typeof indexedDB,
    web_assembly: typeof WebAssembly,
  }));
  receipt = {
    schema: "muniment.opfs-probe/v1",
    outcome: "unsupported",
    reason: "the page never produced a receipt; the engine lacks a prerequisite",
    environment: { user_agent: probe.user_agent, capability_probe: probe },
  };
  console.log(`[${ENGINE}] no receipt — recording an "unsupported" result instead`);
}
receipt.runner = { engine: ENGINE, playwright: true, headless: HEADLESS, lanes: LANES, per_lane: results };

// Close the browser→native half of lane 5 here rather than leaving a
// "pending native verify" placeholder in the receipt: the page cannot run a
// native binary, but this runner can. Pull the database the browser wrote,
// verify it with the native `fixture` binary, and record the real answer.
if (receipt.lanes?.lane5?.portability?.state === "ran") {
  try {
    const base64 = await page.evaluate(() => window.munimentOpfsProbe.exportedBase64());
    if (base64) {
      const out = path.resolve(`fixtures/${ENGINE}-export.redb`);
      await writeFile(out, Buffer.from(base64, "base64"));
      const expected = receipt.lanes.lane5.portability.extended_to_generation;
      // Run from a neutral cwd with --locked, for the same reason the build
      // does: cargo picks up `Code/.cargo/config.toml` otherwise.
      const verify = spawnSync(
        "cargo",
        ["run", "--locked", "--release", "--quiet",
          "--manifest-path", path.resolve("Cargo.toml"), "--bin", "fixture", "--",
          "verify", out, path.resolve("fixtures/portability.json"), String(expected)],
        {
          encoding: "utf8",
          cwd: process.env.NEUTRAL_DIR ?? "C:/t",
          env: { ...process.env, CARGO_TARGET_DIR: process.env.NATIVE_TARGET_DIR ?? "C:/t/muniment-opfs-probe-native" },
        },
      );
      const json = (verify.stdout ?? "").slice((verify.stdout ?? "").indexOf("{"));
      receipt.lanes.lane5.portability.browser_to_native = {
        verified_by: "native fixture verify",
        file: out,
        expected_generation: expected,
        exit_code: verify.status,
        result: json ? JSON.parse(json) : { error: (verify.stderr ?? "").slice(-500) },
      };
      const ok = receipt.lanes.lane5.portability.browser_to_native.result?.ok === true;
      receipt.lanes.lane5.portability.round_trip_both_ways_ok =
        ok && receipt.lanes.lane5.portability.native_to_browser_ok === true;
      console.log(`[${ENGINE}] browser→native verify: ok=${ok}`);
    }
  } catch (error) {
    receipt.lanes.lane5.portability.browser_to_native = { error: String(error) };
  }
}
await writeFile(path.resolve(OUT), JSON.stringify(receipt, null, 2));
console.log(`[${ENGINE}] receipt → ${OUT}`);
console.log(JSON.stringify(receipt.conclusions ?? {}, null, 1).slice(0, 2500));

await browser.close();
