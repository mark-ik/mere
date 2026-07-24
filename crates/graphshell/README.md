# Graphshell session stack

These four crates are Mere's reusable remote-session machinery:

- `graphshell-protocol`: versioned session messages over an unspecified carrier.
- `graphshell-client`: endpoint-scoped snapshots, diffs, resume, and cache policy.
- `graphshell-endpoint`: injected projection and intent traits for authorities.
- `graphshell-stdio`: the first local newline-delimited JSON carrier.

They may depend on Scenograph contracts, serialization, and
content-addressing primitives. They must not depend on the Mere kernel,
products, renderers, GUI toolkits, or network runtimes.

The reference application lives at [`ports/graphshell`](../../ports/graphshell).
Dependency direction is one-way: the port composes these crates; these crates
never depend on the port.
