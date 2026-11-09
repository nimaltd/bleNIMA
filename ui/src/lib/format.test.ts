// Run with npm test: Node runs TypeScript as it is.

import assert from "node:assert/strict";
import { test } from "node:test";
import { encodeNumber, float11073, numbers, parseHex, parseText, sfloat, show } from "./format.ts";
import { decodeProfile, shortUuid } from "./profiles.ts";

test("numbers are read little-endian unless told otherwise", () => {
  assert.deepEqual(numbers([0x34, 0x12], "u16"), [0x1234]);
  assert.deepEqual(numbers([0x12, 0x34], "u16", false), [0x1234]);
  assert.deepEqual(numbers([0xff, 0xff], "i16"), [-1]);
  assert.deepEqual(numbers([0x01, 0x02, 0x03], "u8"), [1, 2, 3]);
  assert.deepEqual(numbers([0xff, 0xff, 0xff], "i24"), [-1]);
  assert.deepEqual(numbers([0x00, 0x00, 0x80, 0x3f], "f32"), [1]);
  // A byte left over is not a value.
  assert.deepEqual(numbers([0x01, 0x00, 0x02], "u16"), [1]);
});

test("an exponent scales the values, without decimal noise", () => {
  assert.deepEqual(numbers([0x7b, 0x00], "i16", true, -1), [12.3]);
  assert.deepEqual(numbers([0x03, 0x00], "u16", true, -1), [0.3]);
});

test("IEEE-11073 floats are read with their special values", () => {
  // Mantissa 365, exponent -1: 36.5.
  assert.equal(sfloat(0xf16d), 36.5);
  assert.ok(Number.isNaN(sfloat(0x07ff)));
  assert.equal(sfloat(0x07fe), Infinity);
  assert.equal(float11073(0xff00016d), 36.5);
  assert.ok(Number.isNaN(float11073(0x007fffff)));
});

test("values are shown in their format", () => {
  assert.equal(show([0x48, 0x69], "utf8"), '"Hi"');
  assert.equal(show([0x0d, 0x0a], "utf8"), '"\\x0d\n"');
  assert.equal(show([0x01, 0x02], "hex"), "01 02");
  assert.equal(show([0xe8, 0x08], "i16", true, -2, "°C"), "22.8 °C");
  assert.equal(show([0x01, 0x00, 0x02], "u16"), "1 +1 bytes");
  assert.equal(show([], "u8"), "(empty)");
  assert.equal(show([0x01], "u16"), "01 (too short for u16)");
});

test("hex is read in the ways people type it", () => {
  assert.deepEqual(parseHex("AA 55 01"), [0xaa, 0x55, 0x01]);
  assert.deepEqual(parseHex("0xAA,0x55"), [0xaa, 0x55]);
  assert.deepEqual(parseHex("aa5501"), [0xaa, 0x55, 0x01]);
  assert.equal(typeof parseHex("AA 5"), "string");
  assert.equal(typeof parseHex("zz"), "string");
});

test("text takes escapes for the bytes that cannot be typed", () => {
  assert.deepEqual(parseText("ver\\r\\n"), [0x76, 0x65, 0x72, 0x0d, 0x0a]);
  assert.deepEqual(parseText("\\x03"), [0x03]);
  assert.deepEqual(parseText("é"), [0xc3, 0xa9]);
  assert.deepEqual(parseText("a\\q"), [0x61, 0x5c, 0x71]);
});

test("numbers typed are written as their bytes, and checked", () => {
  assert.deepEqual(encodeNumber("4660", "u16"), [0x34, 0x12]);
  assert.deepEqual(encodeNumber("4660", "u16", false), [0x12, 0x34]);
  assert.deepEqual(encodeNumber("-1", "i8"), [0xff]);
  assert.deepEqual(encodeNumber("1", "f32"), [0x00, 0x00, 0x80, 0x3f]);
  assert.equal(typeof encodeNumber("256", "u8"), "string");
  assert.equal(typeof encodeNumber("1.5", "u16"), "string");
  assert.equal(typeof encodeNumber("x", "u16"), "string");
});

test("standard characteristics are put into words", () => {
  const battery = "00002a19-0000-1000-8000-00805f9b34fb";
  assert.equal(shortUuid(battery), 0x2a19);
  assert.equal(shortUuid("6e400003-b5a3-f393-e0a9-e50e24dcca9e"), null);
  assert.deepEqual(decodeProfile(battery, [87]), { text: "87 %", values: { battery: 87 } });
  const heart = "00002a37-0000-1000-8000-00805f9b34fb";
  assert.equal(decodeProfile(heart, [0x00, 72])?.text, "72 bpm");
  // 16-bit heart rate, then one RR interval of 1024/1024 s.
  assert.equal(decodeProfile(heart, [0x11, 72, 0x00, 0x00, 0x04])?.text, "72 bpm, RR 1000 ms");
  const temperature = "00002a6e-0000-1000-8000-00805f9b34fb";
  assert.equal(decodeProfile(temperature, [0xe8, 0x08])?.text, "22.8 °C");
  const name = "00002a00-0000-1000-8000-00805f9b34fb";
  assert.equal(decodeProfile(name, [0x6e, 0x72, 0x66, 0x00])?.text, '"nrf"');
  const parameters = "00002a04-0000-1000-8000-00805f9b34fb";
  assert.equal(
    decodeProfile(parameters, [0x18, 0x00, 0x28, 0x00, 0x00, 0x00, 0x90, 0x01])?.text,
    "interval 30 to 50 ms, latency 0, timeout 4000 ms",
  );
  assert.equal(decodeProfile("6e400003-b5a3-f393-e0a9-e50e24dcca9e", [1]), null);
});
