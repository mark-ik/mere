# Browser content worker scaffold

This folder is the small browser-side seam around Meerkat's transferable content
worker path.

`content_worker_host.js` installs the main-thread worker factory expected by
`set_content_worker_factory(...)`.

`content_worker_entry.js` is the worker-side bootstrap: hand it a loader that
initializes the wasm module and returns its exports, and it calls
`install_content_worker()` inside the worker entrypoint.

Example host entry:

```js
import init, * as meerkat from "./pkg/meerkat.js";
import { installContentWorkerFactory } from "./content_worker_host.js";

await init();
installContentWorkerFactory(meerkat.set_content_worker_factory);
```

Example worker entry:

```js
import init, * as meerkat from "./pkg/meerkat.js";
import { bootContentWorker } from "./content_worker_entry.js";

await bootContentWorker(async () => {
  await init();
  return meerkat;
});
```
