// The values of standard characteristics in words: Battery Level, Heart Rate,
// Blood Pressure, Current Time and the others the Bluetooth SIG defines, by
// their 16-bit UUID, and the Nordic UART Service's values as text. It imports
// only format.ts, so its tests run in Node as they are.

import { float11073, hex, numbers, sfloat, text, type Format } from "./format.ts";

/** The 16-bit UUID a full one is short for, or null: "00002a19-0000-1000-8000-00805f9b34fb" is 0x2A19. */
export function shortUuid(uuid: string): number | null {
  const match = /^0000([0-9a-f]{4})-0000-1000-8000-00805f9b34fb$/i.exec(uuid);
  return match ? Number.parseInt(match[1], 16) : null;
}

/** The characteristic's UUID in a key, "service/characteristic". */
export function characteristicUuid(key: string): string {
  return key.split("/")[1] ?? "";
}

/** "0x2A19", for a 16-bit number. */
export function hex4(n: number): string {
  return `0x${n.toString(16).toUpperCase().padStart(4, "0")}`;
}

function u16(bytes: number[], at: number): number {
  return bytes[at] | (bytes[at + 1] << 8);
}

function u32(bytes: number[], at: number): number {
  return (bytes[at] | (bytes[at + 1] << 8) | (bytes[at + 2] << 16) | (bytes[at + 3] << 24)) >>> 0;
}

function utf8(bytes: number[]): string {
  return new TextDecoder().decode(Uint8Array.from(bytes)).replace(/\0+$/, "");
}

/** Reads a value's fields one after another; it throws when the value is too short. */
export class Reader {
  private readonly bytes: number[];
  private at = 0;

  constructor(bytes: number[]) {
    this.bytes = bytes;
  }

  /** How many bytes are not read yet. */
  get left(): number {
    return this.bytes.length - this.at;
  }

  private take(size: number): number {
    if (this.at + size > this.bytes.length) throw new RangeError("The value is too short");
    const at = this.at;
    this.at += size;
    return at;
  }

  u8(): number {
    return this.bytes[this.take(1)];
  }

  i8(): number {
    const value = this.u8();
    return value >= 0x80 ? value - 0x100 : value;
  }

  u16(): number {
    return u16(this.bytes, this.take(2));
  }

  i16(): number {
    const value = this.u16();
    return value >= 0x8000 ? value - 0x10000 : value;
  }

  u24(): number {
    const at = this.take(3);
    return this.bytes[at] | (this.bytes[at + 1] << 8) | (this.bytes[at + 2] << 16);
  }

  u32(): number {
    return u32(this.bytes, this.take(4));
  }

  /** A whole number of `size` bytes, as a BigInt: for the 40-bit and 64-bit ones. */
  big(size: number): bigint {
    const at = this.take(size);
    let value = 0n;
    for (let n = size - 1; n >= 0; n--) value = (value << 8n) | BigInt(this.bytes[at + n]);
    return value;
  }

  /** An IEEE-11073 16-bit SFLOAT. */
  sfloat(): number {
    return sfloat(this.u16());
  }

  /** An IEEE-11073 32-bit FLOAT. */
  float(): number {
    return float11073(this.u32());
  }

  /** A Date Time, 7 bytes: "2026-11-09 19:52:07". */
  dateTime(): string {
    const year = this.u16();
    const [month, day, hours, minutes, seconds] = [this.u8(), this.u8(), this.u8(), this.u8(), this.u8()];
    const two = (n: number) => String(n).padStart(2, "0");
    const date = `${year || "????"}-${month ? two(month) : "??"}-${day ? two(day) : "??"}`;
    return `${date} ${two(hours)}:${two(minutes)}:${two(seconds)}`;
  }

  /** What is left. */
  rest(): number[] {
    const rest = this.bytes.slice(this.at);
    this.at = this.bytes.length;
    return rest;
  }
}

/** The names of the bits that are set, from bit 0 up; an empty name is a bit with no meaning. */
export function bitNames(value: number, names: string[]): string[] {
  return names.filter((name, bit) => name !== "" && Math.floor(value / 2 ** bit) % 2 === 1);
}

/** A number's name in a list, or the number and "reserved". */
export function named(list: Record<number, string>, value: number): string {
  return list[value] ?? `${value} (reserved)`;
}

/** A number that is not one, as IEEE-11073 says with its special values, is left out. */
function real(value: number): boolean {
  return Number.isFinite(value);
}

/** Names that tables elsewhere know: an appearance's, from the Rust side's numbers database. */
export type Names = { appearance?: (value: number) => string | null };

/** Characteristics that are one number: its format, the power of ten it is multiplied by, and its unit. */
export const SIMPLE: Record<number, { name: string; format: Format; exponent: number; unit: string }> = {
  0x2a19: { name: "battery", format: "u8", exponent: 0, unit: "%" },
  0x2a07: { name: "power", format: "i8", exponent: 0, unit: "dBm" },
  0x2a21: { name: "interval", format: "u16", exponent: 0, unit: "s" },
  0x2a6e: { name: "temperature", format: "i16", exponent: -2, unit: "°C" },
  0x2a6f: { name: "humidity", format: "u16", exponent: -2, unit: "%" },
  0x2a6d: { name: "pressure", format: "u32", exponent: -1, unit: "Pa" },
  0x2a6c: { name: "elevation", format: "i24", exponent: -2, unit: "m" },
  0x2a76: { name: "uv index", format: "u8", exponent: 0, unit: "" },
  0x2a7b: { name: "dew point", format: "i8", exponent: 0, unit: "°C" },
  0x2a7a: { name: "heat index", format: "i8", exponent: 0, unit: "°C" },
  0x2a79: { name: "wind chill", format: "i8", exponent: 0, unit: "°C" },
  0x2a70: { name: "wind speed", format: "u16", exponent: -2, unit: "m/s" },
  0x2a71: { name: "wind direction", format: "u16", exponent: -2, unit: "°" },
};

const STRINGS = new Set([0x2a00, 0x2a24, 0x2a25, 0x2a26, 0x2a27, 0x2a28, 0x2a29, 0x2a87, 0x2a8a, 0x2a90]);

/** The Nordic UART Service's RX and TX: text both ways. */
const UART_TEXT = new Set(["6e400002-b5a3-f393-e0a9-e50e24dcca9e", "6e400003-b5a3-f393-e0a9-e50e24dcca9e"]);

/** Whether a characteristic's value is text to bleNIMA without being asked: the Nordic UART Service's. */
export function isUartText(uuid: string): boolean {
  return UART_TEXT.has(uuid.toLowerCase());
}

const DAYS: Record<number, string> = {
  0: "day unknown",
  1: "Monday",
  2: "Tuesday",
  3: "Wednesday",
  4: "Thursday",
  5: "Friday",
  6: "Saturday",
  7: "Sunday",
};

const ALERT_LEVELS: Record<number, string> = { 0: "No alert", 1: "Mild alert", 2: "High alert" };

const BODY_SENSOR_LOCATIONS: Record<number, string> = {
  0: "Other",
  1: "Chest",
  2: "Wrist",
  3: "Finger",
  4: "Hand",
  5: "Ear lobe",
  6: "Foot",
};

const TEMPERATURE_TYPES: Record<number, string> = {
  1: "Armpit",
  2: "Body (general)",
  3: "Ear (usually ear lobe)",
  4: "Finger",
  5: "Gastro-intestinal tract",
  6: "Mouth",
  7: "Rectum",
  8: "Toe",
  9: "Tympanum (ear drum)",
};

/** Where a cycling or running sensor is, as Sensor Location and the control points say it. */
export const SENSOR_LOCATIONS: Record<number, string> = {
  0: "Other",
  1: "Top of shoe",
  2: "In shoe",
  3: "Hip",
  4: "Front wheel",
  5: "Left crank",
  6: "Right crank",
  7: "Left pedal",
  8: "Right pedal",
  9: "Front hub",
  10: "Rear dropout",
  11: "Chainstay",
  12: "Rear wheel",
  13: "Rear hub",
  14: "Chest",
  15: "Spider",
  16: "Chain ring",
};

const TIME_SOURCES: Record<number, string> = {
  0: "unknown",
  1: "Network Time Protocol",
  2: "GPS",
  3: "radio time signal",
  4: "manual",
  5: "atomic clock",
  6: "cellular network",
};

const BLOOD_PRESSURE_FEATURES = [
  "body movement detection",
  "cuff fit detection",
  "irregular pulse detection",
  "pulse rate range detection",
  "measurement position detection",
  "multiple bonds",
  "E2E-CRC",
  "User Data Service",
  "user facing time",
];

const GLUCOSE_FEATURES = [
  "low battery detection",
  "sensor malfunction detection",
  "sensor sample size",
  "strip insertion error detection",
  "strip type error detection",
  "result high-low detection",
  "temperature high-low detection",
  "read interrupt detection",
  "general device fault",
  "time fault",
  "multiple bonds",
];

const GLUCOSE_STATUS = [
  "battery low",
  "sensor malfunction",
  "sample size too small",
  "strip insertion error",
  "wrong strip type",
  "result above the device's range",
  "result below the device's range",
  "temperature too high",
  "temperature too low",
  "read interrupted",
  "general device fault",
  "time fault",
];

const GLUCOSE_TYPES: Record<number, string> = {
  1: "capillary whole blood",
  2: "capillary plasma",
  3: "venous whole blood",
  4: "venous plasma",
  5: "arterial whole blood",
  6: "arterial plasma",
  7: "undetermined whole blood",
  8: "undetermined plasma",
  9: "interstitial fluid",
  10: "control solution",
};

const GLUCOSE_LOCATIONS: Record<number, string> = {
  1: "finger",
  2: "alternate site test",
  3: "earlobe",
  4: "control solution",
  15: "location not available",
};

const PLX_MEASUREMENT_STATUS = [
  "",
  "",
  "",
  "",
  "",
  "measurement ongoing",
  "early estimated data",
  "validated data",
  "fully qualified data",
  "data from measurement storage",
  "data for demonstration",
  "data for testing",
  "calibration ongoing",
  "measurement unavailable",
  "questionable measurement",
  "invalid measurement",
];

const PLX_DEVICE_STATUS = [
  "extended display update ongoing",
  "equipment malfunction",
  "signal processing irregularity",
  "inadequate signal",
  "poor signal",
  "low perfusion",
  "erratic signal",
  "non-pulsatile signal",
  "questionable pulse",
  "signal analysis ongoing",
  "sensor interference",
  "sensor not on the user",
  "unknown sensor connected",
  "sensor displaced",
  "sensor malfunctioning",
  "sensor disconnected",
];

const CYCLING_POWER_FEATURES = [
  "pedal power balance",
  "accumulated torque",
  "wheel revolutions",
  "crank revolutions",
  "extreme magnitudes",
  "extreme angles",
  "dead spot angles",
  "accumulated energy",
  "offset compensation indicator",
  "offset compensation",
  "measurement content masking",
  "multiple sensor locations",
  "crank length adjustment",
  "chain length adjustment",
  "chain weight adjustment",
  "span length adjustment",
  "",
  "instantaneous measurement direction",
  "factory calibration date",
  "enhanced offset compensation",
];

/** A decoded value, with the numbers in it by name, for plotting. */
export type Decoded = { text: string; values: Record<string, number> };

/** Supported features as a list, or "none". */
function features(value: number, names: string[]): string {
  const list = bitNames(value, names);
  return list.length ? list.join(", ") : "none";
}

function adjustReasons(value: number): string[] {
  return bitNames(value, ["manual update", "external reference update", "time zone change", "DST change"]);
}

function currentTime(read: Reader): Decoded {
  const time = read.dateTime();
  const day = named(DAYS, read.u8());
  const parts = [`${time}, ${day}`];
  if (read.left >= 1) {
    const fractions = read.u8();
    if (fractions) parts[0] = `${time} +${Math.round((fractions * 1000) / 256)} ms, ${day}`;
  }
  if (read.left >= 1) {
    const reasons = adjustReasons(read.u8());
    if (reasons.length) parts.push(`adjusted: ${reasons.join(", ")}`);
  }
  return { text: parts.join("; "), values: {} };
}

/** "UTC+03:30", from a time zone in 15-minute steps. */
function utcOffset(quarters: number): string {
  const minutes = Math.abs(quarters) * 15;
  const two = (n: number) => String(n).padStart(2, "0");
  return `UTC${quarters < 0 ? "-" : "+"}${two(Math.floor(minutes / 60))}:${two(minutes % 60)}`;
}

function localTime(read: Reader): Decoded {
  const zone = read.i8();
  const dst = read.u8();
  const dstText: Record<number, string> = {
    0: "standard time",
    2: "half an hour of daylight time",
    4: "daylight time (+1 h)",
    8: "double daylight time (+2 h)",
    255: "DST unknown",
  };
  const zoneText = zone === -128 ? "time zone unknown" : utcOffset(zone);
  return { text: `${zoneText}, ${dstText[dst] ?? `DST ${dst} (reserved)`}`, values: {} };
}

function referenceTime(read: Reader): Decoded {
  const source = named(TIME_SOURCES, read.u8());
  const accuracy = read.u8();
  const days = read.u8();
  const hours = read.u8();
  const accuracyText =
    accuracy === 255 ? "accuracy unknown" : accuracy === 254 ? "accuracy over 31.6 s" : `accuracy ${accuracy / 8} s`;
  const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;
  const ago = days === 255 ? "255 days or more ago" : `${plural(days, "day")} ${plural(hours, "hour")} ago`;
  return { text: `source ${source}, ${accuracyText}, updated ${ago}`, values: {} };
}

function bloodPressure(read: Reader, cuff: boolean): Decoded {
  const flags = read.u8();
  const unit = flags & 0x01 ? "kPa" : "mmHg";
  const systolic = read.sfloat();
  const diastolic = read.sfloat();
  const mean = read.sfloat();
  const parts: string[] = [];
  const values: Record<string, number> = {};
  if (cuff) {
    parts.push(`cuff ${systolic} ${unit}`);
    values.cuff = systolic;
  } else {
    parts.push(`${systolic}/${diastolic} ${unit}${real(mean) ? `, mean ${mean}` : ""}`);
    Object.assign(values, { systolic, diastolic, mean });
  }
  if (flags & 0x02) parts.push(`at ${read.dateTime()}`);
  if (flags & 0x04) {
    const pulse = read.sfloat();
    if (real(pulse)) {
      parts.push(`pulse ${pulse} bpm`);
      values.pulse = pulse;
    }
  }
  if (flags & 0x08) {
    const user = read.u8();
    parts.push(user === 255 ? "user unknown" : `user ${user}`);
  }
  if (flags & 0x10) {
    const status = read.u16();
    const notes = bitNames(status, ["body movement", "cuff too loose", "irregular pulse"]);
    const range = (status >> 3) & 0x03;
    if (range === 1) notes.push("pulse above the upper limit");
    if (range === 2) notes.push("pulse below the lower limit");
    if (status & 0x20) notes.push("improper measurement position");
    parts.push(notes.length ? notes.join(", ") : "measured well");
  }
  return { text: parts.join(", "), values };
}

function weight(read: Reader): Decoded {
  const flags = read.u8();
  const imperial = (flags & 0x01) !== 0;
  const raw = read.u16();
  const parts: string[] = [];
  const values: Record<string, number> = {};
  if (raw === 0xffff) parts.push("measurement unsuccessful");
  else {
    const kilos = Number.parseFloat((raw * (imperial ? 0.01 : 0.005)).toFixed(3));
    parts.push(`${kilos} ${imperial ? "lb" : "kg"}`);
    values.weight = kilos;
  }
  if (flags & 0x02) parts.push(`at ${read.dateTime()}`);
  if (flags & 0x04) {
    const user = read.u8();
    parts.push(user === 255 ? "user unknown" : `user ${user}`);
  }
  if (flags & 0x08) {
    const bmi = read.u16() / 10;
    const height = read.u16();
    const tall = imperial ? `${height / 10} in` : `${height / 1000} m`;
    parts.push(`BMI ${bmi}, height ${tall}`);
    values.bmi = bmi;
  }
  return { text: parts.join(", "), values };
}

function weightFeature(value: number): Decoded {
  const list = bitNames(value, ["time stamp", "multiple users", "BMI"]);
  const weights = [
    "",
    "0.5 kg or 1 lb",
    "0.2 kg or 0.5 lb",
    "0.1 kg or 0.2 lb",
    "0.05 kg or 0.1 lb",
    "0.02 kg or 0.05 lb",
    "0.01 kg or 0.02 lb",
    "0.005 kg or 0.01 lb",
  ];
  const heights = ["", "0.01 m or 1 in", "0.005 m or 0.5 in", "0.001 m or 0.1 in"];
  const weightStep = weights[(value >> 3) & 0x0f];
  const heightStep = heights[(value >> 7) & 0x07];
  const parts = [list.length ? list.join(", ") : "no time stamp, one user, no BMI"];
  if (weightStep) parts.push(`weight in steps of ${weightStep}`);
  if (heightStep) parts.push(`height in steps of ${heightStep}`);
  return { text: parts.join("; "), values: {} };
}

function glucose(read: Reader): Decoded {
  const flags = read.u8();
  const sequence = read.u16();
  const base = read.dateTime();
  const parts = [`#${sequence}`];
  const values: Record<string, number> = {};
  let time = base;
  if (flags & 0x01) {
    const offset = read.u16();
    const minutes = offset >= 0x8000 ? offset - 0x10000 : offset;
    if (minutes !== 0) time += ` ${minutes > 0 ? "+" : "-"}${Math.abs(minutes)} min`;
  }
  parts.push(time);
  if (flags & 0x02) {
    const concentration = read.sfloat();
    const kind = read.u8();
    if (real(concentration)) {
      // kg/L is shown as mg/dL, and mol/L as mmol/L, as meters show them.
      const shown = flags & 0x04 ? concentration * 1000 : concentration * 100000;
      const rounded = Number.parseFloat(shown.toPrecision(6));
      parts.push(`${rounded} ${flags & 0x04 ? "mmol/L" : "mg/dL"}`);
      values.glucose = rounded;
    }
    parts.push(named(GLUCOSE_TYPES, kind & 0x0f), named(GLUCOSE_LOCATIONS, kind >> 4));
  }
  if (flags & 0x08) {
    const status = bitNames(read.u16(), GLUCOSE_STATUS);
    if (status.length) parts.push(status.join(", "));
  }
  if (flags & 0x10) parts.push("context follows");
  return { text: parts.join(", "), values };
}

function plxStatus(read: Reader, flags: number, measurementBit: number, deviceBit: number, parts: string[]): void {
  if (flags & measurementBit) parts.push(...bitNames(read.u16(), PLX_MEASUREMENT_STATUS));
  if (flags & deviceBit) parts.push(...bitNames(read.u24(), PLX_DEVICE_STATUS));
}

function plxSpotCheck(read: Reader): Decoded {
  const flags = read.u8();
  const spo2 = read.sfloat();
  const pulse = read.sfloat();
  const parts = [`SpO2 ${spo2} %, pulse ${pulse} bpm`];
  if (flags & 0x01) parts.push(`at ${read.dateTime()}`);
  plxStatus(read, flags, 0x02, 0x04, parts);
  if (flags & 0x08) parts.push(`pulse amplitude index ${read.sfloat()} %`);
  if (flags & 0x10) parts.push("device clock not set");
  return { text: parts.join(", "), values: { spo2, pulse } };
}

function plxContinuous(read: Reader): Decoded {
  const flags = read.u8();
  const spo2 = read.sfloat();
  const pulse = read.sfloat();
  const parts = [`SpO2 ${spo2} %, pulse ${pulse} bpm`];
  const values: Record<string, number> = { spo2, pulse };
  if (flags & 0x01) {
    values.fastSpo2 = read.sfloat();
    values.fastPulse = read.sfloat();
    parts.push(`fast ${values.fastSpo2} %, ${values.fastPulse} bpm`);
  }
  if (flags & 0x02) {
    values.slowSpo2 = read.sfloat();
    values.slowPulse = read.sfloat();
    parts.push(`slow ${values.slowSpo2} %, ${values.slowPulse} bpm`);
  }
  plxStatus(read, flags, 0x04, 0x08, parts);
  if (flags & 0x10) parts.push(`pulse amplitude index ${read.sfloat()} %`);
  return { text: parts.join(", "), values };
}

function cyclingPower(read: Reader): Decoded {
  const flags = read.u16();
  const power = read.i16();
  const parts = [`${power} W`];
  const values: Record<string, number> = { power };
  if (flags & 0x0001) {
    const balance = read.u8() / 2;
    parts.push(`balance ${balance} %${flags & 0x0002 ? " left" : ""}`);
    values.balance = balance;
  }
  if (flags & 0x0004) {
    const torque = read.u16() / 32;
    parts.push(`torque ${torque} Nm${flags & 0x0008 ? " at the crank" : " at the wheel"}`);
    values.torque = torque;
  }
  if (flags & 0x0010) {
    values.wheel = read.u32();
    parts.push(`wheel ${values.wheel} revs at ${read.u16() / 2048} s`);
  }
  if (flags & 0x0020) {
    values.crank = read.u16();
    parts.push(`crank ${values.crank} revs at ${read.u16() / 1024} s`);
  }
  if (flags & 0x0040) {
    const max = read.i16();
    const min = read.i16();
    parts.push(`force ${min} to ${max} N`);
  }
  if (flags & 0x0080) {
    const max = read.i16() / 32;
    const min = read.i16() / 32;
    parts.push(`torque ${min} to ${max} Nm`);
  }
  if (flags & 0x0100) {
    const angles = read.u24();
    parts.push(`angle max ${angles & 0xfff}°, min ${angles >> 12}°`);
  }
  if (flags & 0x0200) parts.push(`top dead spot ${read.u16()}°`);
  if (flags & 0x0400) parts.push(`bottom dead spot ${read.u16()}°`);
  if (flags & 0x0800) {
    values.energy = read.u16();
    parts.push(`${values.energy} kJ`);
  }
  if (flags & 0x1000) parts.push("offset compensation needed");
  return { text: parts.join(", "), values };
}

function cyclingPowerFeature(value: number): Decoded {
  const list = bitNames(value, CYCLING_POWER_FEATURES);
  list.push(value & 0x10000 ? "torque based" : "force based");
  const distributed = (value >> 20) & 0x03;
  if (distributed === 1) list.push("not for a distributed system");
  if (distributed === 2) list.push("for a distributed system");
  return { text: list.join(", "), values: {} };
}

const RESULTS: Record<number, string> = {
  1: "Success",
  2: "Op code not supported",
  3: "Invalid parameter",
  4: "Operation failed",
};

const SC_OPS: Record<number, string> = {
  1: "Set cumulative value",
  2: "Start sensor calibration",
  3: "Update sensor location",
  4: "Request supported sensor locations",
};

/** The SC Control Point of Cycling Speed and Cadence and of Running Speed and Cadence, 0x2A55. */
function scControlPoint(read: Reader): Decoded {
  const op = read.u8();
  if (op === 0x10) {
    const request = read.u8();
    const result = read.u8();
    let text = `${named(SC_OPS, request)}: ${named(RESULTS, result)}`;
    if (request === 4 && result === 1) text += `: ${read.rest().map((at) => named(SENSOR_LOCATIONS, at)).join(", ")}`;
    return { text, values: {} };
  }
  if (op === 1) return { text: `Set cumulative value ${read.u32()}`, values: {} };
  if (op === 3) return { text: `Update sensor location: ${named(SENSOR_LOCATIONS, read.u8())}`, values: {} };
  return { text: named(SC_OPS, op), values: {} };
}

const CP_OPS: Record<number, string> = {
  1: "Set cumulative value",
  2: "Update sensor location",
  3: "Request supported sensor locations",
  4: "Set crank length",
  5: "Request crank length",
  6: "Set chain length",
  7: "Request chain length",
  8: "Set chain weight",
  9: "Request chain weight",
  10: "Set span length",
  11: "Request span length",
  12: "Start offset compensation",
  13: "Mask measurement content",
  14: "Request sampling rate",
  15: "Request factory calibration date",
  16: "Start enhanced offset compensation",
};

/** The Cycling Power Control Point, 0x2A66. */
function cyclingPowerControlPoint(read: Reader): Decoded {
  const op = read.u8();
  if (op === 0x20) {
    const request = read.u8();
    const result = read.u8();
    let text = `${named(CP_OPS, request)}: ${named(RESULTS, result)}`;
    if (result === 1 && read.left > 0) {
      if (request === 3) text += `: ${read.rest().map((at) => named(SENSOR_LOCATIONS, at)).join(", ")}`;
      else if (request === 5) text += `: ${read.u16() / 2} mm`;
      else if (request === 12) text += `: offset ${read.i16()} (raw force in N, or torque in 1/32 Nm)`;
      else text += `: ${hex(read.rest())}`;
    }
    return { text, values: {} };
  }
  if (op === 1) return { text: `Set cumulative value ${read.u32()}`, values: {} };
  if (op === 2) return { text: `Update sensor location: ${named(SENSOR_LOCATIONS, read.u8())}`, values: {} };
  if (op === 4) return { text: `Set crank length ${read.u16() / 2} mm`, values: {} };
  return { text: named(CP_OPS, op), values: {} };
}

const RACP_OPS: Record<number, string> = {
  1: "Report stored records",
  2: "Delete stored records",
  3: "Abort operation",
  4: "Report number of stored records",
  5: "Number of stored records",
  6: "Response code",
};

const RACP_OPERATORS: Record<number, string> = {
  0: "",
  1: "all records",
  2: "less than or equal to",
  3: "greater than or equal to",
  4: "within range of",
  5: "first record",
  6: "last record",
};

const RACP_RESPONSES: Record<number, string> = {
  1: "Success",
  2: "Op code not supported",
  3: "Invalid operator",
  4: "Operator not supported",
  5: "Invalid operand",
  6: "No records found",
  7: "Abort unsuccessful",
  8: "Procedure not completed",
  9: "Operand not supported",
};

/** The Record Access Control Point, 0x2A52, of Glucose and the other record keepers. */
function recordAccess(read: Reader): Decoded {
  const op = read.u8();
  const operator = read.u8();
  if (op === 5) {
    const count = read.u16();
    return { text: `${count} stored record${count === 1 ? "" : "s"}`, values: { records: count } };
  }
  if (op === 6) {
    const request = read.u8();
    return { text: `${named(RACP_OPS, request)}: ${named(RACP_RESPONSES, read.u8())}`, values: {} };
  }
  const which = named(RACP_OPERATORS, operator);
  const operand = read.left > 0 ? ` ${hex(read.rest())}` : "";
  return { text: `${named(RACP_OPS, op)}${which ? `: ${which}` : ""}${operand}`, values: {} };
}

/** What a standard characteristic's value says, or null when it is not one bleNIMA knows. */
export function decodeProfile(uuid: string, bytes: number[], names: Names = {}): Decoded | null {
  if (bytes.length === 0) return null;
  if (isUartText(uuid)) return { text: `"${text(bytes)}"`, values: {} };
  const short = shortUuid(uuid);
  if (short === null) return null;
  if (STRINGS.has(short)) return { text: `"${utf8(bytes)}"`, values: {} };
  const simple = SIMPLE[short];
  if (simple) {
    const [value] = numbers(bytes, simple.format, true, simple.exponent);
    if (value === undefined) return null;
    if (short === 0x2a21 && value === 0) return { text: "no periodic measurement", values: { interval: 0 } };
    return { text: `${value}${simple.unit ? ` ${simple.unit}` : ""}`, values: { [simple.name]: value } };
  }
  try {
    return decodeRead(short, new Reader(bytes), bytes, names);
  } catch {
    // Too short for what its flags say: shown in its format instead.
    return null;
  }
}

function decodeRead(short: number, read: Reader, bytes: number[], names: Names): Decoded | null {
  switch (short) {
    case 0x2a01: {
      const value = read.u16();
      const name = names.appearance?.(value);
      return { text: name ? `${name} (${hex4(value)})` : `Appearance ${hex4(value)}`, values: {} };
    }
    case 0x2a04: {
      const min = read.u16() * 1.25;
      const max = read.u16() * 1.25;
      const latency = read.u16();
      const timeout = read.u16() * 10;
      return {
        text: `interval ${min} to ${max} ms, latency ${latency}, timeout ${timeout} ms`,
        values: { min, max, latency, timeout },
      };
    }
    case 0x2a05:
      return { text: `handles ${hex4(read.u16())} to ${hex4(read.u16())} changed`, values: {} };
    case 0x2a06: {
      const level = read.u8();
      return { text: named(ALERT_LEVELS, level), values: { alert: level } };
    }
    case 0x2a08:
      return { text: read.dateTime(), values: {} };
    case 0x2a0f:
      return localTime(read);
    case 0x2a14:
      return referenceTime(read);
    case 0x2a18:
      return glucose(read);
    case 0x2a1d:
      return { text: named(TEMPERATURE_TYPES, read.u8()), values: {} };
    case 0x2a23: {
      const manufacturer = read.big(5);
      const oui = read.u24();
      return {
        text: `manufacturer ID 0x${manufacturer.toString(16).toUpperCase().padStart(10, "0")}, OUI 0x${oui.toString(16).toUpperCase().padStart(6, "0")}`,
        values: {},
      };
    }
    case 0x2a2b:
      return currentTime(read);
    case 0x2a35:
      return bloodPressure(read, false);
    case 0x2a36:
      return bloodPressure(read, true);
    case 0x2a38:
      return { text: named(BODY_SENSOR_LOCATIONS, read.u8()), values: {} };
    case 0x2a39: {
      const op = read.u8();
      return { text: op === 1 ? "Reset energy expended" : `Op code ${op} (reserved)`, values: {} };
    }
    case 0x2a49:
      return { text: features(read.u16(), BLOOD_PRESSURE_FEATURES), values: {} };
    case 0x2a50: {
      const source = read.u8();
      const vendor = read.u16();
      const product = read.u16();
      const version = read.u16();
      const from = source === 1 ? "Bluetooth SIG" : source === 2 ? "USB" : `source ${source}`;
      return { text: `vendor ${hex4(vendor)} (${from}), product ${hex4(product)}, version ${hex4(version)}`, values: {} };
    }
    case 0x2a51:
      return { text: features(read.u16(), GLUCOSE_FEATURES), values: {} };
    case 0x2a52:
      return recordAccess(read);
    case 0x2a54:
      return {
        text: features(read.u16(), [
          "stride length",
          "total distance",
          "walking or running status",
          "calibration",
          "multiple sensor locations",
        ]),
        values: {},
      };
    case 0x2a55:
      return scControlPoint(read);
    case 0x2a5c:
      return { text: features(read.u16(), ["wheel revolutions", "crank revolutions", "multiple sensor locations"]), values: {} };
    case 0x2a5d:
      return { text: named(SENSOR_LOCATIONS, read.u8()), values: {} };
    case 0x2a5e:
      return plxSpotCheck(read);
    case 0x2a5f:
      return plxContinuous(read);
    case 0x2a63:
      return cyclingPower(read);
    case 0x2a65:
      return cyclingPowerFeature(read.u32());
    case 0x2a66:
      return cyclingPowerControlPoint(read);
    case 0x2a9d:
      return weight(read);
    case 0x2a9e:
      return weightFeature(read.u32());
    case 0x2aa6:
      return { text: read.u8() === 1 ? "address resolution supported" : "address resolution not supported", values: {} };
    case 0x2b29:
      return { text: features(read.u8(), ["Robust Caching", "Enhanced ATT bearer", "Multiple Handle Value Notifications"]), values: {} };
    case 0x2b2a:
      return { text: `hash ${bytes.map((b) => b.toString(16).toUpperCase().padStart(2, "0")).join("")}`, values: {} };
    case 0x2b3a:
      return { text: features(read.u8(), ["Enhanced ATT bearer"]), values: {} };
    case 0x2a37: {
      const flags = read.u8();
      const bpm = flags & 0x01 ? read.u16() : read.u8();
      const parts = [`${bpm} bpm`];
      const values: Record<string, number> = { bpm };
      if ((flags & 0x06) === 0x04) parts.push("no skin contact");
      if (flags & 0x08) {
        values.energy = read.u16();
        parts.push(`${values.energy} kJ`);
      }
      if (flags & 0x10) {
        const intervals: string[] = [];
        while (read.left >= 2) intervals.push(`${Math.round((read.u16() / 1024) * 1000)} ms`);
        if (intervals.length) parts.push(`RR ${intervals.join(", ")}`);
      }
      return { text: parts.join(", "), values };
    }
    case 0x2a1c:
    case 0x2a1e: {
      const flags = read.u8();
      const temperature = read.float();
      return { text: `${temperature} ${flags & 0x01 ? "°F" : "°C"}`, values: { temperature } };
    }
    case 0x2a5b: {
      const flags = read.u8();
      const parts: string[] = [];
      const values: Record<string, number> = {};
      if (flags & 0x01) {
        values.wheel = read.u32();
        parts.push(`wheel ${values.wheel} revs at ${read.u16() / 1024} s`);
      }
      if (flags & 0x02) {
        values.crank = read.u16();
        parts.push(`crank ${values.crank} revs at ${read.u16() / 1024} s`);
      }
      return { text: parts.join(", ") || "no data", values };
    }
    case 0x2a53: {
      read.u8();
      const speed = Number.parseFloat((read.u16() / 256).toFixed(2));
      const cadence = read.u8();
      return { text: `${speed} m/s, ${cadence} steps/min`, values: { speed, cadence } };
    }
    default:
      return null;
  }
}

/** The rows of the device information card, in the order they are shown: the first half on the left. */
export const DEVICE_INFO: { uuid: number; label: string }[] = [
  { uuid: 0x2a00, label: "Name" },
  { uuid: 0x2a01, label: "Appearance" },
  { uuid: 0x2a19, label: "Battery" },
  { uuid: 0x2a26, label: "Firmware" },
  { uuid: 0x2a28, label: "Software" },
  { uuid: 0x2a29, label: "Manufacturer" },
  { uuid: 0x2a24, label: "Model" },
  { uuid: 0x2a25, label: "Serial" },
  { uuid: 0x2a27, label: "Hardware" },
  { uuid: 0x2a50, label: "PnP ID" },
];

/** A device information card row's value: text without quotes, the rest as decoded. */
export function infoText(uuid: number, bytes: number[], names: Names = {}): string {
  if (STRINGS.has(uuid)) return utf8(bytes);
  const full = `0000${uuid.toString(16).padStart(4, "0")}-0000-1000-8000-00805f9b34fb`;
  return decodeProfile(full, bytes, names)?.text ?? hex(bytes);
}
