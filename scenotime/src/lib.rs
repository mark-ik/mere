//! Scenotime: the runtime for the scenograph projection engine.
//!
//! The inhabited scene through time. Scenotime owns what a one-shot pipeline
//! cannot: incremental re-evaluation of only what changed, caches keyed by
//! signal generation, scene diffs out to hosts, and the reverse path in,
//! where gestures resolve against hit shapes into intents routed back to the
//! authority that owns the fact.
//!
//! Name reservation: substance arrives with the engine's later proofs.
