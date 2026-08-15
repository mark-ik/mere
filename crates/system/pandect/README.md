# pandect

Everything a Mere session gathers under one cover.

A pandect is a complete digest of a body of law — the whole corpus collected
into one volume. This crate is that for a running session: the record it
accretes while it is alive.

- Session manifests, graph and view sidecars, worker declarations.
- Content, image, facet, and manifest stores.
- Application, device, and persona settings.
- The persona wallet carry lane: the device roster, signed device grants,
  epoch history, and the family-shared identity root.

Renamed from `session-runtime` on 2026-08-14 when it was published: the crate
was named for what it did in one host, and the family names published crates
for what they are.

Note for anyone building on the wallet half: it is scheduled to fold into
[personae](https://crates.io/crates/personae), which is the prerequisite the
credential port ([castellan](https://crates.io/crates/castellan)) is waiting
on. Depend on it here knowing it is a way-station.

Part of the [mere](https://github.com/merely-made/mere) workspace.

Written with AI assistance (Claude).

## License

MIT OR Apache-2.0
