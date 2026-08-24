// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MPL-2.0
//
// The page drives one dedicated worker per step, terminates workers at chosen
// points, opens a second tab, reloads itself, and assembles the receipt.
// Every lane is also reachable from automation:
//   window.munimentOpfsProbe.runLane(n) / runAll() / receipt() / exportedBase64()

const $ = (id) => document.getElementById(id);
const status = $("status");
const stateLog = $("state-log");
const receiptView = $("receipt");
const downloadButton = $("download-receipt");
const CHANNEL_NAME = "muniment-opfs-probe";
const RECEIPT_KEY = "muniment-opfs-probe.receipt";
const RESUME_KEY = "muniment-opfs-probe.resume";

let receipt = restoreReceipt();
let exported = null;
let nextId = 1;

function setState(state, text) {
  document.body.dataset.probeState = state;
  status.textContent = text;
}

function logState(lane, state, detail) {
  const item = document.createElement("li");
  item.textContent = `${lane}: ${state} · ${detail}`;
  stateLog.append(item);
  stateLog.scrollTop = stateLog.scrollHeight;
}

const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// Chromium throttles a hidden page's timers to ~1 s. The kill delay inside a
// termination trial must be sub-millisecond-accurate and the worker runs on
// its own thread, so the page simply spins; polling yields through a
// MessageChannel, which is not throttled.
function busyWait(ms) {
  const until = performance.now() + ms;
  while (performance.now() < until) { /* spin */ }
}

const yieldTick = () => new Promise((resolve) => {
  const channel = new MessageChannel();
  channel.port1.onmessage = () => { channel.port1.close(); resolve(); };
  channel.port2.postMessage(0);
});

function shape() {
  return {
    keys_per_commit: Number($("keys-per-commit").value),
    value_bytes: Number($("value-bytes").value),
  };
}

function prefix() {
  return $("opfs-prefix").value.trim() || "muniment-probe";
}

function parseMessage(data) {
  if (typeof data === "string") {
    try { return JSON.parse(data); } catch { return { kind: "unknown", data }; }
  }
  return data;
}

// One worker, one command at a time. `terminate()` settles the pending call
// with `{ terminated: true }` so a trial can kill mid-commit on purpose.
class ProbeWorker {
  constructor(label) {
    this.label = label;
    this.worker = new Worker("./worker.js", { type: "module", name: `muniment-opfs-${label}` });
    this.pending = null;
    this.alive = true;
    this.worker.addEventListener("message", (event) => this.onMessage(parseMessage(event.data)));
    this.worker.addEventListener("error", (event) => {
      const location = event.filename ? `${event.filename}:${event.lineno ?? "?"}` : null;
      this.settle(null, new Error([event.message || "worker error", location].filter(Boolean).join(" @ ")));
    });
  }

  onMessage(message) {
    if (!this.pending) return;
    if (message.kind === "state") {
      logState(this.label, message.state, message.detail);
      setState(message.state, `${this.label}: ${message.detail}`);
      this.pending.onState?.(message);
      return;
    }
    if (message.kind === "progress") {
      this.pending.onProgress?.(message);
      return;
    }
    if (message.kind === "result" && message.id === this.pending.id) {
      this.settle({ report: message.report, bytes: message.bytes ?? null });
      return;
    }
    if (message.kind === "error" && message.id === this.pending.id) {
      this.settle(null, new Error([message.error, message.stack].filter(Boolean).join("\n")));
    }
  }

  settle(value, error = null) {
    const pending = this.pending;
    if (!pending) return;
    this.pending = null;
    if (error) pending.reject(error);
    else pending.resolve(value);
  }

  call(command, hooks = {}) {
    if (this.pending) throw new Error(`${this.label}: a command is already running`);
    if (!this.alive) throw new Error(`${this.label}: worker was terminated`);
    const id = nextId++;
    return new Promise((resolve, reject) => {
      this.pending = { id, resolve, reject, ...hooks };
      this.worker.postMessage({ id, command });
    });
  }

  terminate() {
    if (!this.alive) return;
    this.alive = false;
    this.worker.terminate();
    this.settle({ terminated: true });
  }
}

async function oneShot(label, command, hooks) {
  const worker = new ProbeWorker(label);
  try {
    const { report, bytes } = await worker.call(command, hooks);
    return bytes ? { ...report, bytes } : report;
  } finally {
    worker.terminate();
  }
}

const HANDLE_HELD = /NoModificationAllowedError/;

// The browser releases a terminated worker's sync-access handle
// asynchronously, so an open that follows a kill can be refused for a while.
// That wait is a lane-4 measurement, not a harness failure: retry while the
// refusal is the exclusivity error, and report how long it took.
async function untilHandleReleased(label, attempt, timeoutMs = 30_000) {
  const started = performance.now();
  let attempts = 0;
  for (;;) {
    attempts += 1;
    let report;
    try {
      report = await attempt(attempts);
    } catch (error) {
      if (HANDLE_HELD.test(String(error)) && performance.now() - started < timeoutMs) { await yieldTick(); continue; }
      throw new Error(`${label}: after ${attempts} attempts over ${Math.round(performance.now() - started)} ms: ${error}`);
    }
    const refused = report?.open?.opened === false && HANDLE_HELD.test(report.open.error ?? "");
    if (!refused || performance.now() - started >= timeoutMs) {
      return { report, attempts, release_wait_ms: performance.now() - started, refused_until_timeout: refused };
    }
    await yieldTick();
  }
}

function stashLane(n, value) {
  receipt.lanes[`lane${n}`] = value;
  saveReceipt();
  showReceipt();
}

// Some StorageManager calls never settle in a headless browser:
// `navigator.storage.persist()` awaits a permission prompt that nobody
// answers, which hangs Firefox indefinitely (Chromium auto-denies). A probe
// must not deadlock on a permission UI, so these are raced with a deadline
// and a timeout is recorded as a fact rather than swallowed.
const TIMED_OUT = Symbol("timed out");
function withTimeout(promise, ms) {
  return Promise.race([
    Promise.resolve(promise).catch((error) => ({ __error: String(error) })),
    new Promise((resolve) => setTimeout(() => resolve(TIMED_OUT), ms)),
  ]);
}

async function storageSnapshot() {
  if (!navigator.storage?.estimate) return { state: "unknown", reason: "StorageManager unavailable" };
  const estimate = await withTimeout(navigator.storage.estimate(), 5_000);
  const persisted = await withTimeout(navigator.storage.persisted(), 5_000);
  if (estimate === TIMED_OUT) return { state: "unknown", reason: "storage.estimate() did not settle in 5 s" };
  return {
    state: persisted === TIMED_OUT ? "unknown" : persisted ? "persistent" : "best_effort",
    persisted_timed_out: persisted === TIMED_OUT,
    quota_bytes: estimate.quota ?? null,
    usage_bytes: estimate.usage ?? null,
  };
}

async function sha256Hex(buffer) {
  const digest = await crypto.subtle.digest("SHA-256", buffer);
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

// Hash the wasm actually executing, and the two JS files that drive it, so a
// receipt identifies its own build rather than a commit the probe is not
// tracked in.
async function wasmBundle() {
  try {
    const [wasm, probeJs, workerJs] = await Promise.all([
      fetch("./pkg/muniment_opfs_probe_bg.wasm").then((r) => r.arrayBuffer()),
      fetch("./probe.js").then((r) => r.arrayBuffer()),
      fetch("./worker.js").then((r) => r.arrayBuffer()),
    ]);
    return {
      bytes: wasm.byteLength,
      wasm_sha256: await sha256Hex(wasm),
      probe_js_sha256: await sha256Hex(probeJs),
      worker_js_sha256: await sha256Hex(workerJs),
    };
  } catch (error) {
    return { bytes: null, error: String(error) };
  }
}

async function environment() {
  return {
    user_agent: navigator.userAgent,
    platform: navigator.userAgentData?.platform ?? navigator.platform ?? "unknown",
    hardware_concurrency: navigator.hardwareConcurrency ?? null,
    cross_origin_isolated: crossOriginIsolated,
    page_web_locks: typeof navigator.locks?.request === "function",
    visibility: document.visibilityState,
    wasm_bundle: await wasmBundle(),
    build: await oneShot("build-info", { command: "build_info" }),
    // Storage capabilities are only visible inside a dedicated worker.
    worker: await oneShot("capabilities", { command: "capabilities" }),
  };
}

// ── lanes ────────────────────────────────────────────────────────────────

async function lane1() {
  const report = await oneShot("lane1", { command: "in_memory_smoke" });
  return { report, ok: report.ok === true };
}

async function lane2() {
  const path = `${prefix()}/lane2.redb`;
  const wrote = await oneShot("lane2-write", {
    command: "opfs_round_trip", path, reset: true, commits: 8, shape: shape(), two_phase_commit: false,
  });
  const reopened = await oneShot("lane2-reopen", { command: "reopen", path, shape: shape() });
  const ok = wrote.open.opened
    && wrote.generations_committed === 8
    && reopened.open.opened
    && reopened.integrity_ok === true
    && reopened.check?.ok === true
    && reopened.check.generation === 8
    && reopened.open.repair_invoked === false;
  return { path, wrote, reopened, ok };
}

// The lane-3 invariant, classified. `uninitialized` is the recorded finding:
// a cut between the initial resize and the first complete header write
// leaves a non-empty file redb refuses; acceptable only when creation itself
// was cut, so `created` must be false.
function classifyRecovery(reopen, completed, created) {
  const g = reopen.check?.generation;
  if (!reopen.open.opened) {
    const refusedAsNotADatabase = /magic number mismatch|InvalidData/.test(reopen.open.error ?? "");
    const uninitialized = refusedAsNotADatabase && !created && completed === 0;
    return { classification: uninitialized ? "uninitialized" : "unopenable", ok: uninitialized };
  }
  if (reopen.integrity_ok !== true || reopen.check?.ok !== true) return { classification: "corrupt", ok: false };
  if (g === completed) return { classification: "preceding", ok: true };
  if (g === completed + 1) return { classification: "completed", ok: true };
  return { classification: "other", ok: false };
}

async function faultSweep() {
  const path = `${prefix()}/lane3-fault.redb`;
  const commits = Number($("fault-commits").value);
  const base = { command: "fault", path, commits, shape: shape(), two_phase_commit: false };
  const baseline = await oneShot("lane3-baseline", { ...base, plan: {} });
  const counters = baseline.counters;
  const plans = [];
  for (let k = 1; k <= counters.writes; k += 1) {
    for (const torn of [0, 2 ** 31, 2 ** 32 - 1]) plans.push({ name: `cut_at_write#${k}/torn=${torn}`, plan: { cut_at_write: k, torn_bytes: torn } });
  }
  for (let k = 1; k <= counters.set_lens; k += 1) plans.push({ name: `cut_at_set_len#${k}`, plan: { cut_at_set_len: k } });
  for (let k = 1; k <= counters.writes; k += Math.max(1, Math.floor(counters.writes / 12))) {
    plans.push({ name: `short_write#${k}`, plan: { short_write_at: k } });
    plans.push({ name: `fail_write#${k}`, plan: { fail_write_at: k } });
  }
  for (let k = 1; k <= counters.syncs; k += 1) plans.push({ name: `fail_sync#${k}`, plan: { fail_sync_at: k } });
  for (let k = 1; k <= counters.set_lens; k += 1) plans.push({ name: `fail_set_len#${k}`, plan: { fail_set_len_at: k } });
  for (let q = 4096; q < baseline.reopen.file_len; q += 8192) plans.push({ name: `quota#${q}`, plan: { quota_bytes: q } });

  const rows = [];
  const failures = [];
  let index = 0;
  for (const { name, plan } of plans) {
    index += 1;
    setState("faulting", `lane 3 fault ${index}/${plans.length}: ${name}`);
    const report = await oneShot(`lane3-fault-${index}`, { ...base, plan });
    const { classification, ok } = classifyRecovery(report.reopen, report.commits_completed, report.open.opened);
    const row = {
      name,
      plan,
      created: report.open.opened,
      classification,
      commits_completed: report.commits_completed,
      failed_generation: report.failed_generation,
      error: report.error,
      further_commit_refused: report.further_commit_refused,
      injected: report.counters.injected,
      reopened_generation: report.reopen.check?.generation ?? null,
      repair_invoked: report.reopen.open.repair_invoked,
      integrity_ok: report.reopen.integrity_ok,
      invariant_ok: report.reopen.check?.ok ?? false,
      ok,
    };
    rows.push(row);
    if (!ok) failures.push({ ...row, reopen: report.reopen });
  }
  return {
    path,
    baseline_counters: counters,
    trials: rows.length,
    ok_count: rows.filter((r) => r.ok).length,
    reopened_at_preceding: rows.filter((r) => r.classification === "preceding").length,
    reopened_at_completed: rows.filter((r) => r.classification === "completed").length,
    uninitialized: rows.filter((r) => r.classification === "uninitialized").map((r) => r.name),
    failures,
    rows,
    ok: failures.length === 0,
  };
}

async function terminationTrials(onTrial = null) {
  const path = `${prefix()}/lane3-term.redb`;
  const trials = Number($("termination-trials").value);
  const bound = Number($("kill-delay-bound").value);
  const twoPhase = $("two-phase").checked;
  const rows = [];
  const failures = [];
  let lastCommitted = 0;
  {
    for (let trial = 1; trial <= trials; trial += 1) {
      // First half: the worker never yields, so the browser kills it by force
      // ~2 s later at a random point. Second half: it yields between commits,
      // so terminate() is honored promptly between commits.
      const yieldBetween = trial > Math.floor(trials / 2);
      setState("terminating", `lane 3 termination trial ${trial}/${trials} (${yieldBetween ? "yielding" : "forced"})`);
      const target = lastCommitted + 1 + Math.floor(Math.random() * 10);
      let terminateCalledAt = null;
      let committedAtCall = lastCommitted;
      let ioBefore = null;
      let worker = null;
      try {
        // The previous trial's killed worker may still hold the handle.
        const churnOpen = await untilHandleReleased(`lane3-term-${trial}`, (attempt) => {
          worker = new ProbeWorker(`lane3-term-${trial}-${attempt}`);
          return worker.call({
            command: "churn", path, reset: trial === 1, commits: 2000, shape: shape(), two_phase_commit: twoPhase,
            yield_between_commits: yieldBetween,
          }, {
            onProgress: (message) => {
              if (message.phase === "committing" && message.generation >= target && terminateCalledAt === null) {
                terminateCalledAt = message.generation;
                committedAtCall = message.generation - 1;
                ioBefore = message.io;
                busyWait(Math.random() * bound);
                worker.terminate();
              }
            },
          }).catch((error) => { worker.terminate(); throw error; });
        });
        const outcome = churnOpen.report;
        worker?.terminate();
        if (!outcome.terminated) {
          rows.push({ trial, note: "worker finished before the kill landed", ok: true });
          continue;
        }
        const terminateCalledMs = performance.now();
        const reopenAttempt = await untilHandleReleased(`lane3-term-${trial}-reopen`, (attempt) =>
          oneShot(`lane3-term-${trial}-reopen-${attempt}`, { command: "reopen", path, shape: shape() }));
        // What the dead worker last recorded in its OPFS side file.
        const progress = await oneShot(`lane3-term-${trial}-progress`, { command: "progress", path });
        const lastCommittedSeen = Math.max(committedAtCall, progress.committed ?? 0);
        const lastCommittingSeen = progress.committing ?? 0;
        const reopen = reopenAttempt.report;
        const g = reopen.check?.generation ?? null;
        const classification = g === null ? "unopenable"
          : g < lastCommittedSeen ? "regression"
          : g === lastCommittedSeen ? "preceding"
          : g === lastCommittedSeen + 1 ? "completed"
          : "later";
        const ok = reopen.open.opened && reopen.integrity_ok === true && reopen.check?.ok === true && g >= lastCommittedSeen;
        const row = {
          trial,
          mode: yieldBetween ? "yielding" : "forced",
          terminate_called_while_committing: terminateCalledAt,
          committed_at_terminate_call: committedAtCall,
          last_committed_reported: lastCommittedSeen,
          last_committing_reported: lastCommittingSeen,
          generations_after_terminate_call: g === null ? null : g - committedAtCall,
          kill_landed_inside_commit: lastCommittingSeen > lastCommittedSeen && g === lastCommittedSeen,
          io_before_terminate_call: ioBefore,
          churn_open_release_wait_ms: churnOpen.release_wait_ms,
          churn_open_attempts: churnOpen.attempts,
          reopen_release_wait_ms: reopenAttempt.release_wait_ms,
          reopen_attempts: reopenAttempt.attempts,
          wall_ms_terminate_to_reopen: performance.now() - terminateCalledMs,
          reopened_generation: g,
          classification,
          repair_invoked: reopen.open.repair_invoked,
          integrity_ok: reopen.integrity_ok,
          invariant_ok: reopen.check?.ok ?? false,
          open_ms: reopen.open.open_ms,
          file_len: reopen.file_len,
          ok,
        };
        rows.push(row);
        if (!ok) failures.push({ ...row, reopen });
        lastCommitted = g ?? lastCommittedSeen;
      } catch (error) {
        const row = { trial, classification: "harness_error", error: String(error), ok: false };
        rows.push(row);
        failures.push(row);
      }
      onTrial?.(summarizeTermination(path, twoPhase, bound, rows, failures, true));
    }
  }
  return summarizeTermination(path, twoPhase, bound, rows, failures, false);
}

function summarizeTermination(path, twoPhase, bound, rows, failures, inProgress) {
  const stats = (values) => {
    const sorted = values.filter((v) => typeof v === "number").sort((a, b) => a - b);
    return sorted.length ? { n: sorted.length, min: sorted[0], median: sorted[Math.floor(sorted.length / 2)], max: sorted.at(-1) } : null;
  };
  const byMode = (mode) => {
    const subset = rows.filter((r) => r.mode === mode);
    const count = (c) => subset.filter((r) => r.classification === c).length;
    return {
      trials: subset.length,
      outcomes: { preceding: count("preceding"), completed: count("completed"), later: count("later"), regression: count("regression"), unopenable: count("unopenable"), harness_error: count("harness_error") },
      kills_inside_a_commit: subset.filter((r) => r.kill_landed_inside_commit).length,
      generations_after_terminate_call: stats(subset.map((r) => r.generations_after_terminate_call)),
      handle_release_wait_ms: stats(subset.map((r) => r.reopen_release_wait_ms)),
      reopen_open_ms: stats(subset.map((r) => r.open_ms)),
      repairs_invoked: subset.filter((r) => r.repair_invoked).length,
    };
  };
  return {
    path,
    in_progress: inProgress,
    two_phase_commit: twoPhase,
    kill_delay_bound_ms: bound,
    trials: rows.length,
    forced: byMode("forced"),
    yielding: byMode("yielding"),
    failures,
    rows,
    ok: failures.length === 0,
  };
}

async function lane3() {
  const faults = await faultSweep();
  stashLane(3, { faults, termination: null, ok: faults.ok, partial: true });
  const termination = await terminationTrials((partial) => stashLane(3, { faults, termination: partial, ok: faults.ok && partial.ok, partial: true }));
  return { faults, termination, ok: faults.ok && termination.ok };
}

// A prior record's failure fields must not ride along into a fresh sub-run.
function previousLane3() {
  const { failed, error, partial, ...previous } = receipt.lanes.lane3 ?? {};
  return previous;
}

async function lane3Faults() {
  const previous = previousLane3();
  const faults = await faultSweep();
  return { ...previous, faults, ok: faults.ok && (previous.termination?.ok ?? true) };
}

async function lane3Termination() {
  const previous = previousLane3();
  const termination = await terminationTrials((partial) => stashLane(3, { ...previous, termination: partial, partial: true }));
  return { ...previous, termination, ok: (previous.faults?.ok ?? true) && termination.ok };
}

async function pollOpen(label, path, timeoutMs) {
  const started = performance.now();
  let attempts = 0;
  let last = null;
  while (performance.now() - started < timeoutMs) {
    attempts += 1;
    last = await oneShot(`${label}-${attempts}`, { command: "try_open", path });
    if (last.open.opened) return { opened: true, attempts, ms: performance.now() - started, report: last };
    await yieldTick();
  }
  return { opened: false, attempts, ms: performance.now() - started, report: last };
}

async function holdUntil(label, path, holdMs, reset) {
  const worker = new ProbeWorker(label);
  const holding = new Promise((resolve) => {
    worker.call({ command: "hold", path, reset, hold_ms: holdMs, shape: shape() }, {
      onState: (message) => { if (message.state === "holding") resolve(); },
    }).then((value) => { worker.finished = value; }).catch((error) => { worker.failure = String(error); });
  });
  await holding;
  return worker;
}

async function lane4SameTab() {
  const path = `${prefix()}/lane4.redb`;
  const holder = await holdUntil("lane4-holder", path, 60_000, true);
  const refused = await oneShot("lane4-second", { command: "try_open", path });
  const killedAt = performance.now();
  holder.terminate();
  const afterKill = await pollOpen("lane4-after-kill", path, 10_000);
  const clean = await holdUntil("lane4-clean-holder", path, 300, false);
  await delay(400);
  const afterClean = await oneShot("lane4-after-clean", { command: "try_open", path });
  clean.terminate();
  return {
    path,
    second_writer: {
      refused: refused.open.opened === false,
      dom_exception: refused.dom_exception,
      error_kind: refused.open.error_kind,
      error: refused.open.error,
    },
    release_after_kill: { ...afterKill, wall_ms_since_kill: performance.now() - killedAt },
    release_after_clean_close: { opened: afterClean.open.opened, repair_invoked: afterClean.open.repair_invoked },
    ok: refused.open.opened === false
      && refused.dom_exception === "NoModificationAllowedError"
      && afterKill.opened
      && afterClean.open.opened
      && afterClean.open.repair_invoked === false,
  };
}

// A second tab is coordinated over a same-origin BroadcastChannel. The page
// never calls `window.open` (in a single-pane in-app browser it navigates the
// current context instead of opening a window, which destroys the harness);
// the holder tab is opened by the operator or a real browser's own UI. The
// holder URL is `?role=holder&path=…`. The test waits for the holder to
// announce, and if none appears it records `no_holder` rather than forcing a
// window open.
async function lane4TwoTabs() {
  const path = `${prefix()}/lane4-tabs.redb`;
  if (!("BroadcastChannel" in window)) return { state: "unsupported", reason: "BroadcastChannel unavailable" };
  const channel = new BroadcastChannel(CHANNEL_NAME);
  const holderUrl = (() => { const u = new URL(location.href); u.searchParams.set("role", "holder"); u.searchParams.set("path", path); return u.toString(); })();
  const waitFor = (type, timeoutMs) => new Promise((resolve) => {
    const timer = setTimeout(() => resolve(null), timeoutMs);
    const onMessage = (event) => {
      if (event.data?.type === type) { clearTimeout(timer); channel.removeEventListener("message", onMessage); resolve(event.data); }
    };
    channel.addEventListener("message", onMessage);
  });
  // Announce that a holder is wanted, then wait for one to appear.
  channel.postMessage({ type: "holder-wanted", path });
  window.__munimentHolderUrl = holderUrl;
  setState("waiting_holder", `open a holder tab: ${holderUrl}`);
  const holding = await waitFor("holding", 30_000);
  if (!holding) { channel.close(); return { state: "no_holder", reason: "no holder tab announced within 30 s", holder_url: holderUrl }; }
  try {
    const locksBefore = navigator.locks?.query ? (await navigator.locks.query()).held.map((l) => l.name) : null;
    const refused = await oneShot("lane4-tabs-second", { command: "try_open", path });
    // Controlled takeover: ask, and the holder releases its lock and handle.
    const releasedPromise = waitFor("released", 10_000);
    channel.postMessage({ type: "release" });
    const released = await releasedPromise;
    const takeover = await pollOpen("lane4-tabs-takeover", path, 15_000);
    const lockAfterRelease = await (navigator.locks?.request
      ? navigator.locks.request(`muniment-opfs:${path}`, { ifAvailable: true }, async (lock) => lock !== null)
      : Promise.resolve(null));
    // Abrupt path: the holder holds again, then closes its own tab.
    const holdingAgain = waitFor("holding", 15_000);
    channel.postMessage({ type: "hold-again" });
    const again = await holdingAgain;
    const refusedAgain = again ? await oneShot("lane4-tabs-second-again", { command: "try_open", path }) : null;
    const closedAt = performance.now();
    channel.postMessage({ type: "close-tab" });
    const afterTabClose = await pollOpen("lane4-tabs-after-close", path, 20_000);
    return {
      state: "ran",
      path,
      holder: holding,
      locks_held_before: locksBefore,
      second_writer_refused: refused.open.opened === false,
      second_writer_dom_exception: refused.dom_exception,
      controlled_release: released,
      takeover_after_release: takeover,
      lock_available_after_release: lockAfterRelease,
      second_writer_refused_again: refusedAgain ? refusedAgain.open.opened === false : null,
      release_after_tab_close: { ...afterTabClose, wall_ms_since_close: performance.now() - closedAt },
      ok: refused.open.opened === false
        && refused.dom_exception === "NoModificationAllowedError"
        && takeover.opened
        && (refusedAgain === null || refusedAgain.open.opened === false)
        && afterTabClose.opened,
    };
  } finally {
    channel.close();
  }
}

async function lane4Reload(partial) {
  const path = `${prefix()}/lane4-reload.redb`;
  await holdUntil("lane4-reload-holder", path, 120_000, true);
  sessionStorage.setItem(RESUME_KEY, JSON.stringify({ phase: "after_reload", path, at: Date.now(), partial }));
  saveReceipt();
  location.reload();
  await new Promise(() => {});
}

async function lane4AfterReload(marker) {
  const afterReload = await pollOpen("lane4-after-reload", marker.path, 15_000);
  return {
    path: marker.path,
    wall_ms_since_reload_request: Date.now() - marker.at,
    ...afterReload,
    ok: afterReload.opened,
  };
}

async function lane4(resumeMarker = null) {
  if (resumeMarker?.phase === "after_reload") {
    const reload = await lane4AfterReload(resumeMarker);
    const partial = resumeMarker.partial;
    return { ...partial, reload, ok: partial.same_tab.ok && (partial.two_tabs.ok ?? true) && reload.ok };
  }
  const same_tab = await lane4SameTab();
  const two_tabs = await lane4TwoTabs();
  const partial = {
    same_tab,
    two_tabs,
    visibility: { state: document.visibilityState, note: "background suspension is not forced by the harness; reload is" },
  };
  await lane4Reload(partial);
}

function bytesToBase64(bytes) {
  let binary = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) binary += String.fromCharCode.apply(null, bytes.subarray(i, i + chunk));
  return btoa(binary);
}

async function lane5Portability() {
  const path = `${prefix()}/portability.redb`;
  const base = new URL("../fixtures/", location.href);
  const manifestResponse = await fetch(new URL("portability.json", base));
  if (!manifestResponse.ok) return { state: "missing_fixture", reason: `fixtures/portability.json: HTTP ${manifestResponse.status}` };
  const manifest = await manifestResponse.json();
  const imported = await oneShot("lane5-import", { command: "import", path, url: new URL("portability.redb", base).toString() });
  const reopened = await oneShot("lane5-reopen", { command: "reopen", path, shape: manifest.shape });
  const digest = await oneShot("lane5-digest", { command: "digest", path });
  const extended = await oneShot("lane5-extend", { command: "churn", path, reset: false, commits: 3, shape: manifest.shape, two_phase_commit: false });
  const exportedReport = await oneShot("lane5-export", { command: "export", path });
  exported = exportedReport.bytes;
  const native_to_browser_ok = reopened.open.opened
    && reopened.integrity_ok === true
    && reopened.check?.ok === true
    && reopened.check.generation === manifest.generation
    && digest.digest === manifest.muniment_digest
    && digest.keys === manifest.muniment_keys;
  return {
    state: "ran",
    path,
    manifest,
    imported: { bytes: imported.bytes, blake3: imported.blake3, ms: imported.ms },
    reopened,
    digest: { digest: digest.digest, keys: digest.keys, matches_manifest: digest.digest === manifest.muniment_digest },
    extended_to_generation: extended.last_generation,
    exported: { bytes: exportedReport.bytes.length, blake3: exportedReport.blake3 },
    native_to_browser_ok,
    // The page cannot run a native binary. `run-browser.mjs` fills this in by
    // exporting the bytes and running `fixture verify`; a receipt produced by
    // hand without that step leaves it null rather than claiming a pass.
    browser_to_native: null,
    ok: native_to_browser_ok,
  };
}

function deleteIndexedDb(name) {
  return new Promise((resolve) => {
    const request = indexedDB.deleteDatabase(name);
    request.onsuccess = () => resolve("deleted");
    request.onerror = () => resolve(`error: ${request.error?.message}`);
    request.onblocked = () => resolve("blocked");
  });
}

// Phases are grouped by what they cost, because the totals hide the finding:
// redb pays per durable commit and wins on indexed reads.
const WRITE_PHASES = new Set(["put", "overwrite", "put_shuffled", "apply", "apply_shuffled"]);
const READ_PHASES = new Set(["get", "scan_full", "scan_windows", "verify", "get_verify", "digest"]);

const median = (values) => {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted.length ? sorted[Math.floor(sorted.length / 2)] : null;
};

async function lane5Bench(repeats = 3) {
  const workloads = ["small_slots", "ordered_log", "log_batched", "atomic_batches", "large_blobs"];
  // `indexed_db` is the shipping adapter (full key fetch + filter in Rust);
  // `indexed_db_range` asks IndexedDB for the range. Both are reported,
  // because the shipping one is what muniment has today and the range one is
  // what IndexedDB can actually do.
  const backends = ["memory", "redb_opfs", "indexed_db", "indexed_db_range"];
  const rows = [];
  // Repeat is the OUTER loop, so each repeat sweeps every backend before any
  // backend gets its second sample. Running a backend's repeats consecutively
  // (the earlier shape) let a single load spike land entirely on one backend
  // and look like a property of that backend. This does not remove the
  // ordering confound — backend order within a repeat is still fixed — but it
  // stops one transient from owning a whole row.
  for (let run = 1; run <= repeats; run += 1) {
    for (const workload of workloads) {
      for (const backend of backends) {
        const usesIdb = backend.startsWith("indexed_db");
        const name = usesIdb ? `muniment-opfs-probe-bench-${backend}-${workload}` : `${prefix()}/bench-${workload}.redb`;
        if (usesIdb) await deleteIndexedDb(name);
        setState("bench", `lane 5 bench: ${workload} on ${backend} (run ${run}/${repeats})`);
        const storageBefore = await storageSnapshot();
        const report = await oneShot(`lane5-bench-${workload}-${backend}-${run}`, { command: "bench", backend, workload, name, reset: true });
        const storageAfter = await storageSnapshot();
        const phases = Object.fromEntries(report.phases.map((p) => [p.name, p.ms]));
        rows.push({
          workload, backend, run,
          total_ms: report.total_ms,
          phases,
          write_ms: report.phases.filter((p) => WRITE_PHASES.has(p.name)).reduce((a, p) => a + p.ms, 0),
          read_ms: report.phases.filter((p) => READ_PHASES.has(p.name)).reduce((a, p) => a + p.ms, 0),
          ops: report.outcome.ops,
          bytes: report.outcome.bytes,
          keys: report.outcome.keys,
          digest: report.outcome.digest,
          checks_ok: report.outcome.checks_ok,
          io: report.io,
          usage_delta_bytes: (storageAfter.usage_bytes ?? 0) - (storageBefore.usage_bytes ?? 0),
        });
      }
    }
  }
  const digestsAgree = workloads.every((w) => new Set(rows.filter((r) => r.workload === w).map((r) => r.digest)).size === 1);
  // Medians across repeats, split write vs read, plus every phase separately.
  const summary = workloads.map((workload) => {
    const forBackend = (backend) => {
      const subset = rows.filter((r) => r.workload === workload && r.backend === backend);
      if (!subset.length) return null;
      const phaseNames = [...new Set(subset.flatMap((r) => Object.keys(r.phases)))];
      return {
        runs: subset.length,
        total_ms: median(subset.map((r) => r.total_ms)),
        write_ms: median(subset.map((r) => r.write_ms)),
        read_ms: median(subset.map((r) => r.read_ms)),
        total_spread_ms: [Math.min(...subset.map((r) => r.total_ms)), Math.max(...subset.map((r) => r.total_ms))],
        phase_ms: Object.fromEntries(phaseNames.map((n) => [n, median(subset.map((r) => r.phases[n] ?? 0))])),
      };
    };
    const redb = forBackend("redb_opfs");
    const idb = forBackend("indexed_db");
    const idbRange = forBackend("indexed_db_range");
    const ratio = (a, b) => (a != null && b ? a / b : null);
    // Medians alone overstate a comparison when one repeat is an outlier. The
    // worst OBSERVED ratio (best case for the other backend against worst
    // case for redb) is what a claim should be able to survive.
    const span = (bk, phase) => {
      const v = rows.filter((r) => r.workload === workload && r.backend === bk).map((r) => r[phase]).sort((a, b) => a - b);
      return v.length ? { min: v[0], max: v[v.length - 1] } : null;
    };
    const envelope = (other, phase, redbFaster) => {
      const a = span("redb_opfs", phase);
      const b = span(other, phase);
      if (!a || !b) return null;
      return {
        redb_ms: a,
        other_ms: b,
        // > 1 means redb is slower, matching the median ratios.
        worst_for_redb: redbFaster ? b.min / a.max : a.max / b.min,
        best_for_redb: redbFaster ? b.max / a.min : a.min / b.max,
        // Did every single observed repeat favour the same side?
        disjoint: redbFaster ? a.max < b.min : b.max < a.min,
      };
    };
    const against = (other) => other && {
      total: ratio(redb?.total_ms, other.total_ms),
      write: ratio(redb?.write_ms, other.write_ms),
      read: ratio(redb?.read_ms, other.read_ms),
      by_phase: Object.fromEntries(
        Object.keys(redb?.phase_ms ?? {}).map((n) => [n, ratio(redb.phase_ms[n], other.phase_ms?.[n])]),
      ),
    };
    return {
      workload,
      memory: forBackend("memory"),
      redb_opfs: redb,
      indexed_db: idb,
      indexed_db_range: idbRange,
      // > 1 means redb is slower; < 1 means redb is faster.
      redb_over_indexeddb: against(idb),
      // The fair read comparison: IndexedDB asked for the range it wanted.
      redb_over_indexeddb_range: against(idbRange),
      // Observed envelopes, so a claim can be checked against the worst
      // sample rather than the median.
      envelopes: {
        read_vs_range: envelope("indexed_db_range", "read_ms", true),
        write_vs_range: envelope("indexed_db_range", "write_ms", false),
        // The control: identical write code in the two IndexedDB backends.
        // Compare the two backends REPEAT BY REPEAT, not median to median.
        // Medians hide how far identical code drifted within a single
        // sample: the medians can sit at 0.88-1.11 while a paired repeat is
        // 0.81. The paired span is the honest statement of run quality.
        control_write_shipping_vs_range: (() => {
          const a = span("indexed_db", "write_ms");
          const b = span("indexed_db_range", "write_ms");
          if (!a || !b) return null;
          const paired = [];
          for (let run = 1; run <= repeats; run += 1) {
            const x = rows.find((r) => r.workload === workload && r.backend === "indexed_db" && r.run === run);
            const y = rows.find((r) => r.workload === workload && r.backend === "indexed_db_range" && r.run === run);
            if (x && y && y.write_ms) paired.push(x.write_ms / y.write_ms);
          }
          paired.sort((p, q) => p - q);
          return {
            shipping_ms: a,
            range_ms: b,
            median_ratio: ratio(idb?.write_ms, idbRange?.write_ms),
            paired_ratios: paired,
            paired_span: paired.length ? { min: paired[0], max: paired[paired.length - 1] } : null,
            // How far identical code drifted, as a percentage.
            max_paired_deviation_pct: paired.length
              ? Math.round(Math.max(...paired.map((p) => Math.abs(1 - (p < 1 ? 1 / p : p)))) * 100)
              : null,
            disjoint: a.max < b.min || b.max < a.min,
          };
        })(),
      },
      // How much of the shipping adapter's read cost is the adapter, not
      // IndexedDB: > 1 means the range version is faster. Reported as an
      // envelope like every other comparison — a median alone here was the
      // one place the conservative rule was bypassed.
      indexeddb_adapter_overhead_read: ratio(idb?.read_ms, idbRange?.read_ms),
      adapter_overhead_envelope: (() => {
        const a = span("indexed_db", "read_ms");
        const b = span("indexed_db_range", "read_ms");
        if (!a || !b) return null;
        return {
          shipping_ms: a,
          range_ms: b,
          median: ratio(idb?.read_ms, idbRange?.read_ms),
          // Conservative: slowest range repeat against fastest shipping one.
          worst_for_range: a.min / b.max,
          best_for_range: a.max / b.min,
          disjoint: b.max < a.min,
        };
      })(),
    };
  });
  return {
    repeats,
    rows,
    summary,
    digests_agree: digestsAgree,
    all_checks_ok: rows.every((r) => r.checks_ok),
    ok: digestsAgree && rows.every((r) => r.checks_ok),
  };
}

async function lane5() {
  const portability = await lane5Portability();
  setState("contract", "lane 5: ASCII key contract on the range backend");
  const contractDb = "muniment-opfs-probe-ascii-contract";
  await deleteIndexedDb(contractDb);
  const contract = await oneShot("lane5-ascii-contract", { command: "ascii_contract", name: contractDb });
  const bench = await lane5Bench();
  return {
    portability,
    ascii_contract: contract,
    bench,
    ok: (portability.ok ?? false) && contract.ok && bench.ok,
  };
}

// ── lane 6: staged creation (the §5.4 remedy, crash-tested) ──────────────

async function lane6() {
  const path = `${prefix()}/lane6-staged.redb`;
  const commits = 4;
  const base = { command: "staged_create", path, commits, shape: shape(), two_phase_commit: false };

  // A clean staged creation first, to learn the ACTUAL storage-call counts
  // and whether this browser promotes atomically. An earlier version guessed
  // the write count from `staging_len`, which is zero after a successful
  // promotion, so it silently substituted 40 and the sweep never reached the
  // later writes. The worker now reports its own counters.
  setState("staging", "lane 6 baseline staged creation");
  const baseline = await oneShot("lane6-baseline", { ...base, plan: {} });
  if (!baseline.ok) return { path, baseline, ok: false, reason: "the unfaulted staged creation failed" };
  const counters = baseline.counters ?? {};
  const writes = counters.writes ?? 0;
  const setLens = counters.set_lens ?? 0;
  const syncs = counters.syncs ?? 0;
  if (!writes) return { path, baseline, ok: false, reason: "the baseline reported no write count to sweep" };

  // Then cut the creation everywhere it can be cut, and after every cut the
  // FINAL path must be absent (never an unopenable stub) or sound.
  const plans = [];
  for (let k = 1; k <= writes; k += 1) {
    plans.push({ name: `cut_at_write#${k}`, plan: { cut_at_write: k, torn_bytes: 2 ** 31 } });
  }
  for (let k = 1; k <= setLens; k += 1) plans.push({ name: `cut_at_set_len#${k}`, plan: { cut_at_set_len: k } });
  for (let k = 1; k <= syncs; k += 1) plans.push({ name: `fail_sync#${k}`, plan: { fail_sync_at: k } });
  plans.push({ name: "cut_before_promote", plan: {}, cutBeforePromote: true });

  const rows = [];
  const failures = [];
  let index = 0;
  for (const { name, plan, cutBeforePromote } of plans) {
    index += 1;
    setState("staging", `lane 6 staged-creation trial ${index}/${plans.length}: ${name}`);
    const report = await oneShot(`lane6-${index}`, { ...base, plan, cut_before_promote: !!cutBeforePromote });
    const row = {
      name,
      staged_ok: report.staged_ok,
      promoted: report.promoted,
      atomic_move: report.atomic_move,
      final_exists: report.final_exists,
      final_sound: report.reopen ? (report.reopen.integrity_ok === true && report.reopen.check?.ok === true) : null,
      final_generation: report.reopen?.check?.generation ?? null,
      staging_left: report.staging_left,
      error: report.error,
      ok: report.ok,
    };
    rows.push(row);
    if (!row.ok) failures.push({ ...row, reopen: report.reopen });
  }
  // The promotion window itself: kill the worker while move() is in flight.
  // `cut_before_promote` only covers the moment *before* the rename, so on
  // its own it says nothing about whether the rename is crash-atomic.
  const promotionTrials = await promotionKillTrials(path, base, 12);

  // Clean up the staging debris the trials left.
  await oneShot("lane6-cleanup", { command: "remove", path: `${path}.staging` });
  await oneShot("lane6-cleanup2", { command: "remove", path });
  const allRows = [...rows, ...promotionTrials.rows];
  const allFailures = [...failures, ...promotionTrials.failures];
  return {
    path,
    atomic_move: baseline.atomic_move,
    baseline_counters: counters,
    swept: { writes, set_lens: setLens, syncs },
    baseline,
    trials: allRows.length,
    creation_trials: rows.length,
    final_absent: allRows.filter((r) => !r.final_exists).length,
    final_present_and_sound: allRows.filter((r) => r.final_exists && r.final_sound).length,
    unopenable_stubs: allRows.filter((r) => r.final_exists && r.final_sound === false).length,
    staging_left_behind: allRows.filter((r) => r.staging_left).length,
    promotion: promotionTrials.summary,
    failures: allFailures,
    rows: allRows,
    ok: allFailures.length === 0,
  };
}

// Kill the worker at the PROMOTION BOUNDARY: it announces `promoting` and
// yields, the page terminates it after a staggered delay, and both names are
// then inspected from a fresh worker.
//
// What this does and does not establish. The harness cannot prove the kill
// landed *inside* `move()`. `terminated: true` only means the page killed the
// worker before the whole command returned, and that command also does the
// post-move existence and reopen checks — so a kill could have landed after
// the rename completed. These are therefore "promotion-boundary kills": the
// window they sample straddles the rename rather than being confined to it.
//
// A crash-atomic rename leaves exactly one of:
//   - staging present, final absent   (rename had not taken effect)
//   - staging absent, final sound     (rename took effect)
// Anything else — final present but unopenable, or both names gone — would
// mean the promotion is not atomic. Observing only the two good states across
// many trials is evidence for atomicity, not proof of it.
//
// Inspection waits for both names to stabilize first: an immediate read can
// race a browser operation still settling, which would report a transient
// state as a torn one (or vice versa).
async function promotionKillTrials(path, base, attempts) {
  const staging = `${path}.staging`;
  const rows = [];
  const failures = [];
  for (let trial = 1; trial <= attempts; trial += 1) {
    setState("staging", `lane 6 promotion-kill trial ${trial}/${attempts}`);
    await oneShot(`lane6-pk-${trial}-clean`, { command: "remove", path });
    await oneShot(`lane6-pk-${trial}-clean2`, { command: "remove", path: staging });
    const worker = new ProbeWorker(`lane6-pk-${trial}`);
    let killed = false;
    // Spread the kill across the window: some trials fire the instant the
    // worker announces, others a fraction of a millisecond later.
    const jitter = (trial - 1) * 0.35;
    let outcome;
    try {
      outcome = await worker.call(
        { ...base, plan: {}, announce_promote: true },
        {
          onState: (message) => {
            if (message.state === "promoting" && !killed) {
              killed = true;
              busyWait(jitter);
              worker.terminate();
            }
          },
        },
      );
    } catch (error) {
      outcome = { error: String(error) };
    }
    worker.terminate();
    // Let both names settle before believing either. Two consecutive
    // identical observations, or a bounded number of attempts.
    const observe = async (tag) => {
      const f = await oneShot(`lane6-pk-${trial}-${tag}-final`, { command: "exists", path });
      const s = await oneShot(`lane6-pk-${trial}-${tag}-staging`, { command: "exists", path: staging });
      return { f, s, key: `${f.exists}:${f.len}:${s.exists}:${s.len}` };
    };
    let previous = await observe("s0");
    let settled = false;
    let settleRounds = 0;
    for (let round = 1; round <= 8; round += 1) {
      await delay(60);
      const next = await observe(`s${round}`);
      settleRounds = round;
      if (next.key === previous.key) { previous = next; settled = true; break; }
      previous = next;
    }
    const finalState = previous.f;
    const stagingState = previous.s;
    let reopen = null;
    if (finalState.exists) {
      reopen = await oneShot(`lane6-pk-${trial}-reopen`, { command: "reopen", path, shape: shape() });
    }
    const finalSound = reopen ? (reopen.integrity_ok === true && reopen.check?.ok === true) : null;
    // Classify by which of the two names survived.
    const classification = !finalState.exists && stagingState.exists ? "rename_not_applied"
      : finalState.exists && !stagingState.exists && finalSound ? "rename_applied"
      : finalState.exists && finalSound ? "rename_applied_staging_remains"
      : finalState.exists && finalSound === false ? "final_unopenable"
      : "both_absent";
    // A trial only counts if the names actually settled. Classifying an
    // unsettled sample would be reading a state the browser had not finished
    // producing, and calling it a pass.
    const classificationOk = classification === "rename_not_applied"
      || classification === "rename_applied"
      || classification === "rename_applied_staging_remains";
    const ok = settled && classificationOk;
    const row = {
      name: `promotion_boundary_kill#${trial}`,
      kill_jitter_ms: jitter,
      classification_ok: classificationOk,
      // The page terminated the worker before the command returned. That
      // command spans move() AND the post-move checks, so this does NOT
      // establish the kill landed inside the rename.
      terminated_before_command_returned: outcome?.terminated === true,
      worker_finished_first: outcome?.terminated !== true,
      names_stabilized: settled,
      settle_rounds: settleRounds,
      classification,
      final_exists: finalState.exists,
      final_len: finalState.len,
      final_sound: finalSound,
      final_generation: reopen?.check?.generation ?? null,
      staging_left: stagingState.exists,
      staging_len: stagingState.len,
      ok,
    };
    rows.push(row);
    if (!ok) failures.push({ ...row, reopen });
  }
  const count = (c) => rows.filter((r) => r.classification === c).length;
  return {
    rows,
    failures,
    summary: {
      kind: "promotion_boundary_kills",
      note: "the kill window straddles move(); landing inside the rename is not established",
      trials: rows.length,
      terminated_before_command_returned: rows.filter((r) => r.terminated_before_command_returned).length,
      worker_finished_first: rows.filter((r) => r.worker_finished_first).length,
      names_stabilized: rows.filter((r) => r.names_stabilized).length,
      // A trial passes only if it settled AND classified atomically.
      unsettled_and_therefore_failed: rows.filter((r) => !r.names_stabilized).length,
      rename_not_applied: count("rename_not_applied"),
      rename_applied: count("rename_applied"),
      rename_applied_staging_remains: count("rename_applied_staging_remains"),
      final_unopenable: count("final_unopenable"),
      both_absent: count("both_absent"),
      ok: failures.length === 0,
    },
  };
}

// ── receipt ──────────────────────────────────────────────────────────────

function restoreReceipt() {
  try { return JSON.parse(sessionStorage.getItem(RECEIPT_KEY)) ?? null; } catch { return null; }
}

function saveReceipt() {
  if (receipt) sessionStorage.setItem(RECEIPT_KEY, JSON.stringify(receipt));
}

function showReceipt() {
  receiptView.textContent = receipt ? JSON.stringify(receipt, null, 2) : "No run yet.";
  downloadButton.disabled = !receipt;
}

function conclude() {
  const lanes = receipt.lanes;
  const unrecoverable = [
    ...(lanes.lane3?.faults?.failures ?? []).map((f) => ({ source: "fault", name: f.name })),
    ...(lanes.lane3?.termination?.failures ?? []).map((f) => ({ source: "termination", trial: f.trial, classification: f.classification })),
  ];
  receipt.conclusions = {
    lane1_wasm_transactions: lanes.lane1?.ok ?? null,
    lane2_opfs_create_commit_close_reopen: lanes.lane2?.ok ?? null,
    lane3_fault_trials: lanes.lane3?.faults?.trials ?? null,
    lane3_fault_all_recovered: lanes.lane3?.faults?.ok ?? null,
    lane3_fault_uninitialized_creation_cuts: lanes.lane3?.faults?.uninitialized ?? null,
    lane3_termination_trials: lanes.lane3?.termination?.trials ?? null,
    lane3_termination_forced: lanes.lane3?.termination?.forced ?? null,
    lane3_termination_yielding: lanes.lane3?.termination?.yielding ?? null,
    lane3_termination_all_recovered: lanes.lane3?.termination?.ok ?? null,
    lane3_unrecoverable: unrecoverable,
    stop_condition_hit: unrecoverable.length > 0,
    lane4_second_writer_refused: lanes.lane4?.same_tab?.second_writer?.refused ?? null,
    lane4_refusal_name: lanes.lane4?.same_tab?.second_writer?.dom_exception ?? null,
    lane4_release_after_kill_ms: lanes.lane4?.same_tab?.release_after_kill?.ms ?? null,
    lane4_two_tabs: lanes.lane4?.two_tabs?.state ?? null,
    lane4_two_tabs_ok: lanes.lane4?.two_tabs?.ok ?? null,
    lane4_reload_reopen_ms: lanes.lane4?.reload?.ms ?? null,
    lane5_native_to_browser_ok: lanes.lane5?.portability?.native_to_browser_ok ?? null,
    lane5_digests_agree: lanes.lane5?.bench?.digests_agree ?? null,
    // Split, because the totals hide the finding: redb pays per durable
    // commit and wins on indexed reads. > 1 = redb slower, < 1 = redb faster.
    lane5_redb_over_indexeddb: lanes.lane5?.bench?.summary?.map((s) => ({
      workload: s.workload,
      // vs the shipping adapter (full key fetch + filter in Rust)
      vs_shipping: { total: s.redb_over_indexeddb?.total, write: s.redb_over_indexeddb?.write, read: s.redb_over_indexeddb?.read },
      // vs IndexedDB asked for the range it wanted: the fair read baseline
      vs_range: { total: s.redb_over_indexeddb_range?.total, write: s.redb_over_indexeddb_range?.write, read: s.redb_over_indexeddb_range?.read },
      shipping_adapter_read_overhead: s.indexeddb_adapter_overhead_read,
    })) ?? null,
    lane5_bench_repeats: lanes.lane5?.bench?.repeats ?? null,
    lane6_trials: lanes.lane6?.trials ?? null,
    lane6_creation_sweep: lanes.lane6?.swept ?? null,
    lane6_atomic_move: lanes.lane6?.atomic_move ?? null,
    lane6_unopenable_stubs: lanes.lane6?.unopenable_stubs ?? null,
    lane6_promotion_kill: lanes.lane6?.promotion ?? null,
    lane6_ok: lanes.lane6?.ok ?? null,
    browser_scope: {
      user_agent: receipt.environment?.user_agent ?? null,
      note: "one host per receipt; Firefox/Safari/WKWebView are separate receipts",
    },
  };
}

async function startReceipt() {
  receipt = {
    schema: "muniment.opfs-probe.receipt/v1",
    generated_at: new Date().toISOString(),
    configuration: {
      opfs_prefix: prefix(),
      shape: shape(),
      termination_trials: Number($("termination-trials").value),
      kill_delay_bound_ms: Number($("kill-delay-bound").value),
      fault_commits: Number($("fault-commits").value),
      two_phase_commit: $("two-phase").checked,
    },
    environment: await environment(),
    storage: { before: await storageSnapshot() },
    lanes: {},
  };
  if ($("request-persistence").checked && navigator.storage?.persist) {
    // Headless Firefox never settles this: it awaits a permission prompt.
    const granted = await withTimeout(navigator.storage.persist(), 5_000);
    receipt.configuration.persistence_request = granted === TIMED_OUT
      ? { requested: true, granted: null, note: "storage.persist() did not settle in 5 s (permission prompt with no UI)" }
      : { requested: true, granted };
  }
  saveReceipt();
}

const LANES = { 1: lane1, 2: lane2, 3: lane3, "3a": lane3Faults, "3b": lane3Termination, 4: lane4, 5: lane5, 6: lane6 };
const LAST_LANE = 6;

async function runLane(n, resumeMarker = null) {
  if (!receipt) await startReceipt();
  stateLog.replaceChildren();
  setState("running", `lane ${n}`);
  try {
    const result = await LANES[n](resumeMarker);
    receipt.lanes[`lane${String(n).replace(/[ab]$/, "")}`] = result;
    receipt.storage.after = await storageSnapshot();
    conclude();
    saveReceipt();
    showReceipt();
    setState(result.ok ? "complete" : "stop", `lane ${n}: ${result.ok ? "done" : "a done-condition failed; see the receipt"}`);
    return result;
  } catch (error) {
    const key = `lane${String(n).replace(/[ab]$/, "")}`;
    receipt.lanes[key] = { ...(receipt.lanes[key] ?? {}), failed: true, error: String(error) };
    saveReceipt();
    showReceipt();
    setState("failed", `lane ${n}: ${error}`);
    throw error;
  }
}

async function runAll(fromLane = 1, resumeMarker = null) {
  if (fromLane === 1) {
    receipt = null;
    sessionStorage.removeItem(RECEIPT_KEY);
    await startReceipt();
  }
  for (let n = fromLane; n <= LAST_LANE; n += 1) {
    if (n === 4 && !resumeMarker) {
      sessionStorage.setItem(`${RESUME_KEY}.all`, "true");
    }
    await runLane(n, n === 4 ? resumeMarker : null);
  }
  sessionStorage.removeItem(`${RESUME_KEY}.all`);
  setState(receipt.conclusions.stop_condition_hit ? "stop" : "complete",
    receipt.conclusions.stop_condition_hit ? "STOP CONDITION: an unrecoverable database was observed" : "Every lane ran; read the conclusions.");
  return receipt;
}

// ── holder-tab mode (lane 4, two tabs) ───────────────────────────────────

async function holderMode(path) {
  const channel = new BroadcastChannel(CHANNEL_NAME);
  setState("holder", `holder tab for ${path}`);
  let holder = null;
  let releaseLock = null;
  let announcement = null;
  const announce = () => { if (announcement) channel.postMessage(announcement); };
  const acquire = async (reset) => {
    holder = await holdUntil("holder-tab", path, 600_000, reset);
    const lockName = `muniment-opfs:${path}`;
    const lockHeld = new Promise((resolve) => {
      if (!navigator.locks?.request) return resolve(false);
      navigator.locks.request(lockName, { ifAvailable: true }, (lock) => {
        resolve(lock !== null);
        return new Promise((release) => { releaseLock = release; });
      });
    });
    const lock = await lockHeld;
    announcement = { type: "holding", path, lock_name: lockName, lock_acquired: lock, at: Date.now() };
    announce();
  };
  channel.addEventListener("message", async (event) => {
    const type = event.data?.type;
    // Re-announce so a main tab that started listening late still hears us.
    if (type === "holder-wanted" && event.data.path === path) announce();
    if (type === "release") {
      announcement = null;
      holder?.terminate();
      releaseLock?.();
      releaseLock = null;
      channel.postMessage({ type: "released", at: Date.now() });
    }
    if (type === "hold-again") await acquire(false);
    if (type === "close-tab") { holder?.terminate(); releaseLock?.(); setState("closing", "holder closing"); window.close(); }
  });
  // The holder resets the file so a fresh run has a clean database.
  await acquire(true);
}

// ── wiring ───────────────────────────────────────────────────────────────

const params = new URLSearchParams(location.search);
if (params.get("role") === "holder") {
  holderMode(params.get("path"));
} else {
  showReceipt();
  const marker = JSON.parse(sessionStorage.getItem(RESUME_KEY) ?? "null");
  if (marker?.phase === "after_reload") {
    sessionStorage.removeItem(RESUME_KEY);
    const all = sessionStorage.getItem(`${RESUME_KEY}.all`) === "true";
    (all ? runAll(4, marker) : runLane(4, marker)).catch(() => {});
  }
}

$("probe-controls").addEventListener("submit", async (event) => {
  event.preventDefault();
  $("run-all").disabled = true;
  try { await runAll(); } catch (error) { setState("failed", String(error)); } finally { $("run-all").disabled = false; }
});

for (const button of document.querySelectorAll("button[data-lane]")) {
  button.addEventListener("click", async () => {
    button.disabled = true;
    try { await runLane(Number(button.dataset.lane)); } catch { /* shown in state */ } finally { button.disabled = false; }
  });
}

downloadButton.addEventListener("click", () => {
  if (!receipt) return;
  const blob = new Blob([JSON.stringify(receipt, null, 2)], { type: "application/json" });
  const link = document.createElement("a");
  link.href = URL.createObjectURL(blob);
  link.download = "muniment-opfs-probe.json";
  link.click();
  URL.revokeObjectURL(link.href);
});

const SINK = "http://127.0.0.1:8734";

async function postReceipt(name) {
  const response = await fetch(`${SINK}/receipt?name=${encodeURIComponent(name)}`, {
    method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(receipt),
  });
  return response.json();
}

async function postExport(name) {
  if (!exported) throw new Error("nothing exported yet; run lane 5 first");
  const response = await fetch(`${SINK}/file?name=${encodeURIComponent(name)}`, {
    method: "POST", headers: { "Content-Type": "application/octet-stream" }, body: exported,
  });
  return response.json();
}

window.munimentOpfsProbe = {
  runLane,
  runAll,
  receipt: () => structuredClone(receipt),
  exportedBase64: () => (exported ? bytesToBase64(exported) : null),
  postReceipt,
  postExport,
};
