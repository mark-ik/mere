# AccessKit Screen Reader Verification

Status: manual verification checklist for the D6 OS AccessKit bridge.

This is not a substitute for the harness tests. The harness proves the semantic
application route table and typed action spine. This checklist proves that the
platform adapter exposes the same tree to the OS assistive technology stack.

## Scope

- App: `meerkat`
- Surface: host window, chrome root, frame panes, orrery graph links, gloss links,
  and roster rows.
- Expected action path: screen-reader `Focus` or `Click` -> AccessKit
  `ActionRequest` -> Meerkat route table -> semantic node selection ->
  `meerkat.agent.action_applied` diagnostic.
- Expected degraded path: unsupported or stale target -> no crash and
  `meerkat.agent.intent_dropped` diagnostic.

## Preflight

1. Build and run the app:

   ```shell
   cargo run -p meerkat
   ```

2. Confirm the Apparatus/System diagnostics contain:

   ```text
   a11y_bridge: installed
   ```

   If the record is `degraded`, capture the exact reason and stop the platform
   pass. The internal tree and agent harness may still be healthy, but the OS
   adapter is not live for this run.

3. Open at least one non-welcome URL so the orrery has a second semantic graph
   link target:

   ```text
   https://example.test
   ```

## Windows - Narrator

1. Start Narrator with `Ctrl+Win+Enter`.
2. Focus the Meerkat window.
3. Traverse by item from the window root into the toolbar, frame area, active
   pane, and graph links.
4. Expected names:
   - Window announces `Meerkat`.
   - Chrome and frame roots are traversable as named groups/panes.
   - Graph links expose their URL labels.
   - Roster rows expose the graph member title and selected state.
5. Activate a graph link or roster row.
6. Expected result:
   - Orrery selection changes to that URL.
   - Apparatus diagnostics records `meerkat.agent.action_applied`.
   - No duplicate navigation or coordinate-only click path is required.

## macOS - VoiceOver

1. Start VoiceOver with `Cmd+F5`.
2. Focus the Meerkat window.
3. Traverse the window and interact with the frame area.
4. Expected names and activation behavior match the Windows pass.
5. If VoiceOver cannot enter the window tree, capture whether the bridge probe
   said `installed`; this separates adapter installation from OS traversal.

## Linux - Orca / AT-SPI

1. Start Orca.
2. Run Meerkat in a desktop session with AT-SPI enabled.
3. Focus the Meerkat window and traverse from the window root.
4. Expected names and activation behavior match the Windows pass.
5. Under Wayland, window bounds may be less precise than X11; this is acceptable
   for the first D6 pass if traversal and semantic activation work.

## Recording Results

Append a dated result block to this file:

```text
YYYY-MM-DD platform/screen-reader:
- bridge probe:
- traversal:
- names:
- focus/click action:
- diagnostics:
- failures:
```

## First Result

Pending manual run.
