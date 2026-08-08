# uxtree

Portable accessibility / automation tree for the
[mere](https://crates.io/crates/mere) browser. Projects portable structural
elements into [AccessKit](https://accesskit.dev) 0.24 nodes with stable,
deterministic ids.

The same projection feeds screen readers through AccessKit's platform adapters
(`accesskit_windows`, `accesskit_unix`, `accesskit_macos`), automation tooling
that pins to stable identity, and inspector overlays.

## API

| Item | Role |
| --- | --- |
| `UxTree` | `root: NodeId`, `nodes: Vec<(NodeId, Node)>`. The root is pushed last, after all descendants. |
| `UxTree::to_tree_update(focus: Option<NodeId>) -> accesskit::TreeUpdate` | Wraps the nodes for a platform adapter. `None` focus defaults to the root. |
| `node_id_for_path(&str) -> NodeId` | Hashes a domain path to a stable id. |
| `stitch(root_path: &str, root: Node, subtrees: Vec<UxTree>) -> UxTree` | Merges domain subtrees (workbench, gloss, apparatus, ...) under one application root, overwriting the root's children. |
| `project_document(&inker::EngineDocument) -> UxTree` | Projects a document into a `Role::Document` root. |
| `VERSION`, `STAGE` | Crate version string and lifecycle marker (`"pre-alpha"`). |

## Stable ids

`node_id_for_path` hashes the path with `std::collections::hash_map::DefaultHasher`
and wraps the `u64` as `accesskit::NodeId`. The same path always yields the same
id, so automation and snapshot tests can pin a node without relying on bounds or
render order.

Paths built by `project_document`:

| Node | Path |
| --- | --- |
| Document root | `engine:{address}` |
| Block `i` | `engine:{address}#blocks/{i}` |
| Nested quote block | `{parent}/quote/{i}` |
| List item | `{parent}/list/{i}` |
| Block inside a list item | `{parent}/list/{i}/item/{j}` |
| Link in a paragraph | `{parent}/paragraph-link/{n}` |
| Link in a heading | `{parent}/heading-link/{n}` |

## Block role mapping

| `inker::Block` | AccessKit role |
| --- | --- |
| `Heading` | `Heading` (with `set_level`) |
| `Paragraph` | `Paragraph` |
| `List` | `List`, children `ListItem`; ordered lists get `description = "ordered"` |
| `Quote` | `Blockquote`, children projected recursively |
| `CodeBlock` | `Code` (language recorded in `description`) |
| `Preformatted` | `Code` |
| `Table` | `Table`, cells flattened into the label |
| `Image` | `Image` (alt as label, url as value) |
| `Rule` | `Splitter` |
| `FeedHeader` | `Section` |
| `FeedEntry` | `Article` |
| `MetadataRow` | `Group` (label / value) |
| `Badge` | `Note` |

Inline spans flatten into the enclosing block's accessible name. `InlineSpan::Link`
is the exception: each link becomes an addressable `Role::Link` child carrying the
url as its value.

## Scope

Bounds come from rendering; the host calls `Node::set_bounds` after layout. Input
and event synthesis stay with the host. Dependencies are `accesskit`, `inker`
(git, genet `main`), and `tracing`.

## Status

Pre-1.0. Document blocks and inline links are covered. Mere structural elements
project through the domain crates that sit on top of this one (`workbench`,
`mere-gloss`, `mere-apparatus`), each emitting its own subtree for `stitch`.
