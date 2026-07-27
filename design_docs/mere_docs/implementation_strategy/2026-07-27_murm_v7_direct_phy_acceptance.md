# Murm V7 Direct-PHY Acceptance

**Status: passed headed on 2026-07-27.**

This receipt closes the direct-PHY/RF arm of V7 in the
[low-power managed-network plan](./2026-07-24_low_power_managed_network_plan.md).
It does not close the V0/V2 power and sleep measurements.

## Hardware

- client: Heltec ESP32-S3 V4 direct-PHY firmware, USB VID:PID
  `303a:1001`, enumerated as `COM6`;
- server: nRF/T114 direct-PHY firmware, USB VID:PID `1915:521f`,
  enumerated as `COM10`;
- PHY: 906.875 MHz, 250 kHz bandwidth, SF8, CR 4/5, preamble 16,
  sync word `0x12`, explicit header, CRC, normal IQ, 17 dBm;
- Reticulum link MTU: 255 bytes;
- reliable window: one frame, with a two-second initial RTT estimate.

The ports were re-queried by VID/PID immediately before each headed run.

## Executable receipt

The standalone probe is
`crates/probes/murm-direct-phy`. It is excluded from the main workspace on
purpose because it joins Mere to the live sibling Retinue checkout.

```powershell
$env:CARGO_TARGET_DIR='C:\t\murm-direct-phy-target'
cargo run --manifest-path crates/probes/murm-direct-phy/Cargo.toml --offline -- COM6 COM10
```

The passing output was:

```text
radios online: COM6=client, COM10=server, 906875000 Hz/250000 Hz
admitted: one signed Murm post landed over direct PHY
refused: disabled owner rule stopped the post before Murm
MURM V7 DIRECT-PHY HEADED PASSED
```

The admitted case uses a valid Personae delegation, binds its Notochord proof
to the Reticulum link observed independently at both ends, and ingests one
signed Murm post. The refused case changes the owner's service rule to
`Disabled`; both peers receive `ServiceNotOffered`, and the conversation
still contains exactly the one previously admitted post.

## Defects found by the headed run

1. Mere's authenticated announce was 263 bytes, eight bytes over the
   direct-PHY frame cap. The Retinue identity now uses Mere's master Ed25519
   key as its signing half and derives only the X25519 half. The already
   verified Reticulum announce therefore carries the `PeerID` without a
   duplicate 96-byte key/signature binding.
2. Retinue's best-effort stream relay ignored the negotiated link MTU and
   emitted a 435-byte encrypted packet. It now derives the plaintext chunk
   from the negotiated whole-frame size. A 1 KiB stream regression keeps both
   mock 255-byte Tulle drivers alive.
3. Best-effort delivery was not sufficient for the multi-frame policy
   handshake over LoRa. `ReticulumTransport` can now select Retinue's reliable
   stream lane and configure its initial RTT and in-flight window. Reliable
   accepts retain their destination and physical interface, so Notochord
   receives the same honest ingress facts as on the best-effort path.
4. A packet interface can now request an immediate announce after its physical
   driver starts. This avoids treating a very short periodic announce interval
   as a substitute for attachment ordering on a half-duplex channel.

## Evidence boundary

This was real USB control and real RF between two boards. It proves direct-PHY
discovery, reliable Reticulum stream setup, link-bound admission, Murm ingest,
owner refusal, and orderly shutdown.

Both boards remained USB-powered in their development personalities. The run
did not exercise the UART low-power personality, ESP32-S3 Light-sleep, DIO1
wake, continuous receive across sleep, current, peak draw, or energy. Those
remain the meter-backed V0/V2 bench gate.
