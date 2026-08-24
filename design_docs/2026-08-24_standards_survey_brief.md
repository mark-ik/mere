# Standards Across the Stack: What to Design To

**Date:** 2026-08-24
**Kind:** research brief (survey; nothing here is scheduled)
**Anchors:** [credential port + gazette brief](mere_docs/research/2026-08-10_credential_port_gazette_brief.md)
(corrected in place by this pass), [W3C standards architecture review](mere_docs/research/2026-07-05_w3c_standards_architecture_review.md)
(owns the web-platform spine; this brief deliberately does not re-cover it),
[turnstone suite composition census](2026-08-22_turnstone_suite_composition_and_capability_census.md),
[crypto stack decision](mere_docs/technical_architecture/2026-08-10_crypto_stack_decision.md).

**Scope note.** This is the workspace's one home for cross-repo standards
findings, per DOC_POLICY §2: genet, retinue, smolweb, woodshed and the rest cite
it by path (`mere/design_docs/2026-08-24_standards_survey_brief.md`) rather than
keeping copies. Plans that act on any finding here live in their own repo and
point back at the relevant section.

The origin question (Mark, 2026-08-24): are there password-vault standards we
should adhere to, and standards relevant to the broader stack — with the
observation that genet is bound to W3C/WPT by contingency, while fleece, pelt and
tabard might have standards we could *design to* rather than merely comply with.
That distinction organises the whole brief.

Verdict grammar is the one the [W3C review](mere_docs/research/2026-07-05_w3c_standards_architecture_review.md)
established: **ADOPT** (build to it, it is load-bearing), **PULL** (implement when
a real consumer demands it), **SKIP** (deliberately not, on the record),
**WATCH** (moving target, re-check).

---

## 1. What went stale, and what that says about the shelf life of a standards doc

Three findings invalidated statements in the 2026-08-10 credential brief, all
corrected there in place this session. They are recorded here because the
*pattern* matters more than the three facts.

- **The ssh-agent protocol is now RFC 9987** (Standards Track, May 2026, IETF
  SSHM WG), superseding the expired individual `draft-miller-ssh-agent`.
  personae has shipped an agent since 2026-07-22; it can now make a conformance
  claim. The constraint-extension namespace is normatively specified, which
  makes it the sanctioned wire slot for castellan's per-persona release policy
  rather than a side channel. The `sk-*` FIDO key types remain outside the RFC
  as `@openssh.com` vendor extensions, so conformance says nothing about
  hardware-backed keys — precisely the half a credential keeper cares about.
- **CXF v1.0 is a Proposed Standard**, approved August 2025 with errata folded
  in 2026-03-09. The 2026-08-10 brief called it "drafts public"; castellan's
  README still calls CXF import "follow-on work" against a review draft that is
  a year behind. Note the trap that produced the stale reading:
  fidoalliance.org's surrounding prose still says "early review draft" while the
  artefact links resolve to `cxf-v1.0-ps-errata-20260309.html`. Trust the
  artefact.
- **Linux has an emerging third-party passkey provider seam.** `credentialsd`
  plus a proposed XDG credential portal (linux-credentials org, with
  `libwebauthn` and `oo7`) replaces the old "no blessed seam, use virtual
  hidraw" position.

The lesson for this repo: a standards inventory decays in months, not years, and
it decays *silently* — every stale line still reads as authoritative. Anything
in this brief with a status attached carries a verification date for that reason.

---

## 2. Castellan: the vault

The primitives are right and this brief does not propose changing them.
`personae` seals with Argon2id (RFC 9106) and ChaCha20-Poly1305 /
XChaCha20-Poly1305 (RFC 8439), signs with Ed25519, uses a 16-byte salt and a
32-byte key — which is exactly what RFC 9106 §4 asks for. The
[crypto stack decision](mere_docs/technical_architecture/2026-08-10_crypto_stack_decision.md)
already rules the RustCrypto+dalek posture and already holds `argon2` at 0.5.3 on
the correct grounds that a password-hash change is a stored-format change.

Every real gap is in **crypto agility**, not in crypto choice.

### 2.1 Neither derivation root records its own parameters

Verified in code, 2026-08-24:

- `crates/dramatis/personae/src/passphrase_storage.rs` — the on-disk
  `EncryptedFile` carries `version`, `salt`, `profiles`. It does **not** carry
  the Argon2 cost parameters; `grep` for `m_cost`/`t_cost`/`p_cost` across both
  `passphrase_storage.rs` and `passphrase_root.rs` returns nothing. The module
  doc at the head of `passphrase_storage.rs` nevertheless describes the wire
  format as "ciphertext + nonce + Argon2id parameters". The doc is wrong about
  its own format.
- `derive_kek` calls `Argon2::default()`, so the parameters are whatever the
  compiled-in crate says: argon2 0.5.3 defaults to m=19456 KiB, t=2, p=1. That
  is the OWASP low-memory baseline, and it is below *both* of RFC 9106 §4's
  recommended options (first: t=1, p=4, m=2 GiB; second: t=3, p=4, m=64 MiB).
- `crates/dramatis/personae/src/keypair.rs` — `derive_child` derives every
  persona key as `Ed25519::from_seed(blake3::keyed_hash(master_key, salt))`. A
  sound PRF, but with no versioned context slot.

Two consequences, both structural. A future `argon2` release that changes
`DEFAULT_M_COST` silently makes every existing vault file underivable, with no
version field to trigger a migration. And the cost parameters can never be
raised, because raising them is indistinguishable from corruption. RFC 9106 and
the PHC string format exist partly to prevent exactly this; BLAKE3's own
`derive_key` context-string mode, or RFC 5869 HKDF's `info` parameter, is the
equivalent fix on the derivation side.

This is a v2-root decision rather than a patch, which is the argument for taking
it **before** the vault carries anyone's real credentials rather than after. The
`argon2` 0.5.3 hold already recorded in the crypto stack decision is the right
instinct; this is the same instinct applied to the file format.

**Verdict: ADOPT RFC 9106 §4 as a named parameter target and record it in the
format; ADOPT RFC 5869 (or BLAKE3 `derive_key`) as the versioned derivation
context.** Both are small now and ugly later.

### 2.2 `otpauth://` has no normative specification at all

Castellan already imports it. There is no RFC, no consortium spec: the de-facto
authority is a Google wiki page, `otpauth` sits at IANA only as a *provisional*
URI scheme, and two competing individual drafts (`draft-andesco-otpauth-uri`,
`draft-linuxgemini-otpauth-uri`) disagree on issuer semantics. Implementations
diverge on Base32 padding and on the SHA-256/SHA-512 algorithm values.

The [OTP plan](mere_docs/implementation_strategy/2026-08-10_castellan_otp_plan.md)
already refuses `otpauth-migration://` on exactly these grounds. The plain scheme
deserves the same note on the record: RFC 6238 and RFC 4226 are Informational
RFCs with published test vectors, but the URI that actually carries the seed
between vaults is unspecified, and castellan is already interoperating with it.

**Verdict: no change to behaviour, but record the negative.** This is what CXF is
for — CXF v1.0 defines a dedicated TOTP credential type with actual normative
text.

### 2.3 CXF is the runway item, and it is plaintext by design

One parser buys import from 1Password, Bitwarden, Dashlane, Apple Passwords,
Chrome and Android. The mapping is not a hack: CXF `BasicAuth` maps to chatelaine
passwords, `TOTP` onto the existing RFC 6238 items, `SSHKey` onto personae's
`ssh_slot`, `Passkey` is the forward hook, and the
Header/Account/Collection/Item/Credential tree gives face-scoped separation a
natural landing spot on import. Export in the same format is the anti-lock-in
claim the product needs to make credibly — a local-first vault that cannot get
your data back out is a worse cloud.

Two gotchas, both material:

1. **CXP has not moved since 2024-10-03** and is still a Working Draft. It was
   the half that carried the encryption. CXF explicitly "does not make any
   assumptions about the protocol used for the transfer", so a `.cxf` file on
   disk is plaintext credentials. Every import path must treat the file as
   burning; any export either waits for CXP or gets wrapped by castellan itself
   with a loud, unavoidable warning.
2. **The Rust crate lags the spec by a year.** `credential-exchange-format`
   0.4.0 (MIT, Bitwarden) still declares the March 2025 review draft. Adopting
   it without diffing against the 2026-03-09 CDDL silently inherits a stale
   schema.

The real cost is policy, not parsing: 17 credential types include `Passport`,
`DriversLicense` and `CreditCard`, and which of those castellan stores, drops, or
quarantines on import is a decision for Mark, not an implementation detail.

**Verdict: ADOPT CXF (import first). WATCH CXP.**

### 2.4 A WebAuthn credential is scoped to an RP ID, not to a persona

The identity model partitions by cryptographic face. WebAuthn has no
representation for "which face is this passkey" — the credential is bound to a
relying-party ID. That collision is architectural, it is not solvable inside the
standard, and it is better on the record now than discovered when someone tries
to use two personae on the same site. WebAuthn L3 is a **Candidate
Recommendation Snapshot (26 May 2026)**, proposed for advancement 20 July 2026;
it is routinely cited as a Recommendation and is not one.

### 2.5 The rollback gap has a standard that closes it

Castellan's README is honest that `sealed_record_storage/freshness.rs` detects
rollback only when its separately-rooted ledger was not restored alongside the
record store. **TPM 2.0 NV monotonic counters** (TCG TPM 2.0 Library
Specification, published as ISO/IEC 11889) are the standard answer. Windows and
Linux only — macOS Secure Enclave counters have no public API.

Revision caveat: the survey cited rev 01.83 and the fact-check found 1.85
(released 2026-03-12), but could not read the TCG listing page, which returns 403
to automated fetch. **Re-pin the revision by hand before this goes into a plan.**
The load-bearing content — NV counters — is unchanged across revisions.

**Verdict: PULL**, with a real consumer (the freshness ledger) already present.

### 2.6 Refused, on the record

- **Post-quantum for the vault is a trap.** Sealed records are symmetric and are
  nobody's harvest target. The genuine harvest-now-decrypt-later exposure is the
  X25519 in the P2P *sync transport* — a different artifact. Treating "PQ the
  vault" as the task spends a year on the wrong thing. FIPS 203/204/205: WATCH,
  scoped to transport.
- **FIPS 140-3, SOC 2, ISO/IEC 27001, PCI DSS, MASVS: SKIP.** All are
  organisational or tenancy regimes. A serverless local-first product with no
  tenants is not the subject of any of them. OWASP ASVS 5.0.0 is the one worth a
  PULL, as a checklist rather than a certification.
- **MLS (RFC 9420): SKIP for the vault.** Too much group-state machinery for what
  shared credentials need. **RFC 9180 HPKE is the smaller right thing** — the
  sealing primitive without the group state machine, already the substrate under
  both MLS and TLS Encrypted ClientHello, with pure-Rust implementations. It is
  also what murmur's store-and-forward actually wants: seal a payload to a
  recipient's public key offline, no handshake, no online counterparty.
- **OPAQUE (now RFC 9807, no longer a draft) and SRP: SKIP** — there is no sync
  server to authenticate against. Note RFC 5054 is Updated by RFC 8996, which
  struck the TLS versions SRP ran on.

### 2.7 Foreign key material arrives in formats nobody chose

chatelaine's remit explicitly includes "foreign key material", and `import`
migrates stored browser data. Two omissions the fact-checkers flagged:

- **RFC 9580 (OpenPGP, July 2024)** obsoletes RFC 4880/5581/6637 and is the first
  OpenPGP RFC to standardise v6 keys, AEAD-protected packets and Argon2 S2K.
  Governance gotcha worth deciding deliberately: GnuPG declined 9580 in favour of
  LibrePGP, so "OpenPGP compatibility" is now two mutually incompatible things.
  Rust's `sequoia-openpgp` and `rpgp` sit on the 9580 side.
- **RFC 7292 (PKCS #12)** with RFC 5958 — the canonical trap of this lane. An
  ASN.1 container whose legacy MAC/encryption profile modern toolchains reject by
  default. Note **RFC 9879 (2025)** obsoletes RFC 9579 and updates both RFC 7292
  and RFC 8018; it is the current normative text for a `.p12` carrying a modern
  PBKDF2/HMAC-SHA-256 integrity MAC, and it changes the password encoding from
  BMPString to UTF8String.

### 2.8 Secret Service: the most valuable OS surface, with a silent failure mode

Castellan implements the server side of Secret Service 0.2 on zbus 5.19. That is
the only OS surface in the stack where castellan can *be* the service rather than
plug into someone else's: every libsecret consumer on Linux — GNOME apps,
Chromium's password store, NetworkManager, the Git credential helper, Evolution —
becomes a client without knowing castellan exists. Nothing on Windows or macOS
offers this.

Status, verified 2026-08-24: still **version 0.2, still labelled DRAFT**, with a
publication date of 2026-04-08 (the document is being rebuilt; the version did
not move). The canonical URL moved —
`specifications.freedesktop.org/secret-service-spec/latest/` now 301s to
`.../secret-service/latest/`.

The operational finding, which is the one that matters: **only one process can
own `org.freedesktop.secrets` on a session bus.** Castellan's
own-without-replacement posture is correct, but on a stock GNOME session
gnome-keyring wins and castellan is **silently inert**. Per the house rule that
diagnostics assert invariants rather than merely measure cost, this wants a
runtime assertion that reports which process holds the name — not a comment.

Second finding: the `dh-ietf1024-sha256-aes128-cbc-pkcs7` session algorithm is
weak by modern standards. The honest position, which castellan's design already
assumes, is that Secret Service transfer security comes from the bus and the peer
check (caller binding, `/proc` executable identity), not from the session
algorithm.

---

## 3. genet: fleece, tabard, pelt

Mark's instinct was that these three might carry standards to design to rather
than comply with. Confirmed for all three, most strongly for fleece.

### 3.1 fleece has the best design-to story in the stack

The **Web Annotation Data Model** has been a stable W3C Recommendation since
2017-02-23 and defines eight selector types — `FragmentSelector`, `CssSelector`,
`XPathSelector`, `TextQuoteSelector`, `TextPositionSelector`,
`DataPositionSelector`, `SvgSelector`, `RangeSelector` — plus a `refinedBy`
composition rule. A `TextPositionSelector` refined by a `TextQuoteSelector` is a
fast path and a robust fallback in one object.

fleece is render-free and already walks a profile-neutral LayoutDom in document
order. It can therefore mint exact/prefix/suffix quotes and codepoint offsets
essentially for free *while it is already traversing*. Emitting standard
selectors makes every downstream consumer — gazette's feed pipeline, alembic's
recall, knot — composable with the rest of the world instead of with a
house-private anchor format. Rust prior art is effectively absent: crates.io has
no general implementation.

Pair it with **URL Fragment Text Directives** (`#:~:text=prefix-,exact,-suffix`),
the same idea as a URL a person can paste, shipped in Chrome 80+, Edge 83+,
Safari 16.1+ and Firefox 131+. For a serverless house this is provenance with
zero infrastructure. Three caveats: it is still a WICG Draft Community Group
Report (upstreaming to WHATWG is open as `whatwg/html` PR #11895), so pin the
commit; the `:~:` directive is deliberately **stripped from `document.URL`** as
an anti-exfiltration measure, and genet must reproduce that stripping or it
becomes a privacy leak; and Chrome gates activation on user-gesture or
browser-initiated navigation.

The honest negative, which is itself a finding: **there is no standard for "the
readable article."** Readability, trafilatura and Postlight are all de-facto.
fleece's `Article` shape is correctly proprietary, and saying so on the record
stops the question being re-asked.

Two boundary rulings worth recording:

- **Web Annotation *Protocol*: SKIP** while adopting the Data Model. The Protocol
  builds on Linked Data Platform, models containers as LDP Containers and
  requires HTTP server interactions — a server-tenancy design, flatly against
  serverless-by-default. Adopting the model without the protocol is coherent, but
  it has to be written down or someone will assume they travel together.
- **Selectors and States: SKIP.** It is a 2017 Working Group *Note*, not a
  Recommendation; the selector vocabulary that matters is already normative
  inside the Data Model.

### 3.2 tabard: DTCG went stable, and there is no Rust crate

**Design Tokens Format Module 2025.10**, published 2025-10-28, is the first
stable release: "This specification is considered stable. Further updates will be
provided in superseding specifications." tabard's README already calls DTCG the
non-negotiable interchange form; that call is now backed by a frozen target
rather than a moving one. The **Color Module** ships alongside it; the **Resolver
Module** is still a WATCH.

Conformance is thinner than it looks but has teeth: every token MUST carry an
explicit `$type` (inference from the value is *prohibited*), circular references
MUST be detected, and tools MUST preserve `$extensions` they do not themselves
understand — a round-trip that drops a foreign vendor's extension block is
non-conforming. There is no conformance test suite and no normative reference
implementation, so compliance is self-asserted against a date string.

The internal argument is stronger than the interchange one: if tabard's derived
palette **is** a DTCG token tree, then the CSS custom-property emitter for
livery, the theme-struct emitter for hosts, and any future vendor emitter all
become projections of one model instead of three parallel derivation paths.
`$extensions` under a reverse-DNS key is the sanctioned home for tinct's seed
colours and OKLCH ladder parameters, which is what lets a livery round-trip back
into the authoring surface.

**crates.io has no DTCG parser/emitter as of 2026-08.** All reference
implementations are JS/TS (Style Dictionary, Tokens Studio, Terrazzo). That is a
small, well-scoped, unclaimed job — naming-ledger territory, not decided here.

Two corrections for tabard specifically:

- **APCA: SKIP.** It was pulled from WCAG 3 in July 2023, and the current WCAG
  3.0 Working Draft (3 March 2026) names no contrast algorithm at all — it refers
  only to an unspecified "minimum contrast ratio test". Build contrast-gated text
  roles to **WCAG 2.2 SC 1.4.3 / 1.4.6 / 1.4.11**.
- **CSS Custom Properties Level 1** is the normative definition of tabard's own
  *output* and was missing from the survey. It defines the custom-property
  syntax, the `var()` fallback grammar, and the guaranteed-invalid value that
  governs what happens when a token fails to resolve. It has sat at Candidate
  Recommendation since 2022.

Underneath both: **sRGB is IEC 61966-2-1:1999**, paywalled, which is why
implementations are written from the CSS Color 4 restatement instead. Its
piecewise transfer function (0.04045 / 12.92 / 2.4) is routinely mis-implemented
as a plain 2.2 gamma — exactly the error class that silently breaks contrast
gating.

### 3.3 pelt: mostly unglamorous, with three that matter

The bulk is desktop integration: XDG Base Directory 0.8, Desktop Entry 1.5, XDG
Desktop Portals (Settings/`org.freedesktop.appearance`, FileChooser, Screenshot,
OpenURI) — all ADOPT, all boring, all cheap. shared-mime-info and the Icon Theme
spec are PULL; the Windows/macOS registration equivalents are PULL and are not
standards at all.

Three worth attention:

- **Core-AAM 1.2 is meaningless without WAI-ARIA 1.2** as its source vocabulary.
  Core-AAM is a mapping table. Adopting one without the other is incoherent, and
  the survey did exactly that until the fact-check caught it. ARIA 1.3 remains a
  Working Draft (4 June 2026) and has not reached CR, so 1.2 is the build target.
  Core-AAM 1.2 is now a Candidate Recommendation Draft of 5 August 2026; AccName
  1.2 is a Working Draft of the same date.
- **Global Privacy Control** has one operative California date: **2027-01-01**
  (AB 566). The business-side duty to honour an opt-out signal comes from
  existing CCPA/CPRA regulation, not from AB 566, and conflating them misstates
  what is required and when. The spec surfaces are all confirmed: `Sec-GPC` MUST
  be exactly `1` when enabled and MUST NOT be sent when disabled; intermediaries
  MUST NOT strip it; `navigator.globalPrivacyControl`; `/.well-known/gpc.json`.
- **ITU-T H.273** is the shared code-point registry that PNG's `cICP`, the
  Wayland colour-management protocol, ICC's CICP tag and AV1/HEVC all point at.
  It is the one document that stops colour identity being re-derived at every
  boundary between an image decoder, a compositor protocol and an ICC profile.
  Relevant to netrender and wgpu-scry as much as to pelt.

Wayland colour-management (`color-management-v1`) is Staging, interface version
2, first shipped in wayland-protocols 1.41. Version 2 **deprecates the `srgb` and
`ext_srgb` named transfer functions** in favour of `compound_power_2_4` — an
implementation written to the older enum list targets deprecated values.

---

## 4. The largest hole is in none of them: local-link discovery

**There is no local-link discovery story anywhere in the stack.** The survey
covers global directory (WebFinger, JSContact, DID, NIP-05) and long-haul radio
(Reticulum, LoRa) and has nothing in between.

**RFC 6762 (mDNS) + RFC 6763 (DNS-SD)**, both Proposed Standard, are the layer
that distillery's device ring, moot's places, and turnstone's shared addressable
places all need: peers on a LAN with no accounts and no cloud tenancy. It is the
discovery layer a serverless house posture actually lives on, and `mdns-sd` is
pure Rust. Gotcha: Windows and macOS hold port 5353, so an in-process responder
collides with the platform resolver; "Bonjour" is Apple's trademark for the same
protocol.

**Verdict: ADOPT**, and it deserves its own plan rather than being absorbed into
a port.

Three more cross-cutting items:

- **RFC 8615 (Well-Known URIs)** is the substrate under `/.well-known/webfinger`,
  `/.well-known/nostr.json`, `/.well-known/did.json`,
  `/.well-known/change-password` and `/.well-known/gpc.json` — five consumers
  already named in this brief. Its IANA registration requirement (a name must
  reference a spec defining format and media type) is a design constraint on any
  house-invented well-known path.
- **RFC 9309 (Robots Exclusion Protocol)** has been Standards Track since 2022,
  contrary to the widespread belief that it is convention only. fleece extracts
  from fetched pages and turnstone fetches them; anything retrieving third-party
  content at more than one page at a time is a crawler by the RFC's own
  definition. Four normative pages, cheap to build to, expensive to be caught not
  building to.
- **RFC 5005 (Feed Paging and Archiving)**, Proposed Standard, is the only
  standard that says how a feed exposes its whole history — `fh:complete` and the
  `prev-archive`/`next-archive` link relations. Atom and RSS describe only a
  current window. Without it, gazette and turnstone each invent a paging
  convention.

---

## 5. The other ports, briefly

- **gazette.** **RFC 9553 JSContact** (with RFC 9554/9555 for vCard conversion)
  supersedes the vCard/jCard question: a modern JSON data model rather than a
  line-folded 1998 format. ADOPT over vCard 4.0's PULL. WebFinger (RFC 7033)
  stays ADOPT. **Verifiable Credentials 2.0** is the standard the house "emblem"
  concept most closely already is — graded public-key presentations made to be
  shown — and is worth reading before emblem's own shape is fixed. DIDs are PULL,
  not ADOPT: `did:key` and `did:web` are the only two methods worth carrying.
  CardDAV and JMAP: SKIP, both server-tenancy. **Nostr NIP-01 is the load-bearing
  document, not NIP-05** — NIP-05 maps a name to a hex pubkey and is meaningless
  without the event/relay/signature model NIP-01 defines.
- **moot / murmur.** MLS (RFC 9420) is ADOPT *here* even though it is SKIP for
  the vault — group messaging is what it was designed for. MIMI is WATCH. Matrix:
  SKIP (and note the survey's v1.16 citation was three releases stale; current is
  v1.19, 8 July 2026). Signal's published specs are XEdDSA, X3DH, PQXDH, Double
  Ratchet, Sesame and **The ML-KEM Braid** — "SPQR" and "Triple Ratchet" are
  announcement vocabulary with no citable document. **XMPP (RFC 6120/6121)
  deserves an explicit SKIP on the record** rather than silent omission: it is the
  only IETF Standards Track federated messaging suite, and RFC 6121's
  roster/presence model is prior art that gaz and moot are re-deciding from
  scratch.
- **signalman / retinue.** The binding standards are **regulatory, not
  protocol**, and they are hard constraints: **FCC Part 15.247** (US,
  902–928 MHz), **ETSI EN 300 220-2 V3.3.1** (EU SRD duty cycle, harmonised under
  RED), **ARIB STD-T108** (Japan). ADOPT. FCC Part 97 §97.113(a)(4) prohibits
  encrypted transmissions in the amateur service, which is precisely why
  unlicensed ISM is the floor rather than amateur allocation — that is a SKIP with
  a reason, not an oversight. LoRaWAN: SKIP (a network-server architecture,
  against the house posture). Reticulum RNS and LXMF are single-implementation
  specs, not standards — worth recording so the dependency is understood as such.
  **IEEE 802.15.4-2024 with RPL/6LoWPAN** deserves a named SKIP so retinue's
  bespoke mesh is a deliberate choice rather than an unexamined one.
- **distillery / alembic.** safetensors: ADOPT. GGUF: PULL (current format
  version is 3; v2's change was uint32→uint64 counts, not alignment padding).
  MCP: WATCH. ONNX and OCI model packaging: SKIP. **There is no standard for
  job/lease/heartbeat semantics in a P2P device ring** — Kubernetes Lease,
  Sparkplug B, Redfish and DRMAA were all surveyed and rejected. Staying
  proprietary is correct and is now on the record. **EU AI Act Article 50**
  transparency obligations are worth a read for whether a local-inference product
  is in scope.
- **knot / import.** CommonMark 0.31.2 is ADOPT; **Djot has no versioned
  specification** and its reference implementation is at 0.2.0 — WATCH, and note
  that **there is no registered media type for Djot at all**. RFC 7763 registers
  `text/markdown` with a `variant` parameter and RFC 7764 registers CommonMark as
  a variant; a knot projection or smolweb response serving Djot must pick
  something unregistered and live with it. That should be a recorded decision.
  The Netscape bookmark format `import` parses has no specification whatsoever.

---

## 6. The apps

- **woodshed / hocket.** MusicXML 4.0 is the practical notation target (Final
  Community Group Report, **1 June 2021**). MNX has not stabilised; MEI 5.1
  (released 2025-01-22) has the strongest tablature story via its tablature
  module but is SKIP on cost. MIDI 1.0 is ADOPT and is not deprecated; MIDI 2.0
  is WATCH, and note the actual SMF successor is the unfinished **SMF2 Container
  Format**, not the already-published MIDI Clip File. **ITU-R BS.1770-5 with EBU
  R 128** is ADOPT and is the thing a practice tool must not get wrong. **RFC
  9559 (Matroska, October 2024)** belongs beside RFC 9639 — the same working
  group gave both change control in the same quarter, and Matroska is the
  container a loop recorder writes multi-track takes into. LV2 is the third
  plugin ABI and the only one with no licence question in its history. And the
  cheapest adoption in the whole survey: **ISO 16:1975** (A4 = 440 Hz). Verified
  2026-08-24: it currently lives in `woodshed-audio/src/input.rs:122` and `:125`
  as a bare `440.0` inside the pitch/MIDI conversion, unnamed and unattributed,
  which is also the place a configurable reference pitch would have to enter.
  (The ISO designator itself could not be confirmed against iso.org, which
  returns 403 to automated fetch — check the current edition by hand.)
- **wavicle / pipit.** **FLAC is now RFC 9639** (Proposed Standard, December
  2024) — a genuine change from wiki-spec to change-controlled document, and
  Vorbis comments are normatively defined in its §8.6. For pipit: **PESQ (ITU-T
  P.862) was withdrawn on 2024-01-05**, so any quality claim must use P.863 POLQA
  or ViSQOL. ReplayGain 2.0 lives only on a wiki that 403s automated readers — no
  change control, which strengthens the PULL.
- **sonance / mora.** BCP 47 is ADOPT. **SSML is SKIP** — a 2010 Recommendation
  aimed at synthesiser control, not prosodic analysis — but its lexicon half, the
  **W3C Pronunciation Lexicon Specification (PLS) 1.0**, is exactly sonance's and
  mora's domain: a phoneme/alias lexicon keyed by alphabet identifier and BCP 47
  tag. Covering SSML alone leaves the pronunciation store unstandardised, which
  is where the data actually lives.
- **turquet.** The astronomy standards are unusually binding, and one of them is
  about to move. **Continuous UTC is pending a vote at the 28th CGPM, 13–15
  October 2026.** Draft Resolution C (version 5, 13 July 2026) "decides that
  continuous UTC will become effective on 20 May 2027" with |UT1−UTC| capped at
  3 600 seconds — a dramatic acceleration of the 2022 Resolution 4 language,
  which said only "in, or before, 2035". If adopted, UTC−TAI becomes a constant
  after that date: leap seconds stop, conversion becomes arithmetic rather than a
  table lookup, and turquet loses a live data dependency, which matters for a
  crate whose design goal is self-containedness. The countervailing risk is in
  the draft's own *noting* section: a March 2025 CCTF/IERS workshop put the
  probability of a **negative** leap second at 30% by 2035, one has never been
  applied, and essentially no implementation in the wild handles one. **WATCH,
  with a real date.**
  On reference frames, the survey named only half of what turquet needs.
  **ICRF3** (IAU 2018 Resolution B2) is the *radio* realisation of the ICRS —
  4536 VLBI quasars. turquet plots stars in the optical, which live on
  **Gaia-CRF3**, the fundamental optical realisation per **IAU 2021 Resolution
  B3**, effective 2022-01-01. Code that treats "ICRS" as one thing and reaches
  for ICRF3 cites the wrong realisation for the data it is drawing. Both ADOPT.
  Missing between the frame and the ephemeris is the *transformation* standard:
  **IAU SOFA** with **IERS Conventions (2010), Technical Note 36** is the
  IAU-endorsed chain from ICRS to apparent place, and nothing computes an
  altitude and azimuth without it. Gotcha that makes this a verdict rather than a
  footnote: SOFA ships C and Fortran 77 only under its own bespoke licence —
  neither MPL-2.0 nor MIT/Apache-2.0 — so it is a specification to reimplement
  against and validate with, never a dependency to link. NAIF SPICE and the JPL
  DE ephemerides stay ADOPT for interchange even though the ephemeris choice
  itself is settled: the analytic provider matches DE440s at millidegree
  precision and DE440s survives as an opt-in oracle rather than a runtime
  dependency, per `turquet/design_docs/2026-08-13_provider_architecture.md`.
  FITS/WCS, VOTable and SAMP are PULL. Do **not** quote a fixed total for the IAU
  Catalog of Star Names in any doc — it is continually updated, the survey's
  figure of 605 could not be corroborated against any primary source, and the 59
  names it attributed to 2024–25 were actually adopted and announced on
  2026-02-12. Read the count off the catalogue at time of use.
- **smolweb.** The finding is the taxonomy itself: **gopher (RFC 1436), finger
  (RFC 1288) and DICT (RFC 2229) are real RFCs; Gemini has a maintained
  two-document specification (protocol v0.24.1, gemtext separately since the
  March 2024 refactor); and nine protocols — nex, spartan, guppy, scroll, text,
  scorpion, kepler, misfin, fsp — have no standard designator of any kind**, only
  a reference implementation or a text file at an author's URL. For a repo whose
  stated purpose is spec-faithful implementation, knowing which half has a spec
  to be faithful *to* is the load-bearing fact. Two refinements: the datatracker
  now stamps RFC 1436 and RFC 2229 "Legacy — not endorsed by the IETF and has no
  formal standing", which strengthens rather than weakens that reading; and
  RFC 1288 sits at a maturity level RFC 6410 abolished outright.
  **Actionable, verified in code 2026-08-24: RFC 8446 is obsolete.**
  `draft-ietf-tls-rfc8446bis` was published as **RFC 9846** (Proposed Standard,
  July 2026), which obsoletes RFC 8446 along with RFC 5077, 5246, 6961, 7627 and
  8422 — it keeps the same wire version and is backward compatible, but the
  citable document for the TLS 1.3 that Gemini mandates is now RFC 9846.
  `smolweb/crates/scorpion-protocol/src/tls.rs:29` currently cites "RFC 8446
  §C.4" in a doc comment. That is a one-line fix in a repo outside this brief's
  scope; flagged, not made.
- **iconvg, isometry, the games.** IconVG is one person's format with an
  Apache-2.0 spec file, correctly pinned by the house decoder to a spec commit.
  Tiled TMX is the only virtual-tabletop-adjacent format with a real
  specification; "Universal VTT" has no document number at all. Lottie is at
  **1.0.1**, not 1.0 — a SKIP either way, but the point-release drift is the kind
  the "active but young" reading rests on. Games are a standards-poor area where
  proprietary formats are correct, and glTF is a SKIP absent a consumer.
  Two raster/texture omissions the survey missed entirely, both load-bearing:
  **PNG Specification (Third Edition)** reached W3C Recommendation on 2025-06-24
  and finally specifies APNG normatively, adds HDR through `cICP` with BT.2100
  PQ/HLG, and adds the `mDCV`, `cLLI` and `eXIf` chunks. It matters three ways
  here — isometry's entire asset vocabulary is PNG, tabard/tinct colour has to
  survive a round trip through PNG's colour chunks, and genet/pelt decode PNG on
  every page. Its `cICP` chunk is the concrete bridge between the BT.2100 and CSS
  Color 4 entries, and it is the same ITU-T H.273 registry named in §3.3.
  **Khronos KTX 2.0** is the GPU texture container with Basis Universal
  supercompression, consumed by netrender/wgpu-graft/wgpu-scry and the target of
  glTF's own `KHR_texture_basisu` extension — so the glTF entry had a dangling
  dependency. Likely PULL rather than ADOPT: the UASTC/ETC1S transcoders are the
  encumbrance-sensitive part and pure-Rust coverage is partial.

---

## 7. Verdict index

All 174 surveyed entries, as returned by the survey. Sixty ADOPT, fifty-nine
PULL, thirty-three SKIP, twenty-two WATCH. Consumers are as the survey assigned
them and are a starting point, not a ruling. Where a row below conflicts with
the prose above, **the prose wins**: these rows are as the survey first returned
them, and the fact-checking pass corrected thirty-two entries after the fact.
The four whose designators were most misleading — TLS 1.3, WebRTC, ICC/ISO and
Lottie — have been corrected in place here; the rest of the corrections are
recorded in §1, §3.3, §5, §6 and §8.

| Verdict | Standard | Designator | Consumer |
|---|---|---|---|
| ADOPT | Argon2 memory-hard password hashing | RFC 9106 | personae — crates/dramatis/personae/src/passphrase_storag |
| PULL | scrypt password-based KDF | RFC 7914 | import (the crate that migrates STORED browser and vault |
| PULL | PBKDF2 (PKCS #5 v2.1) and NIST's password-based KDF recommendation | RFC 8018 / NIST SP 800-132 | Two, both narrow. (1) import — KDBX 4, 1Password, Bitwar |
| ADOPT | Digital Identity Guidelines — Authentication and Authenticator Management | NIST SP 800-63B-4 (within SP 800-63-4) | Two real ones. (1) **personae passphrase enrollment** — e |
| SKIP | Information security controls — Authentication information | ISO/IEC 27002:2022 control 5.17 | None in the codebase. The nearest thing is castellan's gat |
| ADOPT | ChaCha20-Poly1305 AEAD (and its X-nonce variant) | RFC 8439 (+ draft-irtf-cfrg-xchacha-03, EXPIRED) | Both halves are already shipped. personae/src/passphrase_ |
| SKIP | AES Galois/Counter Mode (GCM and GMAC) | NIST SP 800-38D | **None in the vault.** ChaCha20-Poly1305 already occupies  |
| PULL | Misuse-resistant AEAD: AES-SIV and AES-GCM-SIV | RFC 5297 (AES-SIV) and RFC 8452 (AES-GCM-SIV) | Not today. The consumer that would create real demand is * |
| SKIP | AES Key Wrap (KW / KWP) | NIST SP 800-38F | None in the vault. Would only appear as an interop require |
| ADOPT | HKDF extract-and-expand key derivation | RFC 5869 (and NIST SP 800-56C Rev. 2 for the key-establishment case) | **personae Ed25519Keypair::derive_child** — crates/dram |
| PULL | Hierarchical deterministic key derivation and seed-phrase carry | BIP-32, BIP-39, SLIP-0010 | personae's master seed and the wallet carry layer. design |
| PULL | Shamir's Secret-Sharing for Mnemonic Codes | SLIP-0039 | personae vault-root recovery. design_docs/mere_docs/imple |
| SKIP | OPAQUE augmented password-authenticated key exchange | RFC 9807 (and, as the balanced alternative, draft-irtf-cfrg-cpace-21) | **None, and that is the finding.** There is no sync server |
| SKIP | Secure Remote Password (SRP-6a) | RFC 2945 (SRP-3) and RFC 5054 (SRP-6a for TLS) | None. Same reason as OPAQUE. Worth knowing only because ** |
| WATCH | Post-quantum cryptography: ML-KEM, ML-DSA, SLH-DSA and the transition timeline | FIPS 203, FIPS 204, FIPS 205 (+ NIST SP 800-227, NIST IR 8547, CNSA 2.0) | For the **vault**: none, honestly. For the **sync transpor |
| WATCH | X-Wing general-purpose hybrid post-quantum KEM | draft-connolly-cfrg-xwing-kem-10 | None today. Would become relevant only in the sync-transpo |
| SKIP | Messaging Layer Security | RFC 9420 | For a shared/family vault: no fit, see below. The real MLS |
| SKIP | Cryptographic module validation | FIPS 140-3 (and the CMVP) | None. There is no US federal buyer, no FedRAMP boundary, n |
| WATCH | Cryptoki — the cryptographic token interface | PKCS #11 Specification Version 3.2 (and PKCS #11 Profiles v3.2) | castellan's authority half, in two opposite directions. ** |
| PULL | TPM 2.0 NV monotonic counters (the standard that closes the rollback gap) | TCG TPM 2.0 Library Specification, Family 2.0, Level 00, Revision 01.83 (published as IS | **personae sealed_record_storage::freshness::FileFreshnes |
| PULL | Credential Exchange Format and Protocol | CXF v1.0 (Proposed Standard, 2025-08-21, with Errata 2026-03-09) / CXP v1.0 (Working Dra | castellan's import path. The README states plainly: 'CXF i |
| ADOPT | FIDO Credential Exchange Format | CXF v1.0 Proposed Standard with Errata, 2026-03-09 | repos/mere/ports/castellan — the README's own open item, ' |
| WATCH | FIDO Credential Exchange Protocol | CXP v1.0 Working Draft, 2024-10-03 | castellan's authority half — it is the only component that |
| PULL | KDBX (KeePass database format) | KDBX 4.1 File Format Specification (format version 0x00040001) | castellan import. KeePassXC is also the closest architectu |
| PULL | 1Password Unencrypted Export (1PUX) and OPVault | 1PUX export format; OPVault design | castellan import, as the pre-CXF fallback for 1Password us |
| PULL | De-facto vault and browser export formats (Bitwarden JSON, Chrome/Firefox CSV, Apple Passwords CSV) | Bitwarden export .json / .csv; Chromium password CSV; Firefox password CSV; Apple Passwo | castellan import — specifically the long tail of users who |
| ADOPT | Freedesktop Secret Service API | Secret Service 0.2 DRAFT (publication date 2026-04-08) | repos/mere/ports/castellan, feature secret-service — alr |
| PULL | XDG Desktop Portal Secret interface | org.freedesktop.portal.Secret, interface version 1 (xdg-desktop-portal ≥ 1.5.0) | Only a hypothetical Flatpak or Snap build of pelt/graphshe |
| ADOPT | HTML autofill field names (autocomplete tokens) | WHATWG HTML Standard, Living Standard — Autofill / autocomplete attribute | repos/genet (the engine and pelt), paired with castellan's |
| PULL | Credential Management API | Credential Management Level 1, W3C Working Draft 2026-02-13 | repos/genet, if and only if WebAuthn support lands there;  |
| ADOPT | A Well-Known URL for Changing Passwords | W3C Working Draft, 3 June 2024 (/.well-known/change-password) | castellan's embeddable half (the 'change this password' af |
| ADOPT | Public Suffix List | publicsuffix.org PSL (public_suffix_list.dat), ICANN and PRIVATE sections | castellan credential scoping (which stored login is offere |
| ADOPT | Web Authentication (WebAuthn) | Web Authentication: An API for accessing Public Key Credentials, Level 3 | castellan as a passkey provider (the chatelaine side, sinc |
| PULL | FIDO Client to Authenticator Protocol | CTAP v2.3 Proposed Standard, 2026-02-26 (v2.3.1 Working Draft, 2026-05-29; v2.2 PS 2025- | Only genet, and only if it needs to talk to roaming authen |
| PULL | Third-party credential-provider seams (platform APIs, not standards) | Windows WebAuthn Plugin APIs / IPluginAuthenticator + WebAuthNPluginAddAuthenticator (Wi | castellan's authority half — but only where a shippable ap |
| WATCH | Linux passkey provider seam (credentialsd / proposed XDG portal) | credentialsd D-Bus service + proposed XDG portal for credential management (linux-creden | castellan on Linux, alongside the Secret Service server it |
| PULL | HIBP Pwned Passwords Range API (k-anonymity) | Have I Been Pwned API v3 — Pwned Passwords range endpoint | castellan's embeddable half — the vault health view, the o |
| ADOPT | Secure Shell (SSH) Agent Protocol | RFC 9987 | repos/mere/crates/personae (the ssh-agent, live on Mark's  |
| PULL | OWASP Application Security Verification Standard | OWASP ASVS 5.0.0 (released 2025-05-30) | castellan and personae as a self-assessment instrument; no |
| SKIP | Organisational assurance regimes (SOC 2, ISO/IEC 27001, PCI DSS, MASVS) | SOC 2 (AICPA Trust Services Criteria); ISO/IEC 27001:2022 (+ Amd 1:2024); PCI DSS v4.0.1 | None. There is no server, no tenancy, no cardholder data e |
| ADOPT | Web Annotation Data Model | W3C Recommendation, 23 February 2017 (REC-annotation-model-20170223) | fleece (emit selectors alongside Article/Block and Ex |
| SKIP | Selectors and States | W3C Working Group Note, 23 February 2017 | None — and that is the point. Anyone in fleece or import r |
| ADOPT | URL Fragment Text Directives (Text Fragments / scroll-to-text) | WICG Draft Community Group Report; upstreaming in progress as whatwg/html PR #11895 | fleece (mint a directive per extracted block); turnstone ( |
| ADOPT | schema.org vocabulary | schema.org release v30.0 | fleece StructuredData (the @type and itemtype values |
| ADOPT | JSON-LD 1.1 | W3C Recommendation, 16 July 2020 (REC-json-ld11-20200716) | fleece extract_structured_data — already parses <script |
| ADOPT | HTML Microdata | WHATWG HTML Living Standard, the Microdata section (html.spec.whatwg.org/multipage/micro | fleece extract_structured_data + microdata_item + col |
| SKIP | RDFa Core 1.1 / RDFa Lite 1.1 | RDFa Core 1.1 – Third Edition, W3C Recommendation, 17 March 2015 | None that justifies it. fleece already gets the only RDFa- |
| ADOPT | The Open Graph protocol | ogp.me (no version number, no release history) | fleece extract_metadata — already harvests og:* with t |
| PULL | microformats2 / h-entry | microformats.org/wiki/h-entry (Living Specification); parsing algorithm at microformats. | gazette, if and when it polls IndieWeb sites; moot (h-card |
| PULL | DCMI Metadata Terms / Dublin Core | DCMI Metadata Terms, DCMI Recommendation 2020-01-20; ISO 15836-1:2017 and ISO 15836-2:20 | fleece Metadata (a DC.* meta-name tier); eidetic corpu |
| SKIP | "The readable article" — no standard exists | n/a — this entry records a verified negative. Nearest de-facto: Mozilla Readability (moz | fleece Article, Block, RootSelector, ExtractionLine |
| SKIP | CSV on the Web (CSVW) | Model for Tabular Data and Metadata on the Web, W3C Recommendation 17 December 2015; wit | None with the right shape. fleece's Block::Table is an e |
| ADOPT | HTML tabular data model and table accessibility semantics | WHATWG HTML Living Standard §4.9 Tabular data (including the 'forming a table' algorithm | fleece Block::Table / TableRow / TableCell (repos/ge |
| WATCH | WAI-ARIA and the accessibility tree as an extraction substrate | WAI-ARIA 1.2 (W3C Recommendation, June 2023); Core Accessibility API Mappings 1.2 (W3C C | fleece, hypothetically. genet-render actually — it already |
| ADOPT | The Atom Syndication Format | RFC 4287 (Proposed Standard, December 2005); companion RFC 5023 (Atom Publishing Protoco | gazette feed polling (explicitly Unbuilt today, engine is  |
| ADOPT | RSS 2.0 and RSS Autodiscovery | RSS 2.0 Specification version 2.0.11 (30 March 2009); RSS Autodiscovery (RSS Advisory Bo | gazette feed polling; mere-crawl as the polling engine; fl |
| PULL | JSON Feed | JSON Feed Version 1.1 (7 August 2020) | gazette feed polling, as a third format behind Atom and RS |
| WATCH | WebSub | W3C Recommendation, 2 June 2026 (revising the Recommendation of 23 January 2018) | gazette feed polling — as the thing it would replace, not  |
| ADOPT | Web Linking and the IANA Link Relation Types registry | RFC 8288 (Proposed Standard, October 2017), obsoletes RFC 5988; IANA Link Relation Types | fleece extract_metadata (already reads rel="canonical" |
| WATCH | Djot | djot syntax reference at djot.net/syntax (no version number on the syntax document); ref | knot (writer::DocumentFormat::Djot; text/x-knot routes |
| ADOPT | CommonMark | CommonMark Spec version 0.31.2, 28 January 2024 | nematic (pulldown-cmark = "0.13", deliberately retained  |
| ADOPT | File System Standard (Origin Private File System) and the File System Access pickers | WHATWG File System Living Standard (fs.spec.whatwg.org) for OPFS and FileSystemHandle; F | mere ports/muniment-opfs-probe and crates/eidetic/mun |
| SKIP | WebDAV | RFC 4918 (Proposed Standard, June 2007), obsoletes RFC 2518; updated by RFC 5689 (Extend | None. knot's projection is a NATIVE filesystem projection, |
| PULL | Netscape Bookmark File Format | No formal specification. DOCTYPE NETSCAPE-Bookmark-file-1. Nearest reference: Microsof | mere/crates/import — detect_bookmark_file_format plus th |
| PULL | OPML 2.0 | OPML 2.0 (opml.org/spec2.opml) | gazette (import and export a subscription list); mere/crat |
| WATCH | EPUB 3.4 and EPUB Accessibility 1.2 | EPUB 3.4, W3C Candidate Recommendation Snapshot, 21 July 2026 (CR-epub-34-20260721); EPU | knot, plausibly — as an EXPORT target for a directory or v |
| SKIP | JATS (Journal Article Tag Suite) | ANSI/NISO Z39.96-2024, JATS: Journal Article Tag Suite, version 1.4 | None. No repo in this workspace publishes, archives or ren |
| PULL | WARC (Web ARChive) file format | ISO 28500:2017 (second edition; cancels and replaces ISO 28500:2009) | mere-crawl (the frontier engine); eidetic corpus (the stor |
| WATCH | WebExtensions API (common core) | W3C WebExtensions Working Group deliverables (chartered; draft state adopted from the We | mere/crates/import — BrowserImportMode::SnapshotBridge a |
| ADOPT | Design Tokens Format Module (DTCG) | Design Tokens 2025.10 — Format Module (W3C Design Tokens Community Group Report) | genet/ports/tabard — the one port whose README already nam |
| ADOPT | Design Tokens Color Module (DTCG) | Design Tokens 2025.10 — Color Module | genet/components/tinct (the derivation math) via genet/por |
| WATCH | Design Tokens Resolver Module (DTCG) | Design Tokens 2025.10 — Resolver Module | genet/ports/tabard, and downstream genet/components/livery |
| ADOPT | CSS Color Module Level 4 | CSS Color Module Level 4 | genet/components/livery — already implemented. components/ |
| ADOPT | CSS Color Module Level 5 | CSS Color Module Level 5 | genet/components/livery — color-mix() and relative color s |
| ADOPT | WCAG 2.2 contrast success criteria | Web Content Accessibility Guidelines (WCAG) 2.2, SC 1.4.3 / 1.4.6 / 1.4.11 | genet/components/tinct — already the basis of the derivati |
| SKIP | APCA / WCAG 3.0 visual contrast | APCA (Accessible Perceptual Contrast Algorithm, base 0.0.98G-4g) and W3C Accessibility G | Would be genet/components/tinct. It is the obvious upgrade |
| ADOPT | CSS user-preference media features and color-scheme | Media Queries Level 5 (prefers-color-scheme, prefers-contrast, prefers-reduced-motion, f | genet/components/livery (parsing, done) and genet/ports/pe |
| ADOPT | XDG Base Directory Specification | XDG Base Directory Specification, Version 0.8 | mere/ports/djinn (src/settings.rs already reads LOCALAPPDA |
| ADOPT | Desktop Entry Specification | Desktop Entry Specification, Version 1.5 | genet/ports/pelt (as a viewer that wants to be openable fr |
| ADOPT | XDG Desktop Portals | XDG Desktop Portal D-Bus interfaces: org.freedesktop.portal.Settings (v2, org.freedeskto | genet/ports/pelt — this is the missing OS-preference sourc |
| PULL | shared-mime-info and Icon Theme Specification | Shared MIME-info Database Specification 0.21 (2018-10-02); Icon Theme Specification 0.13 | genet/ports/pelt (file-manager association and window icon |
| PULL | Windows and macOS application, file-type and URL-scheme registration | Windows: HKCU\Software\Classes ProgID + RegisteredApplications + UserChoice; macOS: Unif | genet/ports/pelt and turnstone on Windows and macOS; mere/ |
| PULL | Web Application Manifest (and protocol_handlers) | Web Application Manifest (W3C) + Manifest Incubations (WICG) for protocol_handlers, file | genet/ports/pelt as a *consumer* (a viewer that installs w |
| ADOPT | Core Accessibility API Mappings | Core Accessibility API Mappings 1.2 (Core-AAM) | genet/components/genet-render (whose Cargo.toml comment al |
| WATCH | EN 301 549 and the European Accessibility Act | EN 301 549 V3.2.1 (harmonised, clause 11 Software) / V4.1.1 (pending); Directive (EU) 20 | genet/ports/pelt and turnstone if either is ever distribut |
| PULL | ICC colour profiles | ICC.1:2022 (profile version 4.4.0.0), published as ISO 15076-1:2025 Edition 3 | genet/components/genet-render and genet/ports/pelt. Verifi |
| WATCH | HDR colour: BT.2100 and the Wayland colour-management protocol | ITU-R BT.2100-3 (PQ/HLG HDR image parameters); wayland-protocols staging/color-managemen | genet/ports/pelt (desktop/smoke_wayland.rs, smoke_windows. |
| ADOPT | Global Privacy Control | Global Privacy Control (W3C) — Sec-GPC header, navigator.globalPrivacyControl, /.well-kn | genet/ports/pelt and turnstone as user agents; the netfetc |
| ADOPT | Public Suffix List | Public Suffix List (publicsuffix.org) | genet's netfetch stack and cookie implementation. The exis |
| PULL | HTTP Strict Transport Security and the preload list | RFC 6797 (HSTS); hstspreload.org preload list | genet's netfetch stack. Verified absent: no HSTS handling  |
| WATCH | Certificate Transparency | RFC 6962 (CT v1), RFC 9162 (CT v2.0), and the Static CT API (C2SP static-ct-api v1.0.0) | genet's TLS/netfetch stack. Absent, as expected for a pre- |
| SKIP | User-Agent Client Hints | HTTP Client Hints (RFC 8942) + User-Agent Client Hints (WICG draft) | genet's netfetch stack, as a would-be sender. Nothing here |
| SKIP | Do Not Track | DNT header / W3C Tracking Preference Expression (TPE) | None. genet has no DNT handling and should not acquire one |
| SKIP | Vendor token and palette interchange formats | Adobe Swatch Exchange (.ase); Figma Variables REST API; Style Dictionary; Tokens Studio | genet/ports/tabard, as candidate import/export formats. |
| ADOPT | WebFinger | RFC 7033 | ports/gazette (already built), gaz HandleKind::Acct |
| ADOPT | JSContact | RFC 9553 (data model), with RFC 9554 (vCard extensions for JSContact) and RFC 9555 (vCar | crates/dramatis/gaz (the contact store), ports/gazette Led |
| PULL | vCard 4.0 and jCard | RFC 6350 (vCard 4.0), RFC 7095 (jCard) | gaz import/export; ports/gazette contact-import UX (the br |
| SKIP | CardDAV, JMAP Core/Mail/Contacts | RFC 6352 (CardDAV); RFC 8620 (JMAP Core), RFC 8621 (JMAP Mail), RFC 9610 (JMAP Contacts) | None. gazette explicitly disclaims delivery ('not a delive |
| PULL | Decentralized Identifiers (DIDs), with did:key and did:web | W3C DID Core 1.1; did:key and did:web method specs (W3C CCG) | crates/dramatis/gaz (HandleKind::Did already exists, carry |
| ADOPT | Verifiable Credentials Data Model 2.0 | W3C VC Data Model 2.0 | crates/dramatis/emblem (the graded identity proof), ports/ |
| PULL | Nostr NIP-05 (DNS-based internet identifiers) | NIP-05 | ports/gazette (README names NIP-05 as landing 'beside' Web |
| PULL | ActivityPub (with Activity Streams 2.0) | W3C ActivityPub; W3C Activity Streams 2.0 | ports/gazette (already classifies ActivityPub actor links  |
| PULL | HTTP Message Signatures | RFC 9421 | Only if gazette or moot ever writes to the fediverse. No c |
| WATCH | AT Protocol (atproto) | draft-newbold-at-architecture, draft-holmgren-at-repository, and the IETF ATP working gr | ports/gazette — HandleKind::Did already accepts did:plc, a |
| ADOPT | Messaging Layer Security (MLS) | RFC 9420 | ports/moot — the murmur surface (invitation-scoped convers |
| WATCH | More Instant Messaging Interoperability (MIMI) | draft-ietf-mimi-protocol, draft-ietf-mimi-content, draft-ietf-mimi-arch, draft-ietf-mimi | ports/moot (murmur) — speculative only |
| SKIP | Matrix | Matrix Specification v1.16 (client-server, server-server, and the Olm/Megolm ratchets) | None. moot/murmur is unimplemented and its README scopes i |
| PULL | Signal protocol: Double Ratchet, X3DH, PQXDH, SPQR | Signal specifications (Double Ratchet; X3DH; PQXDH; Sparse Post-Quantum Ratchet) | crates/murm — the bilateral lane (design_docs/murm_docs/te |
| PULL | WebRTC (and SIP) | W3C WebRTC: Real-Time Communication in Browsers; IETF RTCWEB suite (RFC 8825 overview, 8 | ports/moot (murmur) — 'calls when the transport and media  |
| PULL | Reticulum Network Stack and LXMF | RNS (Reticulum Network Stack, manual v1.5.0) and LXMF (Lightweight Extensible Message Fo | ports/signalman (already built on it via retinue/postilion |
| SKIP | FCC Part 97 — prohibited transmissions (amateur service) | 47 CFR §97.113(a)(4), with §97.113(a)(3) | repos/retinue (all of it), ports/signalman — this constrai |
| ADOPT | Unlicensed ISM regulatory floor: FCC Part 15.247, ETSI EN 300 220-2, ARIB STD-T108 | 47 CFR §15.247 (US, 902–928 MHz); ETSI EN 300 220-2 V3.3.1 (2025-03) (EU SRD 25–1000 MHz | repos/retinue/crates/radio-hand (region.rs, executive.rs)  |
| SKIP | LoRaWAN | LoRa Alliance TS001 (L2 specification) and RP002 Regional Parameters | None. retinue uses raw LoRa PHY via SX1262 with its own me |
| PULL | AX.25, KISS, and APRS | AX.25 Link Access Protocol for Amateur Packet Radio v2.2 (July 1998); the KISS protocol  | repos/retinue — KISS is **already implemented** in crates/ |
| ADOPT | safetensors | safetensors format specification (Hugging Face) | crates/intel/esp — already a dependency (safetensors = "0 |
| PULL | GGUF | GGUF file format specification (ggml docs/gguf.md), format version 3 | ports/distillery — no current GGUF reference anywhere in t |
| SKIP | ONNX, and OCI-based model packaging (ModelPack) | ONNX (Open Neural Network Exchange) IR and opset; CNCF ModelPack Specification over OCI  | None. distillery's model-manifest browsing is unbuilt; esp |
| WATCH | Model Context Protocol (MCP) | MCP specification, revision 2026-07-28 | ports/alembic — the workshop half (agent identity and purp |
| PULL | OpenAI-compatible HTTP API (Chat Completions) | POST /v1/chat/completions — the de-facto local-inference interface | ports/distillery — the streaming console and any path wher |
| ADOPT | EU AI Act — Article 50 transparency obligations | Regulation (EU) 2024/1689 (AI Act), Article 50; as amended by the AI Digital Omnibus | ports/distillery and ports/alembic — anything that generat |
| SKIP | Job, lease and heartbeat semantics for a P2P device ring | No applicable standard. Nearest neighbours surveyed and rejected: Kubernetes coordinatio | ports/distillery (mesh host supervisor, leases, heartbeats |
| ADOPT | MIDI 1.0 Detailed Specification (incl. System Real Time clock and transport) | MMA/AMEI "Complete MIDI 1.0 Detailed Specification", document version 96.1 (3rd edition) | repos/woodshed — crates/woodshed-audio/src/midi.rs (MidiIn |
| PULL | Standard MIDI File | MMA RP-001, "Standard MIDI Files 1.0" (SMF), incorporated into the Complete MIDI 1.0 Det | repos/woodshed — the Set/Rehearsal model (crates/woodshed- |
| WATCH | MIDI 2.0 — Universal MIDI Packet and MIDI-CI | UMP and MIDI 2.0 Protocol Specification M2-104-UM v1.1.2; MIDI-CI M2-101; Common Rules f | repos/woodshed — would replace or sit beside crates/woodsh |
| ADOPT | MusicXML | MusicXML 4.0, W3C Music Notation Community Group Final Report, 7 June 2021 | repos/woodshed — crates/woodshedding (chord.rs, scale.rs,  |
| WATCH | MNX | MNX 1.0 draft specification (W3C Music Notation Community Group) | None today. Would be the same woodshed surface as MusicXML |
| SKIP | Music Encoding Initiative | MEI Guidelines 5.1 (released 22 January 2024); chapter 7, "Repertoire: String Tablature" | None. Would be the same woodshed notation surface as Music |
| PULL | ABC notation | The abc music standard 2.1 (December 2011); abc 2.2 referenced as future work and never  | repos/woodshed — the theory/repertoire side (crates/woodsh |
| SKIP | Guitar Pro file formats and alphaTex | Guitar Pro .gp3/.gp4/.gp5 (binary), .gpx (GP6 container), .gp (GP7/GP8 zip + Content/sco | repos/woodshed — would be an import path only. No export c |
| PULL | RIFF/WAVE, Broadcast Wave Format and BW64 | Microsoft/IBM RIFF + WAVE; EBU Tech 3285 v2 (20 May 2011) "Specification of the Broadcas | repos/hocket — crates/hocket-engine export path (stereo f3 |
| ADOPT | FLAC | RFC 9639, "Free Lossless Audio Codec (FLAC)", Proposed Standard, December 2024 (IETF CEL | repos/hocket — the .hock container's media entries (curr |
| PULL | Opus | RFC 6716 (Definition of the Opus Audio Codec, Standards Track, Sep 2012); RFC 8251 (Upda | repos/pipit — the tier above its own codecs, for links tha |
| ADOPT | Loudness measurement and normalisation | Recommendation ITU-R BS.1770-5 (11/2023), "Algorithms to measure audio programme loudnes | repos/hocket — crates/audio-primitives (which already owns |
| PULL | ReplayGain 2.0 | ReplayGain 2.0 specification (HydrogenAudio Knowledgebase wiki) | repos/hocket — export path, and the .hock media entries  |
| PULL | Audio metadata tag formats | APEv2 (HydrogenAudio spec); Vorbis comments, now normatively defined in RFC 9639 §8.6; I | repos/wavicle — .wv files, whose native tag format is AP |
| PULL | Open Sound Control | OSC 1.0 specification (2002); OSC 1.1 (2009, described in a NIME conference paper rather | repos/hocket — transport control and session state for a l |
| SKIP | Ableton Link | Ableton Link (github.com/Ableton/link) — a C++ library, with no published protocol speci | repos/hocket — this is exactly what a cross-platform loop  |
| PULL | CLAP (CLever Audio Plug-in) | CLAP 1.2.10 (tagged 13 July 2026); MIT licence | None today, by doctrine. Would be repos/hocket (as host, f |
| WATCH | VST3 and ASIO (Steinberg SDKs) | VST 3 SDK, MIT licence since SDK version 3.8; ASIO SDK, now GPLv3+ / proprietary dual | repos/hocket — Windows is the primary platform (per the us |
| ADOPT | Subjective speech quality testing | ITU-T P.800 (08/1996), "Methods for subjective determination of transmission quality"; I | repos/pipit — the only crate in the lane making codec qual |
| SKIP | Objective speech quality metrics — PESQ, POLQA, ViSQOL | ITU-T P.862 (02/2001) PESQ — **withdrawn 5 January 2024**; ITU-T P.863 (03/2018) POLQA + | repos/pipit — the objective half of any quality claim, pai |
| PULL | International Phonetic Alphabet and its Unicode encoding | IPA chart (revised to 2015, itself a minor revision of 2005; re-issued annually since 20 | repos/mora — src/phone.rs (the language-neutral phone mode |
| ADOPT | Language identification tags | BCP 47 — currently RFC 5646 ("Tags for Identifying Languages", Sep 2009) and RFC 4647 (" | repos/mora — src/lib.rs and the module structure. Today a  |
| PULL | Unicode Text Segmentation | Unicode Standard Annex #29, "Unicode Text Segmentation", Revision 47, for Unicode 17.0.0 | Not mora itself. repos/mora reads phones, not spelling, an |
| SKIP | Speech Synthesis Markup Language | SSML Version 1.1, W3C Recommendation, 7 September 2010 | repos/mora — the question the lane brief asks directly: is |
| PULL | Praat TextGrid and ToBI prosodic annotation | Praat TextGrid file format (long text, short text, and binary variants — defined by the  | repos/mora — an import path if mora ever wants time-aligne |
| PULL | FITS (Flexible Image Transport System) | FITS Standard Version 4.0 | turquet — but there is no consumer today. A repo-wide grep |
| PULL | FITS World Coordinate System (WCS) | WCS Paper I (Greisen & Calabretta 2002), Paper II (Calabretta & Greisen 2002), Paper III | turquet, if it ever grows a sky view or chart renderer. ne |
| PULL | IVOA VOTable | IVOA Recommendation VOTable 1.5 | turquet, only if it grows a catalogue-query or table-expor |
| PULL | IVOA SAMP (Simple Application Messaging Protocol) | IVOA Recommendation SAMP 1.3 | turquet, and any turquet-backed viewer in pelt or graphshe |
| SKIP | IVOA HiPS and Simple Cone Search | IVOA Recommendation HiPS 1.0 (Hierarchical Progressive Survey); IVOA Recommendation Simp | None. turquet computes positions and returns typed states; |
| ADOPT | ICRS / ICRF3 celestial reference frame | IAU 2018 Resolution B2 (ICRF3); ICRS per IAU 1997 Resolution B2 and IAU 2000 Resolution  | turquet — already the implicit frame at the head of its ty |
| WATCH | Continuous UTC — the leap second decision | CGPM 27th meeting (2022) Resolution 4; 28th CGPM Draft Resolution C, 'On the technical a | turquet (hifitime's leap-second table, every UTC-TAI-TT co |
| PULL | CCSDS Orbit Data Messages (OMM), and TLE/SGP4 | CCSDS 502.0-B-3, 'Orbit Data Messages', Recommended Standard (Blue Book), Issue 3, April | turquet, if Earth satellites ever join the ten solar-syste |
| PULL | IAU constellation boundaries and star nomenclature | IAU constellation boundaries per Delporte (1930), following the IAU's 1928 adoption of 8 | turquet if it answers 'which constellation is this positio |
| ADOPT | NAIF SPICE kernels and JPL DE ephemerides | NASA/NAIF SPICE Toolkit N0067; SPK (Spacecraft and Planet Kernel) file format per NAIF r | turquet — and this is already decided, landed, and should  |
| ADOPT | W3C WebGPU and WGSL | W3C WebGPU; W3C WebGPU Shading Language (WGSL) | netrender (261 WGSL and 149 SPIR-V references), wgpu-graft |
| ADOPT | SPIR-V and Vulkan | SPIR-V Specification version 1.6, Revision 7 (12 March 2026); Vulkan 1.4 (spec build 1.4 | netrender (SPIR-V is what WGSL lowers to on the Vulkan bac |
| ADOPT | CSS Color Module Level 4 | W3C CSS Color Module Level 4 | netrender, genet-livery, the tinct crate, and tabard. |
| WATCH | ICC profiles, BT.2100 HDR, and Display P3 | ICC.1:2022 (profile v4.4), published as ISO 15076-1:2025 Ed. 3; ISO 20677:2019 (iccMAX); ITU-R BT.2100-3 | netrender / wgpu family, tabard |
| ADOPT | OpenType and COLRv1 colour fonts | OpenType specification version 1.9.1 (page dated 2024-05-14); ISO/IEC 14496-22 'Open Fon | netrender (netrender_text, swash; COLR appears 15 times  |
| ADOPT | Design Tokens Format Module | Design Tokens Format Module, version 2025.10 | tabard — repos/genet/ports/tabard/README.md line 14 name |
| PULL | IconVG | IconVG specification, spec/iconvg-spec.md at nigeltao/iconvg, Apache-2.0. The house de | repos/iconvg itself. The motivating downstream is 'a gra |
| PULL | SVG 1.1 / SVG 2 / SVG Tiny | SVG 1.1 (Second Edition), W3C Recommendation, 16 August 2011; SVG 2, W3C Candidate Recom | genet — an SVG-capable engine must eventually. iconvg only |
| SKIP | Lottie | Lottie Specification version 1.0.1 | None. No repo in this lane has an animated-vector consumer |
| PULL | Tiled TMX map format | TMX Map Format version 1.8 (documented against Tiled 1.10+, with the reference describin | isometry. |
| PULL | Universal VTT (dd2vtt / uvtt / df2vtt) | 'Universal VTT' export format — .dd2vtt, .uvtt, .df2vtt. **No document number exis | isometry — this is the format a GM's existing purchased an |
| SKIP | WebRTC and the data channel | W3C 'WebRTC: Real-Time Communication in Browsers', REC 13 March 2025 (amended); RFC 8831/8832/8834 | isometry host-plus-players session model |
| ADOPT | Gemini protocol and gemtext | Gemini network protocol specification v0.24.1; Gemini hypertext format ('gemtext') speci | repos/smolweb/crates/gemini-protocol — implemented, publ |
| ADOPT | The RFC-backed small-web protocols: gopher, finger, DICT | RFC 1436 (The Internet Gopher Protocol, March 1993); RFC 4266 (The gopher URI Scheme, No | smolweb/crates/{gopher,finger,dict}-protocol — all imple |
| ADOPT | The nine unspecified small-web protocols | **No standard designator exists for any of these.** Nex (spec at nex://nightfall.city/n | Nine smolweb crates, all implemented and published to crat |
| ADOPT | URI and IRI syntax | RFC 3986 (STD 66), 'Uniform Resource Identifier (URI): Generic Syntax', January 2005; RF | Every smolweb crate — scorpion-protocol/src/request.rs c |
| ADOPT | TLS 1.3 | RFC 9846 (July 2026; obsoletes RFC 8446/5077/5246/6961/7627/8422) | gemini, titan, misfin, kepler; scorpion-protocol |
| SKIP | glTF 2.0 — and the games standards vacuum | Khronos glTF 2.0; ISO/IEC 12113:2022, 'Information technology — Runtime 3D asset deliver | mesocosm, paredros, isometry — and the honest answer is th |

---

## 8. Method, and what it is not good for

Seven parallel survey lanes with live web access, each followed by an
adversarial fact-checker instructed to check only against primary sources
(ietf.org datatracker, w3.org/TR, nist.gov, fidoalliance.org, freedesktop.org,
khronos.org, ivoa.net, midi.org and each standard's canonical site) and to
default to suspicion. Fourteen agents, ~2.2M tokens, ~1 220 tool calls across
the initial run and one re-run.

**The fact-checking pass corrected thirty-two of the 174 entries.** That number
is the main methodological result, and it is why this brief carries verification
dates rather than bare statuses. A representative sample of what it caught: a
WebAuthn Proposed Recommendation that does not exist (there is no PR document;
W3C Process 2023 advances a CR Snapshot straight to REC); the wrong GitHub org
for the Public Suffix List; an atproto IETF working group whose datatracker page
reads "Abandoned" because the real group is `atp`, not `atproto`, and a cited
architecture draft that expired at revision -00; a Signal specification named
"SPQR" that has no such document (the citable name is *The ML-KEM Braid*); a
Matrix spec three releases stale; a California statute given an operative date it
does not have; MEI 5.1 misdated by a full year; a MusicXML URL that had moved
twice; and an ICC "orphaned from ISO" gotcha that ISO 15076-1:2025 had dissolved
ten months earlier.

**The sharpest finding is about confidence, not about any one standard.** Two
entries were self-flagged by their surveyor as recalled from training data rather
than re-fetched, on the reasoning that a long-settled specification does not
move. Both had moved, and both mattered: TLS 1.3's RFC 8446 was obsoleted by
RFC 9846 in July 2026, and WebRTC's Recommendation is a 13 March 2025 document
carrying substantive amendments rather than the finished 2021 one. The belief
that a settled standard is safe to quote from memory is precisely what produced
both errors. Treat that as the standing rule for this document's successors.

**Sources that could not be reached, so are recorded as unconfirmed:**

| Source | Blocked | Claim left unverified |
|---|---|---|
| iso.org | HTTP 403 to automated fetch | ISO 16:1975 current edition; ISO catalogue entries generally |
| trustedcomputinggroup.org | HTTP 403 on the listing page | Current TPM 2.0 Library revision (1.85 is search-index evidence, not a page read) |
| registry.khronos.org | HTTP 403 | SPIR-V Revision 7's 2026-03-12 date (the revision number itself is confirmed) |
| wiki.hydrogenaudio.org | HTTP 403 | ReplayGain 2.0's −18 LUFS reference and storage conventions |

Re-pin each of those by hand before any of them enters a plan.

**Three further limits.** The consumer column in §7 is the survey's assignment,
not a ruling — several entries guess at which crate would own a standard, and
those guesses have not been checked against the code. The IAU Catalog of Star
Names has no stable total and must be read off the catalogue at time of use;
do not quote a figure. And when citing DTCG, use the pinned
`designtokens.org/tr/2025.10/` rather than the rolling draft URL, or the version
claim silently drifts to whatever is current.

This brief should be re-checked against primary sources before it is used to
justify work, and certainly before 2027 — §1 exists because the last inventory
went stale in nine months.

---

## 9. Decisions this leaves open

Recorded here rather than resolved, because each has more than one defensible
answer and the choice is Mark's.

1. **The v2-root question (§2.1).** Recording KDF parameters in the vault format
   and giving `derive_child` a versioned context are one decision, not two, and
   both are format changes. Take them now, before the vault holds real
   credentials, or defer and accept that the migration gets harder monotonically?
   If taken: which RFC 9106 §4 option — the second recommended (m=64 MiB, t=3,
   p=4) is the realistic target for a once-per-session unlock, but it is a
   perceptible pause on weak hardware, and the current 19 MiB is a deliberate
   floor rather than an accident.
2. **CXF import policy (§2.3).** CXF v1.0 defines 17 credential types including
   `Passport`, `DriversLicense` and `CreditCard`. Which does castellan store,
   which does it drop on the floor, and which does it quarantine and tell the
   user about? This is a product decision that blocks the parser, not a
   consequence of it.
3. **Whether a `dtcg` crate gets founded (§3.2).** There is no Rust
   implementation of a now-stable W3C-CG format that tabard needs anyway. That
   is either a small well-scoped piece of leverage or a distraction from tabard
   itself. Naming-ledger territory if the answer is yes.
4. **Where mDNS/DNS-SD lands (§4).** It is the largest hole in the stack and it
   has no obvious owner: distillery's ring, moot's places and turnstone's shared
   places all need it, which is an argument for a shared crate rather than three
   implementations, and an argument that it belongs to none of them.
5. **Emblem versus Verifiable Credentials 2.0 (§5).** VC 2.0 is close to what
   emblem already is. Adopting it buys interoperability and costs a large
   specification surface plus a JSON-LD dependency; staying proprietary keeps
   emblem small. Worth deciding before emblem's shape is fixed rather than after.
6. **Djot's wire identity (§5).** There is no registered media type. Serve
   `text/x-djot` and accept an unregistered name, serve rendered HTML, or pursue
   registration. Any of the three is defensible; drifting into one is not.
7. **One out-of-scope one-liner.** `smolweb/crates/scorpion-protocol/src/tls.rs:29`
   cites RFC 8446, now obsoleted by RFC 9846. Flagged, not fixed — smolweb was
   outside this pass's scope.
