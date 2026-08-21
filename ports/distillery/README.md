# distillery

**Distillery** is the model-works port of the Mere platform. Its first authority
slice is implemented as the first real consumer of `mere-mesh-host`.

A distillery takes a raw mash and runs it, batch by batch, through stills into
something concentrated. This port is that works for models: the harness where
a ring of devices runs inference, embedding, and eventually training jobs. It
splits in two, in the castellan mold: an embeddable half any host app composes
(the job board, lease and heartbeat tiles, the device roster, retention and
device-policy panes, model-manifest browsing, a minimal streaming console;
views that render what the supervisor reports, never a placebo), and an
authority half that lives with the device (the supervisor loop that owns mesh
host ticks, device conditions, the resource registry, and blob custody; it
answers to the owner, and reclaim always wins).

The boundaries are the point: not [esp](https://crates.io/crates/esp) (the
inference/embedding seam crate stays the portable burn boundary; distillery
drives it), not mere-mesh or mere-mesh-host (job grammar, leases, and the
supervisor are substrate; distillery embeds and renders them), not servitor
(petition gating stays the gate's office), and not turnstone (the flagship
embeds the same views; distillery is the standalone works).

The trainer lane (Distillery-as-trainer, per the geist brief) lands here
later, behind its own plan: training is one more job the works runs.

## Current slice

`Distillery<B>` composes a `MeshHost<B>`, drives its non-blocking supervisor
ticks, and owns an explicit maintenance operation. Maintenance authors an
owner-governed retention checkpoint, asks the mesh store which blob references
are safe against both the checkpoint and the current replay tail, and can
release this mesh's named custody tags. Collection is off by default.

Releasing a tag is a logical custody change. Physical bytes disappear only
when the collecting `mere-transport` store finds no remaining tag from another
mesh or subsystem. A content hash reused by an unfinished or post-checkpoint
job remains protected.

D1 adds `ResidentAuthority<B>`, the reusable body of a long-lived device
process. The owner supplies supervisor, maintenance, and physical collection
cadences; the type deliberately has no default policy. `ResidentStorage` opens
a disk-backed collecting blob store and binds its custody tags to one mesh. The
resident owns that storage, the live p2p transport, the mesh host, and their
shutdown order. Shutdown waits for the LogSync drain, every topic manager, and
the top-level actor task to drop their store handles, so a successful return is
a real restart boundary rather than an asynchronous stop notification.

Every supervisor turn, completed maintenance run, unchanged maintenance turn,
maintenance refusal, and shutdown request is an ordered `ResidentReceipt`.
Scheduled maintenance keeps running after a visible refusal such as a live
lease. A supervisor failure is fatal. An unchanged event frontier is reported
without authoring another checkpoint.

The D1 proof reopens both the redb mesh store and collecting blob store after a
real resident run. The job remains terminal and released content stays
released across that restart boundary.

`probe/` is D2's standalone headed-browser evidence surface. Its first MiniLM
row proves the Eidetic/Muniment artifact corridor, warm IndexedDB reopen, and
worker lifecycle. Burn/CubeCL BrowserWebGpu execution currently fails the
reference-vector gate, so the probe records a measured limit rather than
claiming a usable browser model.

There is not yet an installed standalone binary because Distillery still needs
a product-level choice of Personae profile and persisted settings authority.
Cambium views, Burn resource migration, the Burn Remote adapter, model manifest
browsing, and training remain later slices.

Lives in the [mere](https://github.com/merely-made/mere) workspace at
`ports/distillery`.

## License

MIT OR Apache-2.0
