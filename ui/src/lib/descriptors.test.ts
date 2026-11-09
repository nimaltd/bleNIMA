// Run with npm test: Node runs TypeScript as it is.

import assert from "node:assert/strict";
import { test } from "node:test";
import { decodeDescriptor, isCccd, valueFormat } from "./descriptors.ts";

/** The full UUID of a 16-bit one. */
function full(short: number): string {
  return `0000${short.toString(16).padStart(4, "0")}-0000-1000-8000-00805f9b34fb`;
}

const battery = valueFormat(full(0x2a19), null);
const temperature = valueFormat(full(0x2a6e), null);

test("a characteristic's value format comes from its presentation format, or the standard", () => {
  assert.deepEqual(battery, { format: "u8", exponent: 0, unit: "%" });
  assert.deepEqual(valueFormat(full(0x1234), { format: "i16", exponent: -2, unit_symbol: "°C" }), {
    format: "i16",
    exponent: -2,
    unit: "°C",
  });
  assert.equal(valueFormat(full(0x1234), { format: "hex", exponent: 0, unit_symbol: null }), null);
  assert.equal(valueFormat("6e400003-b5a3-f393-e0a9-e50e24dcca9e", null), null);
});

test("the configuration descriptors are put into words", () => {
  assert.equal(decodeDescriptor(full(0x2902), [0x01, 0x00]), "Notifications on, indications off");
  assert.equal(decodeDescriptor(full(0x2902), [0x02, 0x00]), "Notifications off, indications on");
  assert.equal(decodeDescriptor(full(0x2900), [0x01, 0x00]), "Reliable write on, writable auxiliaries off");
  assert.equal(decodeDescriptor(full(0x2901), [0x54, 0x65, 0x6d, 0x70]), '"Temp"');
  assert.equal(decodeDescriptor(full(0x2903), [0x01, 0x00]), "Broadcast on");
  assert.ok(isCccd(`a/b/${full(0x2902)}`));
  assert.ok(!isCccd(`a/b/${full(0x2901)}`));
});

test("a presentation format is put into words", () => {
  assert.equal(decodeDescriptor(full(0x2904), [0x0e, 0xfe, 0x2f, 0x27, 0x01, 0x00, 0x00]), "sint16 × 10^-2, unit °C (0x272F)");
  assert.equal(decodeDescriptor(full(0x2904), [0x04, 0x00, 0xad, 0x27, 0x01, 0x01, 0x00]), "uint8, unit % (0x27AD), description 1st");
  assert.equal(decodeDescriptor(full(0x2904), [0x06, 0x00, 0x00, 0x27, 0x01, 0x06, 0x01]), "uint16, no unit, description main");
  assert.equal(decodeDescriptor(full(0x2904), [0x06, 0x00, 0x99, 0x27, 0x00, 0x00, 0x00]), "uint16, unit unknown (0x2799)");
});

test("a valid range is read in its characteristic's format", () => {
  assert.equal(decodeDescriptor(full(0x2906), [0, 100], battery), "0 to 100 %");
  const shown = valueFormat(full(0x1234), { format: "i16", exponent: -2, unit_symbol: "°C" });
  assert.equal(decodeDescriptor(full(0x2906), [0x18, 0xfc, 0xe8, 0x03], shown), "-10 to 10 °C");
  assert.equal(decodeDescriptor(full(0x2906), [1, 2, 3, 4]), "01 02 to 03 04");
});

test("the HID and other descriptors are put into words", () => {
  assert.equal(decodeDescriptor(full(0x2905), [0x12, 0x00, 0x15, 0x00]), "Handles 0x0012, 0x0015");
  assert.equal(decodeDescriptor(full(0x2907), [0x19, 0x2a]), "Characteristic 0x2A19");
  assert.equal(
    decodeDescriptor(full(0x2907), [0x9e, 0xca, 0xdc, 0x24, 0x0e, 0xe5, 0xa9, 0xe0, 0x93, 0xf3, 0xa3, 0xb5, 0x03, 0x00, 0x40, 0x6e]),
    "Characteristic 6E400003-B5A3-F393-E0A9-E50E24DCCA9E",
  );
  assert.equal(decodeDescriptor(full(0x2908), [1, 1]), "Report ID 1, Input report");
  assert.equal(decodeDescriptor(full(0x2908), [2, 3]), "Report ID 2, Feature report");
  assert.equal(decodeDescriptor(full(0x2909), [3]), "3 digitals");
});

test("the trigger settings and Environmental Sensing descriptors are put into words", () => {
  assert.equal(decodeDescriptor(full(0x290a), [0x03, 0x05], battery), "When the value changes by more than 5 %");
  assert.equal(decodeDescriptor(full(0x290a), [0x05, 10, 90], battery), "When the value goes inside or outside 10 to 90 %");
  assert.equal(decodeDescriptor(full(0x290a), [0x07]), "No value trigger");
  assert.equal(decodeDescriptor(full(0x290b), [0x01]), "Triggers combined with OR");
  assert.equal(
    decodeDescriptor(full(0x290c), [0x00, 0x00, 0x02, 0x3c, 0x00, 0x00, 0x0a, 0x00, 0x00, 0x01, 0x04]),
    "sampling arithmetic mean, period 60 s, update interval 10 s, application air, uncertainty 2 %",
  );
  assert.equal(decodeDescriptor(full(0x290d), [0x01, 0x3c, 0x00, 0x00]), "At a fixed interval of 60 s");
  assert.equal(decodeDescriptor(full(0x290d), [0x06, 0x20, 0x08], temperature), "While greater than 20.8 °C");
  assert.equal(decodeDescriptor(full(0x290e), [0x02, 0x0a, 0x00, 0x00]), "No more often than every 10 s");
  assert.equal(decodeDescriptor(full(0x290e), [0x03, 0x05, 0x00]), "After the value changes 5 times");
});

test("unknown and short descriptors are left for hex", () => {
  assert.equal(decodeDescriptor(full(0x2902), [0x01]), null);
  assert.equal(decodeDescriptor(full(0x2999), [0x01]), null);
  assert.equal(decodeDescriptor("6e400003-b5a3-f393-e0a9-e50e24dcca9e", [0x01]), null);
  assert.equal(decodeDescriptor(full(0x2902), []), null);
});
