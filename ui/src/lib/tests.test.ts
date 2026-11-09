// Run with npm test: Node runs TypeScript as it is.

import assert from "node:assert/strict";
import { test } from "node:test";
import { escapes, isTarget, lostPackets, matches, parseScript, stats } from "./tests.ts";

test("stats come out of the numbers", () => {
  const found = stats([5, 1, 3, 2, 4])!;
  assert.equal(found.min, 1);
  assert.equal(found.max, 5);
  assert.equal(found.mean, 3);
  assert.equal(found.median, 3);
  assert.equal(stats([]), null);
});

test("lost packets are counted from a counter that wraps", () => {
  assert.equal(lostPackets([1, 2, 3, 4], 256), 0);
  assert.equal(lostPackets([1, 2, 5, 6], 256), 2);
  assert.equal(lostPackets([254, 255, 0, 1], 256), 0);
  assert.equal(lostPackets([254, 1], 256), 2);
});

test("a script is read step by step", () => {
  const script = `
    # A Zephyr shell over NUS
    subscribe 6E400003
    write-nr 6E400002 "kernel version\\r"
    expect 6E400003 /Zephyr version/ within 3000
    read 2A19 expect hex 64
    read 'Battery Level'
    wait 200
  `;
  const parsed = parseScript(script);
  assert.ok("steps" in parsed);
  const steps = parsed.steps;
  assert.equal(steps.length, 6);
  assert.deepEqual(steps[0], { kind: "subscribe", target: "6E400003", line: 3 });
  assert.equal(steps[1].kind, "write");
  if (steps[1].kind === "write") {
    assert.equal(steps[1].withResponse, false);
    assert.equal(steps[1].data.at(-1), 13);
  }
  if (steps[2].kind === "expect") {
    assert.equal(steps[2].within, 3000);
    assert.equal(steps[2].expect.regex, "Zephyr version");
  }
  if (steps[3].kind === "read") assert.deepEqual(steps[3].expect, { hex: [0x64] });
  if (steps[4].kind === "read") assert.equal(steps[4].target, "Battery Level");
});

test("a mistake in a script is told with its line", () => {
  const parsed = parseScript("wait 10\nsend 2A19 hex 01");
  assert.ok("error" in parsed);
  if ("error" in parsed) {
    assert.equal(parsed.line, 2);
    assert.match(parsed.error, /Unknown step "send"/);
  }
  assert.ok("error" in parseScript("write 2A19 hex 1"));
  assert.ok("error" in parseScript("expect 2A19 /(/"));
});

test("values are matched as text, regex or hex", () => {
  const ok = Array.from(new TextEncoder().encode("Zephyr version 4.2.0"));
  assert.ok(matches({ text: "version" }, ok));
  assert.ok(matches({ regex: "^Zephyr version \\d" }, ok));
  assert.ok(!matches({ regex: "^nRF" }, ok));
  assert.ok(matches({ hex: [0x5a, 0x65] }, ok));
  assert.ok(!matches({ hex: [0x00] }, ok));
  assert.deepEqual(escapes("a\\r\\x03"), [0x61, 0x0d, 0x03]);
});

test("a characteristic is found by its short UUID, its full one, or its name", () => {
  const key = "0000180f-0000-1000-8000-00805f9b34fb/00002a19-0000-1000-8000-00805f9b34fb";
  assert.ok(isTarget("Battery Level", "0x2A19", key, "2A19"));
  assert.ok(isTarget("Battery Level", "0x2A19", key, "battery level"));
  assert.ok(isTarget("Battery Level", "0x2A19", key, "0x2a19"));
  const nus = "6e400001-b5a3-f393-e0a9-e50e24dcca9e/6e400003-b5a3-f393-e0a9-e50e24dcca9e";
  assert.ok(isTarget("", "6E400003-B5A3-F393-E0A9-E50E24DCCA9E", nus, "6E400003"));
  assert.ok(!isTarget("", "6E400003-B5A3-F393-E0A9-E50E24DCCA9E", nus, "6E400002"));
});
