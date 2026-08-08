//! Name reservation for **gaz**, the contact layer.
//!
//! A contact is the local rollup that says "these N addresses are all Bob":
//! your petname for them, rooted on their stable keys, with attested handles
//! and current endpoints, each carrying its own trust state. Records are
//! persona-scoped (a throwaway persona must not share the work persona's
//! contacts), tiered kith / kin, and carry recency. Storage rides `muniment`;
//! records key against `personae`.
//!
//! The boundaries are the point:
//!
//! - **Not the resolver.** The gazetteer turns a name, handle, or key into
//!   reachable endpoints; `gaz` is where the ones you keep live. Gaz is not
//!   short for gazetteer; the two are siblings on the persona tier.
//! - **Not identity itself.** `personae` owns *me*, the key-bag and its carry.
//!   `gaz` owns *them*, your own records about other people's keys.
//! - **Not trust arithmetic.** Verification state is stored per endpoint; how
//!   it is earned belongs to the trust plane.
//!
//! No implementation yet.

#![doc(html_no_source)]
