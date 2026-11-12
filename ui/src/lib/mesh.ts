// Bluetooth mesh access messages: their opcodes, how the ones bleNIMA sends
// are made, and how the ones that come back read. The configuration messages
// are the Mesh Protocol spec's (4.3); the Generic, Light, Sensor and Health
// ones are the Mesh Model spec's. It has no imports, so its tests run in Node
// as they are.

export type Bytes = number[];

/** A model: a SIG one, or a vendor's, with the vendor's company ID. */
export type Model = { id: number; vendor: number | null };

export type Element = { location: number; models: Model[] };

/** Composition Data page 0: what a node is made of. */
export type Composition = {
  cid: number;
  pid: number;
  vid: number;
  /** How many replay protection entries it has. */
  crpl: number;
  /** Bit 0 Relay, 1 Proxy, 2 Friend, 3 Low Power. */
  features: number;
  elements: Element[];
};

const u16 = (value: number): Bytes => [value & 0xff, (value >> 8) & 0xff];
const readU16 = (bytes: Bytes, at: number): number => bytes[at] | (bytes[at + 1] << 8);
const readS16 = (bytes: Bytes, at: number): number => (readU16(bytes, at) << 16) >> 16;
const hex2 = (value: number): string => value.toString(16).toUpperCase().padStart(2, "0");
const hex4 = (value: number): string => `0x${value.toString(16).toUpperCase().padStart(4, "0")}`;
const hexBytes = (bytes: Bytes): string => bytes.map(hex2).join(" ");

/** An address as people write it: "0x0002". */
export function addressText(address: number): string {
  return hex4(address);
}

/** Reads an address or a number typed as "0x1201", "1201h" or plain hex; null when it is not one. */
export function parseAddress(text: string): number | null {
  const digits = text.trim().replace(/^0x/i, "").replace(/h$/i, "");
  if (!/^[0-9a-f]{1,4}$/i.test(digits)) return null;
  return parseInt(digits, 16);
}

/** Hex digits as bytes, with or without spaces; null when they are not. */
export function parseHex(text: string): Bytes | null {
  const digits = text.replace(/\s+/g, "").replace(/^0x/i, "");
  if (digits.length % 2 !== 0 || !/^[0-9a-f]*$/i.test(digits)) return null;
  const bytes: Bytes = [];
  for (let at = 0; at < digits.length; at += 2) bytes.push(parseInt(digits.slice(at, at + 2), 16));
  return bytes;
}

/** What kind of address it is: unicast, a group, virtual, or all of some kind. */
export function addressKind(address: number): string {
  if (address === 0) return "unassigned";
  if (address <= 0x7fff) return "unicast";
  if (address < 0xc000) return "virtual";
  const fixed: Record<number, string> = {
    0xfffb: "all IPT nodes",
    0xfffc: "all proxies",
    0xfffd: "all friends",
    0xfffe: "all relays",
    0xffff: "all nodes",
  };
  return fixed[address] ?? "group";
}

/** How many bytes an opcode starting with this byte has; 0 for 0x7F, which none starts with. */
export function opcodeLength(first: number): number {
  if (first === 0x7f) return 0;
  if ((first & 0x80) === 0) return 1;
  if ((first & 0xc0) === 0x80) return 2;
  return 3;
}

/**
 * A message's opcode and parameters. The opcode is its bytes as sent, read
 * as one big-endian number: 0x8201, or 0xC15900 for a vendor's opcode 1 of
 * company 0x0059.
 */
export function splitOpcode(payload: Bytes): { opcode: number; params: Bytes } | null {
  const length = payload.length > 0 ? opcodeLength(payload[0]) : 0;
  if (length === 0 || payload.length < length) return null;
  let opcode = 0;
  for (const byte of payload.slice(0, length)) opcode = opcode * 256 + byte;
  return { opcode, params: payload.slice(length) };
}

/** An opcode's bytes, as sent. */
export function opcodeBytes(opcode: number): Bytes {
  if (opcode < 0x7f) return [opcode];
  if (opcode <= 0xffff) return [opcode >> 8, opcode & 0xff];
  return [(opcode >> 16) & 0xff, (opcode >> 8) & 0xff, opcode & 0xff];
}

/** "0x8201", or "0xC15900". */
export function opcodeText(opcode: number): string {
  const bytes = opcodeBytes(opcode);
  return `0x${bytes.map(hex2).join("")}`;
}

/** Two 12-bit key indexes in 3 bytes, the first in the low bits (4.3.1.1). */
export function packIndexes(first: number, second: number): Bytes {
  const value = (first & 0xfff) | ((second & 0xfff) << 12);
  return [value & 0xff, (value >> 8) & 0xff, (value >> 16) & 0xff];
}

export function unpackIndexes(bytes: Bytes, at: number): [number, number] {
  const value = bytes[at] | (bytes[at + 1] << 8) | (bytes[at + 2] << 16);
  return [value & 0xfff, (value >> 12) & 0xfff];
}

/** A list of key indexes: two in each 3 bytes, an odd last one in 2. */
export function unpackIndexList(bytes: Bytes): number[] {
  const list: number[] = [];
  let at = 0;
  for (; at + 3 <= bytes.length; at += 3) list.push(...unpackIndexes(bytes, at));
  if (bytes.length - at === 2) list.push(readU16(bytes, at) & 0xfff);
  return list;
}

/** A model's bytes in a configuration message: 2 for a SIG model, 4 for a vendor's. */
export function modelBytes(model: Model): Bytes {
  return model.vendor === null ? u16(model.id) : [...u16(model.vendor), ...u16(model.id)];
}

/** Reads a model at the end of a message: 2 bytes for a SIG model, 4 for a vendor's. */
function readModel(bytes: Bytes, at: number): Model {
  return bytes.length - at >= 4
    ? { id: readU16(bytes, at + 2), vendor: readU16(bytes, at) }
    : { id: readU16(bytes, at), vendor: null };
}

const SIG_MODELS: Record<number, string> = {
  0x0000: "Configuration Server",
  0x0001: "Configuration Client",
  0x0002: "Health Server",
  0x0003: "Health Client",
  0x1000: "Generic OnOff Server",
  0x1001: "Generic OnOff Client",
  0x1002: "Generic Level Server",
  0x1003: "Generic Level Client",
  0x1004: "Generic Default Transition Time Server",
  0x1005: "Generic Default Transition Time Client",
  0x1006: "Generic Power OnOff Server",
  0x1007: "Generic Power OnOff Setup Server",
  0x1008: "Generic Power OnOff Client",
  0x1009: "Generic Power Level Server",
  0x100a: "Generic Power Level Setup Server",
  0x100b: "Generic Power Level Client",
  0x100c: "Generic Battery Server",
  0x100d: "Generic Battery Client",
  0x100e: "Generic Location Server",
  0x100f: "Generic Location Setup Server",
  0x1010: "Generic Location Client",
  0x1011: "Generic Admin Property Server",
  0x1012: "Generic Manufacturer Property Server",
  0x1013: "Generic User Property Server",
  0x1014: "Generic Client Property Server",
  0x1015: "Generic Property Client",
  0x1100: "Sensor Server",
  0x1101: "Sensor Setup Server",
  0x1102: "Sensor Client",
  0x1200: "Time Server",
  0x1201: "Time Setup Server",
  0x1202: "Time Client",
  0x1203: "Scene Server",
  0x1204: "Scene Setup Server",
  0x1205: "Scene Client",
  0x1206: "Scheduler Server",
  0x1207: "Scheduler Setup Server",
  0x1208: "Scheduler Client",
  0x1300: "Light Lightness Server",
  0x1301: "Light Lightness Setup Server",
  0x1302: "Light Lightness Client",
  0x1303: "Light CTL Server",
  0x1304: "Light CTL Setup Server",
  0x1305: "Light CTL Client",
  0x1306: "Light CTL Temperature Server",
  0x1307: "Light HSL Server",
  0x1308: "Light HSL Setup Server",
  0x1309: "Light HSL Client",
  0x130a: "Light HSL Hue Server",
  0x130b: "Light HSL Saturation Server",
  0x130c: "Light xyL Server",
  0x130d: "Light xyL Setup Server",
  0x130e: "Light xyL Client",
  0x130f: "Light LC Server",
  0x1310: "Light LC Setup Server",
  0x1311: "Light LC Client",
};

/** "Generic OnOff Server", "SIG model 0x1234", or "Vendor model 0x0001 of 0x0059". */
export function modelName(model: Model): string {
  if (model.vendor !== null) return `Vendor model ${hex4(model.id)} of ${hex4(model.vendor)}`;
  return SIG_MODELS[model.id] ?? `SIG model ${hex4(model.id)}`;
}

/** The models a client in bleNIMA talks to. */
export const SERVER = {
  config: 0x0000,
  health: 0x0002,
  onOff: 0x1000,
  level: 0x1002,
  sensor: 0x1100,
  lightness: 0x1300,
  ctl: 0x1303,
  hsl: 0x1307,
} as const;

/** Reads Composition Data page 0, after its page number; null when it is too short. */
export function parseComposition(data: Bytes): Composition | null {
  if (data.length < 10) return null;
  const composition: Composition = {
    cid: readU16(data, 0),
    pid: readU16(data, 2),
    vid: readU16(data, 4),
    crpl: readU16(data, 6),
    features: readU16(data, 8),
    elements: [],
  };
  let at = 10;
  while (at + 4 <= data.length) {
    const location = readU16(data, at);
    const sig = data[at + 2];
    const vendor = data[at + 3];
    at += 4;
    if (at + sig * 2 + vendor * 4 > data.length) return null;
    const models: Model[] = [];
    for (let n = 0; n < sig; n++, at += 2) models.push({ id: readU16(data, at), vendor: null });
    for (let n = 0; n < vendor; n++, at += 4) models.push({ id: readU16(data, at + 2), vendor: readU16(data, at) });
    composition.elements.push({ location, models });
  }
  return composition;
}

/** The features a node has, in words. */
export function featuresText(features: number): string {
  const names = ["Relay", "Proxy", "Friend", "Low Power"].filter((_, bit) => (features & (1 << bit)) !== 0);
  return names.length > 0 ? names.join(", ") : "none";
}

/** A transition or remaining time in words: its 6 bits of steps and 2 of resolution. */
export function transitionText(byte: number): string {
  const steps = byte & 0x3f;
  if (steps === 0x3f) return "unknown";
  const tenths = steps * [1, 10, 100, 6000][byte >> 6];
  if (tenths === 0) return "0 s";
  if (tenths < 600) return `${tenths / 10} s`;
  if (tenths < 36000) return `${+(tenths / 600).toFixed(1)} min`;
  return `${+(tenths / 36000).toFixed(1)} h`;
}

/** A publish period in words: its 6 bits of steps and 2 of resolution. */
export function periodText(byte: number): string {
  return (byte & 0x3f) === 0 ? "off" : `every ${transitionText(byte)}`;
}

const STATUS: string[] = [
  "Success",
  "Invalid Address",
  "Invalid Model",
  "Invalid AppKey Index",
  "Invalid NetKey Index",
  "Insufficient Resources",
  "Key Index Already Stored",
  "Invalid Publish Parameters",
  "Not a Subscribe Model",
  "Storage Failure",
  "Feature Not Supported",
  "Cannot Update",
  "Cannot Remove",
  "Cannot Bind",
  "Temporarily Unable to Change State",
  "Cannot Set",
  "Unspecified Error",
  "Invalid Binding",
];

/** A configuration status code in words (4.3.5). */
export function statusText(code: number): string {
  return STATUS[code] ?? `status 0x${hex2(code)}`;
}

/** The fault conditions, in order: each has a warning and an error, from 0x01. */
const FAULTS = [
  "Battery Low",
  "Supply Voltage Too Low",
  "Supply Voltage Too High",
  "Power Supply Interrupted",
  "No Load",
  "Overload",
  "Overheat",
  "Condensation",
  "Vibration",
  "Configuration",
  "Element Not Calibrated",
  "Memory",
  "Self-Test",
  "Input Too Low",
  "Input Too High",
  "Input No Change",
  "Actuator Blocked",
  "Housing Opened",
  "Tamper",
  "Device Moved",
  "Device Dropped",
  "Overflow",
  "Empty",
  "Internal Bus",
  "Mechanism Jammed",
];

/** A Health fault code in words. */
export function faultText(code: number): string {
  if (code === 0) return "No Fault";
  const condition = FAULTS[(code - 1) >> 1];
  if (condition && code <= FAULTS.length * 2) return `${condition} ${code % 2 === 1 ? "Warning" : "Error"}`;
  return code >= 0x80 ? `vendor fault 0x${hex2(code)}` : `fault 0x${hex2(code)}`;
}

const onOffText = (value: number): string => (value === 0 ? "Off" : value === 1 ? "On" : `0x${hex2(value)}`);

/** "present", or "present → target in time" when the message has a target. */
function moving(present: string, params: Bytes, at: number, target: (at: number) => string, size: number): string {
  if (params.length < at + size + 1) return present;
  return `${present} → ${target(at)} in ${transitionText(params[at + size])}`;
}

function lightnessText(value: number): string {
  return `${value} (${((value / 65535) * 100).toFixed(1)} %)`;
}

const hueText = (value: number): string => `${((value / 65536) * 360).toFixed(1)}°`;
const saturationText = (value: number): string => `${((value / 65535) * 100).toFixed(1)} %`;

function ctlSetText(p: Bytes): string {
  return `lightness ${lightnessText(readU16(p, 0))}, ${readU16(p, 2)} K, delta UV ${readS16(p, 4)}, TID ${p[6]}`;
}

function hslSetText(p: Bytes): string {
  return `lightness ${lightnessText(readU16(p, 0))}, hue ${hueText(readU16(p, 2))}, saturation ${saturationText(readU16(p, 4))}, TID ${p[6]}`;
}

/**
 * A status's element and model. The model is a SIG one in the SIG lists, and
 * at the end of other statuses, where its length tells which kind it is.
 */
function elementModel(params: Bytes, elementAt: number, modelAt: number, sig = false): string {
  const model = sig ? { id: readU16(params, modelAt), vendor: null } : readModel(params, modelAt);
  return `element ${hex4(readU16(params, elementAt))}, ${modelName(model)}`;
}

/** A publication's fields, from its element address at `at`: as a Set has them, and a Status after its status. */
function publicationFields(params: Bytes, at: number): string {
  const keyAndFlag = readU16(params, at + 4);
  const retransmit = params[at + 8];
  const address = readU16(params, at + 2);
  const parts = [
    elementModel(params, at, at + 9),
    address === 0
      ? "publishes nowhere"
      : `publishes to ${hex4(address)} with AppKey ${keyAndFlag & 0xfff}${keyAndFlag & 0x1000 ? " and friendship credentials" : ""}`,
  ];
  if (address !== 0) {
    parts.push(
      `TTL ${params[at + 6] === 0xff ? "default" : params[at + 6]}`,
      periodText(params[at + 7]),
      `${retransmit & 0x07} retransmissions ${((retransmit >> 3) + 1) * 50} ms apart`,
    );
  }
  return parts.join(", ");
}

/** A Sensor Status's marshalled data: each property's ID and its raw value. */
export function parseSensorData(params: Bytes): { property: number; value: Bytes }[] {
  const list: { property: number; value: Bytes }[] = [];
  let at = 0;
  while (at < params.length) {
    let property: number;
    let length: number;
    if ((params[at] & 1) === 0) {
      // Format A: a 4-bit length, less one, and an 11-bit property ID.
      if (at + 2 > params.length) break;
      const header = readU16(params, at);
      length = ((header >> 1) & 0x0f) + 1;
      property = header >> 5;
      at += 2;
    } else {
      // Format B: a 7-bit length, less one, 0x7F for none, and a 16-bit property ID.
      if (at + 3 > params.length) break;
      const raw = params[at] >> 1;
      length = raw === 0x7f ? 0 : raw + 1;
      property = readU16(params, at + 1);
      at += 3;
    }
    list.push({ property, value: params.slice(at, at + length) });
    at += length;
  }
  return list;
}

function sensorText(params: Bytes): string {
  const list = parseSensorData(params);
  if (list.length === 0) return "no values";
  return list
    .map(({ property, value }) => `property ${hex4(property)}: ${value.length > 0 ? hexBytes(value) : "none"}`)
    .join("; ");
}

function descriptorText(params: Bytes): string {
  if (params.length === 2) return `no property ${hex4(readU16(params, 0))}`;
  const parts: string[] = [];
  for (let at = 0; at + 8 <= params.length; at += 8) {
    const tolerances = params[at + 2] | (params[at + 3] << 8) | (params[at + 4] << 16);
    parts.push(
      `property ${hex4(readU16(params, at))}: tolerance +${tolerances & 0xfff} / -${tolerances >> 12}, sampling ${params[at + 5]}, period ${params[at + 6]}, update ${params[at + 7]}`,
    );
  }
  return parts.join("; ");
}

function faultsText(params: Bytes): string {
  // A fault of 0x00 is no fault at all.
  const faults = params.slice(3).filter((fault) => fault !== 0);
  return `test ${params[0]}, company ${hex4(readU16(params, 1))}, ${
    faults.length > 0 ? faults.map(faultText).join(", ") : "no faults"
  }`;
}

type Known = { name: string; read?: (params: Bytes) => string; length?: number };

/** The messages bleNIMA knows by name, and how the ones that come back read. */
const MESSAGES: Record<number, Known> = {
  0x8008: { name: "Config Composition Data Get", read: (p) => `page ${p[0]}` },
  0x02: {
    name: "Config Composition Data Status",
    read: (p) => {
      const composition = parseComposition(p.slice(1));
      if (!composition) return `page ${p[0]}`;
      const models = composition.elements.reduce((sum, element) => sum + element.models.length, 0);
      return `page ${p[0]}: company ${hex4(composition.cid)}, product ${hex4(composition.pid)}, version ${hex4(composition.vid)}, ${composition.elements.length} elements, ${models} models, features ${featuresText(composition.features)}`;
    },
  },
  0x00: { name: "Config AppKey Add", read: (p) => `NetKey ${unpackIndexes(p, 0)[0]}, AppKey ${unpackIndexes(p, 0)[1]}` },
  0x8000: { name: "Config AppKey Delete", read: (p) => `NetKey ${unpackIndexes(p, 0)[0]}, AppKey ${unpackIndexes(p, 0)[1]}` },
  0x8001: { name: "Config AppKey Get", read: (p) => `NetKey ${readU16(p, 0) & 0xfff}` },
  0x8002: {
    name: "Config AppKey List",
    read: (p) => `${statusText(p[0])}: NetKey ${readU16(p, 1) & 0xfff} has AppKeys ${unpackIndexList(p.slice(3)).join(", ") || "none"}`,
  },
  0x8003: {
    name: "Config AppKey Status",
    read: (p) => {
      const [net, app] = unpackIndexes(p, 1);
      return `${statusText(p[0])}: NetKey ${net}, AppKey ${app}`;
    },
  },
  0x803d: { name: "Config Model App Bind", read: (p) => `${elementModel(p, 0, 4)}, AppKey ${readU16(p, 2) & 0xfff}` },
  0x803f: { name: "Config Model App Unbind", read: (p) => `${elementModel(p, 0, 4)}, AppKey ${readU16(p, 2) & 0xfff}` },
  0x803e: {
    name: "Config Model App Status",
    read: (p) => `${statusText(p[0])}: ${elementModel(p, 1, 5)}, AppKey ${readU16(p, 3) & 0xfff}`,
  },
  0x804b: { name: "Config SIG Model App Get", read: (p) => elementModel(p, 0, 2, true) },
  0x804d: { name: "Config Vendor Model App Get", read: (p) => elementModel(p, 0, 2) },
  0x804c: {
    name: "Config SIG Model App List",
    read: (p) => `${statusText(p[0])}: ${elementModel(p, 1, 3, true)}, AppKeys ${unpackIndexList(p.slice(5)).join(", ") || "none"}`,
  },
  0x804e: {
    name: "Config Vendor Model App List",
    read: (p) =>
      `${statusText(p[0])}: element ${hex4(readU16(p, 1))}, ${modelName({ id: readU16(p, 5), vendor: readU16(p, 3) })}, AppKeys ${unpackIndexList(p.slice(7)).join(", ") || "none"}`,
  },
  0x8018: { name: "Config Model Publication Get", read: (p) => elementModel(p, 0, 2) },
  0x03: { name: "Config Model Publication Set", read: (p) => publicationFields(p, 0) },
  0x8019: { name: "Config Model Publication Status", read: (p) => `${statusText(p[0])}: ${publicationFields(p, 1)}` },
  0x801b: { name: "Config Model Subscription Add", read: (p) => `${elementModel(p, 0, 4)}, address ${hex4(readU16(p, 2))}` },
  0x801c: { name: "Config Model Subscription Delete", read: (p) => `${elementModel(p, 0, 4)}, address ${hex4(readU16(p, 2))}` },
  0x801d: { name: "Config Model Subscription Delete All", read: (p) => elementModel(p, 0, 2) },
  0x801e: { name: "Config Model Subscription Overwrite", read: (p) => `${elementModel(p, 0, 4)}, address ${hex4(readU16(p, 2))}` },
  0x801f: {
    name: "Config Model Subscription Status",
    read: (p) => `${statusText(p[0])}: ${elementModel(p, 1, 5)}, address ${hex4(readU16(p, 3))}`,
  },
  0x8029: { name: "Config SIG Model Subscription Get", read: (p) => elementModel(p, 0, 2, true) },
  0x802b: { name: "Config Vendor Model Subscription Get", read: (p) => elementModel(p, 0, 2) },
  0x802a: {
    name: "Config SIG Model Subscription List",
    read: (p) => {
      const addresses: string[] = [];
      for (let at = 5; at + 2 <= p.length; at += 2) addresses.push(hex4(readU16(p, at)));
      return `${statusText(p[0])}: ${elementModel(p, 1, 3, true)}, subscribed to ${addresses.join(", ") || "nothing"}`;
    },
  },
  0x802c: {
    name: "Config Vendor Model Subscription List",
    read: (p) => {
      const addresses: string[] = [];
      for (let at = 7; at + 2 <= p.length; at += 2) addresses.push(hex4(readU16(p, at)));
      return `${statusText(p[0])}: element ${hex4(readU16(p, 1))}, ${modelName({ id: readU16(p, 5), vendor: readU16(p, 3) })}, subscribed to ${addresses.join(", ") || "nothing"}`;
    },
  },
  0x8049: { name: "Config Node Reset" },
  0x804a: { name: "Config Node Reset Status", read: () => "the node left the network" },

  0x8201: { name: "Generic OnOff Get" },
  0x8202: { name: "Generic OnOff Set", read: (p) => `${onOffText(p[0])}, TID ${p[1]}` },
  0x8203: { name: "Generic OnOff Set Unacknowledged", read: (p) => `${onOffText(p[0])}, TID ${p[1]}` },
  0x8204: { name: "Generic OnOff Status", read: (p) => moving(onOffText(p[0]), p, 1, (at) => onOffText(p[at]), 1) },
  0x8205: { name: "Generic Level Get" },
  0x8206: { name: "Generic Level Set", read: (p) => `${readS16(p, 0)}, TID ${p[2]}` },
  0x8207: { name: "Generic Level Set Unacknowledged", read: (p) => `${readS16(p, 0)}, TID ${p[2]}` },
  0x8208: {
    name: "Generic Level Status",
    read: (p) => moving(String(readS16(p, 0)), p, 2, (at) => String(readS16(p, at)), 2),
  },
  0x8209: { name: "Generic Delta Set", read: (p) => `delta ${readU16(p, 0) | (readU16(p, 2) << 16)}, TID ${p[4]}` },
  0x820a: { name: "Generic Delta Set Unacknowledged", read: (p) => `delta ${readU16(p, 0) | (readU16(p, 2) << 16)}, TID ${p[4]}` },
  0x824b: { name: "Light Lightness Get" },
  0x824c: { name: "Light Lightness Set", read: (p) => `${lightnessText(readU16(p, 0))}, TID ${p[2]}` },
  0x824d: { name: "Light Lightness Set Unacknowledged", read: (p) => `${lightnessText(readU16(p, 0))}, TID ${p[2]}` },
  0x824e: {
    name: "Light Lightness Status",
    read: (p) => moving(lightnessText(readU16(p, 0)), p, 2, (at) => lightnessText(readU16(p, at)), 2),
  },
  0x825d: { name: "Light CTL Get" },
  0x825e: { name: "Light CTL Set", read: ctlSetText },
  0x825f: { name: "Light CTL Set Unacknowledged", read: ctlSetText },
  0x8260: {
    name: "Light CTL Status",
    read: (p) =>
      moving(
        `lightness ${lightnessText(readU16(p, 0))}, ${readU16(p, 2)} K`,
        p,
        4,
        (at) => `lightness ${lightnessText(readU16(p, at))}, ${readU16(p, at + 2)} K`,
        4,
      ),
  },
  0x826d: { name: "Light HSL Get" },
  0x8276: { name: "Light HSL Set", read: hslSetText },
  0x8277: { name: "Light HSL Set Unacknowledged", read: hslSetText },
  0x8278: {
    name: "Light HSL Status",
    read: (p) =>
      `lightness ${lightnessText(readU16(p, 0))}, hue ${hueText(readU16(p, 2))}, saturation ${saturationText(readU16(p, 4))}${p.length > 6 ? `, remaining ${transitionText(p[6])}` : ""}`,
  },
  0x8230: { name: "Sensor Descriptor Get", read: (p) => `property ${hex4(readU16(p, 0))}` },
  0x51: { name: "Sensor Descriptor Status", read: descriptorText },
  0x8231: { name: "Sensor Get", read: (p) => `property ${hex4(readU16(p, 0))}` },
  0x52: { name: "Sensor Status", read: sensorText },

  0x8031: { name: "Health Fault Get", read: (p) => `company ${hex4(readU16(p, 0))}` },
  0x802f: { name: "Health Fault Clear", read: (p) => `company ${hex4(readU16(p, 0))}` },
  0x8030: { name: "Health Fault Clear Unacknowledged", read: (p) => `company ${hex4(readU16(p, 0))}` },
  0x8032: { name: "Health Fault Test", read: (p) => `test ${p[0]}, company ${hex4(readU16(p, 1))}` },
  0x8033: { name: "Health Fault Test Unacknowledged", read: (p) => `test ${p[0]}, company ${hex4(readU16(p, 1))}` },
  0x05: { name: "Health Fault Status", read: faultsText },
  0x04: { name: "Health Current Status", read: faultsText },
  0x8034: { name: "Health Period Get" },
  0x8035: { name: "Health Period Set", read: (p) => `fast period divisor ${p[0]}` },
  0x8036: { name: "Health Period Set Unacknowledged", read: (p) => `fast period divisor ${p[0]}` },
  0x8037: { name: "Health Period Status", read: (p) => `fast period divisor ${p[0]}` },
  0x8004: { name: "Health Attention Get" },
  0x8005: { name: "Health Attention Set", read: (p) => `${p[0]} s` },
  0x8006: { name: "Health Attention Set Unacknowledged", read: (p) => `${p[0]} s` },
  0x8007: { name: "Health Attention Status", read: (p) => `${p[0]} s` },
};

/** A message in words, for the log: its name, and what its parameters say. */
export function describeMessage(payload: Bytes): { name: string; text: string } {
  const split = splitOpcode(payload);
  if (!split) return { name: "Not a message", text: hexBytes(payload) };
  const known = MESSAGES[split.opcode];
  if (!known) {
    return { name: `Opcode ${opcodeText(split.opcode)}`, text: split.params.length > 0 ? hexBytes(split.params) : "" };
  }
  let text = "";
  try {
    text = known.read && split.params.length > 0 ? known.read(split.params) : hexBytes(split.params);
  } catch {
    text = hexBytes(split.params);
  }
  return { name: known.name, text };
}

/** The opcode of the status a message is answered with; null when none is. */
export function answerOf(opcode: number): number | null {
  const answers: Record<number, number> = {
    0x8008: 0x02,
    0x00: 0x8003,
    0x8000: 0x8003,
    0x8001: 0x8002,
    0x803d: 0x803e,
    0x803f: 0x803e,
    0x804b: 0x804c,
    0x804d: 0x804e,
    0x8018: 0x8019,
    0x03: 0x8019,
    0x801b: 0x801f,
    0x801c: 0x801f,
    0x801d: 0x801f,
    0x801e: 0x801f,
    0x8029: 0x802a,
    0x802b: 0x802c,
    0x8049: 0x804a,
    0x8201: 0x8204,
    0x8202: 0x8204,
    0x8205: 0x8208,
    0x8206: 0x8208,
    0x8209: 0x8208,
    0x824b: 0x824e,
    0x824c: 0x824e,
    0x825d: 0x8260,
    0x825e: 0x8260,
    0x826d: 0x8278,
    0x8276: 0x8278,
    0x8230: 0x51,
    0x8231: 0x52,
    0x8031: 0x05,
    0x802f: 0x05,
    0x8032: 0x05,
    0x8034: 0x8037,
    0x8035: 0x8037,
    0x8004: 0x8007,
    0x8005: 0x8007,
  };
  return answers[opcode] ?? null;
}

/** Where a model publishes, and how. */
export type Publication = {
  address: number;
  appKey: number;
  /** Publishes with the friendship credentials. */
  credentials: boolean;
  /** 0xFF for the node's default TTL. */
  ttl: number;
  /** 6 bits of steps and 2 of resolution: 0 for none. */
  period: number;
  /** How many times it is sent again, 0 to 7. */
  retransmitCount: number;
  /** How far apart, in steps of 50 ms less one, 0 to 31. */
  retransmitSteps: number;
};

/** The configuration messages, sent with the node's device key. */
export const config = {
  compositionGet: (page = 0): Bytes => [0x80, 0x08, page],
  appKeyAdd: (netKey: number, appKey: number, key: Bytes): Bytes => [0x00, ...packIndexes(netKey, appKey), ...key],
  appKeyDelete: (netKey: number, appKey: number): Bytes => [0x80, 0x00, ...packIndexes(netKey, appKey)],
  appKeyGet: (netKey: number): Bytes => [0x80, 0x01, ...u16(netKey)],
  bind: (element: number, appKey: number, model: Model): Bytes => [0x80, 0x3d, ...u16(element), ...u16(appKey), ...modelBytes(model)],
  unbind: (element: number, appKey: number, model: Model): Bytes => [0x80, 0x3f, ...u16(element), ...u16(appKey), ...modelBytes(model)],
  bindingsGet: (element: number, model: Model): Bytes => [0x80, model.vendor === null ? 0x4b : 0x4d, ...u16(element), ...modelBytes(model)],
  publicationGet: (element: number, model: Model): Bytes => [0x80, 0x18, ...u16(element), ...modelBytes(model)],
  publicationSet: (element: number, model: Model, p: Publication): Bytes => [
    0x03,
    ...u16(element),
    ...u16(p.address),
    ...u16((p.appKey & 0xfff) | (p.credentials ? 0x1000 : 0)),
    p.ttl & 0xff,
    p.period & 0xff,
    (p.retransmitCount & 0x07) | ((p.retransmitSteps & 0x1f) << 3),
    ...modelBytes(model),
  ],
  subscribe: (element: number, address: number, model: Model): Bytes => [0x80, 0x1b, ...u16(element), ...u16(address), ...modelBytes(model)],
  unsubscribe: (element: number, address: number, model: Model): Bytes => [0x80, 0x1c, ...u16(element), ...u16(address), ...modelBytes(model)],
  subscriptionsClear: (element: number, model: Model): Bytes => [0x80, 0x1d, ...u16(element), ...modelBytes(model)],
  subscriptionsGet: (element: number, model: Model): Bytes => [0x80, model.vendor === null ? 0x29 : 0x2b, ...u16(element), ...modelBytes(model)],
  nodeReset: (): Bytes => [0x80, 0x49],
};

/** The model clients' messages, sent with an application key. `ack` picks the acknowledged one. */
export const client = {
  onOffGet: (): Bytes => [0x82, 0x01],
  onOffSet: (on: boolean, tid: number, ack = true): Bytes => [0x82, ack ? 0x02 : 0x03, on ? 1 : 0, tid & 0xff],
  levelGet: (): Bytes => [0x82, 0x05],
  levelSet: (level: number, tid: number, ack = true): Bytes => [0x82, ack ? 0x06 : 0x07, ...u16(level), tid & 0xff],
  levelDelta: (delta: number, tid: number, ack = true): Bytes => [
    0x82,
    ack ? 0x09 : 0x0a,
    delta & 0xff,
    (delta >> 8) & 0xff,
    (delta >> 16) & 0xff,
    (delta >> 24) & 0xff,
    tid & 0xff,
  ],
  lightnessGet: (): Bytes => [0x82, 0x4b],
  lightnessSet: (lightness: number, tid: number, ack = true): Bytes => [0x82, ack ? 0x4c : 0x4d, ...u16(lightness), tid & 0xff],
  ctlGet: (): Bytes => [0x82, 0x5d],
  ctlSet: (lightness: number, temperature: number, deltaUv: number, tid: number, ack = true): Bytes => [
    0x82,
    ack ? 0x5e : 0x5f,
    ...u16(lightness),
    ...u16(temperature),
    ...u16(deltaUv),
    tid & 0xff,
  ],
  hslGet: (): Bytes => [0x82, 0x6d],
  hslSet: (lightness: number, hue: number, saturation: number, tid: number, ack = true): Bytes => [
    0x82,
    ack ? 0x76 : 0x77,
    ...u16(lightness),
    ...u16(hue),
    ...u16(saturation),
    tid & 0xff,
  ],
  sensorDescriptorGet: (property?: number): Bytes => [0x82, 0x30, ...(property === undefined ? [] : u16(property))],
  sensorGet: (property?: number): Bytes => [0x82, 0x31, ...(property === undefined ? [] : u16(property))],
  faultGet: (company: number): Bytes => [0x80, 0x31, ...u16(company)],
  faultClear: (company: number, ack = true): Bytes => [0x80, ack ? 0x2f : 0x30, ...u16(company)],
  faultTest: (test: number, company: number, ack = true): Bytes => [0x80, ack ? 0x32 : 0x33, test & 0xff, ...u16(company)],
  periodGet: (): Bytes => [0x80, 0x34],
  periodSet: (divisor: number, ack = true): Bytes => [0x80, ack ? 0x35 : 0x36, divisor & 0x0f],
  attentionGet: (): Bytes => [0x80, 0x04],
  attentionSet: (seconds: number, ack = true): Bytes => [0x80, ack ? 0x05 : 0x06, seconds & 0xff],
};

/** Where an unprovisioned device says its OOB information is, in words (3.10.2). */
export function oobInfoText(oob: number): string {
  const names = [
    "other",
    "a URI",
    "a 2D code",
    "a bar code",
    "NFC",
    "a number",
    "a string",
    "a certificate",
    "provisioning records",
    "",
    "",
    "on the box",
    "inside the box",
    "on a piece of paper",
    "in the manual",
    "on the device",
  ];
  const found = names.filter((name, bit) => name && (oob & (1 << bit)) !== 0);
  return found.length > 0 ? found.join(", ") : "none";
}
