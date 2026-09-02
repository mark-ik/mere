# Djinn

Djinn is Mere's local desktop resident. It composes the owner-held parts of a
personal device: Personae authority, the SSH agent, Graphshell's local browser
and application brokers, personal sync,
[Knot's](https://github.com/merely-made/knot-editor) source/sync/evidence custody,
per-profile Castellan custody, and the shared physical blob store.

Graphshell remains the local session and admission protocol. Knot remains the
Djot editor and document authority. Djinn owns their shared process lifetime
when they use persona-held state.

## Run

```text
cargo run -p djinn --bin djinn -- --vault-dir <personae-vault> --data-root <data-root>
```

The default configuration continues to read the existing Graphshell
application directory. That is a compatibility bridge for selected profiles,
pairing records, and content-store migrations, not an additional resident.

## Publishing

`0.0.2` is the source version of this workspace resident, not a crates.io
release. Knot is pinned from its public repository; Djinn's Graphshell
composition still uses workspace-only dependencies, so `cargo package`
correctly refuses it. A public Djinn release needs an installable package
boundary and a staged release of the Mere dependencies it exposes.

## Security boundary

Djinn holds durable authority but does not manufacture public services. Its
default personal-sync policy can use local discovery when the owner has
configured it; relays remain owner-selected transport configuration.

The following deployments are intentionally absent until a forcing consumer
defines their policy and acceptance receipt:

- a public Knot publisher;
- a Misfin receiver;
- a Gemot community host;
- a dedicated relay;
- a Secret Service daemon.

Castellan record and freshness keys are separately derived from the unlocked
Personae identity and opened per Djinn profile. Starting Secret Service also
requires a concrete persona selection and allowed-caller policy, so Djinn does
not guess either from a profile name.

## Lifecycle

Djinn uses `mere-resident` for the small rule shared with Distillery: close
resources in a concrete order, attempt every close, and retain every failure.
It does not share product policy, configuration, or service APIs with
Distillery.

## License

MPL-2.0
