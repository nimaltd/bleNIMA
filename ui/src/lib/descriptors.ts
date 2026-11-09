// Standard descriptors in words: the Client Characteristic Configuration,
// Presentation Format, Valid Range, Report Reference and the others the
// Bluetooth SIG defines. It imports only format.ts and profiles.ts, so its
// tests run in Node as they are.

import { hex, numbers, show, width, type Format } from "./format.ts";
import { Reader, SIMPLE, hex4, named, shortUuid } from "./profiles.ts";

/** How the characteristic's own value is read: for Valid Range and the trigger settings. */
export type ValueFormat = { format: Format; exponent: number; unit: string };

/** A Presentation Format as the Rust side reads it, in `CharacteristicInfo`. */
type Presentation = { format: string; exponent: number; unit_symbol: string | null };

/** The characteristic's value format: from its Presentation Format, else the standard one, else null. */
export function valueFormat(uuid: string, presentation: Presentation | null): ValueFormat | null {
  if (presentation && width(presentation.format as Format) > 0) {
    return { format: presentation.format as Format, exponent: presentation.exponent, unit: presentation.unit_symbol ?? "" };
  }
  const short = shortUuid(uuid);
  const simple = short === null ? undefined : SIMPLE[short];
  return simple ? { format: simple.format, exponent: simple.exponent, unit: simple.unit } : null;
}

/** The formats of the Presentation Format descriptor, by their assigned numbers. */
const FORMAT_NAMES: Record<number, string> = {
  0x01: "boolean",
  0x02: "2-bit",
  0x03: "4-bit",
  0x04: "uint8",
  0x05: "uint12",
  0x06: "uint16",
  0x07: "uint24",
  0x08: "uint32",
  0x09: "uint48",
  0x0a: "uint64",
  0x0b: "uint128",
  0x0c: "sint8",
  0x0d: "sint12",
  0x0e: "sint16",
  0x0f: "sint24",
  0x10: "sint32",
  0x11: "sint48",
  0x12: "sint64",
  0x13: "sint128",
  0x14: "float32",
  0x15: "float64",
  0x16: "SFLOAT",
  0x17: "FLOAT",
  0x18: "duint16",
  0x19: "UTF-8 text",
  0x1a: "UTF-16 text",
  0x1b: "struct",
};

/** The symbols of the GATT units, by their assigned numbers. */
const UNITS: Record<number, string> = {
  0x2700: "no unit",
  0x2701: "m",
  0x2702: "kg",
  0x2703: "s",
  0x2704: "A",
  0x2705: "K",
  0x2706: "mol",
  0x2707: "cd",
  0x2710: "m²",
  0x2711: "m³",
  0x2712: "m/s",
  0x2713: "m/s²",
  0x2714: "1/m",
  0x2715: "kg/m³",
  0x2716: "kg/m²",
  0x2717: "m³/kg",
  0x2718: "A/m²",
  0x2719: "A/m",
  0x271a: "mol/m³",
  0x271b: "kg/m³",
  0x271c: "cd/m²",
  0x271d: "refractive index",
  0x271e: "relative permeability",
  0x2720: "rad",
  0x2721: "sr",
  0x2722: "Hz",
  0x2723: "N",
  0x2724: "Pa",
  0x2725: "J",
  0x2726: "W",
  0x2727: "C",
  0x2728: "V",
  0x2729: "F",
  0x272a: "Ω",
  0x272b: "S",
  0x272c: "Wb",
  0x272d: "T",
  0x272e: "H",
  0x272f: "°C",
  0x2730: "lm",
  0x2731: "lx",
  0x2732: "Bq",
  0x2733: "Gy",
  0x2734: "Sv",
  0x2735: "kat",
  0x2740: "Pa s",
  0x2741: "N m",
  0x2742: "N/m",
  0x2743: "rad/s",
  0x2744: "rad/s²",
  0x2745: "W/m²",
  0x2746: "J/K",
  0x2747: "J/(kg K)",
  0x2748: "J/kg",
  0x2749: "W/(m K)",
  0x274a: "J/m³",
  0x274b: "V/m",
  0x274c: "C/m³",
  0x274d: "C/m²",
  0x274e: "C/m²",
  0x274f: "F/m",
  0x2750: "H/m",
  0x2751: "J/mol",
  0x2752: "J/(mol K)",
  0x2753: "C/kg",
  0x2754: "Gy/s",
  0x2755: "W/sr",
  0x2756: "W/(m² sr)",
  0x2757: "kat/m³",
  0x2760: "min",
  0x2761: "h",
  0x2762: "d",
  0x2763: "°",
  0x2764: "′",
  0x2765: "″",
  0x2766: "ha",
  0x2767: "L",
  0x2768: "t",
  0x2780: "bar",
  0x2781: "mmHg",
  0x2782: "Å",
  0x2783: "nmi",
  0x2784: "b",
  0x2785: "kn",
  0x2786: "Np",
  0x2787: "B",
  0x27a0: "yd",
  0x27a1: "pc",
  0x27a2: "in",
  0x27a3: "ft",
  0x27a4: "mi",
  0x27a5: "psi",
  0x27a6: "km/h",
  0x27a7: "mph",
  0x27a8: "rpm",
  0x27a9: "cal",
  0x27aa: "kcal",
  0x27ab: "kWh",
  0x27ac: "°F",
  0x27ad: "%",
  0x27ae: "‰",
  0x27af: "bpm",
  0x27b0: "Ah",
  0x27b1: "mg/dL",
  0x27b2: "mmol/L",
  0x27b3: "years",
  0x27b4: "months",
  0x27b5: "1/m³",
  0x27b6: "W/m²",
  0x27b7: "mL/(kg min)",
  0x27b8: "lb",
  0x27b9: "MET",
  0x27ba: "steps/min",
  0x27bc: "strokes/min",
  0x27bd: "km/min",
  0x27be: "lm/W",
  0x27bf: "lm h",
  0x27c0: "lx h",
  0x27c1: "g/s",
  0x27c2: "L/s",
  0x27c3: "dB",
  0x27c4: "ppm",
  0x27c5: "ppb",
  0x27c6: "mg/dL/min",
  0x27c7: "kVAh",
  0x27c8: "VA",
};

/** The Bluetooth SIG's names for the Presentation Format's description field. */
const DESCRIPTIONS: Record<number, string> = {
  0x0000: "unknown",
  0x0100: "front",
  0x0101: "back",
  0x0102: "top",
  0x0103: "bottom",
  0x0104: "upper",
  0x0105: "lower",
  0x0106: "main",
  0x0107: "backup",
  0x0108: "auxiliary",
  0x0109: "supplementary",
  0x010a: "flash",
  0x010b: "inside",
  0x010c: "outside",
  0x010d: "left",
  0x010e: "right",
  0x010f: "internal",
  0x0110: "external",
};

/** "1st", "2nd", "3rd", "11th"... */
function ordinal(n: number): string {
  const tens = n % 100;
  const suffix = tens >= 11 && tens <= 13 ? "th" : ({ 1: "st", 2: "nd", 3: "rd" } as Record<number, string>)[n % 10] ?? "th";
  return `${n}${suffix}`;
}

function presentationFormat(read: Reader): string {
  const format = read.u8();
  const exponent = read.i8();
  const unit = read.u16();
  const parts = [FORMAT_NAMES[format] ?? `format ${format} (reserved)`];
  if (exponent !== 0) parts[0] += ` × 10^${exponent}`;
  parts.push(unit === 0x2700 ? "no unit" : `unit ${UNITS[unit] ?? "unknown"} (${hex4(unit)})`);
  if (read.left >= 3) {
    const namespace = read.u8();
    const description = read.u16();
    if (namespace === 1) {
      const text = description >= 1 && description <= 0xff ? ordinal(description) : DESCRIPTIONS[description];
      if (description !== 0) parts.push(`description ${text ?? hex4(description)}`);
    } else if (namespace !== 0 || description !== 0) {
      parts.push(`namespace ${namespace}, description ${hex4(description)}`);
    }
  }
  return parts.join(", ");
}

/** Bytes in the characteristic's own format, or in hex when it is not known. */
function operand(bytes: number[], format: ValueFormat | null): string {
  if (bytes.length === 0) return "";
  if (!format || width(format.format) === 0) return hex(bytes);
  return show(bytes, format.format, true, format.exponent, format.unit);
}

/** Two values in the characteristic's format, as "low to high"; null when they are not two. */
function pair(bytes: number[], format: ValueFormat | null): string | null {
  if (!format || bytes.length !== 2 * width(format.format)) return null;
  const [low, high] = numbers(bytes, format.format, true, format.exponent);
  return `${low} to ${high}${format.unit ? ` ${format.unit}` : ""}`;
}

function validRange(bytes: number[], format: ValueFormat | null): string {
  const both = pair(bytes, format);
  if (both) return both;
  const half = bytes.length / 2;
  if (Number.isInteger(half) && half > 0) return `${hex(bytes.slice(0, half))} to ${hex(bytes.slice(half))}`;
  return hex(bytes);
}

const VALUE_TRIGGERS: Record<number, string> = {
  0: "Every change of the value",
  1: "When the value crosses a boundary",
  2: "When the value is on a boundary",
  3: "When the value changes by more than",
  4: "When the value, masked, changes; mask",
  5: "When the value goes inside or outside",
  6: "When the value is on either of",
  7: "No value trigger",
};

function valueTrigger(read: Reader, format: ValueFormat | null): string {
  const condition = read.u8();
  const rest = read.rest();
  const text = named(VALUE_TRIGGERS, condition);
  if (rest.length === 0) return text;
  if (condition === 4) return `${text} ${hex(rest)}`;
  if (condition === 5 || condition === 6) return `${text} ${pair(rest, format) ?? hex(rest)}`;
  return `${text} ${operand(rest, format)}`;
}

const SAMPLING: Record<number, string> = {
  0: "unspecified",
  1: "instantaneous",
  2: "arithmetic mean",
  3: "RMS",
  4: "maximum",
  5: "minimum",
  6: "accumulated",
  7: "count",
};

const APPLICATIONS: Record<number, string> = {
  0: "unspecified",
  1: "air",
  2: "water",
  3: "barometric",
  4: "soil",
  5: "infrared",
  6: "map database",
  7: "barometric elevation source",
  8: "GPS only elevation source",
  9: "GPS and map database elevation source",
  10: "vertical datum elevation source",
  11: "onshore",
  12: "on boat or vessel",
  13: "front",
  14: "back or rear",
  15: "upper",
  16: "lower",
  17: "primary",
  18: "secondary",
  19: "outdoor",
  20: "indoor",
  21: "top",
  22: "bottom",
  23: "main",
  24: "backup",
  25: "auxiliary",
  26: "supplementary",
  27: "inside",
  28: "outside",
  29: "left",
  30: "right",
  31: "internal",
  32: "external",
  33: "solar",
};

function seconds(value: number): string {
  return value === 0 ? "not in use" : `${value} s`;
}

function esMeasurement(read: Reader): string {
  read.u16();
  const sampling = named(SAMPLING, read.u8());
  const period = read.u24();
  const interval = read.u24();
  const application = named(APPLICATIONS, read.u8());
  const uncertainty = read.u8();
  return [
    `sampling ${sampling}`,
    `period ${seconds(period)}`,
    `update interval ${seconds(interval)}`,
    `application ${application}`,
    `uncertainty ${uncertainty === 0xff ? "unknown" : `${uncertainty / 2} %`}`,
  ].join(", ");
}

const ES_TRIGGERS: Record<number, string> = {
  0: "Trigger inactive",
  1: "At a fixed interval of",
  2: "No more often than every",
  3: "When the value changes",
  4: "While less than",
  5: "While less than or equal to",
  6: "While greater than",
  7: "While greater than or equal to",
  8: "While equal to",
  9: "While not equal to",
};

function esTrigger(read: Reader, format: ValueFormat | null): string {
  const condition = read.u8();
  const text = named(ES_TRIGGERS, condition);
  if (condition === 1 || condition === 2) return `${text} ${read.u24()} s`;
  const rest = read.rest();
  return rest.length ? `${text} ${operand(rest, format)}` : text;
}

function timeTrigger(read: Reader): string {
  const condition = read.u8();
  if (condition === 0) return "No time trigger";
  if (condition === 1) return `Every ${read.u24()} s, changed or not`;
  if (condition === 2) return `No more often than every ${read.u24()} s`;
  if (condition === 3) return `After the value changes ${read.u16()} times`;
  return `Condition ${condition} (reserved)`;
}

function onOff(on: boolean): string {
  return on ? "on" : "off";
}

/** A UUID as it is sent, least significant byte first, as text: "0x2A19", or all 128 bits. */
function uuidText(bytes: number[]): string {
  if (bytes.length === 2) return hex4(bytes[0] | (bytes[1] << 8));
  const digits = [...bytes].reverse().map((b) => b.toString(16).padStart(2, "0")).join("").toUpperCase();
  return `${digits.slice(0, 8)}-${digits.slice(8, 12)}-${digits.slice(12, 16)}-${digits.slice(16, 20)}-${digits.slice(20)}`;
}

/**
 * What a standard descriptor's value says, or null when it is not one bleNIMA
 * knows. `format` is how its characteristic's value is read, for the ones that
 * hold such values: Valid Range and the trigger settings.
 */
export function decodeDescriptor(uuid: string, bytes: number[], format: ValueFormat | null = null): string | null {
  const short = shortUuid(uuid);
  if (short === null || bytes.length === 0) return null;
  const read = new Reader(bytes);
  try {
    switch (short) {
      case 0x2900: {
        const bits = read.u16();
        return `Reliable write ${onOff((bits & 0x01) !== 0)}, writable auxiliaries ${onOff((bits & 0x02) !== 0)}`;
      }
      case 0x2901:
        return `"${new TextDecoder().decode(Uint8Array.from(bytes)).replace(/\0+$/, "")}"`;
      case 0x2902: {
        const bits = read.u16();
        return `Notifications ${onOff((bits & 0x01) !== 0)}, indications ${onOff((bits & 0x02) !== 0)}`;
      }
      case 0x2903:
        return `Broadcast ${onOff((read.u16() & 0x01) !== 0)}`;
      case 0x2904:
        return presentationFormat(read);
      case 0x2905: {
        const handles: string[] = [];
        while (read.left >= 2) handles.push(hex4(read.u16()));
        return `Handles ${handles.join(", ")}`;
      }
      case 0x2906:
        return validRange(bytes, format);
      case 0x2907:
        return `Characteristic ${uuidText(bytes)}`;
      case 0x2908: {
        const id = read.u8();
        const type = named({ 1: "Input report", 2: "Output report", 3: "Feature report" }, read.u8());
        return `Report ID ${id}, ${type}`;
      }
      case 0x2909: {
        const count = read.u8();
        return `${count} digital${count === 1 ? "" : "s"}`;
      }
      case 0x290a:
        return valueTrigger(read, format);
      case 0x290b:
        return named({ 0: "Triggers combined with AND", 1: "Triggers combined with OR" }, read.u8());
      case 0x290c:
        return esMeasurement(read);
      case 0x290d:
        return esTrigger(read, format);
      case 0x290e:
        return timeTrigger(read);
      default:
        return null;
    }
  } catch {
    return null;
  }
}

/** The descriptor's UUID in a key, "service/characteristic/descriptor". */
export function descriptorUuid(key: string): string {
  return key.split("/")[2] ?? "";
}

/** Whether a descriptor is the Client Characteristic Configuration, 0x2902. */
export function isCccd(key: string): boolean {
  return shortUuid(descriptorUuid(key)) === 0x2902;
}
