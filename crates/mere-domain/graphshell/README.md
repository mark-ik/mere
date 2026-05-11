# mere-graphshell

Graphshell domain module for the [mere](https://crates.io/crates/mere)
browser — the user-facing UX surface of the application's chrome.

In VS Code terms: the title bar row with File / Edit / View menus,
navigation arrows, the address/omnibar, layout switcher buttons. In
mere terms: anything that's *the application's outer interface*
between the user and the system.

This crate is **narrower than the legacy `crates/graphshell/`** crate,
which bundled session state, runtime traits, host adapters, and the
shell itself. mere-graphshell is the new home for the **UX-concept
slice** — view-models for toolbar, omnibar, command palette, focus
authorities, etc. — extracted incrementally from
`graphshell-shell-state` as the migration progresses.

## Migration progress

| Slice | Source | Status |
| --- | --- | --- |
| toolbar (location bar, drafts, viewer status) | `graphshell-shell-state::toolbar` | ✅ landed (first slice, 2026-05-09) |
| omnibar | `graphshell-shell-state::omnibar` | pending |
| command palette | `graphshell-shell-state::command_palette` | pending |
| focus authorities | `graphshell-shell-state::authorities` | pending |
| graph search | `graphshell-shell-state::frame_model::GraphSearchViewModel` | pending |

`graphshell-shell-state` re-exports each migrated module from its
original path so existing call sites (`graphshell_shell_state::toolbar`)
keep resolving while consumers gradually switch to direct
`mere_graphshell::toolbar` imports.

## Status

Pre-1.0. Toolbar lane shipped as the first slice; other lanes follow
as they're extracted. Eventual home for `project_graphshell(state) ->
UxTree` once the UX surface is wired through (root toolbar
projection).
