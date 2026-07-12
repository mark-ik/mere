# armillary

A host-neutral actor-kernel runtime. A single-threaded *host kernel* owns all
canonical state (the document model, the GPU device, the window); a constellation
of *actors* runs off-thread and talks to it only by message. The name is the
instrument: an armillary sphere is a frame of rings around a central point, the
kernel at the center and the actor rings around it.

The runtime spine, and nothing host-specific:

- **A typed boundary** (`KernelThread`) that is `!Send` by construction, so the
  compiler refuses to move kernel authority onto an actor thread. Boundary drift
  is a compile error, not a code-review catch. GPUI's discipline made structural.
- **An actor harness** (`spawn` / `spawn_on`) that runs a subsystem on its own
  thread (or a pooled worker), driven by `Send` commands and drained of `Send`
  updates. The actor's internals may be `!Send` (a JS engine, a DOM, a layout
  engine) because they are built *on* the actor thread, never moved across.
- **A growable, reusing worker pool** so long-lived actors bound the OS-thread
  count (and any leaked per-thread state) to peak concurrency, not total spawns.
- **Generation counters** for backpressure, so a result from a superseded state
  is dropped rather than applied.
- **Request correlation** (`RequestId`, `RequestIds`, `Correlated<T>`) so a host
  can pair actor progress or completion updates with the command that caused
  them without Armillary defining the app's outcome vocabulary.

```rust
use std::sync::Arc;
use armillary::{spawn, Wake};

let wake: Wake = Arc::new(|| { /* poke the kernel's event loop */ });
let (handle, updates) = spawn::<u32, u32, _>(wake, |commands, out| {
    let mut total = 0;                 // actor-thread-local; never crosses the boundary
    while let Ok(n) = commands.recv() {
        total += n;
        out.emit(total);
    }
});
handle.command(2);
handle.command(3);
handle.join();
assert_eq!(updates.iter().collect::<Vec<_>>(), vec![2, 5]);
```

Promoted from mere's `crates/armillary`, host-neutral from the start (its only
dependency is `tracing`). The concrete `Command` / `Update` taxonomy and the
kernel inbox belong to the host; this crate is generic over them. See
[`design_docs/`](design_docs/) for the founding proposal.

License: dual MIT OR Apache-2.0, at your option.
