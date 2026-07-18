//! The gate: one authority pipeline for every petition.
//!
//! A denizen proposes a **petition** (a batch of [`EditSpec`]s against its
//! nested graph, claiming to act under a `capability_path`). The gate:
//!
//! 1. refuses any spec that would touch a **grant projection** (the reserved
//!    [`GRANT_PREFIX`] namespace) — a denizen can read its grants but never
//!    edit them, so it cannot escalate itself;
//! 2. checks **authority**: the [`AuthorityProvider`] must cover the claimed
//!    path at [`Mode::Write`];
//! 3. checks **scope**: every node a spec touches must fall under the claimed
//!    path;
//! 4. **commits** the batch attributed to the denizen, revision-checked, atomic
//!    (chartulary's `commit_batch`).
//!
//! Authority is materialized elsewhere and *projected* here read-only:
//! [`Gate::project_grant`] renders a [`Grant`] into the nested graph as a
//! reserved-namespace node, committed by the gate's own author, so "what may
//! this denizen do" is a browsable question answered from the graph itself.

use chartulary::{Author, Committed, CommitError, Container, EditSpec, GraphLog, Relation};

use crate::grant::{AuthorityProvider, Grant, Mode};
use crate::Subject;

/// The reserved node-id prefix for grant projections. The gate writes these and
/// refuses any petition that touches them, so a denizen cannot rewrite its own
/// authority.
pub const GRANT_PREFIX: &str = "grant:";

/// Why the gate refused a petition. Nothing was applied.
#[derive(Clone, Debug, PartialEq)]
pub enum GateError {
    /// The subject holds no capability covering the claimed path at write mode.
    Unauthorized {
        /// The path the petition claimed.
        path: String,
    },
    /// A spec targets a node outside the claimed path.
    OutOfScope {
        /// The offending node id.
        node: String,
        /// The path the petition claimed.
        path: String,
    },
    /// A spec would touch a grant projection (the reserved namespace).
    TouchesProjection {
        /// The offending node id.
        node: String,
    },
    /// The underlying attributed commit refused (revision conflict, unknown
    /// node, and so on). Carries chartulary's error, including the current
    /// revision on a conflict so the denizen can rebase.
    Commit(CommitError<String>),
}

/// The node ids a spec touches (for scope and projection checks). Edge-level
/// specs ([`EditSpec::Disconnect`]) touch no node id.
fn touched_nodes(spec: &EditSpec<Container, Relation>) -> Vec<&str> {
    match spec {
        EditSpec::InsertNode(node) => vec![node.id.as_str()],
        EditSpec::RemoveNode(id) => vec![id.as_str()],
        EditSpec::Connect { from, to, .. } => vec![from.as_str(), to.as_str()],
        EditSpec::Disconnect(_) => Vec::new(),
        EditSpec::Derive { node, .. } => vec![node.as_str()],
    }
}

/// The authority gate. Holds the author it commits **grant projections** under
/// (distinct from any denizen), so projections are attributable to the gate,
/// not to the helper they describe.
#[derive(Clone, Debug)]
pub struct Gate {
    author: Author,
}

impl Default for Gate {
    fn default() -> Self {
        Self::new()
    }
}

impl Gate {
    /// A gate whose projections are authored `gate`.
    pub fn new() -> Self {
        Self {
            author: Author::new("gate"),
        }
    }

    /// A gate whose projections carry a specific author.
    pub fn with_author(author: Author) -> Self {
        Self { author }
    }

    /// The projection node id for a grant over `path_prefix`.
    pub fn projection_id(path_prefix: &str) -> String {
        format!("{GRANT_PREFIX}{path_prefix}")
    }

    /// Render `grant` into `nested` as a read-only projection node, committed by
    /// the gate's own author. A browsable record of what the denizen may do.
    pub fn project_grant(
        &self,
        nested: &mut GraphLog<Container, Relation>,
        grant: &Grant,
    ) -> Result<Committed, GateError> {
        let node = Container::new(Self::projection_id(&grant.path_prefix))
            .with_tag("grant-projection")
            .with_title(format!(
                "{:?} {} {}",
                grant.mode,
                grant.path_prefix,
                grant.subject.to_hex()
            ));
        let expected = nested.revision();
        nested
            .commit_batch(self.author.clone(), expected, vec![EditSpec::InsertNode(node)])
            .map_err(GateError::Commit)
    }

    /// Run a petition through the gate: projection guard, authority, scope, then
    /// an attributed revision-checked commit. Returns the commit receipt or the
    /// reason nothing applied.
    pub fn petition(
        &self,
        provider: &impl AuthorityProvider,
        nested: &mut GraphLog<Container, Relation>,
        subject: Subject,
        claimed_path: &str,
        expected: u64,
        specs: Vec<EditSpec<Container, Relation>>,
    ) -> Result<Committed, GateError> {
        // 1. Projection guard: a denizen may never touch its own grants.
        for spec in &specs {
            for node in touched_nodes(spec) {
                if node.starts_with(GRANT_PREFIX) {
                    return Err(GateError::TouchesProjection { node: node.to_string() });
                }
            }
        }
        // 2. Authority: the subject must hold a write cap covering the path.
        if !provider.covers(subject, claimed_path, Mode::Write) {
            return Err(GateError::Unauthorized { path: claimed_path.to_string() });
        }
        // 3. Scope: every touched node must fall under the claimed path.
        for spec in &specs {
            for node in touched_nodes(spec) {
                if !node.starts_with(claimed_path) {
                    return Err(GateError::OutOfScope {
                        node: node.to_string(),
                        path: claimed_path.to_string(),
                    });
                }
            }
        }
        // 4. Commit, attributed to the denizen, revision-checked and atomic.
        nested
            .commit_batch(subject.to_author(), expected, specs)
            .map_err(GateError::Commit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grant::PrefixAuthority;
    use chartulary::Container;

    fn subject(tag: u8) -> Subject {
        Subject::new([tag; 32])
    }

    fn authority(subject: Subject) -> PrefixAuthority {
        PrefixAuthority::new().with_grant(Grant::new(subject, "trail/", Mode::Write))
    }

    fn insert(id: &str) -> EditSpec<Container, Relation> {
        EditSpec::InsertNode(Container::new(id))
    }

    #[test]
    fn a_grant_projects_read_only_and_attributed_to_the_gate() {
        let gate = Gate::new();
        let mut nested = GraphLog::<Container, Relation>::new();
        let grant = Grant::new(subject(1), "trail/", Mode::Write);

        let committed = gate.project_grant(&mut nested, &grant).unwrap();
        let entry = &nested.log().entries()[committed.batch.0 as usize];
        assert_eq!(entry.author, Author::new("gate"), "projection is the gate's, not the denizen's");
        assert!(
            nested.graph().key_of(&Gate::projection_id("trail/")).is_some(),
            "the projection node exists in the nested graph"
        );
    }

    #[test]
    fn an_in_scope_petition_commits_attributed_to_the_denizen() {
        let gate = Gate::new();
        let sub = subject(1);
        let auth = authority(sub);
        let mut nested = GraphLog::<Container, Relation>::new();

        let rev = nested.revision();
        let committed = gate
            .petition(&auth, &mut nested, sub, "trail/", rev, vec![
                insert("trail/step1"),
                insert("trail/step2"),
            ])
            .unwrap();

        let entry = &nested.log().entries()[committed.batch.0 as usize];
        assert_eq!(entry.author, sub.to_author(), "the journal attributes the change to the denizen");
        assert_eq!(nested.graph().node_count(), 2);
    }

    #[test]
    fn an_unauthorized_subject_is_refused_and_nothing_applies() {
        let gate = Gate::new();
        let granted = subject(1);
        let intruder = subject(2);
        let auth = authority(granted);
        let mut nested = GraphLog::<Container, Relation>::new();

        let rev = nested.revision();
        let err = gate
            .petition(&auth, &mut nested, intruder, "trail/", rev, vec![insert("trail/x")])
            .unwrap_err();
        assert_eq!(err, GateError::Unauthorized { path: "trail/".into() });
        assert_eq!(nested.graph().node_count(), 0, "nothing applied");
    }

    #[test]
    fn a_petition_outside_the_claimed_path_is_refused() {
        let gate = Gate::new();
        let sub = subject(1);
        let auth = authority(sub);
        let mut nested = GraphLog::<Container, Relation>::new();

        let rev = nested.revision();
        let err = gate
            .petition(&auth, &mut nested, sub, "trail/", rev, vec![insert("notes/sneaky")])
            .unwrap_err();
        assert_eq!(
            err,
            GateError::OutOfScope { node: "notes/sneaky".into(), path: "trail/".into() }
        );
        assert_eq!(nested.graph().node_count(), 0);
    }

    #[test]
    fn a_denizen_cannot_touch_its_own_grant_projection() {
        let gate = Gate::new();
        let sub = subject(1);
        // Grant the denizen the reserved namespace itself — the guard still bites.
        let auth = PrefixAuthority::new().with_grant(Grant::new(sub, GRANT_PREFIX, Mode::Write));
        let mut nested = GraphLog::<Container, Relation>::new();
        gate.project_grant(&mut nested, &Grant::new(sub, "trail/", Mode::Write)).unwrap();

        let rev = nested.revision();
        let err = gate
            .petition(
                &auth,
                &mut nested,
                sub,
                GRANT_PREFIX,
                rev,
                vec![EditSpec::RemoveNode(Gate::projection_id("trail/"))],
            )
            .unwrap_err();
        assert_eq!(err, GateError::TouchesProjection { node: Gate::projection_id("trail/") });
        assert!(
            nested.graph().key_of(&Gate::projection_id("trail/")).is_some(),
            "the projection survived the attempt"
        );
    }

    #[test]
    fn a_stale_petition_surfaces_the_revision_conflict() {
        let gate = Gate::new();
        let sub = subject(1);
        let auth = authority(sub);
        let mut nested = GraphLog::<Container, Relation>::new();
        let stale = nested.revision();
        // A concurrent commit moves the revision past `stale`.
        gate.petition(&auth, &mut nested, sub, "trail/", stale, vec![insert("trail/a")])
            .unwrap();

        let err = gate
            .petition(&auth, &mut nested, sub, "trail/", stale, vec![insert("trail/b")])
            .unwrap_err();
        match err {
            GateError::Commit(CommitError::RevisionConflict { current }) => {
                assert_eq!(current, stale + 1, "the denizen learns the revision to rebase onto");
            }
            other => panic!("expected a revision conflict, got {other:?}"),
        }
    }
}
