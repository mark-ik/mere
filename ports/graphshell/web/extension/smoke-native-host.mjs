// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

import { randomBytes } from "node:crypto";
import { mkdirSync } from "node:fs";
import { endianness } from "node:os";
import { resolve } from "node:path";
import { spawn } from "node:child_process";

const [binaryArgument, scratchArgument, browser = "chromium"] = process.argv.slice(2);
if (!binaryArgument || !scratchArgument) {
  throw new Error(
    "usage: node smoke-native-host.mjs NATIVE_HOST SCRATCH_ROOT [chromium|firefox]",
  );
}
if (!["chromium", "firefox"].includes(browser)) {
  throw new Error(`unsupported browser: ${browser}`);
}

const binary = resolve(binaryArgument);
const scratch = resolve(scratchArgument);
mkdirSync(scratch, { recursive: true });

const launcherArguments = browser === "firefox"
  ? ["org.mere.graphshell.firefox.json", "graphshell@mere.systems"]
  : ["chrome-extension://oajkkocppbpbmfblepgbiidagliniofd/"];
const child = spawn(binary, launcherArguments, {
  env: {
    ...process.env,
    LOCALAPPDATA: scratch,
    PERSONAE_PASSPHRASE: "graphshell-h4d-smoke-passphrase",
  },
  stdio: ["pipe", "pipe", "pipe"],
  windowsHide: true,
});

let stdout = Buffer.alloc(0);
let stderr = "";
const readers = [];

child.stdout.on("data", (chunk) => {
  stdout = Buffer.concat([stdout, chunk]);
  drainFrames();
});
child.stderr.setEncoding("utf8");
child.stderr.on("data", (chunk) => {
  stderr += chunk;
});

function drainFrames() {
  while (stdout.length >= 4 && readers.length > 0) {
    const length = endianness() === "LE"
      ? stdout.readUInt32LE(0)
      : stdout.readUInt32BE(0);
    if (length > 1024 * 1024) {
      readers.shift().reject(new Error(`oversized native message: ${length}`));
      return;
    }
    if (stdout.length < length + 4) {
      return;
    }
    const body = stdout.subarray(4, length + 4);
    stdout = stdout.subarray(length + 4);
    readers.shift().resolve(JSON.parse(body.toString("utf8")));
  }
}

function readMessage() {
  return new Promise((resolveFrame, rejectFrame) => {
    readers.push({ resolve: resolveFrame, reject: rejectFrame });
    drainFrames();
  });
}

function writeMessage(message) {
  const body = Buffer.from(JSON.stringify(message), "utf8");
  const prefix = Buffer.alloc(4);
  if (endianness() === "LE") {
    prefix.writeUInt32LE(body.length);
  } else {
    prefix.writeUInt32BE(body.length);
  }
  child.stdin.write(Buffer.concat([prefix, body]));
}

function responseBody(message) {
  if (message.type !== "response") {
    throw new Error(`expected response, got ${JSON.stringify(message)}`);
  }
  if (message.response.body.Err) {
    throw new Error(message.response.body.Err.message);
  }
  return message.response.body.Ok;
}

const challenge = await readMessage();
if (challenge.type !== "challenge") {
  throw new Error(`expected challenge, got ${JSON.stringify(challenge)}`);
}
writeMessage({
  type: "connect",
  schema: "mere.graphshell/browser-connect/v1",
  host_nonce: challenge.challenge.host_nonce,
  client_nonce: randomBytes(32).toString("base64url"),
});

const connected = await readMessage();
if (connected.type !== "connected") {
  throw new Error(`expected connected, got ${JSON.stringify(connected)}`);
}

writeMessage({
  type: "request",
  request: {
    id: 1,
    body: {
      Open: {
        version: { major: 1, minor: 3 },
        capabilities: { capabilities: ["PortableCard"] },
      },
    },
  },
});
const opened = responseBody(await readMessage()).Opened;
const projection = opened.descriptor.projections[0]?.request;
if (!projection) {
  throw new Error("identity projection was not disclosed");
}

writeMessage({
  type: "request",
  request: { id: 2, body: { Snapshot: projection } },
});
const snapshot = responseBody(await readMessage()).Snapshot;
const firstOffer = Object.values(snapshot.presentation.offers)
  .flat()
  .find((offer) => offer.codec === "PortableCardV1");
if (!firstOffer) {
  throw new Error("identity snapshot contained no portable card");
}

const chunks = [];
let offset = 0;
let totalLength = null;
let requestId = 3;
for (;;) {
  writeMessage({
    type: "request",
    request: {
      id: requestId,
      body: {
        ResourceChunk: {
          session: snapshot.session,
          resource: firstOffer.resource,
          offset,
          length: 64 * 1024,
        },
      },
    },
  });
  const chunk = responseBody(await readMessage()).ResourceChunk;
  if (!chunk || chunk.offset !== offset) {
    throw new Error(`resource chunk did not continue at ${offset}`);
  }
  if (totalLength === null) {
    totalLength = chunk.total_len;
  } else if (totalLength !== chunk.total_len) {
    throw new Error("resource chunk changed the total length");
  }
  const bytes = Buffer.from(chunk.bytes, "base64");
  if (bytes.length === 0 && offset < totalLength) {
    throw new Error("resource chunk made no progress");
  }
  chunks.push(bytes);
  offset += bytes.length;
  requestId += 1;
  if (offset >= totalLength) {
    break;
  }
}
const resource = Buffer.concat(chunks);
if (resource.length !== totalLength) {
  throw new Error(`resource assembled ${resource.length} bytes, expected ${totalLength}`);
}
const resourceText = resource.toString("utf8");
const card = JSON.parse(resourceText);
if (resourceText.includes("PRIVATE KEY")) {
  throw new Error("browser resource disclosed private key material");
}

writeMessage({
  type: "request",
  request: { id: requestId, body: "Close" },
});
const closed = responseBody(await readMessage());
if (closed !== "Closed") {
  throw new Error(`session did not close cleanly: ${JSON.stringify(closed)}`);
}
child.stdin.end();

const exitCode = await new Promise((resolveExit, rejectExit) => {
  child.once("error", rejectExit);
  child.once("exit", resolveExit);
});
if (exitCode !== 0) {
  throw new Error(`native host exited ${exitCode}: ${stderr}`);
}

process.stdout.write(`${JSON.stringify({
  schema: "graphshell.h4d.native-messaging-smoke/v1",
  launcher: connected.launcher,
  session_bound: connected.session === snapshot.session,
  subject_present: connected.subject.length > 0,
  projection: opened.descriptor.label,
  portable_card: card.title,
  private_material_absent: true,
  closed: true,
}, null, 2)}\n`);
