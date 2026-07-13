# Eidetic onto muniment: delete the duplicated storage seam

**Date:** 2026-07-12
**Status:** **LANDED 2026-07-12** (mere d9cd0c6). All done conditions met
except the meerkat receipt, which waits on serval's in-flight ring-3 rename
(see Progress).

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

## Progress

**2026-07-12 — landed** (mere d9cd0c6). Receipts: eidetic 84,
eidetic-fjall 7, session-runtime 188, fetch 3, embed 23 — green;
eidetic-search / https-fetcher / iroh-fetcher check clean. `grep` finds no
`trait Store` defined outside muniment.

Findings the plan under-called:

- **muniment's `Backend` is richer than the plan said**: six methods, not
  four (`scan` = ordered range, `apply` = atomic batch), and on native it is
  `Send`-bounded where eidetic's `Store` was `?Send`. Both were free: fjall's
  LSM is key-ordered (a native `scan`) and its handles are `&self`-safe.
- **`delete`'s return type is a real semantic change.** muniment's is
  idempotent and returns `()`; eidetic's returned "was it present?". Three
  functions depend on that answer — `delete_manifest` (age-out counts
  evictions), `evict_content`, `delete_image` (the orphan sweep counts
  reclamations) — and now probe with `get` first. That is one extra read per
  delete on those paths; if it ever matters, muniment could grow a
  `delete_if_present`, but the honest cost is recorded rather than hidden.
- **Error vocabulary is muniment's now.** `StoreError` flows into
  `eidetic::Error` through `From`, so `?` works everywhere, but messages gain
  the variant prefix ("backend: disk on fire"). The propagation test asserts
  the new truth.
- **11 hand-rolled in-memory test stores** collapsed into
  `muniment::MemoryBackend`. Every one was the same `HashMap` behind the same
  seam — the duplication was not just the trait.

Not verified: **meerkat**. Its cookie store's four renamed lines are
byte-identical to `system/fetch`'s (green), but the crate cannot build while
serval's ring-3 fork rename (stylo -> `serval-stylo`) leaves mere's serval
cone unresolvable. Run `cargo test -p meerkat` once that lands. Also
pre-existing and unrelated: `eidetic-search`'s example fails a `TypedPayload`
bound (broken before this change; verified by stashing).

## Done conditions

1. ~~`eidetic::Store` is gone; eidetic-core deps muniment; no `Store` trait
   defined outside muniment.~~ **DONE** (it survives as an alias for
   `muniment::Backend`, so the layers above read unchanged).
2. ~~eidetic-fjall is a `muniment::Backend` impl~~ **DONE** — it is now a
   backend the whole family can reuse. (mooting adopting it is a follow-on:
   it rides muniment already, so it is a manifest change, not a port.)
3. ~~eidetic's suite green~~ **DONE** (84 + 7 + 188 + 3 + 23). meerkat's
   suite: **pending the serval ring-3 rename**, see Progress.
4. The boundary-pass plan's point 5 stamped done with the
   "not-a-prohibition" correction: **pending** (a one-line edit there).
