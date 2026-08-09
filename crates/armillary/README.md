# armillary

A host-neutral actor-kernel runtime. A single-threaded host kernel owns the
canonical state (the document model, the GPU device, the window); actors run
off-thread and talk to it only by `Send` message. The crate is the runtime spine
and carries nothing host-specific; its only dependency is `tracing`.

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

## Modules

| Module | Contents |
| --- | --- |
| `boundary` | `KernelThread`, a zero-size `!Send` + `!Sync` marker (`PhantomData<Rc<()>>`). Embed it in the host's kernel context and moving that context to another thread becomes a compile error. |
| `actor` | `spawn`, `spawn_named`, `spawn_on`, `spawn_named_on`; `ActorHandle<C>` (`command`, `join`), `Emitter<U>` (`emit`, `Clone` without requiring `U: Clone`), `Wake = Arc<dyn Fn() + Send + Sync>`. The `run` closure executes on the actor thread, so `!Send` internals are constructed there and never move across. Each run loop is wrapped in a `tracing` span on target `armillary` with `actor started` / `actor finished` (`lifetime_ms`) events. |
| `pool` | `Pool` (`new`, `submit`, `workers`), a growable worker pool built on `Mutex` + `Condvar`. A worker runs one job for that job's whole lifetime, then waits for the next instead of exiting, so the OS-thread count tracks peak concurrent actors rather than total spawns. |
| `message` | `Generations` (`nav`, `viewport`, `accepts`), `NavGeneration` and `ViewportGeneration` (`bump`), `RequestId`, `RequestIds::issue`, `Correlated<T>` (`new`, `map`). Stamp outgoing work, drop returning work whose stamp no longer matches. |

`spawn` returns an `ActorHandle<C>` plus a `Receiver<U>` the kernel drains.
Dropping the handle closes the command channel, which is how the actor loop ends.
`spawn_on` is the same shape on a pooled worker, and its handle carries no
`JoinHandle`.

The concrete `Command` / `Update` taxonomy and the kernel inbox belong to the
host; this crate is generic over them. `Correlated<T>` pairs updates with the
command that caused them without defining the outcome vocabulary.

## Next

- `design_docs/2026-07-07_armillary_founding_proposal.md`
- Consumers in this workspace: `crates/system/fetch`, `crates/crawl`,
  `crates/canvas/canvas`, and `crates/intel/esp` under its `actor` feature.

License: dual MIT OR Apache-2.0, at your option.
