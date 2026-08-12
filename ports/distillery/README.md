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

The current proof is authority-only. There is no resident binary, Cambium view,
Burn resource migration, Burn Remote adapter, model manifest browser, or
trainer yet. Those remain separate slices rather than implied behavior.

Lives in the [mere](https://github.com/merely-made/mere) workspace at
`ports/distillery`.

## License

MIT OR Apache-2.0
