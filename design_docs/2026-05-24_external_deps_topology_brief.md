# External Dependency Topology & Rename Lineage

**2026-05-24.** Cross-cutting reference for the `Code/` workspace root (which
is not itself a git repo). Records the `crates/` ↔ `repos/` split, the
relative-path convention between them, the cross-repo rename lineage, and which
"broken" path-deps are intentional. Companion to the *mere-internal* supercrate
map in [`mere_docs/technical_architecture/2026-05-19_workspace_topology_status.md`](mere_docs/technical_architecture/2026-05-19_workspace_topology_status.md)
— that doc is about `repos/mere/crates/`; this one is about the workspace root.

## The `crates/` ↔ `repos/` split

Two sibling directories under `Code/`:

- **`Code/crates/`** — vendored upstream libraries Mark does **not** maintain
  (tracks upstream, carries minimal fork patches). Current set: `xilem`
  (plus `xilem-graft-seam`, `xilem-pr-masonry-defaults`,
  `xilem-pr-with-default-properties`), `imaging`, `nova`, `glass-gpui`, `blitz`,
  `weave`, `boa`.
  - **`boa`** (added 2026-05-25; shallow @ `v0.21.1`, `serval` branch) — fork of the
    Boa JS engine. Carries an icu-family pin widen (`~2.0` → `^2.1`) so its
    `icu_normalizer` unifies with serval's parley/nova-forced icu 2.2.x; serval
    redirects `boa_engine`/`boa_gc` to it via `[patch.crates-io]` (same pattern as
    `nova`). Owned so it can later be restructured for weval-based AOT (the real reason
    to fork — the icu fix was free along the way). See
    [`serval/docs/2026-05-20_serval_script_engine_plan.md`](../../serval/docs/2026-05-20_serval_script_engine_plan.md).
- **`Code/repos/`** — Mark's own projects: `mere`, `serval`, `netrender`,
  `netfetcher`, `errand`, `strophe`, `woodshed`, `wgpu-graft`, `wgpu-scry`,
  `wgpu-weld`. (**`graphshell`** was here until 2026-05-27, when it was
  GitHub-archived and the local clone deleted — see the [donor-repo code
  salvage map](mere_docs/research/2026-05-27_donor_graphshell_repo_salvage_map.md)
  and [full docs harvest](mere_docs/research/2026-05-27_graphshell_docs_full_harvest.md).)
  (**`netfetcher`** added 2026-05-25 — a scaffold; the portable
  WHATWG-Fetch network engine. Mere owns it; serval/consumers receive bytes. Plan:
  [`mere_docs/.../2026-05-25_netfetcher_plan.md`](mere_docs/implementation_strategy/2026-05-25_netfetcher_plan.md).)

Anything "essentially a library I don't maintain" moves into `crates/`. The
`xilem` fork (branch `mere-wgpu-29-vello-0-9`, rebased onto the `mark-ik/xilem`
fork's main) lives there alongside the `imaging` fork it depends on.

## Path-dep convention (`repos/` → `crates/`)

Because `crates/` and `repos/` are siblings, a path-dep from a repo into a
vendored crate ascends to `Code/` then descends into `crates/`. From a repo
root that is `"../../crates/<lib>/<sub>"`; deeper crates add one `../` per level.
The 2026-05-24 reorg sweep repointed all stale `../<lib>/…` deps (which had
pointed at a now-absent `repos/<lib>/`) across `woodshed`, `strophe`, `serval`,
and `mere`. See the sweep transform: *add one `../`, insert `crates/`*.

## Cross-repo rename lineage

graphshell's old sibling-repo deps map to current homes:

| Old name (graphshell-era) | Current home |
| --- | --- |
| `servo-wgpu` | `repos/serval` |
| `webrender-wgpu` | `repos/netrender` |
| `wgpu-graft/wgpu-native-texture-interop` | `repos/wgpu-graft/grafting` |

## Intentionally-broken path-deps (do not "fix")

A repo-wide path sweep flags these as broken; they are **expected**:

- **`graphshell`** — **archived 2026-05-27** (read-only at
  <https://github.com/mark-ik/graphshell>; local clone deleted). It was
  archive-bound: its root `Cargo.toml` no longer built as-is, and its
  `graph-memory` / `graph-cartography` / `graphshell-core` / `graphshell-runtime`
  members had been donor-superseded (roles absorbed by mere's `node-lineage`,
  `orrery/cartography`, `graph/graph-kernel`, `system/session-runtime`). Its remaining salvage was
  pulled into mere (engines, `crates/import`, the `register-*` cluster, `murm`
  misfin/webfinger) and its 633 design docs were harvested before archiving.
  No longer in `repos/`; the path sweep no longer applies to it.
- **`serval`** carries servo-stack path-deps (`html5ever`, `mozjs`, `stylo`,
  `rust-content-security-policy`) that are being incorporated **piece by
  piece** into the architecture. The dangling paths are deliberate WIP, not
  reorg breakage.

The archived graphshell remains a donor / grab-bag of prior thinking — cite the
GitHub archive for ideas, never treat as prescriptive; mere-kernel is canonical.

## Workspace tooling: sem & weave (Ataraxy Labs)

Two agent-native dev tools are in use across `repos/`. Both are
**non-authoritative** — they understand structural entities (via tree-sitter),
not program semantics or project invariants, and **never replace compile/test
validation**.

- **`weave`** — entity-level semantic git **merge driver**. Resolves false
  conflicts where independent edits touch different functions/structs/keys in
  the same file. **Enabled repo-wide** as of 2026-05-24: every repo (mere,
  serval, netrender, strophe, woodshed, graphshell, wgpu-graft/scry/weld) has a
  committed `.gitattributes` mapping ~50 file types (`*.rs`, `*.toml`, `*.md`,
  …) to `merge=weave`.
- **`sem`** — semantic version control (entity-level diff / listing / context /
  impact queries on top of Git). Run ad-hoc via `npx @ataraxy-labs/sem …` or
  the binary; **not committed-wired** into any repo.

**Non-evident prerequisite (the reason this is recorded):** the `merge=weave`
attribute is committed, but the driver definition is **local `.git/config` per
clone** —
`merge.weave.driver = ~/.cargo/bin/weave-driver %O %A %B %L %P` — and depends on
`weave-driver` being installed (`~/.cargo/bin/`, via `crates/weave`). On a fresh
clone or a machine without it installed, `merge=weave` is a dangling pointer and
git **silently falls back** to its default merge. New clones need the driver
installed + the local config set before weave actually engages.

Origin / first evaluation: [`serval/docs/2026-05-23_sem_weave_smoke_test.md`](../../serval/docs/2026-05-23_sem_weave_smoke_test.md).
