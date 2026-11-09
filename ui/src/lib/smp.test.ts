// Run with npm test: Node runs TypeScript as it is.

import assert from "node:assert/strict";
import { test } from "node:test";
import { GROUP, OP, Reassembler, chunkSize, decodeCbor, encodeCbor, frame, header, imageInfo, slotImages } from "./smp.ts";

const hexOf = (bytes: Uint8Array) => Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join(" ");

test("CBOR is written as the spec's examples", () => {
  assert.equal(hexOf(encodeCbor(0)), "00");
  assert.equal(hexOf(encodeCbor(23)), "17");
  assert.equal(hexOf(encodeCbor(24)), "18 18");
  assert.equal(hexOf(encodeCbor(1000)), "19 03 e8");
  assert.equal(hexOf(encodeCbor(1000000)), "1a 00 0f 42 40");
  assert.equal(hexOf(encodeCbor(-1)), "20");
  assert.equal(hexOf(encodeCbor(-1000)), "39 03 e7");
  assert.equal(hexOf(encodeCbor("a")), "61 61");
  assert.equal(hexOf(encodeCbor(true)), "f5");
  assert.equal(hexOf(encodeCbor(new Uint8Array([1, 2]))), "42 01 02");
  assert.equal(hexOf(encodeCbor({ d: "hi" })), "a1 61 64 62 68 69");
});

test("CBOR comes back as it went", () => {
  const value = { image: 0, len: 123456, off: 0, sha: new Uint8Array([9, 8]), list: [1, -2, "x", false], big: 4294967296 };
  const back = decodeCbor(encodeCbor(value)) as Record<string, unknown>;
  assert.equal(back.len, 123456);
  assert.deepEqual(back.sha, new Uint8Array([9, 8]));
  assert.deepEqual(back.list, [1, -2, "x", false]);
  assert.equal(back.big, 4294967296);
  // Indefinite-length maps, as some devices send.
  assert.deepEqual(decodeCbor(Uint8Array.from([0xbf, 0x62, 0x72, 0x63, 0x00, 0xff])), { rc: 0 });
  // A half float.
  assert.equal(decodeCbor(Uint8Array.from([0xf9, 0x3c, 0x00])), 1);
  assert.throws(() => decodeCbor(Uint8Array.from([0x62, 0x61])));
});

test("an SMP frame has its header, then its body", () => {
  const echo = frame(OP.write, GROUP.os, 0, 7, { d: "hi" });
  const head = header(echo)!;
  assert.deepEqual(head, { op: 2, flags: 0, length: 6, group: 0, seq: 7, id: 0 });
  assert.equal(hexOf(echo.slice(0, 8)), "02 00 00 06 00 00 07 00");
});

test("responses split over notifications are put back together", () => {
  const response = frame(OP.writeResponse, GROUP.os, 0, 7, { r: "hello there" });
  const joiner = new Reassembler();
  assert.deepEqual(joiner.push(response.slice(0, 5)), []);
  assert.deepEqual(joiner.push(response.slice(5, 12)), []);
  const frames = joiner.push(response.slice(12));
  assert.equal(frames.length, 1);
  assert.equal(frames[0].header.seq, 7);
  assert.deepEqual(frames[0].body, { r: "hello there" });
});

test("an image state response lists the slots", () => {
  const body = decodeCbor(
    encodeCbor({
      images: [
        { slot: 0, version: "1.2.0", hash: new Uint8Array(32), bootable: true, confirmed: true, active: true },
        { slot: 1, version: "1.3.0", hash: new Uint8Array(32).fill(1), bootable: true, pending: false },
      ],
      splitStatus: 0,
    }),
  );
  const images = slotImages(body);
  assert.equal(images.length, 2);
  assert.equal(images[0].active, true);
  assert.equal(images[1].version, "1.3.0");
  assert.equal(images[1].hash?.[0], 1);
});

test("the MCUboot header says the image's version", () => {
  const image = new Uint8Array(64);
  const view = new DataView(image.buffer);
  view.setUint32(0, 0x96f3b83d, true);
  view.setUint16(8, 0x200, true);
  view.setUint32(12, 40000, true);
  image[20] = 1;
  image[21] = 4;
  view.setUint16(22, 2, true);
  assert.deepEqual(imageInfo(image), { version: "1.4.2", headerSize: 512, imageSize: 40000 });
  assert.equal(imageInfo(new Uint8Array(64)), null);
});

test("an upload request fits in one write", () => {
  // At small MTUs the first request does not fit: the device must put writes back together.
  assert.equal(chunkSize(23, true, 300000), 16);
  assert.equal(chunkSize(65, true, 300000), 16);
  for (const mtu of [185, 247, 498]) {
    for (const first of [true, false]) {
      const size = chunkSize(mtu, first, 300000);
      const body = first
        ? { image: 0, len: 300000, off: 0, sha: new Uint8Array(32), data: new Uint8Array(size) }
        : { off: 299999, data: new Uint8Array(size) };
      const whole = frame(OP.write, GROUP.image, 1, 0, body).length;
      assert.ok(whole <= mtu - 3, `mtu ${mtu} first ${first}: ${whole}`);
    }
  }
});
