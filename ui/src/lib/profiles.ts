// The values of standard characteristics in words: Battery Level, Heart Rate,
// Temperature and the others the Bluetooth SIG defines, by their 16-bit UUID.
// It has no imports, so its tests run in Node as they are.

/** The 16-bit UUID a full one is short for, or null: "00002a19-0000-1000-8000-00805f9b34fb" is 0x2A19. */
export function shortUuid(uuid: string): number | null {
  const match = /^0000([0-9a-f]{4})-0000-1000-8000-00805f9b34fb$/i.exec(uuid);
  return match ? Number.parseInt(match[1], 16) : null;
}

/** The characteristic's UUID in a key, "service/characteristic". */
export function characteristicUuid(key: string): string {
  return key.split("/")[1] ?? "";
}

function u16(bytes: number[], at: number): number {
  return bytes[at] | (bytes[at + 1] << 8);
}

function u32(bytes: number[], at: number): number {
  return (bytes[at] | (bytes[at + 1] << 8) | (bytes[at + 2] << 16) | (bytes[at + 3] << 24)) >>> 0;
}

function i16(bytes: number[], at: number): number {
  const value = u16(bytes, at);
  return value >= 0x8000 ? value - 0x10000 : value;
}

function utf8(bytes: number[]): string {
  return new TextDecoder().decode(Uint8Array.from(bytes)).replace(/\0+$/, "");
}

/** An IEEE-11073 FLOAT, as health devices send temperatures. */
function float11073(raw: number): number {
  let mantissa = raw & 0xffffff;
  if (mantissa >= 0x800000) mantissa -= 0x1000000;
  let exponent = (raw >>> 24) & 0xff;
  if (exponent >= 0x80) exponent -= 0x100;
  return Number.parseFloat((mantissa * 10 ** exponent).toPrecision(12));
}

const STRINGS = new Set([0x2a00, 0x2a24, 0x2a25, 0x2a26, 0x2a27, 0x2a28, 0x2a29, 0x2a87, 0x2a8a, 0x2a90]);

/** A decoded value, with the numbers in it by name, for plotting. */
export type Decoded = { text: string; values: Record<string, number> };

/** What a standard characteristic's value says, or null when it is not one bleNIMA knows. */
export function decodeProfile(uuid: string, bytes: number[]): Decoded | null {
  const short = shortUuid(uuid);
  if (short === null || bytes.length === 0) return null;
  if (STRINGS.has(short)) return { text: `"${utf8(bytes)}"`, values: {} };
  switch (short) {
    case 0x2a19:
      return { text: `${bytes[0]} %`, values: { battery: bytes[0] } };
    case 0x2a04: {
      if (bytes.length < 8) return null;
      const min = u16(bytes, 0) * 1.25;
      const max = u16(bytes, 2) * 1.25;
      const latency = u16(bytes, 4);
      const timeout = u16(bytes, 6) * 10;
      return {
        text: `interval ${min} to ${max} ms, latency ${latency}, timeout ${timeout} ms`,
        values: { min, max, latency, timeout },
      };
    }
    case 0x2a50: {
      if (bytes.length < 7) return null;
      const source = bytes[0] === 1 ? "Bluetooth SIG" : bytes[0] === 2 ? "USB" : `source ${bytes[0]}`;
      const hex4 = (n: number) => `0x${n.toString(16).toUpperCase().padStart(4, "0")}`;
      return {
        text: `vendor ${hex4(u16(bytes, 1))} (${source}), product ${hex4(u16(bytes, 3))}, version ${hex4(u16(bytes, 5))}`,
        values: {},
      };
    }
    case 0x2a37: {
      const flags = bytes[0];
      let at = 1;
      const wide = flags & 0x01;
      const bpm = wide ? u16(bytes, at) : bytes[at];
      at += wide ? 2 : 1;
      const parts = [`${bpm} bpm`];
      const values: Record<string, number> = { bpm };
      if ((flags & 0x06) === 0x04) parts.push("no skin contact");
      if (flags & 0x08) {
        values.energy = u16(bytes, at);
        parts.push(`${values.energy} kJ`);
        at += 2;
      }
      if (flags & 0x10) {
        const intervals: string[] = [];
        for (; at + 1 < bytes.length; at += 2) intervals.push(`${Math.round((u16(bytes, at) / 1024) * 1000)} ms`);
        if (intervals.length) parts.push(`RR ${intervals.join(", ")}`);
      }
      return { text: parts.join(", "), values };
    }
    case 0x2a1c:
    case 0x2a1e: {
      if (bytes.length < 5) return null;
      const temperature = float11073(u32(bytes, 1));
      const unit = bytes[0] & 0x01 ? "°F" : "°C";
      return { text: `${temperature} ${unit}`, values: { temperature } };
    }
    case 0x2a6e: {
      const temperature = i16(bytes, 0) / 100;
      return { text: `${temperature} °C`, values: { temperature } };
    }
    case 0x2a6f: {
      const humidity = u16(bytes, 0) / 100;
      return { text: `${humidity} %`, values: { humidity } };
    }
    case 0x2a6d: {
      const pressure = u32(bytes, 0) / 10;
      return { text: `${pressure} Pa`, values: { pressure } };
    }
    case 0x2a5b: {
      const flags = bytes[0];
      let at = 1;
      const parts: string[] = [];
      const values: Record<string, number> = {};
      if (flags & 0x01) {
        values.wheel = u32(bytes, at);
        parts.push(`wheel ${values.wheel} revs at ${u16(bytes, at + 4) / 1024} s`);
        at += 6;
      }
      if (flags & 0x02) {
        values.crank = u16(bytes, at);
        parts.push(`crank ${values.crank} revs at ${u16(bytes, at + 2) / 1024} s`);
      }
      return { text: parts.join(", ") || "no data", values };
    }
    case 0x2a53: {
      if (bytes.length < 4) return null;
      const speed = Number.parseFloat((u16(bytes, 1) / 256).toFixed(2));
      const cadence = bytes[3];
      return { text: `${speed} m/s, ${cadence} steps/min`, values: { speed, cadence } };
    }
    default:
      return null;
  }
}
