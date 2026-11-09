// Run with npm test: Node runs TypeScript as it is.

import assert from "node:assert/strict";
import { test } from "node:test";
import { decodeProfile, infoText } from "./profiles.ts";

/** The full UUID of a 16-bit one. */
function full(short: number): string {
  return `0000${short.toString(16).padStart(4, "0")}-0000-1000-8000-00805f9b34fb`;
}

function said(short: number, bytes: number[]): string | undefined {
  return decodeProfile(full(short), bytes)?.text;
}

const names = { appearance: (value: number) => (value === 0x0341 ? "Heart Rate Sensor: Heart Rate Belt" : null) };

test("the basic GAP and GATT characteristics are put into words", () => {
  assert.equal(decodeProfile(full(0x2a01), [0x41, 0x03], names)?.text, "Heart Rate Sensor: Heart Rate Belt (0x0341)");
  assert.equal(said(0x2a01, [0x41, 0x03]), "Appearance 0x0341");
  assert.equal(said(0x2a05, [0x01, 0x00, 0xff, 0xff]), "handles 0x0001 to 0xFFFF changed");
  assert.deepEqual(decodeProfile(full(0x2a06), [2]), { text: "High alert", values: { alert: 2 } });
  assert.equal(said(0x2a06, [5]), "5 (reserved)");
  assert.equal(said(0x2a07, [0xfc]), "-4 dBm");
  assert.equal(said(0x2a23, [0x01, 0x02, 0x03, 0x04, 0x05, 0xaa, 0xbb, 0xcc]), "manufacturer ID 0x0504030201, OUI 0xCCBBAA");
  assert.equal(said(0x2aa6, [1]), "address resolution supported");
  assert.equal(said(0x2b29, [0x03]), "Robust Caching, Enhanced ATT bearer");
  assert.equal(said(0x2b29, [0x00]), "none");
  assert.equal(said(0x2b3a, [0x01]), "Enhanced ATT bearer");
  const hash = Array.from({ length: 16 }, (_, at) => at);
  assert.equal(said(0x2b2a, hash), "hash 000102030405060708090A0B0C0D0E0F");
});

test("times are put into words", () => {
  // 2026-11-09 19:52:07, Monday, 128/256 s, set by hand.
  assert.equal(
    said(0x2a2b, [0xea, 0x07, 11, 9, 19, 52, 7, 1, 128, 0x01]),
    "2026-11-09 19:52:07 +500 ms, Monday; adjusted: manual update",
  );
  assert.equal(said(0x2a2b, [0x00, 0x00, 0, 0, 1, 2, 3, 0, 0, 0]), "????-??-?? 01:02:03, day unknown");
  assert.equal(said(0x2a0f, [14, 0]), "UTC+03:30, standard time");
  assert.equal(said(0x2a0f, [0xfc, 4]), "UTC-01:00, daylight time (+1 h)");
  assert.equal(said(0x2a14, [2, 8, 1, 3]), "source GPS, accuracy 1 s, updated 1 day 3 hours ago");
  assert.equal(said(0x2a08, [0xea, 0x07, 1, 2, 3, 4, 5]), "2026-01-02 03:04:05");
});

test("health measurements are put into words", () => {
  // Pulse rate present: 120/80, mean 93, pulse 72, all SFLOATs with exponent 0.
  assert.deepEqual(decodeProfile(full(0x2a35), [0x04, 0x78, 0, 0x50, 0, 0x5d, 0, 0x48, 0]), {
    text: "120/80 mmHg, mean 93, pulse 72 bpm",
    values: { systolic: 120, diastolic: 80, mean: 93, pulse: 72 },
  });
  // Measurement status: irregular pulse, and pulse above the upper limit.
  assert.equal(
    said(0x2a35, [0x11, 0x78, 0, 0x50, 0, 0x5d, 0, 0x0c, 0x00]),
    "120/80 kPa, mean 93, irregular pulse, pulse above the upper limit",
  );
  // The time stamp flag with no time stamp is too short to be read.
  assert.equal(decodeProfile(full(0x2a35), [0x02, 0x78, 0, 0x50, 0, 0x5d, 0]), null);
  // Intermediate cuff pressure: the other two are NaN.
  assert.equal(said(0x2a36, [0x00, 0x55, 0x00, 0xff, 0x07, 0xff, 0x07]), "cuff 85 mmHg");

  assert.equal(said(0x2a9d, [0x00, 0xa4, 0x38]), "72.5 kg");
  assert.equal(said(0x2a9d, [0x08, 0xa4, 0x38, 0xea, 0x00, 0xe0, 0x06]), "72.5 kg, BMI 23.4, height 1.76 m");
  assert.equal(said(0x2a9d, [0x01, 0x80, 0x3e]), "160 lb");
  assert.equal(said(0x2a9d, [0x00, 0xff, 0xff]), "measurement unsuccessful");
  assert.equal(
    said(0x2a9e, [0xa5, 0x01, 0x00, 0x00]),
    "time stamp, BMI; weight in steps of 0.05 kg or 0.1 lb; height in steps of 0.001 m or 0.1 in",
  );

  // Sequence 12, 2026-11-09 08:30:00, 5 minutes later, 98 mg/dL (98e-5 kg/L), capillary whole blood from a finger.
  const base = [0xea, 0x07, 11, 9, 8, 30, 0];
  assert.deepEqual(decodeProfile(full(0x2a18), [0x03, 0x0c, 0x00, ...base, 0x05, 0x00, 0x62, 0xb0, 0x11]), {
    text: "#12, 2026-11-09 08:30:00 +5 min, 98 mg/dL, capillary whole blood, finger",
    values: { glucose: 98 },
  });
  // 5.4 mmol/L (54e-4 mol/L), with no time offset.
  assert.equal(
    said(0x2a18, [0x06, 0x01, 0x00, ...base, 0x36, 0xc0, 0x11]),
    "#1, 2026-11-09 08:30:00, 5.4 mmol/L, capillary whole blood, finger",
  );

  assert.equal(said(0x2a5e, [0x00, 0x61, 0x00, 0x48, 0x00]), "SpO2 97 %, pulse 72 bpm");
  assert.equal(
    said(0x2a5f, [0x01, 0x61, 0x00, 0x48, 0x00, 0x60, 0x00, 0x4b, 0x00]),
    "SpO2 97 %, pulse 72 bpm, fast 96 %, 75 bpm",
  );
  assert.equal(said(0x2a38, [1]), "Chest");
  assert.equal(said(0x2a1d, [6]), "Mouth");
  assert.equal(said(0x2a21, [0x3c, 0x00]), "60 s");
  assert.equal(said(0x2a21, [0x00, 0x00]), "no periodic measurement");
  assert.equal(said(0x2a49, [0x05, 0x00]), "body movement detection, irregular pulse detection");
  assert.equal(said(0x2a51, [0x01, 0x04]), "low battery detection, multiple bonds");
});

test("fitness and environment values are put into words", () => {
  assert.deepEqual(decodeProfile(full(0x2a63), [0x20, 0x00, 0xfa, 0x00, 0x64, 0x00, 0x00, 0x04]), {
    text: "250 W, crank 100 revs at 1 s",
    values: { power: 250, crank: 100 },
  });
  assert.equal(said(0x2a63, [0x03, 0x00, 0xfa, 0x00, 100]), "250 W, balance 50 % left");
  assert.equal(said(0x2a65, [0x0c, 0x00, 0x00, 0x00]), "wheel revolutions, crank revolutions, force based");
  assert.equal(said(0x2a5d, [5]), "Left crank");
  assert.equal(said(0x2a5c, [0x03, 0x00]), "wheel revolutions, crank revolutions");
  assert.equal(said(0x2a54, [0x01, 0x00]), "stride length");
  assert.equal(said(0x2a6c, [0x39, 0x30, 0x00]), "123.45 m");
  assert.equal(said(0x2a6c, [0x6a, 0xff, 0xff]), "-1.5 m");
  assert.equal(said(0x2a76, [5]), "5");
  assert.equal(said(0x2a7b, [0xfe]), "-2 °C");
  assert.equal(said(0x2a70, [0xe8, 0x03]), "10 m/s");
  assert.equal(said(0x2a71, [0x39, 0x30]), "123.45 °");
});

test("control point requests and answers are put into words", () => {
  assert.equal(said(0x2a39, [0x01]), "Reset energy expended");
  assert.equal(said(0x2a55, [0x01, 0x10, 0x00, 0x00, 0x00]), "Set cumulative value 16");
  assert.equal(said(0x2a55, [0x03, 4]), "Update sensor location: Front wheel");
  assert.equal(said(0x2a55, [0x10, 0x04, 0x01, 4, 5]), "Request supported sensor locations: Success: Front wheel, Left crank");
  assert.equal(said(0x2a55, [0x10, 0x02, 0x02]), "Start sensor calibration: Op code not supported");
  assert.equal(said(0x2a66, [0x04, 0x5a, 0x01]), "Set crank length 173 mm");
  assert.equal(said(0x2a66, [0x0c]), "Start offset compensation");
  assert.equal(said(0x2a66, [0x20, 0x05, 0x01, 0x5a, 0x01]), "Request crank length: Success: 173 mm");
  assert.equal(
    said(0x2a66, [0x20, 0x0c, 0x01, 0xf4, 0xff]),
    "Start offset compensation: Success: offset -12 (raw force in N, or torque in 1/32 Nm)",
  );
  assert.equal(said(0x2a52, [0x01, 0x01]), "Report stored records: all records");
  assert.equal(said(0x2a52, [0x03, 0x00]), "Abort operation");
  assert.deepEqual(decodeProfile(full(0x2a52), [0x05, 0x00, 0x03, 0x00]), { text: "3 stored records", values: { records: 3 } });
  assert.equal(said(0x2a52, [0x06, 0x00, 0x01, 0x06]), "Report stored records: No records found");
});

test("the Nordic UART Service's values are text", () => {
  assert.equal(decodeProfile("6e400003-b5a3-f393-e0a9-e50e24dcca9e", [0x68, 0x69, 0x0a])?.text, '"hi\n"');
  assert.equal(decodeProfile("6E400002-B5A3-F393-E0A9-E50E24DCCA9E", [0x41, 0x03])?.text, '"A\\x03"');
});

test("the device information card shows text without quotes", () => {
  assert.equal(infoText(0x2a29, [0x4e, 0x6f, 0x72, 0x64, 0x69, 0x63, 0x00]), "Nordic");
  assert.equal(infoText(0x2a19, [87]), "87 %");
  assert.equal(infoText(0x2a01, [0x41, 0x03], names), "Heart Rate Sensor: Heart Rate Belt (0x0341)");
  assert.equal(
    infoText(0x2a50, [0x01, 0x59, 0x00, 0x01, 0x00, 0x00, 0x01]),
    "vendor 0x0059 (Bluetooth SIG), product 0x0001, version 0x0100",
  );
});
