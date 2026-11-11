// The GATT server of a device simulator tab, as data: its presets, the
// made-up live values its timers send, and its services as the Rust side
// takes them. It has no imports, so its tests run in Node as they are.

export type Property = "read" | "write" | "write_without_response" | "notify" | "indicate";

/** The properties a served characteristic can have, in the order they are shown. */
export const PROPERTIES: { property: Property; bit: number; badge: string; label: string }[] = [
  { property: "read", bit: 0x02, badge: "R", label: "Read" },
  { property: "write", bit: 0x08, badge: "W", label: "Write" },
  { property: "write_without_response", bit: 0x04, badge: "WnR", label: "Write without response" },
  { property: "notify", bit: 0x10, badge: "N", label: "Notify" },
  { property: "indicate", bit: 0x20, badge: "I", label: "Indicate" },
];

/** A characteristic as the tab keeps it: the same fields as `ServedCharacteristic` in workspace.rs. */
export type CharacteristicSetup = {
  /** As typed or picked: "2A19", or the full 128 bits. */
  uuid: string;
  /** Its user description, the 0x2901 descriptor; none when empty. */
  description: string;
  properties: Property[];
  /** The value it starts with, in hex: "48 69". */
  value: string;
  /** How the value is typed and shown: "hex", "utf8", "u8", "u16:be"...; "" for its meaning, typed in hex. */
  format: string;
  /** How often it is notified, or changed by itself, in ms; 0 for never. */
  timer_ms: number;
  /** The timer sends made-up live values, where bleNIMA knows the characteristic. */
  live: boolean;
};

/** A service as the tab keeps it: the same fields as `ServedService` in workspace.rs. */
export type ServiceSetup = {
  uuid: string;
  /** The preset it came from, such as "nus"; "" for one made by hand. */
  preset: string;
  /** A UART service sends what is written to it back, as a notification. */
  echo: boolean;
  characteristics: CharacteristicSetup[];
};

/** A served characteristic as Rust's `server::Characteristic` takes it. */
export type ServedCharacteristic = {
  uuid: string;
  properties: number;
  value: number[];
  description: string;
  echo: number | null;
};

/** A served service as Rust's `server::Service` takes it. */
export type Service = { uuid: string; characteristics: ServedCharacteristic[] };

/** Where a characteristic is: the same fields as `Place` in server.rs. */
export type Place = { service: number; characteristic: number };

/** What happens on the server: the same as `ServerEvent` in server.rs. */
export type ServerEvent =
  | { type: "advertising"; service: number; on: boolean; text: string }
  | { type: "read"; place: Place; client: string; offset: number; data: number[] }
  | { type: "written"; place: Place; client: string; with_response: boolean; offset: number; data: number[]; value: number[] }
  | { type: "subscribers"; place: Place; clients: string[] }
  | { type: "notified"; place: Place; data: number[]; sent: string[]; failed: string[]; echo: boolean }
  | { type: "failed"; place: Place | null; action: string; problem: string };

/** The presets whose services are a UART, so they can echo. */
export const UART_PRESETS = ["nus", "hm10", "microchip"];

const BASE = "-0000-1000-8000-00805f9b34fb";

/** A UUID in full, small letters: "180F" is "0000180f-0000-1000-8000-00805f9b34fb". Null if it is not one. */
export function fullUuid(text: string): string | null {
  const hex = text.trim().replace(/^0x/i, "").replace(/-/g, "").toLowerCase();
  if (!/^[0-9a-f]+$/.test(hex)) return null;
  if (hex.length === 4) return `0000${hex}${BASE}`;
  if (hex.length === 8) return `${hex}${BASE}`;
  if (hex.length !== 32) return null;
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

/** "0x180F" for a UUID on the Bluetooth base, else the UUID in capitals. */
export function uuidText(uuid: string): string {
  const full = fullUuid(uuid);
  if (!full) return uuid;
  if (full.endsWith(BASE)) {
    const short = full.slice(0, 8);
    return short.startsWith("0000") ? `0x${short.slice(4).toUpperCase()}` : `0x${short.toUpperCase()}`;
  }
  return full.toUpperCase();
}

/** The property bits of a list of properties. */
export function bitsOf(properties: Property[]): number {
  return PROPERTIES.filter((entry) => properties.includes(entry.property)).reduce((bits, entry) => bits | entry.bit, 0);
}

/** Whether the server sends this characteristic's value by itself: notify or indicate. */
export function sends(characteristic: CharacteristicSetup): boolean {
  return characteristic.properties.includes("notify") || characteristic.properties.includes("indicate");
}

/** Whether clients can write to it. */
export function takesWrites(characteristic: CharacteristicSetup): boolean {
  return characteristic.properties.includes("write") || characteristic.properties.includes("write_without_response");
}

/** Bytes as hex: "48 69". */
export function hexText(bytes: number[]): string {
  return bytes.map((byte) => byte.toString(16).toUpperCase().padStart(2, "0")).join(" ");
}

/** "48 69" or "4869" as bytes; null if it is not hex. */
export function hexBytes(text: string): number[] | null {
  const digits = text.replace(/[\s,]/g, "");
  if (!/^([0-9a-f]{2})*$/i.test(digits)) return null;
  return (digits.match(/../g) ?? []).map((byte) => Number.parseInt(byte, 16));
}

/** Where a UART service's writes are echoed: its first characteristic that notifies or indicates. */
export function echoTarget(service: ServiceSetup): number | null {
  if (!service.echo) return null;
  const at = service.characteristics.findIndex(sends);
  return at < 0 ? null : at;
}

/** The services as the Rust side takes them, or what is wrong with what was typed, in words. */
export function toServices(setups: ServiceSetup[]): Service[] | string {
  const services: Service[] = [];
  for (const [at, setup] of setups.entries()) {
    const uuid = fullUuid(setup.uuid);
    if (!uuid) return `Service ${at + 1}: "${setup.uuid}" is not a UUID. It is 4 or 8 hex digits, such as 180F, or all 32`;
    const echo = echoTarget(setup);
    const characteristics: ServedCharacteristic[] = [];
    for (const [index, characteristic] of setup.characteristics.entries()) {
      const where = `Characteristic ${index + 1} of service ${at + 1}`;
      const id = fullUuid(characteristic.uuid);
      if (!id) return `${where}: "${characteristic.uuid}" is not a UUID. It is 4 or 8 hex digits, such as 2A19, or all 32`;
      const value = hexBytes(characteristic.value);
      if (!value) return `${where}: its value is not hex bytes`;
      characteristics.push({
        uuid: id,
        properties: bitsOf(characteristic.properties),
        value,
        description: characteristic.description.trim(),
        echo: echo !== null && takesWrites(characteristic) ? echo : null,
      });
    }
    services.push({ uuid, characteristics });
  }
  return services;
}

/** Names of UUIDs that the Bluetooth numbers database does not have. */
export const KNOWN_NAMES: Record<string, string> = {
  "0000ffe0-0000-1000-8000-00805f9b34fb": "HM-10 UART",
  "0000ffe1-0000-1000-8000-00805f9b34fb": "HM-10 data",
  "49535343-fe7d-4ae5-8fa9-9fafd205e455": "Microchip Transparent UART",
  "49535343-1e4d-4bd9-ba61-23c647249616": "Microchip TX",
  "49535343-8841-43f4-a8d4-ecbe34729bb3": "Microchip RX",
};

// ---- Bytes of numbers ----

function u16(value: number): number[] {
  return [value & 0xff, (value >> 8) & 0xff];
}

function u32(value: number): number[] {
  return [value & 0xff, (value >>> 8) & 0xff, (value >>> 16) & 0xff, (value >>> 24) & 0xff];
}

/** IEEE-11073 SFLOAT: a 12-bit mantissa and a 4-bit exponent. */
export function sfloatBytes(mantissa: number, exponent = 0): number[] {
  return u16(((exponent & 0x0f) << 12) | (mantissa & 0x0fff));
}

/** IEEE-11073 FLOAT: a 24-bit mantissa and an 8-bit exponent. */
export function floatBytes(mantissa: number, exponent = 0): number[] {
  return u32((((exponent & 0xff) << 24) | (mantissa & 0xffffff)) >>> 0);
}

function text(value: string): number[] {
  return [...new TextEncoder().encode(value)];
}

/** A slow, smooth swing around a middle value, the same for the same tick. */
function swing(middle: number, size: number, tick: number, period: number): number {
  return Math.round(middle + size * Math.sin(tick / period));
}

// ---- Live values ----

/** One tick of a timer: how many came before it, the time now, and the timer's interval. */
export type Tick = { n: number; now: Date; ms: number };

const UART_TX = [
  "6e400003-b5a3-f393-e0a9-e50e24dcca9e",
  "0000ffe1-0000-1000-8000-00805f9b34fb",
  "49535343-1e4d-4bd9-ba61-23c647249616",
];

/** A wheel 2.1 m round at 25 km/h, and a crank at 90 rpm. */
const WHEEL_PER_S = 25 / 3.6 / 2.1;
const CRANK_PER_S = 1.5;

/** Made-up live values of the characteristics bleNIMA knows, by their full UUID. */
const LIVE: Record<string, (current: number[], tick: Tick) => number[]> = {
  // Battery Level: down 1 % each tick, back to 100 after 0.
  "00002a19-0000-1000-8000-00805f9b34fb": (current) => [current.length === 0 || current[0] === 0 ? 100 : Math.min(current[0], 101) - 1],
  // Heart Rate Measurement: 8-bit bpm, 60 to 100.
  "00002a37-0000-1000-8000-00805f9b34fb": (_, { n }) => [0x00, swing(80, 20, n, 8)],
  // Temperature Measurement: Celsius, 36.5 to 37.5, as a FLOAT in hundredths.
  "00002a1c-0000-1000-8000-00805f9b34fb": (_, { n }) => [0x00, ...floatBytes(swing(3700, 50, n, 10), -2)],
  // Environmental Sensing: temperature in 0.01 °C, humidity in 0.01 %, pressure in 0.1 Pa.
  "00002a6e-0000-1000-8000-00805f9b34fb": (_, { n }) => u16(swing(2200, 300, n, 12)),
  "00002a6f-0000-1000-8000-00805f9b34fb": (_, { n }) => u16(swing(4500, 1000, n, 15)),
  "00002a6d-0000-1000-8000-00805f9b34fb": (_, { n }) => u32(swing(1013250, 500, n, 20)),
  // Current Time: the computer's clock.
  "00002a2b-0000-1000-8000-00805f9b34fb": (_, { now }) => [
    ...u16(now.getFullYear()),
    now.getMonth() + 1,
    now.getDate(),
    now.getHours(),
    now.getMinutes(),
    now.getSeconds(),
    now.getDay() === 0 ? 7 : now.getDay(),
    Math.floor((now.getMilliseconds() * 256) / 1000),
    0,
  ],
  // CSC Measurement: wheel and crank revolutions, and the time of the last one in 1/1024 s.
  "00002a5b-0000-1000-8000-00805f9b34fb": (_, { n, ms }) => {
    const seconds = (n * ms) / 1000;
    const wheel = Math.floor(seconds * WHEEL_PER_S);
    const crank = Math.floor(seconds * CRANK_PER_S);
    return [
      0x03,
      ...u32(wheel),
      ...u16(Math.round((wheel / WHEEL_PER_S) * 1024)),
      ...u16(crank),
      ...u16(Math.round((crank / CRANK_PER_S) * 1024)),
    ];
  },
  // RSC Measurement: running, about 3 m/s in 1/256 m/s, about 170 steps a minute.
  "00002a53-0000-1000-8000-00805f9b34fb": (_, { n }) => [0x04, ...u16(swing(768, 128, n, 10)), swing(170, 5, n, 7)],
  // Blood Pressure Measurement: mmHg, with the pulse rate.
  "00002a35-0000-1000-8000-00805f9b34fb": (_, { n }) => {
    const systolic = swing(120, 5, n, 6);
    const diastolic = swing(80, 4, n, 7);
    const mean = Math.round((systolic + 2 * diastolic) / 3);
    return [0x04, ...sfloatBytes(systolic), ...sfloatBytes(diastolic), ...sfloatBytes(mean), ...sfloatBytes(swing(72, 6, n, 5))];
  },
  // PLX Continuous Measurement: SpO2 in % and the pulse rate.
  "00002a5f-0000-1000-8000-00805f9b34fb": (_, { n }) => [0x00, ...sfloatBytes(swing(97, 2, n, 9)), ...sfloatBytes(swing(70, 6, n, 5))],
  // Nordic Blinky's button: pressed and let go in turn.
  "00001524-1212-efde-1523-785feabcd123": (current) => [current[0] ? 0 : 1],
  // Cycling Power Measurement: 150 to 250 W.
  "00002a63-0000-1000-8000-00805f9b34fb": (_, { n }) => [...u16(0), ...u16(swing(200, 50, n, 6))],
  // Indoor Bike Data: speed in 0.01 km/h, cadence in 0.5 rpm and power in W.
  "00002ad2-0000-1000-8000-00805f9b34fb": (_, { n }) => [
    ...u16(0x0044),
    ...u16(swing(2800, 300, n, 9)),
    ...u16(2 * swing(85, 5, n, 7)),
    ...u16(swing(180, 40, n, 6)),
  ],
  // Weight Measurement: kilograms, in 0.005 kg, about 75 kg.
  "00002a9d-0000-1000-8000-00805f9b34fb": (_, { n }) => [0x00, ...u16(swing(15000, 60, n, 10))],
  // Location and Speed: about 5 m/s in 0.01 m/s, going round a 100 m circle at Greenwich.
  "00002a67-0000-1000-8000-00805f9b34fb": (_, { n }) => {
    const angle = n / 20;
    const latitude = Math.round((51.4769 + 0.0009 * Math.sin(angle)) * 1e7);
    const longitude = Math.round((-0.0005 + 0.0014 * Math.cos(angle)) * 1e7);
    return [...u16(0x0005), ...u16(swing(500, 50, n, 8)), ...u32(latitude), ...u32(longitude)];
  },
  // New Alert: an email, one more each tick up to 9, with its text.
  "00002a46-0000-1000-8000-00805f9b34fb": (_, { n }) => [0x01, (n % 9) + 1, ...text(`Message ${n + 1}`)],
  // Unread Alert Status: emails not read yet.
  "00002a45-0000-1000-8000-00805f9b34fb": (_, { n }) => [0x01, (n % 9) + 1],
  // Body Composition Measurement: body fat in 0.1 %, about 22 %.
  "00002a9c-0000-1000-8000-00805f9b34fb": (_, { n }) => [...u16(0x0000), ...u16(swing(220, 10, n, 10))],
};
for (const uuid of UART_TX) LIVE[uuid] = (_, { n }) => text(`bleNIMA ${n + 1}\r\n`);

/** Whether bleNIMA has made-up live values for a characteristic. */
export function hasLive(uuid: string): boolean {
  const full = fullUuid(uuid);
  return full !== null && full in LIVE;
}

/** The next made-up live value of a characteristic, from its value now; null where bleNIMA has none. */
export function liveValue(uuid: string, current: number[], tick: Tick): number[] | null {
  const full = fullUuid(uuid);
  const make = full ? LIVE[full] : undefined;
  return make ? make(current, tick) : null;
}

/** What a value says, for a few characteristics the device tab does not read out; null for the rest. */
export function describeServed(uuid: string, bytes: number[]): string | null {
  const full = fullUuid(uuid);
  if (bytes.length === 0) return null;
  switch (full) {
    case "00002a06-0000-1000-8000-00805f9b34fb":
      return ["No alert", "Mild alert", "High alert"][bytes[0]] ?? `Alert level ${bytes[0]}`;
    case "00001525-1212-efde-1523-785feabcd123":
      return bytes[0] ? "LED on" : "LED off";
    case "00001524-1212-efde-1523-785feabcd123":
      return bytes[0] ? "Button pressed" : "Button let go";
    case "00002a2b-0000-1000-8000-00805f9b34fb": {
      if (bytes.length < 7) return null;
      const two = (n: number) => String(n).padStart(2, "0");
      return `${bytes[0] | (bytes[1] << 8)}-${two(bytes[2])}-${two(bytes[3])} ${two(bytes[4])}:${two(bytes[5])}:${two(bytes[6])}`;
    }
    default:
      return null;
  }
}

// ---- Presets ----

export type Preset = {
  id: string;
  name: string;
  /** What it is for, under its name in the list. */
  detail: string;
  services: ServiceSetup[];
};

function characteristic(
  uuid: string,
  properties: Property[],
  value: number[] | string,
  format = "",
  timer_ms = 0,
): CharacteristicSetup {
  const bytes = typeof value === "string" ? text(value) : value;
  return { uuid, description: "", properties, value: hexText(bytes), format, timer_ms, live: hasLive(uuid) };
}

/** A characteristic with live values, sent every second. */
function live(uuid: string, properties: Property[], format = ""): CharacteristicSetup {
  const tick = { n: 0, now: new Date(2026, 0, 1), ms: 1000 };
  return characteristic(uuid, properties, liveValue(uuid, [], tick) ?? [], format, 1000);
}

function service(uuid: string, preset: string, characteristics: CharacteristicSetup[], echo = false): ServiceSetup {
  return { uuid, preset, echo, characteristics };
}

const READ: Property[] = ["read"];
const READ_NOTIFY: Property[] = ["read", "notify"];
const NOTIFY: Property[] = ["notify"];
const INDICATE: Property[] = ["indicate"];
const WRITES: Property[] = ["write", "write_without_response"];

export const PRESETS: Preset[] = [
  {
    id: "nus",
    name: "Nordic UART (NUS)",
    detail: "RX to write, TX to notify: a UART over Bluetooth, with echo",
    services: [
      service("6E400001-B5A3-F393-E0A9-E50E24DCCA9E", "nus", [
        characteristic("6E400002-B5A3-F393-E0A9-E50E24DCCA9E", WRITES, [], "utf8"),
        characteristic("6E400003-B5A3-F393-E0A9-E50E24DCCA9E", NOTIFY, [], "utf8", 1000),
      ]),
    ],
  },
  {
    id: "hm10",
    name: "HM-10 UART",
    detail: "FFE0 with one characteristic, FFE1, to write and notify",
    services: [service("FFE0", "hm10", [characteristic("FFE1", ["read", "write_without_response", "notify"], [], "utf8", 1000)])],
  },
  {
    id: "microchip",
    name: "Microchip Transparent UART",
    detail: "As RN4870 and BM70 modules: RX to write, TX to notify",
    services: [
      service("49535343-FE7D-4AE5-8FA9-9FAFD205E455", "microchip", [
        characteristic("49535343-1E4D-4BD9-BA61-23C647249616", NOTIFY, [], "utf8", 1000),
        characteristic("49535343-8841-43F4-A8D4-ECBE34729BB3", WRITES, [], "utf8"),
      ]),
    ],
  },
  {
    id: "battery",
    name: "Battery",
    detail: "Battery Level, down 1 % each tick",
    services: [service("180F", "battery", [live("2A19", READ_NOTIFY, "u8")])],
  },
  {
    id: "heart_rate",
    name: "Heart Rate",
    detail: "Heart Rate Measurement, 60 to 100 bpm, and the sensor on the chest",
    services: [service("180D", "heart_rate", [live("2A37", NOTIFY), characteristic("2A38", READ, [0x01], "u8")])],
  },
  {
    id: "device_information",
    name: "Device Information",
    detail: "Manufacturer, model number and firmware version, to read",
    services: [
      service("180A", "device_information", [
        characteristic("2A29", READ, "NimaLTD", "utf8"),
        characteristic("2A24", READ, "bleNIMA simulator", "utf8"),
        characteristic("2A26", READ, "1.0.0", "utf8"),
      ]),
    ],
  },
  {
    id: "health_thermometer",
    name: "Health Thermometer",
    detail: "Temperature Measurement, 36.5 to 37.5 °C, indicated",
    services: [service("1809", "health_thermometer", [live("2A1C", INDICATE)])],
  },
  {
    id: "environmental_sensing",
    name: "Environmental Sensing",
    detail: "Temperature, humidity and air pressure",
    services: [
      service("181A", "environmental_sensing", [
        live("2A6E", READ_NOTIFY),
        live("2A6F", READ_NOTIFY),
        live("2A6D", READ_NOTIFY),
      ]),
    ],
  },
  {
    id: "current_time",
    name: "Current Time",
    detail: "The computer's date and time, every second",
    services: [service("1805", "current_time", [live("2A2B", READ_NOTIFY)])],
  },
  {
    id: "immediate_alert",
    name: "Immediate Alert (Find Me)",
    detail: "Alert Level, written by the phone: no, mild or high alert",
    services: [service("1802", "immediate_alert", [characteristic("2A06", ["write_without_response"], [0x00])])],
  },
  {
    id: "csc",
    name: "Cycling Speed and Cadence",
    detail: "Wheel and crank revolutions, as a bike sensor",
    services: [service("1816", "csc", [live("2A5B", NOTIFY), characteristic("2A5C", READ, [0x03, 0x00])])],
  },
  {
    id: "rsc",
    name: "Running Speed and Cadence",
    detail: "Speed and steps a minute, as a foot pod",
    services: [service("1814", "rsc", [live("2A53", NOTIFY), characteristic("2A54", READ, [0x04, 0x00])])],
  },
  {
    id: "blood_pressure",
    name: "Blood Pressure",
    detail: "Blood pressure and pulse rate, indicated",
    services: [service("1810", "blood_pressure", [live("2A35", INDICATE), characteristic("2A49", READ, [0x00, 0x00])])],
  },
  {
    id: "pulse_oximeter",
    name: "Pulse Oximeter",
    detail: "SpO2 and pulse rate, notified",
    services: [service("1822", "pulse_oximeter", [live("2A5F", NOTIFY), characteristic("2A60", READ, [0x00, 0x00])])],
  },
  {
    id: "blinky",
    name: "Nordic LED Button (Blinky)",
    detail: "A button that notifies and an LED to write, for the nRF Blinky app",
    services: [
      service("00001523-1212-EFDE-1523-785FEABCD123", "blinky", [
        live("00001524-1212-EFDE-1523-785FEABCD123", READ_NOTIFY),
        characteristic("00001525-1212-EFDE-1523-785FEABCD123", ["read", "write"], [0x00]),
      ]),
    ],
  },
  {
    id: "cycling_power",
    name: "Cycling Power",
    detail: "Power, 150 to 250 W",
    services: [
      service("1818", "cycling_power", [
        live("2A63", NOTIFY),
        characteristic("2A65", READ, [0x00, 0x00, 0x00, 0x00]),
        characteristic("2A5D", READ, [0x05], "u8"),
      ]),
    ],
  },
  {
    id: "indoor_bike",
    name: "Fitness Machine: Indoor Bike",
    detail: "Speed, cadence and power, for indoor cycling apps",
    services: [
      service("1826", "indoor_bike", [
        characteristic("2ACC", READ, [0x02, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]),
        live("2AD2", NOTIFY),
      ]),
    ],
  },
  {
    id: "weight_scale",
    name: "Weight Scale",
    detail: "Weight, about 75 kg, indicated",
    services: [service("181D", "weight_scale", [characteristic("2A9E", READ, [0x00, 0x00, 0x00, 0x00]), live("2A9D", INDICATE)])],
  },
  {
    id: "proximity",
    name: "Proximity: Link Loss + Tx Power",
    detail: "The alert level for a lost link, and the transmit power",
    services: [
      service("1803", "proximity", [characteristic("2A06", ["read", "write"], [0x00])]),
      service("1804", "proximity", [characteristic("2A07", READ, [0x00], "i8")]),
    ],
  },
  {
    id: "location_navigation",
    name: "Location and Navigation",
    detail: "Location and speed, going round a circle",
    services: [
      service("1819", "location_navigation", [
        characteristic("2A6A", READ, [0x05, 0x00, 0x00, 0x00]),
        live("2A67", NOTIFY),
      ]),
    ],
  },
  {
    id: "alert_notification",
    name: "Alert Notification",
    detail: "New emails and how many are not read yet",
    services: [
      service("1811", "alert_notification", [
        characteristic("2A47", READ, [0x02]),
        live("2A46", NOTIFY),
        characteristic("2A48", READ, [0x02]),
        live("2A45", NOTIFY),
        characteristic("2A44", ["write"], []),
      ]),
    ],
  },
  {
    id: "body_composition",
    name: "Body Composition",
    detail: "Body fat, about 22 %, indicated",
    services: [
      service("181B", "body_composition", [characteristic("2A9B", READ, [0x00, 0x00, 0x00, 0x00]), live("2A9C", INDICATE)]),
    ],
  },
];

/** A preset's services, as new copies to add to a server. */
export function presetServices(id: string): ServiceSetup[] {
  const preset = PRESETS.find((entry) => entry.id === id);
  return preset ? structuredClone(preset.services) : [];
}

/** A new service, made by hand: a custom UUID with one characteristic to read and write. */
export function newService(): ServiceSetup {
  return service("0000FEE0-0000-1000-8000-00805F9B34FB", "", [newCharacteristic()]);
}

/** A new characteristic, made by hand. */
export function newCharacteristic(): CharacteristicSetup {
  return characteristic("0000FEE1-0000-1000-8000-00805F9B34FB", ["read", "write", "notify"], [], "hex");
}
