// MCUmgr's SMP protocol, as Zephyr and nRF Connect SDK devices speak it over
// Bluetooth: an 8-byte header, then a CBOR map. The image group's requests and
// answers: slots, upload, test, confirm, erase, slot sizes. Also the MCUboot
// image header, to show what is about to be uploaded. It has no imports, so
// its tests run in Node as they are.

export type Cbor = number | bigint | string | boolean | null | Uint8Array | Cbor[] | { [key: string]: Cbor };

/** CBOR, as SMP uses it: maps with text keys, numbers, text, bytes, arrays and true/false. */
export function encodeCbor(value: Cbor): Uint8Array {
  const out: number[] = [];
  const head = (major: number, length: number | bigint) => {
    const n = BigInt(length);
    if (n < 24n) out.push((major << 5) | Number(n));
    else if (n < 0x100n) out.push((major << 5) | 24, Number(n));
    else if (n < 0x10000n) out.push((major << 5) | 25, Number(n >> 8n), Number(n & 0xffn));
    else if (n < 0x100000000n) {
      out.push((major << 5) | 26);
      for (let shift = 24n; shift >= 0n; shift -= 8n) out.push(Number((n >> shift) & 0xffn));
    } else {
      out.push((major << 5) | 27);
      for (let shift = 56n; shift >= 0n; shift -= 8n) out.push(Number((n >> shift) & 0xffn));
    }
  };
  const write = (item: Cbor) => {
    if (item === null) out.push(0xf6);
    else if (item === false) out.push(0xf4);
    else if (item === true) out.push(0xf5);
    else if (typeof item === "number" || typeof item === "bigint") {
      const n = BigInt(Math.trunc(Number(item)) === Number(item) ? item : Math.trunc(Number(item)));
      if (n >= 0n) head(0, n);
      else head(1, -1n - n);
    } else if (typeof item === "string") {
      const bytes = new TextEncoder().encode(item);
      head(3, bytes.length);
      out.push(...bytes);
    } else if (item instanceof Uint8Array) {
      head(2, item.length);
      for (const byte of item) out.push(byte);
    } else if (Array.isArray(item)) {
      head(4, item.length);
      item.forEach(write);
    } else {
      const entries = Object.entries(item);
      head(5, entries.length);
      for (const [key, entry] of entries) {
        write(key);
        write(entry);
      }
    }
  };
  write(value);
  return Uint8Array.from(out);
}

/** Reads one CBOR item. Throws on what SMP never sends, or on bytes cut short. */
export function decodeCbor(bytes: Uint8Array): Cbor {
  let at = 0;
  const need = (count: number) => {
    if (at + count > bytes.length) throw new Error("CBOR cut short");
  };
  const length = (info: number): number => {
    if (info < 24) return info;
    const size = { 24: 1, 25: 2, 26: 4, 27: 8 }[info];
    if (!size) throw new Error(`CBOR length ${info} not supported`);
    need(size);
    let n = 0n;
    for (let i = 0; i < size; i++) n = (n << 8n) | BigInt(bytes[at++]);
    return Number(n);
  };
  const read = (): Cbor => {
    need(1);
    const first = bytes[at++];
    const major = first >> 5;
    const info = first & 0x1f;
    switch (major) {
      case 0:
        return length(info);
      case 1:
        return -1 - length(info);
      case 2: {
        const n = length(info);
        need(n);
        const value = bytes.slice(at, at + n);
        at += n;
        return value;
      }
      case 3: {
        const n = length(info);
        need(n);
        const value = new TextDecoder().decode(bytes.subarray(at, at + n));
        at += n;
        return value;
      }
      case 4: {
        const items: Cbor[] = [];
        if (info === 31) {
          while (bytes[at] !== 0xff) items.push(read());
          at++;
        } else {
          const n = length(info);
          for (let i = 0; i < n; i++) items.push(read());
        }
        return items;
      }
      case 5: {
        const map: { [key: string]: Cbor } = {};
        const entry = () => {
          const key = read();
          map[String(key)] = read();
        };
        if (info === 31) {
          while (bytes[at] !== 0xff) entry();
          at++;
        } else {
          const n = length(info);
          for (let i = 0; i < n; i++) entry();
        }
        return map;
      }
      case 7:
        if (info === 20) return false;
        if (info === 21) return true;
        if (info === 22 || info === 23) return null;
        if (info === 25) {
          need(2);
          const half = (bytes[at] << 8) | bytes[at + 1];
          at += 2;
          const exponent = (half >> 10) & 0x1f;
          const fraction = half & 0x3ff;
          const sign = half & 0x8000 ? -1 : 1;
          if (exponent === 0) return sign * 2 ** -14 * (fraction / 1024);
          if (exponent === 31) return fraction ? NaN : sign * Infinity;
          return sign * 2 ** (exponent - 15) * (1 + fraction / 1024);
        }
        if (info === 26 || info === 27) {
          const size = info === 26 ? 4 : 8;
          need(size);
          const view = new DataView(bytes.buffer, bytes.byteOffset + at, size);
          at += size;
          return size === 4 ? view.getFloat32(0) : view.getFloat64(0);
        }
        throw new Error(`CBOR simple value ${info} not supported`);
      default:
        throw new Error(`CBOR major type ${major} not supported`);
    }
  };
  return read();
}

export const OP = { read: 0, readResponse: 1, write: 2, writeResponse: 3 } as const;
export const GROUP = { os: 0, image: 1, stat: 2, fs: 8, shell: 9 } as const;

export type Header = { op: number; flags: number; length: number; group: number; seq: number; id: number };

/** An SMP frame: its header and its CBOR body. */
export function frame(op: number, group: number, id: number, seq: number, body: Cbor): Uint8Array {
  const cbor = encodeCbor(body);
  const out = new Uint8Array(8 + cbor.length);
  out[0] = op & 0x07;
  out[1] = 0;
  out[2] = cbor.length >> 8;
  out[3] = cbor.length & 0xff;
  out[4] = group >> 8;
  out[5] = group & 0xff;
  out[6] = seq & 0xff;
  out[7] = id;
  out.set(cbor, 8);
  return out;
}

export function header(bytes: ArrayLike<number>): Header | null {
  if (bytes.length < 8) return null;
  return {
    op: bytes[0] & 0x07,
    flags: bytes[1],
    length: (bytes[2] << 8) | bytes[3],
    group: (bytes[4] << 8) | bytes[5],
    seq: bytes[6],
    id: bytes[7],
  };
}

/**
 * Puts responses back together from notifications: a frame longer than the
 * MTU comes in pieces. Returns the frames that are whole, and keeps the rest.
 */
export class Reassembler {
  private buffer: number[] = [];

  push(piece: ArrayLike<number>): { header: Header; body: Cbor }[] {
    this.buffer.push(...Array.from(piece));
    const frames: { header: Header; body: Cbor }[] = [];
    for (;;) {
      const head = header(this.buffer);
      if (!head || this.buffer.length < 8 + head.length) break;
      const body = Uint8Array.from(this.buffer.slice(8, 8 + head.length));
      this.buffer.splice(0, 8 + head.length);
      frames.push({ header: head, body: head.length > 0 ? decodeCbor(body) : {} });
    }
    return frames;
  }

  clear(): void {
    this.buffer = [];
  }
}

/** The image group's command IDs. */
export const IMAGE = { state: 0, upload: 1, erase: 5, slotInfo: 6 } as const;

/** The OS group's command IDs. */
export const OS = { echo: 0, reset: 5, params: 6 } as const;

/** A CBOR map's entries, or null when it is not a map. */
function asMap(body: Cbor): { [key: string]: Cbor } | null {
  if (!body || typeof body !== "object" || Array.isArray(body) || body instanceof Uint8Array) return null;
  return body as { [key: string]: Cbor };
}

/** MCUmgr's return codes in words. */
export function rcText(rc: number): string {
  const codes: Record<number, string> = {
    0: "OK",
    1: "Unknown error",
    2: "Out of memory",
    3: "Invalid value",
    4: "Timed out",
    5: "No such entry",
    6: "Bad state",
    7: "Response too large",
    8: "Not supported",
    9: "Corrupt",
    10: "Busy",
    11: "Access denied",
    12: "Unsupported protocol version",
  };
  return codes[rc] ?? `Error ${rc}`;
}

/** The image group's own error codes, as SMP version 2 answers give them. */
const IMAGE_ERRORS: Record<number, string> = {
  1: "Unknown error",
  2: "Could not read the flash's layout",
  3: "No image in that slot",
  4: "The image has no TLVs",
  5: "The image has an invalid TLV",
  6: "The image has more than one hash",
  7: "The image has a TLV of the wrong size",
  8: "The image has no hash",
  9: "No free slot",
  10: "Could not open the flash",
  11: "Could not read the flash",
  12: "Could not write the flash",
  13: "Could not erase the flash",
  14: "No such slot",
  15: "Out of memory",
  16: "The flash is in use already",
  17: "The flash is not in use",
  18: "The flash area has no device",
  19: "Invalid page offset",
  20: "Invalid offset",
  21: "Invalid length",
  22: "Invalid image header",
  23: "The image header's magic is wrong: it is not an MCUboot image",
  24: "Invalid hash",
  25: "Invalid flash address",
  26: "Could not read the version",
  27: "The image that runs is newer",
  28: "An image is pending already",
  29: "The image's vector table is invalid",
  30: "The image is too large for its slot",
  31: "The image's data runs over",
  32: "The image cannot be confirmed",
  33: "The image that runs cannot be set to test",
  34: "The device does not know its active slot",
};

/**
 * What went wrong, in words, from an answer: SMP version 1 says it with "rc",
 * version 2 with "err" and the group it is from. Null when nothing did.
 */
export function smpError(body: Cbor): string | null {
  const map = asMap(body);
  if (!map) return null;
  const err = asMap(map.err ?? null);
  if (err && typeof err.rc === "number" && err.rc !== 0) {
    const group = Number(err.group ?? -1);
    const words = group === GROUP.image ? (IMAGE_ERRORS[err.rc] ?? `error ${err.rc}`) : `error ${err.rc}`;
    return `${words} (group ${group}, rc ${err.rc})`;
  }
  if (typeof map.rc === "number" && map.rc !== 0) return `${rcText(map.rc)} (rc ${map.rc})`;
  return null;
}

/** What the device's SMP server can take at once, from its MCUmgr parameters. */
export type McumgrParams = { bufSize: number; bufCount: number };

export function mcumgrParams(body: Cbor): McumgrParams | null {
  const map = asMap(body);
  if (!map || typeof map.buf_size !== "number") return null;
  return { bufSize: map.buf_size, bufCount: typeof map.buf_count === "number" ? map.buf_count : 1 };
}

/** The sizes of an image's slots, from the image group's slot info. */
export type ImageSlots = { image: number; slots: { slot: number; size: number }[]; maxImageSize: number | null };

export function slotInfo(body: Cbor): ImageSlots[] {
  const images = asMap(body)?.images;
  if (!Array.isArray(images)) return [];
  return images.map((item) => {
    const entry = asMap(item) ?? {};
    const slots = Array.isArray(entry.slots) ? entry.slots : [];
    return {
      image: Number(entry.image ?? 0),
      slots: slots.map((slot) => {
        const one = asMap(slot) ?? {};
        return { slot: Number(one.slot ?? 0), size: Number(one.size ?? 0) };
      }),
      maxImageSize: typeof entry.max_image_size === "number" ? entry.max_image_size : null,
    };
  });
}

/**
 * The body of an image erase request. MCUmgr counts the slot over every image:
 * image 0 has slots 0 and 1, image 1 has 2 and 3.
 */
export function eraseBody(image: number, slot: number): { slot: number } {
  return { slot: image * 2 + slot };
}

/** The body of an upload request: the first one says the image, its length and hash, and whether only a newer one may go. */
export function uploadBody(
  offset: number,
  data: Uint8Array,
  first: { image: number; len: number; sha: Uint8Array; upgrade: boolean } | null,
): Record<string, Cbor> {
  if (!first) return { off: offset, data };
  const body: Record<string, Cbor> = { image: first.image, len: first.len, off: offset, sha: first.sha, data };
  if (first.upgrade) body.upgrade = true;
  return body;
}

/** Whether two hashes are the same bytes. */
export function sameHash(a: Uint8Array | null, b: Uint8Array | null): boolean {
  return a !== null && b !== null && a.length === b.length && a.every((byte, at) => byte === b[at]);
}

/** The image that runs now, of an image number. */
export function activeImage(images: SlotImage[], image: number): SlotImage | undefined {
  return images.find((entry) => entry.image === image && entry.active);
}

/** The image waiting in the slot an upload goes to: the one that is not active. */
export function standbyImage(images: SlotImage[], image: number): SlotImage | undefined {
  return images.find((entry) => entry.image === image && !entry.active);
}

/** What the MCUboot header at the start of an image says about it. */
export type ImageInfo = { version: string; headerSize: number; imageSize: number };

export function imageInfo(bytes: Uint8Array): ImageInfo | null {
  if (bytes.length < 32) return null;
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (view.getUint32(0, true) !== 0x96f3b83d) return null;
  const major = bytes[20];
  const minor = bytes[21];
  const revision = view.getUint16(22, true);
  const build = view.getUint32(24, true);
  return {
    version: `${major}.${minor}.${revision}${build ? `+${build}` : ""}`,
    headerSize: view.getUint16(8, true),
    imageSize: view.getUint32(12, true),
  };
}

/** One image a device reports in its slots. */
export type SlotImage = {
  image: number;
  slot: number;
  version: string;
  hash: Uint8Array | null;
  bootable: boolean;
  pending: boolean;
  confirmed: boolean;
  active: boolean;
  permanent: boolean;
};

/** The images in an image state response. */
export function slotImages(body: Cbor): SlotImage[] {
  if (!body || typeof body !== "object" || Array.isArray(body) || body instanceof Uint8Array) return [];
  const images = (body as { images?: Cbor }).images;
  if (!Array.isArray(images)) return [];
  return images.map((item) => {
    const entry = item as { [key: string]: Cbor };
    return {
      image: Number(entry.image ?? 0),
      slot: Number(entry.slot ?? 0),
      version: String(entry.version ?? ""),
      hash: entry.hash instanceof Uint8Array ? entry.hash : null,
      bootable: entry.bootable === true,
      pending: entry.pending === true,
      confirmed: entry.confirmed === true,
      active: entry.active === true,
      permanent: entry.permanent === true,
    };
  });
}

/**
 * How many bytes of image fit in one upload request, so the whole frame fits
 * in one write and in the device's SMP buffer: the smaller of the MTU less the
 * ATT header and the buffer, less the SMP header and the CBOR around the data.
 * The first request also carries the image, its length and hash, and the
 * upgrade flag.
 */
export function chunkSize(
  mtu: number,
  first: boolean,
  total: number,
  bufSize = Number.POSITIVE_INFINITY,
  image = 0,
  upgrade = false,
): number {
  const room = Math.min(mtu - 3, bufSize) - 8;
  const overhead = encodeCbor(
    uploadBody(first ? 0 : total, new Uint8Array(0), first ? { image, len: total, sha: new Uint8Array(32), upgrade } : null),
  ).length;
  // The data's own CBOR length grows by up to 2 bytes as it gets longer.
  return Math.max(16, room - overhead - 2);
}
