# chrome

Graphshell domain module for the [mere](https://crates.io/crates/mere)
browser — the user-facing UX surface of the application's chrome.

In VS Code terms: the title bar row with File / Edit / View menus,
navigation arrows, the address/omnibar, layout switcher buttons. In
mere terms: anything that's *the application's outer interface*
between the user and the system.

The crate is the home for the **UX-concept slice** — view-models for
toolbar, omnibar, command palette, focus authorities, etc. The
`shell-state` crate re-exports each module from its original path so
existing call sites (`shell_state::toolbar`) keep resolving while
consumers gradually switch to direct `chrome::toolbar` imports.

## Modules

- `authorities` — focus authority view-models.
- `command_palette` — command palette state.
- `frame_model` — frame view-model + host-input types.
- `host_intent` — host→runtime intent shape.
- `omnibar` — address bar / omnibar.
- `routing` — return-target routing for tool surfaces.
- `toolbar` — root toolbar view-model.

## Status

Pre-1.0. Eventual home for `project_chrome(state) -> UxTree` once
the UX surface is wired through (root toolbar projection).
