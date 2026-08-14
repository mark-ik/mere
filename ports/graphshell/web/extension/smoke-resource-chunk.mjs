import assert from "node:assert/strict";
import "./resource-chunk.js";

const { CHUNK_LENGTH, ResourceAssembly } = globalThis.GraphshellResourceChunk;
const session = "native:resource-smoke";
const resource = Array.from({ length: 32 }, (_, index) => index);
const chunk = Array.from({ length: 32 }, (_, index) => 255 - index);

function encoded(bytes) {
  return Buffer.from(bytes).toString("base64");
}

function reply(offset, totalLength, bytes) {
  return {
    session,
    resource,
    offset,
    total_len: totalLength,
    chunk,
    bytes: encoded(bytes),
  };
}

const source = Uint8Array.from({ length: CHUNK_LENGTH + 17 }, (_, index) => index % 251);
const assembly = new ResourceAssembly(session, resource);
assert.deepEqual(assembly.nextRequest(), {
  ResourceChunk: {
    session,
    resource,
    offset: 0,
    length: CHUNK_LENGTH,
  },
});

assert.equal(assembly.accept(reply(0, source.length, source.slice(0, CHUNK_LENGTH))), null);
assert.equal(assembly.nextRequest().ResourceChunk.offset, CHUNK_LENGTH);
const completed = assembly.accept(reply(CHUNK_LENGTH, source.length, source.slice(CHUNK_LENGTH)));
assert.deepEqual(completed, source);

const empty = new ResourceAssembly(session, resource);
assert.deepEqual(empty.accept(reply(0, 0, new Uint8Array())), new Uint8Array());

assert.throws(
  () => new ResourceAssembly(session, resource).accept(reply(1, 1, Uint8Array.of(1))),
  /arrived at 1; expected 0/,
);
assert.throws(
  () => new ResourceAssembly(session, resource).accept({
    ...reply(0, 1, Uint8Array.of(1)),
    session: "native:other",
  }),
  /another session/,
);
assert.throws(
  () => new ResourceAssembly(session, resource).accept(reply(0, 1, new Uint8Array())),
  /made no progress/,
);

console.log("smoke-resource-chunk: ok");
