//! **Djinn**, the local desktop resident for the Mere stack.
//!
//! Djinn owns process lifetime, profile-scoped settings, physical content
//! custody, and local endpoint assembly. Product ports retain their own
//! semantics: Graphshell provides admitted session surfaces, Knot provides
//! Djot source, sync, and evidence rules, Personae provides identity, and
//! Castellan provides credential custody.

#![doc(html_no_source)]

pub mod pairing;
pub mod personal_sync;
pub mod resident;
pub mod resident_blobs;
pub mod resident_knot;
pub mod settings;
