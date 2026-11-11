// What the Rust side sends about Bluetooth, with the same fields as in
// blenima-core: the radio, the scanner's rows and a device's GATT table.

import { invoke } from "@tauri-apps/api/core";

export type RadioInfo = {
  name: string;
  raw_adverts: boolean;
  real_addresses: boolean;
  connection_parameters: boolean;
  pairing: boolean;
  /** bleNIMA can be a peripheral here, with a GATT server. */
  peripheral: boolean;
  /** Why it cannot be a peripheral, in words; null when it can. */
  peripheral_problem: string | null;
  /** Why a connection's PHY is not shown, in words; null where the OS can tell it. */
  phy_problem: string | null;
  /** Why the scanner cannot scan passively, in words; null when it can. */
  passive_problem: string | null;
  /** Why the scanner cannot hear the LE Coded PHY, in words; null when it can. */
  coded_scan_problem: string | null;
  /** Why an advert's transmit power cannot be chosen, in words; null when it can. */
  tx_power_problem: string | null;
  /** Why an advert cannot be an extended one, in words; null when it can. */
  extended_problem: string | null;
  /** Why an extended advert's PHYs cannot be chosen, in words; null when they can. */
  advert_phy_problem: string | null;
  /** The adapter has the LE 2M PHY; null where the OS does not say. */
  phy_2m: boolean | null;
  /** The adapter has the LE Coded PHY; null where the OS does not say. */
  phy_coded: boolean | null;
  limits: string[];
};

/** How the scanner listens: the same fields as `ScanSetup` in scanner.rs. */
export type ScanSetup = {
  /** Only listens: sends no scan requests, so hears no scan responses. */
  passive: boolean;
  /** Hears the LE Coded PHY too, where the OS and the adapter can. */
  coded: boolean;
};

/** A PHY an extended advert is sent on. */
export type Phy = "1m" | "2m" | "coded";

/** What an advert is sent with besides its AD structures: the same fields as `AdvertOptions` in advert.rs. */
export type AdvertOptions = {
  /** The transmit power asked for, in dBm; null for the OS's own. */
  tx_power: number | null;
  /** Says the transmit power in an extended advert's header. */
  include_tx_power: boolean;
  /** A Bluetooth 5 extended advert: longer, and on the PHYs below. */
  extended: boolean;
  primary_phy: Phy;
  secondary_phy: Phy;
};

export type AddressKind = "public" | "random" | "random_static" | "resolvable_private" | "non_resolvable_private";

export type AdvKind =
  | "connectable_undirected"
  | "connectable_directed"
  | "scannable_undirected"
  | "non_connectable_undirected"
  | "scan_response"
  | "extended";

export type DeviceRow = {
  id: string;
  address: string | null;
  address_kind: AddressKind | null;
  name: string | null;
  rssi: number | null;
  tx_power: number | null;
  connectable: boolean | null;
  kind: AdvKind | null;
  interval_ms: number | null;
  advertised_interval_ms: number | null;
  count: number;
  first_ms: number;
  last_ms: number;
  companies: string[];
  services: string[];
  service_uuids: string[];
  appearance: string | null;
  beacon: string | null;
  raw: boolean;
};

export type Changes = {
  rows: DeviceRow[];
  points: [string, [number, number][]][];
  devices: number;
  adverts: number;
  problem?: string;
};

export type Field = { ad_type: number; name: string; value: string; hex: string };

export type DeviceDetails = {
  row: DeviceRow;
  advert: Field[];
  response: Field[];
  advert_hex: string;
  response_hex: string;
  beacon: { kind: string; fields: [string, string][] } | null;
  rssi: [number, number][];
};

export type PresentationFormat = {
  format: string;
  exponent: number;
  unit: number;
  unit_symbol: string | null;
};

export type DescriptorInfo = { key: string; uuid: string; name: string | null };

export type CharacteristicInfo = {
  key: string;
  uuid: string;
  name: string | null;
  properties: number;
  property_names: string[];
  descriptors: DescriptorInfo[];
  user_description: string | null;
  format: PresentationFormat | null;
};

export type ServiceInfo = {
  key: string;
  uuid: string;
  name: string | null;
  primary: boolean;
  characteristics: CharacteristicInfo[];
};

export type LinkState = "searching" | "connecting" | "discovering" | "connected" | "waiting" | "closed";

export type Source = "read" | "notify" | "write" | "write_without_response" | "descriptor";

export type Link = { mtu: number; interval_ms: number | null; latency: number | null; timeout_ms: number | null };

export type DeviceEvent =
  | { type: "state"; state: LinkState; text: string }
  | { type: "services"; services: ServiceInfo[] }
  | { type: "value"; key: string; data: number[]; source: Source; at_ms: number }
  | { type: "subscribed"; key: string; on: boolean }
  | { type: "failed"; key: string | null; action: string; problem: string; at_ms: number }
  | ({ type: "link" } & Link)
  | { type: "phy"; tx: string; rx: string }
  | { type: "mark"; text: string; at_ms: number };

export type Command =
  | { type: "read"; key: string }
  | { type: "write"; key: string; data: number[]; with_response: boolean }
  | { type: "subscribe"; key: string; on: boolean }
  | { type: "read_descriptor"; key: string }
  | { type: "parameters"; preset: "balanced" | "throughput" | "power" };

/** Characteristic property bits. */
export const PROPERTY = {
  read: 0x02,
  writeWithoutResponse: 0x04,
  write: 0x08,
  notify: 0x10,
  indicate: 0x20,
} as const;

/** The radio, opened the first time it is asked for. */
let radio: Promise<RadioInfo> | null = null;

export function radioInfo(): Promise<RadioInfo> {
  radio ??= invoke<RadioInfo>("radio_info").catch((error) => {
    radio = null;
    throw error;
  });
  return radio;
}

/** "Public", "Random static"... */
export function addressKindText(kind: AddressKind | null): string {
  switch (kind) {
    case "public":
      return "Public";
    case "random_static":
      return "Random static";
    case "resolvable_private":
      return "Resolvable private";
    case "non_resolvable_private":
      return "Non-resolvable private";
    case "random":
      return "Random";
    default:
      return "";
  }
}

/** "ADV_IND, connectable"... */
export function advKindText(kind: AdvKind | null): string {
  switch (kind) {
    case "connectable_undirected":
      return "ADV_IND: connectable";
    case "connectable_directed":
      return "ADV_DIRECT_IND: for one central";
    case "scannable_undirected":
      return "ADV_SCAN_IND: scannable, not connectable";
    case "non_connectable_undirected":
      return "ADV_NONCONN_IND: broadcast only";
    case "scan_response":
      return "SCAN_RSP";
    case "extended":
      return "Extended advert";
    default:
      return "";
  }
}

/** Known UART-like services: what to write, and what to listen to. */
export const TERMINAL_PRESETS: { name: string; service: string; write: string; notify: string }[] = [
  {
    name: "Nordic UART Service",
    service: "6e400001-b5a3-f393-e0a9-e50e24dcca9e",
    write: "6e400002-b5a3-f393-e0a9-e50e24dcca9e",
    notify: "6e400003-b5a3-f393-e0a9-e50e24dcca9e",
  },
  {
    name: "HM-10",
    service: "0000ffe0-0000-1000-8000-00805f9b34fb",
    write: "0000ffe1-0000-1000-8000-00805f9b34fb",
    notify: "0000ffe1-0000-1000-8000-00805f9b34fb",
  },
  {
    name: "Microchip Transparent UART",
    service: "49535343-fe7d-4ae5-8fa9-9fafd205e455",
    write: "49535343-8841-43f4-a8d4-ecbe34729bb3",
    notify: "49535343-1e4d-4bd9-ba61-23c647249616",
  },
];

/** The MCUmgr SMP service and its one characteristic. */
export const SMP_SERVICE = "8d53dc1d-1db7-4cd3-868b-8a527460aa84";
export const SMP_CHARACTERISTIC = "da2e7828-fbce-4e01-ae9e-261174997c48";
