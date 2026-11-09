// Run with npm test: Node runs TypeScript as it is.

import assert from "node:assert/strict";
import { test } from "node:test";
import { decodeProfile } from "./profiles.ts";
import {
  RESET_ENERGY_EXPENDED,
  alertLevel,
  currentTime,
  cyclingPowerControlPoint,
  localTimeInformation,
  measurementInterval,
  pcTimeZone,
  recordAccess,
  scControlPoint,
} from "./writes.ts";

/** The full UUID of a 16-bit one. */
function full(short: number): string {
  return `0000${short.toString(16).padStart(4, "0")}-0000-1000-8000-00805f9b34fb`;
}

test("the current time is written with its day of the week and a manual reason", () => {
  // Monday 2026-11-09 19:52:07.500.
  const monday = currentTime(new Date(2026, 10, 9, 19, 52, 7, 500));
  assert.deepEqual(monday, [0xea, 0x07, 11, 9, 19, 52, 7, 1, 128, 0x01]);
  assert.equal(
    decodeProfile(full(0x2a2b), monday)?.text,
    "2026-11-09 19:52:07 +500 ms, Monday; adjusted: manual update",
  );
  // Sunday is 7, not 0.
  assert.equal(currentTime(new Date(2026, 10, 8, 12, 0, 0))[7], 7);
});

test("the time zone is written in 15-minute steps, with the DST code", () => {
  assert.deepEqual(localTimeInformation(210, 0), [14, 0]);
  assert.deepEqual(localTimeInformation(-300, 60), [0xec, 4]);
  assert.deepEqual(localTimeInformation(60, 45), [4, 255]);
  assert.equal(decodeProfile(full(0x2a0f), localTimeInformation(-300, 60))?.text, "UTC-05:00, daylight time (+1 h)");
  const now = new Date();
  const { standard, dst } = pcTimeZone(now);
  assert.equal(standard + dst, -now.getTimezoneOffset());
  assert.ok(dst >= 0);
});

test("alert levels and control point requests are the bytes the standards give", () => {
  assert.deepEqual(alertLevel(0), [0]);
  assert.deepEqual(alertLevel(2), [2]);
  assert.deepEqual(RESET_ENERGY_EXPENDED, [0x01]);
  assert.deepEqual(scControlPoint.setCumulativeValue("16"), [0x01, 16, 0, 0, 0]);
  assert.deepEqual(scControlPoint.setCumulativeValue("4294967295"), [0x01, 0xff, 0xff, 0xff, 0xff]);
  assert.equal(typeof scControlPoint.setCumulativeValue("-1"), "string");
  assert.equal(typeof scControlPoint.setCumulativeValue("1.5"), "string");
  assert.deepEqual(scControlPoint.startCalibration, [0x02]);
  assert.deepEqual(scControlPoint.updateLocation(4), [0x03, 4]);
  assert.deepEqual(scControlPoint.requestLocations, [0x04]);
  assert.deepEqual(cyclingPowerControlPoint.setCrankLength("172.5"), [0x04, 0x59, 0x01]);
  assert.equal(typeof cyclingPowerControlPoint.setCrankLength("172.3"), "string");
  assert.equal(typeof cyclingPowerControlPoint.setCrankLength(""), "string");
  assert.deepEqual(cyclingPowerControlPoint.requestCrankLength, [0x05]);
  assert.deepEqual(cyclingPowerControlPoint.startOffsetCompensation, [0x0c]);
  assert.deepEqual(recordAccess.reportAll, [0x01, 0x01]);
  assert.deepEqual(recordAccess.countAll, [0x04, 0x01]);
  assert.deepEqual(recordAccess.deleteAll, [0x02, 0x01]);
  assert.deepEqual(recordAccess.abort, [0x03, 0x00]);
  assert.deepEqual(measurementInterval("60"), [60, 0]);
  assert.deepEqual(measurementInterval("300"), [0x2c, 0x01]);
  assert.equal(typeof measurementInterval("70000"), "string");
});
