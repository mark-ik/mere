# Mere burn-remote patch

Source: crates.io `burn-remote 0.22.0-pre.2`, upstream Burn commit
`89bcc85f75c55e3451442f5371de45b243865340`.

License: MIT OR Apache-2.0, unchanged from upstream.

Reason: the released server authorizes an Iroh session only at admission and
exposes no way for its host to end one already-admitted session. Its internal
`SessionManager::close` removes the manager's map entry, but the live pump owns
another task sender and therefore continues accepting work. Mere's owner-reclaim
rule requires the session to stop before `LeaseRevokedByOwner` is authored.

Change: each session now carries a server-close signal observed by the duplex
pump. `IrohRemoteProtocol::sessions` reports active session ids and their opaque
credentials, and `close_session` targets one pump. The existing client-close
path uses the same teardown.

Removal condition: replace this patch with the first upstream `burn-remote`
release that exposes equivalent targeted session control and passes Mere's
lease-bound admission, targeted reclaim, and client-termination receipts.
