// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

(() => {
  // Keep the request below the protocol's 512 KiB raw ceiling. A smaller
  // window leaves room for the native-messaging envelope and keeps one
  // outstanding request per resource as the flow-control boundary.
  const CHUNK_LENGTH = 64 * 1024;

  function sameBytes(left, right) {
    return Array.isArray(left)
      && Array.isArray(right)
      && left.length === right.length
      && left.every((byte, index) => byte === right[index]);
  }

  function validateHash(hash, label) {
    if (!Array.isArray(hash) || hash.length !== 32
      || !hash.every((byte) => Number.isInteger(byte) && byte >= 0 && byte <= 255)) {
      throw new Error(`${label} is not a 32-byte content address.`);
    }
  }

  function decodeBase64(encoded) {
    if (typeof encoded !== "string") {
      throw new Error("resource chunk bytes are not base64 text.");
    }
    let binary;
    try {
      binary = atob(encoded);
    } catch {
      throw new Error("resource chunk bytes are not valid base64.");
    }
    const bytes = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index += 1) {
      bytes[index] = binary.charCodeAt(index);
    }
    return bytes;
  }

  class ResourceAssembly {
    constructor(session, resource) {
      if (typeof session !== "string" || session.length === 0) {
        throw new Error("resource chunk session is missing.");
      }
      validateHash(resource, "resource chunk resource");
      this.session = session;
      this.resource = resource;
      this.parts = [];
      this.received = 0;
      this.totalLength = null;
    }

    nextRequest() {
      return {
        ResourceChunk: {
          session: this.session,
          resource: this.resource,
          offset: this.received,
          length: CHUNK_LENGTH,
        },
      };
    }

    accept(response) {
      if (!response || typeof response !== "object") {
        throw new Error("native host returned no resource chunk.");
      }
      if (response.session !== this.session) {
        throw new Error("resource chunk belongs to another session.");
      }
      validateHash(response.resource, "resource chunk resource");
      validateHash(response.chunk, "resource chunk address");
      if (!sameBytes(response.resource, this.resource)) {
        throw new Error("resource chunk belongs to another resource.");
      }
      if (!Number.isSafeInteger(response.offset) || response.offset !== this.received) {
        throw new Error(
          `resource chunk arrived at ${response.offset}; expected ${this.received}.`,
        );
      }
      if (!Number.isSafeInteger(response.total_len) || response.total_len < 0) {
        throw new Error("resource chunk has an invalid total length.");
      }
      if (this.totalLength === null) {
        this.totalLength = response.total_len;
      } else if (this.totalLength !== response.total_len) {
        throw new Error("resource chunk changed the resource length.");
      }

      const bytes = decodeBase64(response.bytes);
      const next = this.received + bytes.length;
      if (next > this.totalLength) {
        throw new Error("resource chunk extends past the resource length.");
      }
      this.parts.push(bytes);
      this.received = next;
      if (this.received < this.totalLength) {
        if (bytes.length === 0) {
          throw new Error("resource chunk made no progress before the end.");
        }
        return null;
      }

      const assembled = new Uint8Array(this.totalLength);
      let offset = 0;
      for (const part of this.parts) {
        assembled.set(part, offset);
        offset += part.length;
      }
      return assembled;
    }
  }

  globalThis.GraphshellResourceChunk = Object.freeze({
    CHUNK_LENGTH,
    ResourceAssembly,
  });
})();
