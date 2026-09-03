# titulus

The inscription that says what a thing is.

A titulus was the placard naming what hung beneath it, and the label on a
scroll or amphora identifying its contents. This crate is that label for a
projected resource: a small, portable, semantic description an endpoint hands
a host, plus the bounded contract for what may be done about it.

- `PortableCardV1` — title, labeled values, badges, content addresses.
  Deliberately not a widget tree: enough for a host to render something honest
  about a resource it does not own, and not enough to make it a rendering
  engine for somebody else's application truth.
- `ActionFormV1` — an endpoint-authored input contract bounded to exact string
  choices, so a host composes an action payload without interpreting
  application meaning.
- `ContentHash` — a content address for a separately transferred resource.

Extracted 2026-08-14 from the projection protocol (now
[chirograph](https://crates.io/crates/chirograph)), because the card
vocabulary is neutral and [castellan](https://crates.io/crates/castellan), the
credential-keeper port, wanted it without the wire.

Part of the [mere](https://github.com/merely-made/mere) workspace.

Written with AI assistance (Claude).

## License

MPL-2.0
