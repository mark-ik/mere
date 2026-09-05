# External Dependency Topology & Rename Lineage

**2026-05-24.** Cross-cutting reference for the `Code/` workspace root (which
is not itself a git repo). Records the `crates/` ↔ `repos/` split, the
relative-path convention between them, the cross-repo rename lineage, and which
"broken" path-deps are intentional. Companion to the *mere-internal* supercrate
map in [`mere_docs/technical_architecture/2026-05-19_workspace_topology_status.md`](mere_docs/technical_architecture/2026-05-19_workspace_topology_status.md)
— that doc is about `repos/mere/crates/`; this one is about the workspace root.

> **Historical note (2026-09-05):** This is a May workspace-topology and rename
> snapshot. Preserve its path-dependency context, but use the current
> [`DOC_README`](DOC_README.md) and active repository-boundary plans for present
> ownership and layout.

## The `crates/` ↔ `repos/` split

Two sibling directories under `Code/`:

- **`Code/crates/`** — vendored upstream libraries Mark does **not** maintain
  (tracks upstream, carries minimal fork patches). Current set: `xilem`
  (plus `xilem-graft-seam`, `xilem-pr-masonry-defaults`,
  `xilem-pr-with-default-properties`), `imaging`, `nova`, `glass-gpui`, `blitz`,
  `weave`, `boa`, `piccolo`.
  - **`boa`** (added 2026-05-25; shallow @ `v0.21.1`, `genet` branch) — fork of the
    Boa JS engine. Carries an icu-family pin widen (`~2.0` → `^2.1`) so its
    `icu_normalizer` unifies with genet's parley/nova-forced icu 2.2.x; genet
    redirects `boa_engine`/`boa_gc` to it via `[patch.crates-io]` (same pattern as
    `nova`). Owned so it can later be restructured for weval-based AOT (the real reason
    to fork — the icu fix was free along the way). See
    [`genet/docs/2026-05-20_genet_script_engine_plan.md`](../../genet/docs/2026-05-20_genet_script_engine_plan.md).
  - **`piccolo`** (noted 2026-06-11; v0.3.3, MIT) — fork of kyren's stackless Lua
    VM. Two intended dividends: a `ScriptEngine` seam backend (the modding-Lua
    *option*, not a third first-party substrate — the Rust+JS decision stands)
    and gc-arena technique/dependency for the `genet-scripted-dom` refit.
    Consumes `gc-arena` as a git dep pinned to kyren's `5a7534b`; when genet
    takes gc-arena directly, align both on one deliberate workspace pin. Plan:
    [`genet/docs/2026-06-11_gc_arena_dom_plan.md`](../../genet/docs/2026-06-11_gc_arena_dom_plan.md).
- **`Code/repos/`** — Mark's own projects: `mere`, `genet`, `netrender`,
  `netfetcher`, `errand`, `strophe`, `woodshed`, `wgpu-graft`, `wgpu-scry`,
  `wgpu-weld`. (**`graphshell`** was here until 2026-05-27, when it was
  GitHub-archived and the local clone deleted — see the [donor-repo code
  salvage map](mere_docs/research/2026-05-27_donor_graphshell_repo_salvage_map.md)
  and [full docs harvest](mere_docs/research/2026-05-27_graphshell_docs_full_harvest.md).)
  (**`netfetcher`** added 2026-05-25 — a scaffold; the portable
  WHATWG-Fetch network engine. Mere owns it; genet/consumers receive bytes. Plan:
  [`mere_docs/.../2026-05-25_netfetcher_plan.md`](mere_docs/implementation_strategy/2026-05-25_netfetcher_plan.md).)

Anything "essentially a library I don't maintain" moves into `crates/`. The
`xilem` fork (branch `mere-wgpu-29-vello-0-9`, rebased onto the `mark-ik/xilem`
fork's main) lives there alongside the `imaging` fork it depends on.

## Cross-repo smoke (added 2026-06-12)

The lattice has no CI, so a `git pull` (or an agent edit) in any sibling can
break the others silently — the audit's standing "no net under the
five-checkout lattice" gap. The net is
[`repos/mere/scripts/cross-repo-smoke.ps1`](../scripts/cross-repo-smoke.ps1):
targeted `cargo check`s of the load-bearing crates in dependency order,
innermost first (netrender → netfetcher → genet components → pelt → orrery →
meerkat), so a failure names the repo that introduced it; `-Tests` adds the
fast lib suites, `-KeepGoing` collects all failures; logs land in
`target/smoke/` (gitignored). Run it after pulling or landing cross-repo
work. First run 2026-06-12: green across the lattice, ~3 minutes warm.

## Local Cargo config (added 2026-06-29, corrected 2026-08-01)

Local overrides are repo-scoped. Do not put `paths = [...]` overrides in
`Code/.cargo/config.toml`; Cargo inherits that parent config into every repo
under `Code/`, so a package-name match can affect unrelated workspaces. Mere's
ignored `repos/mere/.cargo/config.toml` owns its local edit loop with
source-specific `[patch."https://github.com/merely-made/<repo>.git"]` entries
for genet, netrender, wgpu-scry, wgpu-graft, and retinue.

**The patch table's URL must match the dependency's URL exactly, owner
included.** A table naming a different owner is not an error; Cargo simply does
not apply it and the build silently resolves from git instead of the local
checkout. This has already cost real time twice, so it is worth stating: the
owner moved `mark-ik` → `merely-made`, and on 2026-08-01
`ports/graphshell/web/.cargo/config.toml` was still naming `mark-ik` for both
genet and netrender, so every patch in it had been inert and the committed
103 MB wasm artifact was built against the remotes rather than the sibling
checkouts. Cargo does say so, as `patch ... was not used in the crate graph`.
Read that warning; `scripts/cross-repo-smoke.ps1` now fails on it.

Note that these configs are gitignored, so a correction on one machine does not
reach the others. Check each machine's own copy.

The Meerkat runner scripts that this section used to describe
(`scripts/meerkat.ps1` and its wrappers) were removed on 2026-08-01 with the
app itself; `scripts/cross-repo-smoke.ps1` is the surviving local net.

## Path-dep convention (`repos/` → `crates/`)

Because `crates/` and `repos/` are siblings, a path-dep from a repo into a
vendored crate ascends to `Code/` then descends into `crates/`. From a repo
root that is `"../../crates/<lib>/<sub>"`; deeper crates add one `../` per level.
The 2026-05-24 reorg sweep repointed all stale `../<lib>/…` deps (which had
pointed at a now-absent `repos/<lib>/`) across `woodshed`, `strophe`, `genet`,
and `mere`. See the sweep transform: *add one `../`, insert `crates/`*.

## Cross-repo rename lineage

graphshell's old sibling-repo deps map to current homes:

| Old name (graphshell-era) | Current home |
| --- | --- |
| `servo-wgpu` | `repos/genet` |
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
- **`genet`** carries servo-stack path-deps (`html5ever`, `mozjs`, `stylo`,
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
  the same file. **Enabled repo-wide** as of 2026-05-24, and **completed
  2026-07-24**: every repo in `repos/` (genet, hocket, isometry, mere,
  turnstone, merely-made.github, netrender, retinue, smolweb, wavicle,
  wgpu-graft, wgpu-scry, wgpu-weld, woodshed — graphshell is archived) has a
  committed `.gitattributes` mapping ~46-54 file types (`*.rs`, `*.toml`,
  `*.md`, …) to `merge=weave`. `retinue` was cloned after the initial sweep
  and wired the same day once it appeared.
- **`sem`** — semantic version control (entity-level diff / listing / context /
  impact queries on top of Git). **Fully adopted 2026-07-24**, superseding the
  original ad-hoc-npx posture below: installed via `cargo install --git
  https://github.com/Ataraxy-Labs/sem sem-cli` and registered as a
  user-scoped Claude Code MCP server (`claude mcp add sem -s user -- sem
  mcp`), exposing `sem_diff`/`sem_context`/`sem_impact`/`sem_entities`/
  `sem_blame`/`sem_log` as native tools in every session, not just via CLI.

**Update 2026-07-24 — fresh-clone gap closed.** The prerequisite recorded
below (driver definition living in per-repo local `.git/config`, so fresh
clones silently fell back to plain git merge) is fixed on this machine: the
driver is now set via `git config --global merge.weave.driver "weave-driver
%O %A %B %L %P"` (and `merge.weave.name`), which covers every repo,
including future fresh clones, without any per-repo step. The committed
`.gitattributes` still travels with each clone as before. The one remaining
gap is genuinely unavoidable: a **new machine** still needs `weave-driver`
(and `sem`) installed and the global git config set once — see each repo's
`CLAUDE.md` ("Workspace Tooling: sem & weave" section) for the install
commands. Per-repo local `.git/config` driver entries from the original
rollout are still present in the already-wired repos; they're redundant with
the global config now but harmless (local overrides global with an identical
value).

**Original non-evident prerequisite (superseded by the update above, kept for
history):** the `merge=weave` attribute is committed, but the driver
definition is **local `.git/config` per clone** —
`merge.weave.driver = ~/.cargo/bin/weave-driver %O %A %B %L %P` — and depends on
`weave-driver` being installed (`~/.cargo/bin/`, via `crates/weave`). On a fresh
clone or a machine without it installed, `merge=weave` is a dangling pointer and
git **silently falls back** to its default merge. New clones need the driver
installed + the local config set before weave actually engages.

Origin / first evaluation: [`genet/docs/2026-05-23_sem_weave_smoke_test.md`](../../genet/docs/2026-05-23_sem_weave_smoke_test.md).
