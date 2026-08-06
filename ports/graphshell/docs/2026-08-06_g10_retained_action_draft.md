# G10 retained action draft

**Scope:** let a retained Graphshell client open and submit an
endpoint-advertised bounded action form over any carrier.

## Contract

`RetainedEndpointSession` is now available without the native feature. Its
`over` constructor remains carrier-neutral: an embedded endpoint may supply a
`LocalCarrier`; a process host may use `spawn` when native support is enabled;
another host may supply its own admitted carrier.

The session retains the exact projection request used at mount. It can:

1. inspect the compatible semantic action at a chosen presentation instance;
2. capture an `ActionDraft` and acknowledgement-bound `ActionDraftTarget`;
3. compose and submit only endpoint-advertised form values; and
4. request a fresh full snapshot using that retained projection request.

The draft does not make a new authority decision. Missing or invalid local
choices fail before the carrier call. Cleromancy and every other endpoint still
own advertised membership, revision checks, authorization, and domain writes.

## Consumer proof

Cleromancy A17 mounts its authorized local endpoint through
`RetainedEndpointSession::over(Box<dyn Carrier>)`. It opens the actual
`cleromancy.create-concurrence` action from an astrology-facts card, chooses
the endpoint's saved facts digest and reading-session ID, receives acceptance,
resnapshots, and observes the new Pattern occasion card.

## Checks

```powershell
cargo test -p graphshell --no-default-features --lib --offline

Set-Location ..\cleromancy
cargo test --test a17_graphshell_action_draft --offline
```

## Stop rule

This does not create browser-to-process transport, a generic JSON editor, or a
Cleromancy-specific Graphshell dependency. A headed host still needs its own
admitted carrier and renders the existing `ActionDraftSemantics` surface.
