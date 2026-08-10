# castellan

Name reservation for **castellan**, the credential-keeper port of the Mere
platform.

A castellan holds a keep in trust for its lord: custody without ownership, and
the office of the gate. This port is that keeper for your credentials. It
splits in two: an embeddable half any host app composes (vault browse, status,
code tiles; views that render *about* secrets and never contain them), and an
authority half that lives with the resident (release, signing, presentation),
answering participant-gate petitions over an agent-style channel the way the
personae ssh-agent already works. Apps talk to a pipe; apps never see the key.

The vocabulary it keeps, per the dramatis tier model:

- **chatelaine**: the secrets. Passwords, 2FA seeds, tokens, foreign key
  material. Never presented, only exercised.
- **emblem**: the proofs. Graded presentations of identity a persona hands
  out, from a bare handle to signed cross-attestations. Made to be shown; what
  lands in someone else's gaz.

The boundaries are the point: not [personae](https://crates.io/crates/personae)
(the faces and vault substrate castellan serves), and not
[gaz](https://crates.io/crates/gaz) or gazette (which keep and find the other
players; castellan guards and presents you).

Lives in the [mere](https://github.com/merely-made/mere) workspace at
`ports/castellan`. No implementation yet.

## License

MIT OR Apache-2.0
