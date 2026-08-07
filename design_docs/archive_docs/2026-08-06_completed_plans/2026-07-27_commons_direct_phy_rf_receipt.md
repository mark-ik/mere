# Commons Direct-PHY RF Receipt

**Date:** 2026-07-27  
**Status:** passed on real hardware

## Claim

One canonical Commons operation can cross a direct-PHY radio link through
Retinue without changing its signed or encrypted representation.

## Bench

- Receiver: Heltec Wireless Stick Lite V4 on `COM6`
- Publisher: Nordic T114 on `COM10`
- Profile: 906.875 MHz, BW 250 kHz, SF8, CR 4/5, 16-symbol preamble,
  sync word `0x12`, explicit header, CRC, 17 dBm
- Carrier: Retinue `Endpoint` and `Resource`, request window 1, link MTU 255
- Operation:
  `99d5a2eec8a7d36ac7a3eac5cde2ebe7f2458e360f06b649a764ce8f0a287919`
- Canonical encoded size: 1,177 bytes

The headed carriage completed byte-exact in 9.9 seconds:

```text
radios online: COM6=receiver, COM10=sender
discovery: byte-carriage destination announced over direct PHY
carriage: 1177 bytes passed byte-exact in 9.9s
RETINUE DIRECT-PHY BYTES HEADED PASSED
```

The Commons verifier then:

1. decoded the received canonical p2panda operation;
2. recomputed the same operation id;
3. verified its author signature;
4. decrypted its p2panda Data `GroupCiphertext` with the saved Stickleback
   keyring; and
5. matched the expected `commons.message` event.

```text
verified Commons operation 99d5a2eec8a7d36ac7a3eac5cde2ebe7f2458e360f06b649a764ce8f0a287919 after direct PHY (1177 bytes)
COMMONS DIRECT-PHY RF RECEIPT PASSED
```

## Diagnostic observation

An earlier headed attempt timed out in this direction. A direct Resource trace
then carried the same 1,177 bytes successfully and exposed one malformed
request context. The receiver's three-second retry recovered and completed the
transfer. The following full Endpoint run passed. This receipt therefore closes
carrier identity and real-RF carriage, but it is not a packet-loss or range
characterization.

The Retinue bench programs are:

- `crates/retinue/examples/direct_phy_bytes.rs` for the full Endpoint path;
- `crates/retinue/examples/direct_phy_resource_trace.rs` for packet-level
  Resource diagnosis; and
- `crates/moot/commons/examples/commons_rf_fixture.rs` for fixture
  emission and domain verification.
