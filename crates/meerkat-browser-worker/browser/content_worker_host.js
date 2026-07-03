export function installContentWorkerFactory(
  setContentWorkerFactory,
  workerEntryUrl = new URL("./content_worker_entry.js", import.meta.url),
) {
  setContentWorkerFactory(() => new Worker(workerEntryUrl, { type: "module" }));
}
