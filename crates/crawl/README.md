# mere-crawl

Host-neutral crawl frontier and bounded crawl runtime for Mere.

`enqueue_fetched_html_links` accepts an already fetched HTML response, extracts
raw links through Fleece's static DOM path, resolves them against the supplied
page URL, and passes them to the frontier. URL resolution, deduplication, scope,
depth, fan-out, and host policy remain crawl-owned. It does not fetch documents
or treat non-HTML responses as crawlable markup.
