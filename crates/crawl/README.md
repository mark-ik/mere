# mere-crawl

Host-neutral crawl frontier and bounded crawl runtime for Mere.

**Fleece intent receipt (2026-08-23; audited 2026-08-26):** `mere-crawl` names
`fleece`, but no production source calls it yet. The implementation packet in
`genet/design_docs/2026-08-26_fleece_followthrough_plan.md` adds a supplied-HTML
helper that extracts raw links, then leaves URL resolution, deduplication,
scope, depth, and host policy here in crawl.
