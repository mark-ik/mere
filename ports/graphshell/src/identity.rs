//! Castellan's secret-free identity read model, at its pre-founding path.
//!
//! The types moved home to [`castellan::view`] when the keeper surface was
//! founded (2026-08-14): by the port law, identity is a capability the stack
//! owns, castellan is its port, and graphshell composes it. This shim keeps
//! every existing graphshell call site — the endpoint, the native hosts, the
//! receipt bins — compiling unchanged.

pub use castellan::view::*;
