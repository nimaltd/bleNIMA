// Run with npm test: Node runs TypeScript as it is.

import assert from "node:assert/strict";
import { test } from "node:test";
import { decodeProfile } from "./profiles.ts";
import {
  PRESETS,
  bitsOf,
  describeServed,
  floatBytes,
  fullUuid,
  hasLive,
  hexBytes,
  hexText,
  liveValue,
  newService,
  presetServices,
  sfloatBytes,
  toServices,
  uuidText,
  type Tick,
} from "./server.ts";

const tick = (n: number, ms = 1000): Tick => ({ n, now: new Date(2026, 10, 9, 14, 2, 11, 500), ms });

test("UUIDs are read as people write them", () => {
  assert.equal(fullUuid("180F"), "0000180f-0000-1000-8000-00805f9b34fb");
  assert.equal(fullUuid("0x2a19"), "00002a19-0000-1000-8000-00805f9b34fb");
  assert.equal(fullUuid("12345678"), "12345678-0000-1000-8000-00805f9b34fb");
  assert.equal(fullUuid("6E400001B5A3F393E0A9E50E24DCCA9E"), "6e400001-b5a3-f393-e0a9-e50e24dcca9e");
  assert.equal(fullUuid("180"), null);
  assert.equal(fullUuid("zz0F"), null);
  assert.equal(uuidText("0000180f-0000-1000-8000-00805f9b34fb"), "0x180F");
  assert.equal(uuidText("6e400001-b5a3-f393-e0a9-e50e24dcca9e"), "6E400001-B5A3-F393-E0A9-E50E24DCCA9E");
});

test("properties become the bits of the declaration", () => {
  assert.equal(bitsOf(["read", "notify"]), 0x12);
  assert.equal(bitsOf(["write", "write_without_response"]), 0x0c);
  assert.equal(bitsOf([]), 0);
});

test("hex goes both ways", () => {
  assert.deepEqual(hexBytes("48 69"), [0x48, 0x69]);
  assert.deepEqual(hexBytes("4869"), [0x48, 0x69]);
  assert.deepEqual(hexBytes(""), []);
  assert.equal(hexBytes("486"), null);
  assert.equal(hexText([0x48, 0x0a]), "48 0A");
});

test("every preset is made of good services", () => {
  const ids = new Set<string>();
  for (const preset of PRESETS) {
    assert.ok(!ids.has(preset.id), `${preset.id} twice`);
    ids.add(preset.id);
    const services = toServices(preset.services);
    assert.ok(typeof services !== "string", `${preset.id}: ${services}`);
    for (const service of services) {
      for (const characteristic of service.characteristics) {
        assert.ok(characteristic.properties !== 0, `${preset.id}: a characteristic with no properties`);
        assert.ok(characteristic.value.length <= 20, `${preset.id}: a value longer than the smallest MTU takes`);
      }
    }
  }
  assert.ok(PRESETS.length >= 20);
});

test("a preset's services are copies, so a tab can change them", () => {
  const first = presetServices("battery");
  first[0].characteristics[0].value = "01";
  assert.equal(presetServices("battery")[0].characteristics[0].value, "64");
  assert.deepEqual(presetServices("no such preset"), []);
});

test("a UART's writes echo to the characteristic that notifies", () => {
  const [nus] = presetServices("nus");
  nus.echo = true;
  const services = toServices([nus]);
  assert.ok(typeof services !== "string");
  assert.equal(services[0].characteristics[0].echo, 1);
  assert.equal(services[0].characteristics[1].echo, null);
  // HM-10 has one characteristic for both: it echoes to itself.
  const [hm10] = presetServices("hm10");
  hm10.echo = true;
  const echoed = toServices([hm10]);
  assert.ok(typeof echoed !== "string");
  assert.equal(echoed[0].characteristics[0].echo, 0);
  nus.echo = false;
  const quiet = toServices([nus]);
  assert.ok(typeof quiet !== "string");
  assert.equal(quiet[0].characteristics[0].echo, null);
});

test("what is typed wrong is said in words", () => {
  const bad = newService();
  bad.uuid = "12";
  assert.match(toServices([bad]) as string, /^Service 1: "12" is not a UUID/);
  const value = newService();
  value.characteristics[0].value = "4";
  assert.match(toServices([value]) as string, /^Characteristic 1 of service 1: its value is not hex/);
});

test("the battery runs down and starts again", () => {
  assert.deepEqual(liveValue("2A19", [87], tick(0)), [86]);
  assert.deepEqual(liveValue("2A19", [0], tick(0)), [100]);
  assert.deepEqual(liveValue("2A19", [], tick(0)), [100]);
});

test("live values read out as the device tab reads them", () => {
  for (let n = 0; n < 200; n++) {
    const heart = decodeProfile("00002a37-0000-1000-8000-00805f9b34fb", liveValue("2A37", [], tick(n))!)!;
    assert.ok(heart.values.bpm >= 60 && heart.values.bpm <= 100, heart.text);
    const body = decodeProfile("00002a1c-0000-1000-8000-00805f9b34fb", liveValue("2A1C", [], tick(n))!)!;
    const celsius = Object.values(body.values)[0];
    assert.ok(celsius >= 36.5 && celsius <= 37.5, body.text);
  }
  const air = decodeProfile("00002a6e-0000-1000-8000-00805f9b34fb", liveValue("2A6E", [], tick(0))!)!;
  assert.match(air.text, /^22(\.0+)? °C/);
});

test("cycling counts revolutions and their times", () => {
  const ten = liveValue("2A5B", [], tick(10))!;
  assert.equal(ten[0], 0x03);
  const wheel = ten[1] | (ten[2] << 8) | (ten[3] << 16) | (ten[4] << 24);
  // 25 km/h on a 2.1 m wheel is 3.3 turns a second.
  assert.equal(wheel, 33);
  const crank = ten[7] | (ten[8] << 8);
  assert.equal(crank, 15);
});

test("the current time is the computer's", () => {
  const time = liveValue("2A2B", [], tick(0))!;
  assert.deepEqual(time, [0xea, 0x07, 11, 9, 14, 2, 11, 1, 128, 0]);
  assert.equal(describeServed("2A2B", time), "2026-11-09 14:02:11");
});

test("a UART sends a line with a counter", () => {
  const line = liveValue("6E400003-B5A3-F393-E0A9-E50E24DCCA9E", [], tick(4))!;
  assert.equal(new TextDecoder().decode(Uint8Array.from(line)), "bleNIMA 5\r\n");
  assert.ok(hasLive("ffe1"));
  assert.ok(!hasLive("2A38"));
  assert.equal(liveValue("2A38", [1], tick(0)), null);
});

test("IEEE-11073 numbers are made as health devices send them", () => {
  // 36.5 as 365 * 10^-1.
  assert.deepEqual(floatBytes(365, -1), [0x6d, 0x01, 0x00, 0xff]);
  // 120 with exponent 0, and -1 * 10^-1.
  assert.deepEqual(sfloatBytes(120), [0x78, 0x00]);
  assert.deepEqual(sfloatBytes(-1, -1), [0xff, 0xff]);
});

test("an alert level is put into words", () => {
  assert.equal(describeServed("2A06", [2]), "High alert");
  assert.equal(describeServed("2A06", [7]), "Alert level 7");
  assert.equal(describeServed("2A19", [1]), null);
});
