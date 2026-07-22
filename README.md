# scenograph

A projection compiler and runtime for interactive surfaces: heterogeneous
sources (graphs, maps, timelines, instruments, meshes) projected into
inhabited scenes through one grammar.

```text
source data + relationships + signals + score
    → select and derive
    → map data to visual channels
    → solve placement
    → produce an interactive scene
    → route gestures back as authorized intents
```

The family shares the `sceno-` stem, one function morpheme each:

- **[sceno](sceno/)** — core contracts: source references, scores
  (serialized projection settings), channels, coordinate spaces, footprints,
  scene snapshots, action intents.
- **[scenomise](scenomise/)** — choreography: placement solvers that realize
  scores into arranged scenes (mise-en-scène lives in the name).
- **[scenotime](scenotime/)** — runtime vocabulary: stable scene epochs,
  revisions, tombstones, and transactional idempotent diffs.
- **[scenograph](scenograph/)** — a thin facade re-exporting the three.

Sources keep their native truth behind adapters; what is shared is the scene
contract, not a data model. The representation measures content; the
projection places it.

**Status:** `sceno` owns the portable scene and serialized score contracts,
`scenomise` solves spiral, board, and geographic arrangements, and `scenotime`
owns stable epoch/revision snapshots plus idempotent diffs. Incremental signal
evaluation, hit-to-intent routing, and renderer realization remain later work.
See
[the scene contract note](design_docs/2026-07-22_scene_contract_note.md) and
[the epoch/diff note](design_docs/2026-07-22_scenotime_epoch_diff_note.md).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.
