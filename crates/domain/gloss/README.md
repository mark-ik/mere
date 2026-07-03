# gloss

Gloss domain layer for Mere.

This crate now owns the host-neutral gloss vocabulary and geometry used by the
Meerkat gloss pane:

- outline row intents and snapshots
- minimap fit/scene helpers
- pane section math
- the older `EngineDocument -> UxTree` outline projection

Host-specific snapshot building and event application still live in `meerkat`.
