// Adverts to send as a peripheral: iBeacon and Eddystone URL made into AD
// structures, and their size. It has no imports, so its tests run in Node as they are.

export type Structure = { ad_type: number; data: number[] };

/** The most an advert can carry before Bluetooth 5's extended adverts. */
export const LEGACY_MAX = 31;

/** A UUID's 16 bytes, as written: "E2C56DB5-DFFB-48D2-B060-D0F5A71096E0". Null if it is not one. */
export function uuidBytes(uuid: string): number[] | null {
  const hex = uuid.replace(/-/g, "").trim();
  if (!/^[0-9a-f]{32}$/i.test(hex)) return null;
  return hex.match(/../g)!.map((byte) => Number.parseInt(byte, 16));
}

/** iBeacon: Apple's manufacturer data with a UUID, a major, a minor and the power at 1 m. */
export function ibeacon(uuid: string, major: number, minor: number, power: number): Structure[] | string {
  const bytes = uuidBytes(uuid);
  if (!bytes) return "The UUID needs 32 hex digits, such as E2C56DB5-DFFB-48D2-B060-D0F5A71096E0";
  for (const [name, value] of [
    ["Major", major],
    ["Minor", minor],
  ] as const) {
    if (!Number.isInteger(value) || value < 0 || value > 0xffff) return `${name} is a whole number from 0 to 65535`;
  }
  if (!Number.isInteger(power) || power < -128 || power > 127) return "The power is a whole number of dBm, -128 to 127";
  return [
    {
      ad_type: 0xff,
      data: [0x4c, 0x00, 0x02, 0x15, ...bytes, major >> 8, major & 0xff, minor >> 8, minor & 0xff, power & 0xff],
    },
  ];
}

const SCHEMES = ["http://www.", "https://www.", "http://", "https://"];
const ENDINGS = [".com/", ".org/", ".edu/", ".net/", ".info/", ".biz/", ".gov/", ".com", ".org", ".edu", ".net", ".info", ".biz", ".gov"];

/** A URL as Eddystone sends it: a scheme byte, then the text with common endings as one byte. */
export function encodeUrl(url: string): number[] | string {
  const scheme = SCHEMES.findIndex((prefix) => url.startsWith(prefix));
  if (scheme < 0) return "The URL starts with http:// or https://";
  let rest = url.slice(SCHEMES[scheme].length);
  const out = [scheme];
  while (rest.length > 0) {
    const ending = ENDINGS.findIndex((text) => rest.startsWith(text));
    if (ending >= 0) {
      out.push(ending);
      rest = rest.slice(ENDINGS[ending].length);
    } else {
      const code = rest.charCodeAt(0);
      if (code < 0x21 || code > 0x7e) return "The URL has a character Eddystone cannot send";
      out.push(code);
      rest = rest.slice(1);
    }
  }
  if (out.length > 18) return `The URL is ${out.length - 1} bytes once shortened; Eddystone takes 17`;
  return out;
}

/** Eddystone URL: the Eddystone service UUID, and its service data with the URL. */
export function eddystoneUrl(url: string, power: number): Structure[] | string {
  const encoded = encodeUrl(url);
  if (typeof encoded === "string") return encoded;
  if (!Number.isInteger(power) || power < -100 || power > 20) return "The power at 0 m is a whole number of dBm, -100 to 20";
  return [
    { ad_type: 0x03, data: [0xaa, 0xfe] },
    { ad_type: 0x16, data: [0xaa, 0xfe, 0x10, power & 0xff, ...encoded] },
  ];
}

/** How many bytes the structures take on the air: a length and a type byte each, then the data. */
export function advertLength(structures: Structure[]): number {
  return structures.reduce((sum, structure) => sum + 2 + structure.data.length, 0);
}
