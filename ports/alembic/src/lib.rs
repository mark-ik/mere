//! **Alembic**, the Mere platform's recall and workshop port.
//!
//! An alembic is the still that sits on the athanor's constant low heat: what
//! distils is what the furnace has been holding. This port is that pair for
//! your own work — the memory it accretes, and the bounded actors that run
//! over it.
//!
//! It splits in the castellan mold:
//!
//! - **The embeddable half** (feature `recall`) is the memory surface: the
//!   three levels (short-term, long-term, engram), promotion and eviction,
//!   the engram browser, and lexical and embedding recall over a mere's
//!   traces. A host that wants memory and no agents takes this alone.
//! - **The authority half** is the workshop: agent identity and purpose,
//!   granted reads, writes, actions and watches, model and tool selection,
//!   run history, pending petitions, refusals and costs, pause, revoke,
//!   retry, and dissolve — with exact attribution into the target
//!   application's history. It lives with the resident, whose scheduler runs
//!   the jobs.
//!
//! **Athanor was always an agent.** The distillation furnace — the steady
//! background actor that consolidates memory and mints distillates while you
//! work — is a bounded actor under a grant, so the workshop is that furnace
//! generalized rather than a second system beside it. Agent continuity rides
//! the same engram and `tulpa` machinery: what is retold stays manifest.
//!
//! The boundaries are the point:
//!
//! - **Not distillery.** Distillery is the model works: jobs, leases,
//!   manifests, device policy, retention. Distillery runs models; alembic
//!   runs work, and an agent here may use a model there.
//! - **Not the store.** Engrams, retention, and the browsing corpus are
//!   `eidetic`'s; the graph-engram spine and memory levels currently sit in
//!   `pandect`.
//! - **Not the grant algebra.** Scoped capability and the validating gate are
//!   `servitor`'s, and the identity a grant attenuates from is `personae`'s.
//!
//! The package is `mere-alembic` because crates.io `alembic` is the Linux
//! Foundation's VFX-format binding. The library keeps the product name.
//!
//! Founding gate, per the 2026-08-22 suite census: one bounded agent
//! completing a useful workflow in two hosts through the same grant and
//! observation surface.
//!
//! No implementation yet.

#![doc(html_no_source)]
