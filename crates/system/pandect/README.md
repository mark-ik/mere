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

Renamed from `session-runtime` on 2026-08-15 when it was published: the crate
was named for what it did in one host, and the family names published crates
for what they are.

Note for anyone building on the wallet half: the carry *model* lives in
[personae](https://crates.io/crates/personae) as `personae::carry`. What stays
here is the adapter: the on-disk layout under the data root, the unlock ladder,
sealed-record wiring, grant envelopes, and the epoch seal. That split was ruled
deliberate on 2026-08-10, so this is a home rather than a way-station.

Part of the [mere](https://github.com/merely-made/mere) workspace.

Written with AI assistance (Claude).

## License

MPL-2.0
