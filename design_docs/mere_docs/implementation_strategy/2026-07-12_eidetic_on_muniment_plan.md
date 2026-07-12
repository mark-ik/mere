# Eidetic onto muniment: delete the duplicated storage seam

**Date:** 2026-07-12
**Status:** plan, endorsed (Mark, 2026-07-12: "let's update eidetic to use
muniment"). Not started — blocked on a green tree (see Sequencing).

Completes the boundary-pass plan's point 5, which said the eventual move is
"the inverse: rebase eidetic's backends onto muniment's seam (the way
`mooting` already rides muniment)."

## The finding

Eidetic's `Store` (eidetic-core/src/lib.rs) and muniment's `Backend`
(muniment/src/backend.rs) are **the same trait, declared twice**:

| | eidetic `Store` | muniment `Backend` |
| --- | --- | --- |
| read | `load_blob(&mut self, key) -> Option<Vec<u8>>` | `get(&self, key) -> Option<Vec<u8>>` |
| write | `save_blob(&mut self, key, &[u8])` | `put(&self, key, &[u8])` |
| delete | `delete_blob` (default: `Err`) | `delete` (required) |
| list | `iter_keys(prefix)` (default: `Err`) | `list(prefix)` (required) |
| async | `#[async_trait(?Send)]` | same |

Two real differences, both in muniment's favour: it takes `&self` (not
`&mut self`), and delete/list are required rather than
`Err`-by-default — eidetic's defaults exist only because it had no way to
say "a backend must enumerate," and its own doc comment admits every real
backend can.

Above the byte layer they do **not** overlap. Muniment adds `SlotStore`
(typed mutable slots over a pluggable codec) and `BlobStore`
(content-addressed immutable bytes) — a storage floor. Eidetic adds
manifests, schemas, sealed payloads, typed engrams, quota/age-out policy,
remote blob fetchers, and a search index — a memory system. Muniment is the
floor; eidetic is a building standing on a floor it poured itself before
muniment existed.

## The correction this records

The boundary pass's "eidetic stays mere-side, over muniment" decision has
been mis-cited (by Claude, 2026-07-12) as "eidetic must not depend on
muniment" — used to argue against folding stemma into chartulary, since
eidetic → chartulary → muniment would thread the seam. **That reading is
wrong.** The decision refused *promoting eidetic into a competing storage
sibling*; it explicitly named this rebase as the intended destination. No
seam is forbidden. (The stemma fold went ahead on its own merits.)

## The change

1. **eidetic-core deps muniment**; its `Store` trait is deleted and every
   `Store` bound becomes `muniment::Backend`.
2. **Method renames at the call sites**: `load_blob`/`save_blob`/
   `delete_blob`/`iter_keys` → `get`/`put`/`delete`/`list`. Mechanical, but
   ~107 references across the workspace (eidetic-core, eidetic-fjall,
   eidetic-search, eidetic-iroh-fetcher, embed persistence, meerkat).
3. **`&mut self` → `&self`**: eidetic's stores are held behind `&mut` today;
   muniment's shared-reference backends should *simplify* call sites (fewer
   `RefCell`/`&mut` threading), but each site needs a look.
4. **eidetic-fjall becomes a muniment backend** — that is the prize: one
   fjall backend every family crate (eidetic, mooting, future stores) can
   reuse, instead of one per subsystem.
5. **eidetic keeps everything above the seam.** Manifests, schemas, sealing,
   quotas, engrams: untouched. This is a floor swap, not a redesign.

Not in scope: moving eidetic to a sibling repo (the boundary pass refused
that, and this rebase does not change it), and folding eidetic's
`BlobFetcher` (https/iroh) into muniment — that is a fetch concern, not
storage.

## Sequencing

**Blocked on a green mere tree.** As of 2026-07-12 mere's workspace does not
resolve: serval's in-flight ring-3 fork rename (stylo → `serval-stylo` on
the `serval-publish-names` branch) puts two crates linking
`servo_style_crate` in the graph. A 107-site trait swap without a compiler
is not a thing to attempt; start when `cargo check -p eidetic` runs.

## Done conditions

1. `eidetic::Store` is gone; eidetic-core deps muniment; the workspace
   compiles with no `Store` trait defined outside muniment.
2. eidetic-fjall is a `muniment::Backend` impl, and mooting/eidetic share it.
3. eidetic's test suite green; meerkat's suite green (its recall paths are
   the biggest consumer).
4. The boundary-pass plan's point 5 is stamped done, with the
   "not-a-prohibition" correction recorded there too.
