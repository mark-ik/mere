# Crypto Stack: RustCrypto + dalek, One Generation

**Date:** 2026-08-10
**Kind:** technical architecture (the decision that was never written)
**Status:** decided by Mark 2026-08-10 ("we must unify on the newest version")

## Why this exists

Asked how the stack was chosen, the honest answer was: it was not. A search of
mere, retinue, and genet found no crypto-library decision document. The stack
arrived through three inheritances and one comment:

- **iroh and p2panda** pin the dalek family, so consuming them pins it too.
- **Reticulum's spec** dictates retinue's primitives (X25519, Ed25519, AES,
  HKDF, HMAC-SHA256); retinue implements framing, never primitives.
- **Cable** brought BLAKE2b, later dropped for BLAKE3 on wire-format grounds.
- The only written rationale is `MURM_AS_BILATERAL.md` open question #6,
  "pure Rust, for consistency with the rest of Mere," recorded as an *open*
  question and closed silently by a comment in `identity/lib.rs`.

Alternatives were compared twice, both narrowly: sodiumoxide vs dalek for murm
(deferred, never decided in a doc), and ring vs rustls-rustcrypto vs aws-lc in
genet, which was **TLS provider only** and decided on build-environment cost.
No document anywhere discussed audit status, provenance, or maintenance risk.

## The decision

**RustCrypto for primitives, dalek for the curve25519 family, one generation
across the workspace.**

The library question is largely foreclosed, and saying so is more useful than
pretending it is open. Iroh, p2panda, and Reticulum-compatibility all pull
dalek and RustCrypto regardless; switching wholesale means forking them. Of
the alternatives: `ring` is BoringSSL-derived with prebuilt assembly, `aws-lc-rs`
reintroduces the NASM build requirement genet deliberately removed, and
`sodiumoxide` is unmaintained. None is worth a fork of the p2p stack.

So the real decision is not *which library* but *which generation, unified* —
and that one was live, because the workspace was carrying two at once.

### What "one generation" means

RustCrypto moves as a family, keyed on the `digest` major:

| `digest` | goes with |
|---|---|
| 0.10 | sha2 0.10, sha1 0.10, hmac 0.12, hkdf 0.12, chacha20poly1305 0.10 |
| 0.11 | sha2 0.11, sha1 0.11, hmac 0.13, hkdf 0.13, chacha20poly1305 0.11 |

Mixing across the row does not compile: `hmac` 0.13 needs a `digest` 0.11
hash. So a bump is never one crate, it is the row.

### The state that forced the call (measured 2026-08-10)

Both rows were resolved in the lockfile at once:

| Row | Pulled by |
|---|---|
| sha2 0.11 / hmac 0.13 / digest 0.11 | **retinue**, ed25519-dalek 3.0.0, hpke-rs |
| sha2 0.10 / hmac 0.12 / digest 0.10 | graphshell, knot, mere-transport, misfin, p2panda-encryption, castellan |

`ed25519-dalek` was in the graph at **both 2.2.0 and 3.0.0**. Retinue had
already moved to the newer row unilaterally; mere had not. The 2026-07-04
dependency footprint brief had already named this as one migration unit and it
was never executed.

Carrying both rows costs compile time, binary size, and two implementations of
the same primitive in one process. It also means "which SHA-256 is this?" has
an answer that depends on the call site, which is precisely the kind of
question a crypto stack should never make interesting.

## Rules going forward

1. **Bump the row, not the crate.** Changing `sha2` means changing `hmac`,
   `hkdf`, `sha1`, and `chacha20poly1305` in the same commit.
2. **Pin the row in `[workspace.dependencies]`**, so a new crate cannot join
   the wrong one by accident. That is how castellan landed on the old row: it
   took `sha2.workspace = true` and matched `hmac` to it, correctly, against a
   stale pin.
3. **Primitives are never implemented here.** Framing, envelopes, and token
   formats are ours; the maths is not. Retinue's founding plan already says
   this and it generalizes.
4. **Constant-time comparison is explicit.** `subtle`, or `Mac::verify_slice`,
   never `==` on a secret and never a short-circuiting `find`. Retinue's wire
   reference caught exactly this bug in an upstream crate.
5. **A transitive dep on the other row is tolerable; a first-party one is
   not.** We do not control what p2panda pins. We do control our own manifests.

## Known residue

- **`ed25519-dalek` 2.2.0 remains** via transitive pins we do not own
  (p2panda's fork line, misfin). Unifying it is a separate exercise against
  the sibling manifests, and the archived 2026-07-15 iroh/p2panda bump plan is
  the record of how much work that is.
- **`argon2` stays at 0.5.3**: 0.6 is release-candidate only, and a
  password-hash change is a stored-format change, so it waits for a stable
  release and its own migration note.
- **`signature` 2 to 3** rides with whatever consumes it rather than moving on
  its own.

## Related

- The migration itself: `../implementation_strategy/2026-08-10_crypto_generation_unification_plan.md`
- Prior art on the version seam: `../../2026-07-04_dependency_footprint_brief.md`
