# Local Genet development

Cambium landed in mere on 2026-09-03 (the platform boundary plan's P2). Its
manifests now name the Genet seam crates through mere's root
`[workspace.dependencies]`, which pins `genet.git` at one revision for the
whole repository — not crates.io, and not a relative path into a sibling
checkout. Repointing them is a one-file edit at mere's root.

To test unpublished Genet seam changes, redirect that git source to a local
Genet checkout in the uncommitted `.cargo/config.toml` at mere's root; copy
`.cargo/config.toml.example` and edit from there. Two standing rules that file
records, both learned the hard way:

- a patch table that redirects a git source must name **every** package the
  graph pulls from it, or half the family resolves from the git checkout and
  the other half from the working copy, and one crate present twice is a type
  error rather than a resolution failure; and
- no workspace member may appear in it. Patching a git or crates.io source at
  a path that is also a workspace member is a hard lockfile collision, so
  `cambium`, `sprigging`, `workbench`, `mere-surface-api` and the rest of the
  family that landed with them are permanently out of that table.

A change that requires the redirect is ready to publish only after the matching
Genet seam release is on the pinned revision.
