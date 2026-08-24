# moot

Founding reservation for **Moot**, the Mere platform's places port: the
first-party application surface over governed shared spaces.

One port, two surfaces, because the workflows separate cleanly:

- **murmur** — conversation. Direct and invitation-scoped conversations,
  store-and-forward mail, history, drafts, delivery and refusal, attachments,
  and calls when the transport and media receipts support them.
- **moot** — community. Find, preview, join and reconnect ceremony;
  membership and roles; proposals, decisions, moderation and appeals; storage
  and compute contributions; space health, replication, and reachability.

**murmur mounts alone.** Signalman wants messages and voice drops without
governance UI, and that constraint is what keeps the two surfaces separable
rather than one screen with tabs.

The boundaries are the point: not [gemot](https://crates.io/crates/gemot)
(governance, membership, and constitution stay the authority's), not
[murm](https://crates.io/crates/murm) (the post grammar, signed logs, and
sync lanes), not the commons spine or
[stickleback](https://crates.io/crates/stickleback) (the shared graph and its
replication), and no longer a Turnstone feature — the 2026-07-28 place-port
ruling's application half was reversed by the 2026-08-22 suite census, while
its authority half stands.

The package is `mere-moot` because crates.io `moot` is held by an unrelated
crate with real code, and `murmur` is likewise taken; the library keeps the
product name.

Lives in the [mere](https://github.com/merely-made/mere) workspace at
`ports/moot`. No implementation yet.

## License

MPL-2.0
