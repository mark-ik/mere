# Doc audit protocol — mere design_docs, judgment pass (D2)

> Preserved 2026-09-06 with the original 281-document aggregate. The pinned
> paths below describe the 2026-09-02 run; supplemental batches record later
> active documents against their own named base commit.

You are auditing a batch of design documents from the `mere` repository
against the code they describe. Your job is verification, not editing.

## Hard rules

1. **Read-only, everywhere.** Do not edit, create, move, or delete any file
   outside your single output file. Do not run `cargo build`, `cargo test`,
   `cargo check`, `cargo metadata`, or anything that writes a `target/`
   directory. Do not run `git checkout`, `git stash`, `git clean`, `git
   worktree`, or any git command that changes a working tree or index.
   Allowed git commands: `git log`, `git show`, `git cat-file`, `git
   ls-tree`, `git grep`, `git blame`, `git rev-parse`.
2. **Audit the pinned snapshot.** The mere tree you audit is a detached
   worktree pinned at commit `bcb222ce`:
   `C:\Users\mark_\AppData\Local\Temp\claude\C--Users-mark--Code\8850fc33-9dd6-4624-a4c2-b6e2f310d99a\scratchpad\mere-audit-wt`
   Every path a doc cites as `crates/...`, `ports/...`, `design_docs/...`
   resolves relative to that root. Do **not** audit
   `C:\Users\mark_\Code\repos\mere` — another session is editing it.
3. **Sibling repos are live, read-only.** For claims about other repos
   (genet, turnstone, retinue, smolweb, mer3ly, distillery, ...), read
   `C:\Users\mark_\Code\repos\<name>` or `C:\Users\mark_\Code\crates\<name>`.
   Their HEAD may be newer than what the doc cites; say so when it matters.
   Never write there.
4. **Do not rewrite or judge prose.** Design rationale, opinions, and
   narrative are out of scope. You verify checkable claims.
5. **Fill the schema for every doc in your batch**, even when the answer is
   "historical, nothing to verify". A missing doc block is a failed batch.

## What "checkable" means

For each doc, extract and verify:

- **Status line vs tree.** If the doc says X landed / is complete / is
  deleted / remains open — is that true at `bcb222ce`? A plan whose status
  says "in progress" but whose subject crate no longer exists is a finding.
- **Code-font paths.** `crates/foo/bar.rs`, `ports/knot/src/...`,
  `design_docs/...` — does each exist at the pinned root (or the named
  sibling repo)? The manifest lists the ones a mechanical pass already
  flagged missing; confirm, and classify each as *renamed* (say to what),
  *deleted*, or *never existed*.
- **Named symbols.** Functions, types, modules, traits, feature flags,
  binary names, crate package names the doc asserts exist. `grep -rn` the
  pinned tree. Report the file:line that proves or disproves.
- **Versions and counts.** Crate versions claimed vs `Cargo.toml`;
  published-version claims vs the manifest; test counts only when the doc
  names a specific test file or module you can count directly.
- **Commit hashes.** `git cat-file -e <hash>^{commit}` in the repo the doc
  attributes it to (default: mere). A hash that resolves nowhere is a
  finding; a 40-hex or 32-hex string in a model/receipt context may be a
  model revision, not a commit — say which.
- **Cross-doc claims.** "See plan X, which is complete" — open X (in the
  pinned tree) and check. "Superseded by Y" — does Y exist and say so?
- **Internal contradictions.** Status line says one thing, a later Progress
  entry says another, or two sections disagree about what landed.

## Disposition vocabulary

Assign exactly one per doc:

- `current` — presents itself as describing the present state, and should
  be held to it.
- `historical-marked` — explicitly marks itself as a dated record (a
  rename-key banner, "historical record", "superseded by", an archive
  pointer, a status that says complete/landed with a date). Stale paths
  inside are expected and are **not** findings unless the doc claims they
  are current.
- `historical-unmarked` — reads as current but its subject has moved on
  (renamed crates, deleted code, landed work described as future) and
  nothing in the doc says so. This is the most valuable finding class.
- `superseded` — another doc explicitly replaces it, and the doc does not
  say so (or points at the wrong successor).
- `dead` — a plan whose subject no longer exists or whose status has been
  stale long enough that it is abandonment in all but name (in this tree,
  an "in progress" plan untouched for two months is a candidate).

## Output

Write exactly one file: the path given in your manifest under **Output**.
Markdown, this shape, one block per doc, in manifest order:

```
# Batch NN — <group name>

| doc | disposition | status accurate | claims | holds | stale | unverifiable |
|---|---|---|---|---|---|---|
| <short path> | current | yes | 12 | 10 | 2 | 0 |
...

## <doc path relative to design_docs/>
- disposition: current | historical-marked | historical-unmarked | superseded | dead
- status line: "<quoted, or (none)>" — accurate: yes | no | n/a
- claims checked: N — holds: N, stale: N, unverifiable: N
### Stale claims
- <the claim, quoted or paraphrased> — evidence: `<path:line>` or `<command>` → `<one-line output>`
### Contradictions
- <doc-internal or cross-doc> — evidence: ...
### Recommended action
- none | mark-historical (add banner) | update-status | archive (extract open points: ...) | fix-refs (<list>) | supersede-pointer (→ <doc>) | escalate (<why a human must rule>)
### Notes
- <anything the aggregator needs; blind spots you hit; sibling-repo HEAD drift>
```

Keep each stale-claim line to one or two sentences with the evidence
inline. Do not pad. If a doc has no stale claims, write "- none" under
that heading. Do not omit headings.

End your final message with only: the output path, and the totals from
your summary table (docs, stale claims, contradictions). The aggregator
reads the file; the message is a receipt.

## Priorities if you run short

1. Every doc gets a block with disposition and status verdict.
2. `current` docs get full claim verification.
3. `historical-*` docs get paths and status only.
