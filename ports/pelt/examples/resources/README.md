<!--
Copyright 2026 Mark Alan Boykin
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
SPDX-License-Identifier: MPL-2.0
-->

# resources

Test fixture assets that Pelt's example documents reference **by repository-root
relative path**, carried here with Pelt when it landed from genet 2026-09-03
(`design_docs/mere_docs/implementation_strategy/2026-09-02_platform_boundary_and_repository_topology_plan.md`).

They are here rather than under `ports/pelt/` because the authored URLs in the
fixtures resolve to the repository root, and those exact strings are what
`ports/pelt/desktop/static_viewer.rs` asserts on: the tests exercise relative
URL resolution across four, five and six `..` segments, so moving the files
would mean rewriting the very strings under test.

| file | referenced by |
|---|---|
| `servo_64.png` | `ports/pelt/examples/livery-route/` (the `article` product receipt) and `ports/pelt/examples/p5-resources/` |
| `servo_1024.png` | `ports/pelt/examples/livery-scripted-route/index.html` |

Both are byte-identical copies of genet's `resources/`, which is Servo-derived.
They are fixture inputs, not shipped product assets.
