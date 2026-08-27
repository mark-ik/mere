// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

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
