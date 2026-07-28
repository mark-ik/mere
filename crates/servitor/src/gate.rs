//! The gate: one authority pipeline for every petition.
//!
//! A denizen proposes a **petition** (a batch of [`EditSpec`]s against its
//! nested graph, claiming to act under a [`ScopePath`]). A petition is always
//! scope-claimed, never power-claimed: powers name app abilities, scopes name
//! places in a graph, and only the second can contain a node id. The gate:
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

use chartulary::{Author, CommitError, Committed, Container, EditSpec, GraphLog, Relation};

use crate::Subject;
use crate::cap::{Cap, ScopePath};
use crate::grant::{AuthorityProvider, Grant, Mode};

/// The reserved node-id prefix for grant projections. The gate writes these and
/// refuses any petition that touches them, so a denizen cannot rewrite its own
/// authority.
///
/// This one IS a string-prefix test, correctly: it reserves an id namespace
/// whose delimiter (`:`) is part of the prefix, so there is no segment
/// ambiguity to exploit (`grantx` does not start with `grant:`). The capability
/// checks below are the ones that had to become typed.
pub const GRANT_PREFIX: &str = "grant:";

/// The tag marking a node as a grant projection.
pub const PROJECTION_TAG: &str = "grant-projection";

/// The media type naming the projection record's schema, so a reader knows the
/// parse before it trusts the tags.
pub const PROJECTION_MEDIA_TYPE: &str = "application/vnd.mere.grant+json";

const CAP_TAG: &str = "cap:";
const MODE_TAG: &str = "mode:";
const SUBJECT_TAG: &str = "subject:";
const EXPIRES_TAG: &str = "expires:";
const EXPIRES_NEVER: &str = "never";

fn mode_wire(mode: Mode) -> &'static str {
    match mode {
        Mode::Read => "read",
        Mode::Write => "write",
        Mode::Delegate => "delegate",
    }
}

fn mode_from_wire(raw: &str) -> Option<Mode> {
    match raw {
        "read" => Some(Mode::Read),
        "write" => Some(Mode::Write),
        "delegate" => Some(Mode::Delegate),
        _ => None,
    }
}

fn tag_value<'a>(node: &'a Container, prefix: &str) -> Option<&'a str> {
    node.tags.iter().find_map(|t| t.strip_prefix(prefix))
}

/// The `cap`/`mode`/`subject`/`expires` tags a grant projects. Shared by the
/// grant and delegation records so their field encoding cannot drift.
fn grant_tags(grant: &Grant) -> Vec<String> {
    vec![
        format!("{CAP_TAG}{}", grant.cap.to_wire()),
        format!("{MODE_TAG}{}", mode_wire(grant.mode)),
        format!("{SUBJECT_TAG}{}", grant.subject.to_hex()),
        format!(
            "{EXPIRES_TAG}{}",
            match grant.expires_at_ms {
                Some(at) => at.to_string(),
                None => EXPIRES_NEVER.to_string(),
            }
        ),
    ]
}

/// Reconstruct the grant a projection node carries, or `None` if a field is
/// missing or unparseable. Shared reader for the grant and delegation records.
fn read_grant(node: &Container) -> Option<Grant> {
    let cap = Cap::parse(tag_value(node, CAP_TAG)?).ok()?;
    let mode = mode_from_wire(tag_value(node, MODE_TAG)?)?;
    let subject = Subject::from_hex(tag_value(node, SUBJECT_TAG)?)?;
    let grant = Grant::new(subject, cap, mode);
    // A record written before expiry existed carries no tag and is
    // open-ended, which is what it was.
    Some(match tag_value(node, EXPIRES_TAG) {
        None | Some(EXPIRES_NEVER) => grant,
        Some(raw) => grant.expiring_at(raw.parse().ok()?),
    })
}

/// Read a grant back out of a projection node, or `None` if the node is not a
/// well-formed projection (not tagged, or missing/unparseable fields).
///
/// Fails closed and loudly-by-absence: a projection this build cannot fully
/// understand yields no grant at all, rather than a partially-reconstructed one
/// that might be wider than what was written.
pub fn read_projection(node: &Container) -> Option<Grant> {
    if !node.tags.iter().any(|t| t == PROJECTION_TAG) {
        return None;
    }
    read_grant(node)
}

/// Why the gate refused a petition. Nothing was applied.
#[derive(Clone, Debug, PartialEq)]
pub enum GateError {
    /// The subject holds no capability covering the claimed path at write mode.
    Unauthorized {
        /// The scope the petition claimed.
        path: String,
    },
    /// A spec targets a node outside the claimed scope, or one whose id is not
    /// a well-formed scope at all (which fails closed).
    OutOfScope {
        /// The offending node id.
        node: String,
        /// The scope the petition claimed.
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
        EditSpec::SetFacet { node, .. } | EditSpec::RemoveFacet { node, .. } => {
            vec![node.as_str()]
        }
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

    /// The projection node id for a grant over `cap`, in the capability's wire
    /// form so the record round-trips (`grant:power:navigate`).
    pub fn projection_id(cap: &Cap) -> String {
        format!("{GRANT_PREFIX}{}", cap.to_wire())
    }

    fn commit_projection(
        &self,
        nested: &mut GraphLog<Container, Relation>,
        mut node: Container,
    ) -> Result<Committed, GateError> {
        node.media_type = Some(PROJECTION_MEDIA_TYPE.to_string());
        let expected = nested.revision();
        nested
            .commit_batch(
                self.author.clone(),
                expected,
                vec![EditSpec::InsertNode(node)],
            )
            .map_err(GateError::Commit)
    }

    /// Render `grant` into `nested` as a read-only projection node, committed by
    /// the gate's own author. A browsable record of what the denizen may do.
    ///
    /// The record is **lossless**: every field rides an explicit `key:value`
    /// tag, so [`read_projection`] reconstructs the grant exactly. (Before the
    /// capability-model round the mode lived only in the display title and
    /// readers hardcoded [`Mode::Write`], so a `Read` grant came back as
    /// `Write` after a restart. A field that does not survive replay is not a
    /// field.)
    pub fn project_grant(
        &self,
        nested: &mut GraphLog<Container, Relation>,
        grant: &Grant,
    ) -> Result<Committed, GateError> {
        let mut node = Container::new(Self::projection_id(&grant.cap)).with_tag(PROJECTION_TAG);
        for tag in grant_tags(grant) {
            node = node.with_tag(tag);
        }
        node = node.with_title(format!(
            "{:?} {} {}",
            grant.mode,
            grant.cap,
            grant.subject.to_hex()
        ));
        self.commit_projection(nested, node)
    }

    /// Run a petition through the gate: projection guard, authority, scope, then
    /// an attributed revision-checked commit. Returns the commit receipt or the
    /// reason nothing applied.
    pub fn petition(
        &self,
        provider: &impl AuthorityProvider,
        nested: &mut GraphLog<Container, Relation>,
        subject: Subject,
        claimed: &ScopePath,
        expected: u64,
        specs: Vec<EditSpec<Container, Relation>>,
    ) -> Result<Committed, GateError> {
        // 1. Projection guard: a denizen may never touch its own grants.
        for spec in &specs {
            for node in touched_nodes(spec) {
                if node.starts_with(GRANT_PREFIX) {
                    return Err(GateError::TouchesProjection {
                        node: node.to_string(),
                    });
                }
            }
        }
        // 2. Authority: the subject must hold a write cap covering the scope.
        let claimed_cap = Cap::Scope(claimed.clone());
        if !provider.covers(subject, &claimed_cap, Mode::Write) {
            return Err(GateError::Unauthorized {
                path: claimed.to_string(),
            });
        }
        // 3. Scope: every touched node must fall under the claimed scope, by
        //    SEGMENT (`trail` does not contain `trailer/x`). A node id that is
        //    not a well-formed scope fails closed rather than being compared as
        //    a raw string.
        for spec in &specs {
            for node in touched_nodes(spec) {
                let inside = ScopePath::parse(node)
                    .is_ok_and(|node_scope| claimed.covers_scope(&node_scope));
                if !inside {
                    return Err(GateError::OutOfScope {
                        node: node.to_string(),
                        path: claimed.to_string(),
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
    use crate::grant::GrantTable;
    use chartulary::Container;

    fn subject(tag: u8) -> Subject {
        Subject::new([tag; 32])
    }

    fn authority(subject: Subject) -> GrantTable {
        GrantTable::new().with_grant(Grant::new(subject, trail(), Mode::Write))
    }

    /// The capability every test petitions under.
    fn trail() -> Cap {
        Cap::scope("trail").unwrap()
    }

    fn trail_scope() -> ScopePath {
        ScopePath::parse("trail").unwrap()
    }

    fn insert(id: &str) -> EditSpec<Container, Relation> {
        EditSpec::InsertNode(Container::new(id))
    }

    #[test]
    fn a_grant_projects_read_only_and_attributed_to_the_gate() {
        let gate = Gate::new();
        let mut nested = GraphLog::<Container, Relation>::new();
        let grant = Grant::new(subject(1), trail(), Mode::Write);

        let committed = gate.project_grant(&mut nested, &grant).unwrap();
        let entry = &nested.log().entries()[committed.batch.0 as usize];
        assert_eq!(
            entry.author,
            Author::new("gate"),
            "projection is the gate's, not the denizen's"
        );
        assert!(
            nested
                .graph()
                .key_of(&Gate::projection_id(&trail()))
                .is_some(),
            "the projection node exists in the nested graph"
        );
    }

    #[test]
    fn a_projection_round_trips_every_field_including_a_read_mode() {
        // F3, the bug this encoding exists to kill: the mode used to live only
        // in the display title, and readers hardcoded Write, so a Read grant
        // came back as Write after a restart.
        let gate = Gate::new();
        for (cap, mode) in [
            (trail(), Mode::Read),
            (trail(), Mode::Write),
            (Cap::power("navigate").unwrap(), Mode::Delegate),
            (Cap::root_scope(), Mode::Write),
        ] {
            let mut nested = GraphLog::<Container, Relation>::new();
            // Bounded on the Delegate case, so expiry rides the round trip too.
            let grant = match mode {
                Mode::Delegate => Grant::new(subject(7), cap.clone(), mode).expiring_at(1_234),
                _ => Grant::new(subject(7), cap.clone(), mode),
            };
            gate.project_grant(&mut nested, &grant).unwrap();

            let key = nested.graph().key_of(&Gate::projection_id(&cap)).unwrap();
            let node = nested.graph().node(key).unwrap();
            assert_eq!(
                read_projection(node),
                Some(grant),
                "{cap:?} at {mode:?} must survive the round trip exactly"
            );
        }
    }

    #[test]
    fn a_node_that_is_not_a_projection_reads_as_none() {
        // Fails closed: an ordinary node, and a mistagged one missing fields,
        // both yield no grant rather than a guessed one.
        assert_eq!(read_projection(&Container::new("trail/step1")), None);
        let partial = Container::new("grant:scope:trail").with_tag(PROJECTION_TAG);
        assert_eq!(read_projection(&partial), None, "no cap/mode/subject tags");
    }

    #[test]
    fn an_in_scope_petition_commits_attributed_to_the_denizen() {
        let gate = Gate::new();
        let sub = subject(1);
        let auth = authority(sub);
        let mut nested = GraphLog::<Container, Relation>::new();

        let rev = nested.revision();
        let committed = gate
            .petition(
                &auth,
                &mut nested,
                sub,
                &trail_scope(),
                rev,
                vec![insert("trail/step1"), insert("trail/step2")],
            )
            .unwrap();

        let entry = &nested.log().entries()[committed.batch.0 as usize];
        assert_eq!(
            entry.author,
            sub.to_author(),
            "the journal attributes the change to the denizen"
        );
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
            .petition(
                &auth,
                &mut nested,
                intruder,
                &trail_scope(),
                rev,
                vec![insert("trail/x")],
            )
            .unwrap_err();
        assert_eq!(
            err,
            GateError::Unauthorized {
                path: "trail".into()
            }
        );
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
            .petition(
                &auth,
                &mut nested,
                sub,
                &trail_scope(),
                rev,
                vec![insert("notes/sneaky")],
            )
            .unwrap_err();
        assert_eq!(
            err,
            GateError::OutOfScope {
                node: "notes/sneaky".into(),
                path: "trail".into()
            }
        );
        assert_eq!(nested.graph().node_count(), 0);
    }

    #[test]
    fn a_denizen_cannot_touch_its_own_grant_projection() {
        let gate = Gate::new();
        let sub = subject(1);
        // Grant the denizen the reserved namespace itself — the guard still bites.
        // Grant the denizen the reserved namespace as a scope; the guard still bites.
        let auth = GrantTable::new().with_grant(Grant::new(
            sub,
            Cap::scope("grant:").unwrap(),
            Mode::Write,
        ));
        let mut nested = GraphLog::<Container, Relation>::new();
        gate.project_grant(&mut nested, &Grant::new(sub, trail(), Mode::Write))
            .unwrap();

        let rev = nested.revision();
        let err = gate
            .petition(
                &auth,
                &mut nested,
                sub,
                &ScopePath::parse("grant:").unwrap(),
                rev,
                vec![EditSpec::RemoveNode(Gate::projection_id(&trail()))],
            )
            .unwrap_err();
        assert_eq!(
            err,
            GateError::TouchesProjection {
                node: Gate::projection_id(&trail())
            }
        );
        assert!(
            nested
                .graph()
                .key_of(&Gate::projection_id(&trail()))
                .is_some(),
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
        gate.petition(
            &auth,
            &mut nested,
            sub,
            &trail_scope(),
            stale,
            vec![insert("trail/a")],
        )
        .unwrap();

        let err = gate
            .petition(
                &auth,
                &mut nested,
                sub,
                &trail_scope(),
                stale,
                vec![insert("trail/b")],
            )
            .unwrap_err();
        match err {
            GateError::Commit(CommitError::RevisionConflict { current }) => {
                assert_eq!(
                    current,
                    stale + 1,
                    "the denizen learns the revision to rebase onto"
                );
            }
            other => panic!("expected a revision conflict, got {other:?}"),
        }
    }
}
