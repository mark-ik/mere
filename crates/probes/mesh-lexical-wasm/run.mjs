// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Loads the probe's wasm32-unknown-unknown module in Node's WebAssembly engine
// and prints the canonical-output length and blake3 digest.

import { readFile } from "node:fs/promises";

const wasmPath = process.argv[2];
if (!wasmPath) {
  console.error("usage: node run.mjs <module.wasm>");
  process.exit(2);
}

const { instance } = await WebAssembly.instantiate(await readFile(wasmPath), {});
const { canonical_len, receipt_digest, memory } = instance.exports;

const len = canonical_len();
const ptr = receipt_digest();
const digest = Buffer.from(new Uint8Array(memory.buffer, ptr, 32)).toString("hex");

console.log(`canonical_len ${len}`);
console.log(`digest ${digest}`);
