// Run with npm test: Node runs TypeScript as it is.

import assert from "node:assert/strict";
import { test } from "node:test";
import { advertLength, eddystoneUrl, encodeUrl, ibeacon, uuidBytes } from "./advertise.ts";

test("an iBeacon is Apple's manufacturer data", () => {
  const made = ibeacon("E2C56DB5-DFFB-48D2-B060-D0F5A71096E0", 1, 42, -59);
  assert.ok(Array.isArray(made));
  if (Array.isArray(made)) {
    const data = made[0].data;
    assert.equal(made[0].ad_type, 0xff);
    assert.deepEqual(data.slice(0, 4), [0x4c, 0x00, 0x02, 0x15]);
    assert.equal(data[4], 0xe2);
    assert.deepEqual(data.slice(20), [0x00, 0x01, 0x00, 0x2a, 0xc5]);
    assert.equal(advertLength(made), 27);
  }
  assert.equal(typeof ibeacon("not a uuid", 1, 2, -59), "string");
  assert.equal(typeof ibeacon("E2C56DB5-DFFB-48D2-B060-D0F5A71096E0", 70000, 2, -59), "string");
});

test("an Eddystone URL is shortened as the spec says", () => {
  assert.deepEqual(encodeUrl("https://nimaltd.com"), [0x03, ...Array.from("nimaltd", (c) => c.charCodeAt(0)), 0x07]);
  assert.deepEqual(encodeUrl("http://www.example.org/"), [0x00, ...Array.from("example", (c) => c.charCodeAt(0)), 0x01]);
  assert.equal(typeof encodeUrl("ftp://x"), "string");
  assert.equal(typeof encodeUrl("https://a-very-long-name-that-does-not-fit.com"), "string");
  const made = eddystoneUrl("https://nimaltd.com", -20);
  assert.ok(Array.isArray(made));
  if (Array.isArray(made)) {
    assert.deepEqual(made[0], { ad_type: 0x03, data: [0xaa, 0xfe] });
    assert.deepEqual(made[1].data.slice(0, 4), [0xaa, 0xfe, 0x10, 0xec]);
  }
});

test("a UUID is read with or without dashes", () => {
  assert.equal(uuidBytes("e2c56db5dffb48d2b060d0f5a71096e0")?.length, 16);
  assert.equal(uuidBytes("E2C56DB5-DFFB"), null);
});
