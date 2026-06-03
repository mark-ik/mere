//! Probe 2: a Mere-native, Meadowcap-shaped capability over GRAPH-CLUSTER paths.
//!
//! Maps the Willow / Meadowcap data model onto Mere's graph-cluster-namespace
//! vision (substrate brief §14):
//!
//! - **namespace** = a moot (NamespaceId = moot id)
//! - **subspace**  = a member/author (SubspaceId = member pubkey; communal model)
//! - **path**      = a node's position in the graph's community decomposition
//!                   (Louvain/Leiden), e.g. `[3, 7, 1]` = cluster 3 / sub 7 / node 1
//!
//! A capability grants an **Area** = (subspace restriction, cluster-path prefix,
//! mode). Delegation monotonically NARROWS the area and is signed by the prior
//! holder. `grants(entry)` = valid signed chain + area includes the entry.
//!
//! The point: **path-prefix containment = sharing a coherent sub-graph
//! neighborhood.** Granting `[3]` shares Alice's whole cluster-3 community;
//! narrowing to `[3, 7]` shares just that sub-neighborhood. p2panda's flat
//! topics (a single hash) are all-or-nothing and cannot express this, which is
//! why the spatial namespace layer wants this shape, not flat topics. We can
//! own the interpretation natively; this probe is the core of it.
//!
//! See design_docs/mere_docs/implementation_strategy/2026-06-01_p2panda_substrate_spike_plan.md (Probe 2).

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

/// A node's position in the graph's community decomposition. Components are
/// cluster ids at successive granularities. (Willow `Path` is bytestring
/// components; we use ids for readability.)
type ClusterPath = Vec<u32>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Read,
    Write,
}

/// A Meadowcap "granted area": which subspace (author) and which cluster-path
/// prefix this capability covers.
#[derive(Clone)]
struct Area {
    /// `None` = any author in the namespace; `Some(pk)` = a specific member's subspace.
    subspace: Option<VerifyingKey>,
    /// Cluster-path prefix. `[]` = the whole namespace; `[3]` = cluster 3; `[3,7]` = sub 3/7.
    path_prefix: ClusterPath,
}

impl Area {
    /// Does this area authorize an entry authored by `author` at `path`?
    fn includes_entry(&self, author: &VerifyingKey, path: &ClusterPath) -> bool {
        self.subspace_ok(author) && is_prefix(&self.path_prefix, path)
    }

    /// For delegation: is `inner` strictly within (or equal to) this area?
    /// (Monotonic restriction: a child grant can only narrow.)
    fn includes_area(&self, inner: &Area) -> bool {
        let subspace_narrows = match (self.subspace, inner.subspace) {
            (None, _) => true,                 // parent allows any author
            (Some(p), Some(i)) => p == i,      // child pinned to same author
            (Some(_), None) => false,          // child cannot widen to any author
        };
        subspace_narrows && is_prefix(&self.path_prefix, &inner.path_prefix)
    }

    fn subspace_ok(&self, author: &VerifyingKey) -> bool {
        match self.subspace {
            None => true,
            Some(pk) => pk == *author,
        }
    }
}

/// `prefix` is a prefix of `path` iff its components are the first components of `path`.
fn is_prefix(prefix: &ClusterPath, path: &ClusterPath) -> bool {
    prefix.len() <= path.len() && path[..prefix.len()] == prefix[..]
}

/// One delegation step: narrow to `area`, hand to `receiver`, signed by the
/// prior holder.
struct Delegation {
    area: Area,
    receiver: VerifyingKey,
    signature: Signature,
}

/// A capability chain (communal model): the subspace owner roots it, then a
/// chain of delegations narrows it.
struct Capability {
    namespace: [u8; 32], // moot id
    mode: Mode,
    /// Root grant. In the communal model the subspace owner (== `initial_area.subspace`)
    /// signs `root_signature` over the initial grant.
    initial_area: Area,
    initial_receiver: VerifyingKey,
    root_signature: Signature,
    delegations: Vec<Delegation>,
}

/// Canonical bytes signed at each step: namespace || mode || area || receiver.
fn grant_msg(namespace: &[u8; 32], mode: Mode, area: &Area, receiver: &VerifyingKey) -> Vec<u8> {
    let mut m = Vec::new();
    m.extend_from_slice(namespace);
    m.push(match mode {
        Mode::Read => 0,
        Mode::Write => 1,
    });
    match area.subspace {
        Some(pk) => {
            m.push(1);
            m.extend_from_slice(pk.as_bytes());
        }
        None => m.push(0),
    }
    for c in &area.path_prefix {
        m.extend_from_slice(&c.to_le_bytes());
    }
    m.push(0xff); // path terminator
    m.extend_from_slice(receiver.as_bytes());
    m
}

impl Capability {
    /// Root a communal capability: the subspace owner grants `initial_area`
    /// (whose subspace must be the owner) to `initial_receiver`.
    fn root(
        namespace: [u8; 32],
        mode: Mode,
        owner: &SigningKey,
        initial_area: Area,
        initial_receiver: VerifyingKey,
    ) -> Self {
        assert_eq!(
            initial_area.subspace,
            Some(owner.verifying_key()),
            "communal root: initial area's subspace must be the signing owner"
        );
        let msg = grant_msg(&namespace, mode, &initial_area, &initial_receiver);
        let root_signature = owner.sign(&msg);
        Self {
            namespace,
            mode,
            initial_area,
            initial_receiver,
            root_signature,
            delegations: Vec::new(),
        }
    }

    /// Delegate a narrower area to a new receiver. `holder` must be the current
    /// final receiver of the chain; `new_area` must be within the current area.
    /// Returns `Err` if the delegation would widen the area or the holder is wrong.
    fn delegate(
        &mut self,
        holder: &SigningKey,
        new_area: Area,
        new_receiver: VerifyingKey,
    ) -> Result<(), &'static str> {
        if !self.effective_area().includes_area(&new_area) {
            return Err("delegation must narrow, not widen, the area");
        }
        if holder.verifying_key() != self.current_holder() {
            return Err("only the current holder can delegate");
        }
        let msg = grant_msg(&self.namespace, self.mode, &new_area, &new_receiver);
        let signature = holder.sign(&msg);
        self.delegations.push(Delegation {
            area: new_area,
            receiver: new_receiver,
            signature,
        });
        Ok(())
    }

    fn effective_area(&self) -> &Area {
        self.delegations
            .last()
            .map(|d| &d.area)
            .unwrap_or(&self.initial_area)
    }

    fn current_holder(&self) -> VerifyingKey {
        self.delegations
            .last()
            .map(|d| d.receiver)
            .unwrap_or(self.initial_receiver)
    }

    /// Verify the full signed chain and monotonic restriction.
    fn is_valid(&self) -> bool {
        // Root: signed by the subspace owner (communal) over the initial grant.
        let Some(owner) = self.initial_area.subspace else {
            return false; // communal model requires a subspace owner
        };
        let root_msg = grant_msg(&self.namespace, self.mode, &self.initial_area, &self.initial_receiver);
        if owner.verify(&root_msg, &self.root_signature).is_err() {
            return false;
        }
        // Each delegation: signed by the prior holder, area within prior area.
        let mut prior_area = &self.initial_area;
        let mut prior_holder = self.initial_receiver;
        for d in &self.delegations {
            if !prior_area.includes_area(&d.area) {
                return false;
            }
            let msg = grant_msg(&self.namespace, self.mode, &d.area, &d.receiver);
            if prior_holder.verify(&msg, &d.signature).is_err() {
                return false;
            }
            prior_area = &d.area;
            prior_holder = d.receiver;
        }
        true
    }

    /// Does this capability authorize `mode` access to an entry by `author` at `path`?
    fn grants(&self, mode: Mode, author: &VerifyingKey, path: &ClusterPath) -> bool {
        self.mode == mode && self.is_valid() && self.effective_area().includes_entry(author, path)
    }
}

fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn main() {
    let moot: [u8; 32] = [0x11; 32]; // moot id (NamespaceId)

    // Members.
    let alice = key(1); // owns her subspace
    let bob = key(2);
    let carol = key(3);
    let alice_pk = alice.verifying_key();

    // Alice's nodes live at cluster-paths in the moot's graph decomposition.
    let n_3_7_1 = vec![3u32, 7, 1]; // cluster 3 / sub 7 / node 1
    let n_3_9_5 = vec![3u32, 9, 5]; // cluster 3 / sub 9 / node 5
    let n_4_1_1 = vec![4u32, 1, 1]; // cluster 4 (a different community)

    // Alice grants Bob READ over her whole cluster-3 neighborhood: area = (alice, [3]).
    let cap = Capability::root(
        moot,
        Mode::Read,
        &alice,
        Area { subspace: Some(alice_pk), path_prefix: vec![3] },
        bob.verifying_key(),
    );
    println!("cap valid: {}", cap.is_valid());
    println!("Bob can read alice@[3,7,1] (in cluster 3): {}", cap.grants(Mode::Read, &alice_pk, &n_3_7_1));
    println!("Bob can read alice@[3,9,5] (in cluster 3): {}", cap.grants(Mode::Read, &alice_pk, &n_3_9_5));
    println!("Bob can read alice@[4,1,1] (cluster 4):    {}", cap.grants(Mode::Read, &alice_pk, &n_4_1_1));
    assert!(cap.grants(Mode::Read, &alice_pk, &n_3_7_1));
    assert!(cap.grants(Mode::Read, &alice_pk, &n_3_9_5));
    assert!(!cap.grants(Mode::Read, &alice_pk, &n_4_1_1), "cluster 4 is outside the [3] grant");

    // Bob delegates a NARROWER area to Carol: just sub-cluster 3/7.
    let mut cap2 = Capability::root(
        moot,
        Mode::Read,
        &alice,
        Area { subspace: Some(alice_pk), path_prefix: vec![3] },
        bob.verifying_key(),
    );
    cap2.delegate(&bob, Area { subspace: Some(alice_pk), path_prefix: vec![3, 7] }, carol.verifying_key())
        .expect("valid narrowing delegation");
    println!("\nafter Bob -> Carol delegation narrowed to [3,7]:");
    println!("cap2 valid: {}", cap2.is_valid());
    println!("Carol can read alice@[3,7,1] (in 3/7): {}", cap2.grants(Mode::Read, &alice_pk, &n_3_7_1));
    println!("Carol can read alice@[3,9,5] (in 3/9): {}", cap2.grants(Mode::Read, &alice_pk, &n_3_9_5));
    assert!(cap2.grants(Mode::Read, &alice_pk, &n_3_7_1));
    assert!(!cap2.grants(Mode::Read, &alice_pk, &n_3_9_5), "3/9 is outside the narrowed [3,7] grant");

    // Forgery / widening attempts must fail.
    let mut forged = Capability::root(
        moot, Mode::Read, &alice,
        Area { subspace: Some(alice_pk), path_prefix: vec![3] },
        bob.verifying_key(),
    );
    // Bob tries to WIDEN to [] (whole subspace) — rejected by the monotonic-restriction rule.
    let widen = forged.delegate(&bob, Area { subspace: Some(alice_pk), path_prefix: vec![] }, carol.verifying_key());
    println!("\nwidening delegation rejected: {} ({})", widen.is_err(), widen.err().unwrap_or(""));
    assert!(forged.delegate(&bob, Area { subspace: Some(alice_pk), path_prefix: vec![] }, carol.verifying_key()).is_err());

    println!("\n--- Probe 2 VERDICT ---");
    println!("A Mere-native, Meadowcap-shaped capability over graph-cluster paths works:");
    println!("path-prefix containment makes 'share cluster 3' and 'narrow to sub 3/7' first-class,");
    println!("delegation is signed + monotonic (can only narrow), and forged/widening grants fail.");
    println!("This is the spatial-namespace benefit (share a coherent sub-graph neighborhood) that");
    println!("p2panda's FLAT topics cannot express: a topic is one 32-byte id, all-or-nothing, with");
    println!("no sub-scoping and no neighborhood structure.");
    println!("=> Own a willow-esque interpretation for the spatial namespace + cap layer (this is its");
    println!("   core). Use p2panda-net for transport/discovery; keep this layer Mere-native (or willow25).");
}
