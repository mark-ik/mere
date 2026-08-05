# Graphshell G8: structured action inputs

**Date:** 2026-08-05
**Scope:** let an endpoint advertise a small, verifiable input contract for a
typed action without making Graphshell a domain-form engine.

## Boundary

`graphshell-protocol::AdvertisedAction` may carry an optional `ActionFormV1`.
It declares an exact payload schema and named endpoint-supplied `choice`
fields. A host validates the form and composes only this JSON shape:

```json
{
  "schema": "endpoint.intent.example/v1",
  "selected_record": "opaque-endpoint-value"
}
```

Choice values are opaque. The endpoint supplies their label and optional
description, and it still validates the resulting payload against stored truth
when invoked. A host cannot derive a digest, session ID, interpretation, or
authorization from a card label.

The contract is intentionally narrow. It supports selecting exact values such
as a saved facts digest and reading-session ID. It does not model free text,
nested records, arbitrary JSON, application-specific rules, credentials, or
permission grants.

## Ownership

- Graphshell protocol owns transportable form descriptors and local composition
  checks.
- The endpoint owns field labels, the offered values, payload schema, replay,
  authorization, and persistence.
- A product host owns focus, layout, draft state, confirmation, and resnapshot.
- Cambium is the appropriate future renderer for that host composition and
  already drives Graphshell's painted chrome. Its separate semantic browser
  controls remain hand-wired, so this contract makes no false claim of a
  Cambium-backed chooser yet.

For Cleromancy, a future action can expose `astrology_facts_digest` and
`reading_session_id` as exact choices. The host submits the familiar typed
payload; Cleromancy replays both records before its Servitor-gated write.

## Acceptance

1. A form composes only the action's advertised schema and exact supplied
   values.
2. Missing required fields, unknown names, and values outside advertised
   choices fail before invocation.
3. Existing actions without a form retain the empty-payload path and old wire
   messages deserialize as `input_form: None`.
4. No portable Graphshell crate depends on Cambium, Mere, Cleromancy, or a
   renderer.

## Stop rule

Do not add an action-specific panel to the existing hand-built browser chrome.
The next slice is one real Cambium-backed host surface that consumes this
contract, with an endpoint fixture and a headed receipt. Cleromancy should use
that surface only after it can advertise bounded saved-record choices.
