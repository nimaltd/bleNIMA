// Values as bytes and as people read them: the formats a characteristic's value
// can be shown in, and text or hex typed to write. It has no imports, so its
// tests run in Node as they are.

export type Format =
  | "hex"
  | "utf8"
  | "bool"
  | "u8"
  | "i8"
  | "u16"
  | "i16"
  | "u24"
  | "i24"
  | "u32"
  | "i32"
  | "u64"
  | "i64"
  | "f32"
  | "f64"
  | "sfloat"
  | "float";

export const FORMATS: { format: Format; label: string }[] = [
  { format: "hex", label: "Hex" },
  { format: "utf8", label: "Text (UTF-8)" },
  { format: "u8", label: "uint8" },
  { format: "i8", label: "int8" },
  { format: "u16", label: "uint16" },
  { format: "i16", label: "int16" },
  { format: "u24", label: "uint24" },
  { format: "i24", label: "int24" },
  { format: "u32", label: "uint32" },
  { format: "i32", label: "int32" },
  { format: "u64", label: "uint64" },
  { format: "i64", label: "int64" },
  { format: "f32", label: "float32" },
  { format: "f64", label: "float64" },
  { format: "sfloat", label: "SFLOAT (IEEE-11073)" },
  { format: "float", label: "FLOAT (IEEE-11073)" },
  { format: "bool", label: "Boolean" },
];

/** How many bytes one value of a number format takes; 0 for hex and text. */
export function width(format: Format): number {
  switch (format) {
    case "bool":
    case "u8":
    case "i8":
      return 1;
    case "u16":
    case "i16":
    case "sfloat":
      return 2;
    case "u24":
    case "i24":
      return 3;
    case "u32":
    case "i32":
    case "f32":
    case "float":
      return 4;
    case "u64":
    case "i64":
    case "f64":
      return 8;
    default:
      return 0;
  }
}

/** Bytes as hex, a space between each: "02 01 06". */
export function hex(bytes: ArrayLike<number>): string {
  return Array.from(bytes, (byte) => byte.toString(16).toUpperCase().padStart(2, "0")).join(" ");
}

/** UTF-8 bytes as text, with control characters shown as escapes. */
export function text(bytes: ArrayLike<number>): string {
  const decoded = new TextDecoder("utf-8", { fatal: false }).decode(Uint8Array.from(bytes));
  return decoded.replace(/[\x00-\x08\x0b-\x1f\x7f]/g, (c) => `\\x${c.charCodeAt(0).toString(16).padStart(2, "0")}`);
}

/** An IEEE-11073 16-bit SFLOAT: 4-bit exponent, 12-bit mantissa, both signed. */
export function sfloat(raw: number): number {
  const special: Record<number, number> = { 0x07ff: NaN, 0x0800: NaN, 0x07fe: Infinity, 0x0802: -Infinity, 0x0801: NaN };
  if (raw in special) return special[raw];
  let mantissa = raw & 0x0fff;
  let exponent = raw >> 12;
  if (mantissa >= 0x0800) mantissa -= 0x1000;
  if (exponent >= 0x8) exponent -= 0x10;
  return round(mantissa * 10 ** exponent);
}

/** An IEEE-11073 32-bit FLOAT: 8-bit exponent, 24-bit mantissa, both signed. */
export function float11073(raw: number): number {
  const mantissaBits = raw & 0xffffff;
  const special: Record<number, number> = {
    0x7fffff: NaN,
    0x800000: NaN,
    0x7ffffe: Infinity,
    0x800002: -Infinity,
    0x800001: NaN,
  };
  if (mantissaBits in special) return special[mantissaBits];
  const mantissa = mantissaBits >= 0x800000 ? mantissaBits - 0x1000000 : mantissaBits;
  let exponent = (raw >>> 24) & 0xff;
  if (exponent >= 0x80) exponent -= 0x100;
  return round(mantissa * 10 ** exponent);
}

/** Rounds away the noise of decimal powers: 0.1 * 3 is 0.3, not 0.30000000000000004. */
function round(value: number): number {
  return Number.parseFloat(value.toPrecision(12));
}

/**
 * Every value of a number format in the bytes, one after another, multiplied by
 * 10 to `exponent`. Bytes left over that do not make a whole value are dropped.
 */
export function numbers(bytes: ArrayLike<number>, format: Format, littleEndian = true, exponent = 0): number[] {
  const size = width(format);
  if (size === 0) return [];
  const data = Uint8Array.from(bytes);
  const view = new DataView(data.buffer);
  const values: number[] = [];
  for (let at = 0; at + size <= data.length; at += size) {
    let value: number;
    switch (format) {
      case "bool":
        value = data[at] === 0 ? 0 : 1;
        break;
      case "u8":
        value = view.getUint8(at);
        break;
      case "i8":
        value = view.getInt8(at);
        break;
      case "u16":
        value = view.getUint16(at, littleEndian);
        break;
      case "i16":
        value = view.getInt16(at, littleEndian);
        break;
      case "u24":
      case "i24": {
        const [a, b, c] = littleEndian ? [data[at], data[at + 1], data[at + 2]] : [data[at + 2], data[at + 1], data[at]];
        value = a | (b << 8) | (c << 16);
        if (format === "i24" && value >= 0x800000) value -= 0x1000000;
        break;
      }
      case "u32":
        value = view.getUint32(at, littleEndian);
        break;
      case "i32":
        value = view.getInt32(at, littleEndian);
        break;
      case "u64":
        value = Number(view.getBigUint64(at, littleEndian));
        break;
      case "i64":
        value = Number(view.getBigInt64(at, littleEndian));
        break;
      case "f32":
        value = view.getFloat32(at, littleEndian);
        break;
      case "f64":
        value = view.getFloat64(at, littleEndian);
        break;
      case "sfloat":
        value = sfloat(view.getUint16(at, littleEndian));
        break;
      case "float":
        value = float11073(view.getUint32(at, littleEndian));
        break;
      default:
        continue;
    }
    values.push(exponent === 0 ? value : round(value * 10 ** exponent));
  }
  return values;
}

/** A value in a format, as text: "21.5", "1, 2, 3", "hello", "02 01". */
export function show(bytes: ArrayLike<number>, format: Format, littleEndian = true, exponent = 0, unit = ""): string {
  if (bytes.length === 0) return "(empty)";
  if (format === "hex") return hex(bytes);
  if (format === "utf8") return `"${text(bytes)}"`;
  const values = numbers(bytes, format, littleEndian, exponent);
  if (values.length === 0) return `${hex(bytes)} (too short for ${format})`;
  const shown = values.map((value) => (format === "bool" ? (value ? "true" : "false") : String(value))).join(", ");
  const extra = bytes.length % width(format) === 0 ? "" : ` +${bytes.length % width(format)} bytes`;
  return `${shown}${unit ? ` ${unit}` : ""}${extra}`;
}

/** Hex digits as bytes: "AA 55 01", "aa5501" or "0xAA,0x55". An error in words if they are not. */
export function parseHex(input: string): number[] | string {
  const digits = input.replace(/0x/gi, "").replace(/[\s,;:-]/g, "");
  if (digits.length === 0) return [];
  if (!/^[0-9a-f]*$/i.test(digits)) return "Only hex digits, such as AA 55 01";
  if (digits.length % 2 !== 0) return "An odd number of hex digits: each byte takes two";
  const bytes: number[] = [];
  for (let at = 0; at < digits.length; at += 2) bytes.push(Number.parseInt(digits.slice(at, at + 2), 16));
  return bytes;
}

/** Text as UTF-8 bytes, with \r, \n, \t, \\ and \xNN taken as the bytes they stand for. */
export function parseText(input: string): number[] {
  const bytes: number[] = [];
  const encoder = new TextEncoder();
  for (let at = 0; at < input.length; at++) {
    const char = input[at];
    if (char === "\\" && at + 1 < input.length) {
      const next = input[at + 1];
      const simple: Record<string, number> = { r: 13, n: 10, t: 9, "\\": 92, "0": 0 };
      if (next in simple) {
        bytes.push(simple[next]);
        at++;
        continue;
      }
      if (next === "x" && /^[0-9a-f]{2}$/i.test(input.slice(at + 2, at + 4))) {
        bytes.push(Number.parseInt(input.slice(at + 2, at + 4), 16));
        at += 3;
        continue;
      }
    }
    bytes.push(...encoder.encode(char));
  }
  return bytes;
}

/** A number typed for a number format, as its bytes. An error in words if it does not fit. */
export function encodeNumber(input: string, format: Format, littleEndian = true): number[] | string {
  const size = width(format);
  if (size === 0 || format === "sfloat" || format === "float") return `${format} values cannot be typed as a number`;
  const value = Number(input.trim());
  if (input.trim() === "" || Number.isNaN(value)) return "Not a number";
  const view = new DataView(new ArrayBuffer(8));
  const ranges: Partial<Record<Format, [number, number]>> = {
    bool: [0, 1],
    u8: [0, 0xff],
    i8: [-0x80, 0x7f],
    u16: [0, 0xffff],
    i16: [-0x8000, 0x7fff],
    u24: [0, 0xffffff],
    i24: [-0x800000, 0x7fffff],
    u32: [0, 0xffffffff],
    i32: [-0x80000000, 0x7fffffff],
  };
  const range = ranges[format];
  if (range && (!Number.isInteger(value) || value < range[0] || value > range[1])) {
    return `A whole number from ${range[0]} to ${range[1]}`;
  }
  switch (format) {
    case "bool":
    case "u8":
    case "i8":
      return [value & 0xff];
    case "u16":
    case "i16":
      view.setUint16(0, value & 0xffff, littleEndian);
      break;
    case "u24":
    case "i24": {
      const raw = value & 0xffffff;
      const bytes = [raw & 0xff, (raw >> 8) & 0xff, (raw >> 16) & 0xff];
      return littleEndian ? bytes : bytes.reverse();
    }
    case "u32":
    case "i32":
      view.setUint32(0, value >>> 0, littleEndian);
      break;
    case "u64":
      view.setBigUint64(0, BigInt(Math.trunc(value)), littleEndian);
      break;
    case "i64":
      view.setBigInt64(0, BigInt(Math.trunc(value)), littleEndian);
      break;
    case "f32":
      view.setFloat32(0, value, littleEndian);
      break;
    case "f64":
      view.setFloat64(0, value, littleEndian);
      break;
  }
  return Array.from(new Uint8Array(view.buffer, 0, size));
}
