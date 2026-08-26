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
row localized and recovered two Burn/CubeCL browser defects. The configured
D2c embedding matrix now passes BGE Micro, MiniLM, E5-small, and E5-base in
headed Chromium, including integrity reopen, numerical references, worker
termination, and frame receipts. The largest success is a 437,955,512-byte F32
artifact. A first 269,060,552-byte BF16 SmolLM2 decoder row also passes exact
Transformers and ESP NdArray token references, cold/warm worker streaming,
cooperative token-boundary cancellation, explicit `GPUDevice.destroy()`, and
exact recovery in a fresh worker. The upper model boundaries and physical GPU
allocation release remain unmeasured.

The lease-bound Burn Remote adapter is now live behind Distillery's `remote`
feature. It mounts on the resident p2panda/Iroh endpoint, admits only a signed
claim projected from the host's exact active mesh lease, and closes every
matching session before owner reclaim is authored. The native MiniLM forcing
fixture under `probe/remote-fixture` exercises ESP over that adapter on WGPU;
the checked-in receipt records the numerical, cancellation, and fresh-session
recovery boundaries.

The installed-authority slice now persists one explicit Personae profile at
`<data-root>/distillery/settings.json`. The record contains no seed, vault
secret, scheduler cadence, device policy, or mesh-retention policy. Opening it
requires that selected profile to already exist in the shared Personae vault;
the mesh author is derived under `mesh::MESH_AUTHOR_SALT` and the transport
uses that profile's master identity. Per-mesh private paths live below
`<data-root>/distillery/meshes/<mesh-id>/`.

`distillery-installed configure --data-root <path> --profile <id>` persists
that selection. `inspect` opens it and reports the selected profile and
Personae protection, but intentionally does not start a resident. A real start
still requires a mesh owner to supply its store and retention policy, and a
device owner to supply `HostConfig` and `ResidentSettings`; Distillery must not
invent scheduler or device-policy authority. Cambium views, model manifest
browsing, physical GPU-allocation telemetry, and training remain later slices.

Lives in the [mere](https://github.com/merely-made/mere) workspace at
`ports/distillery`.

## License

MIT OR Apache-2.0
