// The kinds of tab that File > New tab and the + button open: one list, so
// both offer the same, in the same words and order. There is one scanner at
// most, and device and terminal tabs open from it.

import { keys } from "./shortcuts";

/** What a tab is for: the same names as `TabKind` in blenima-core's workspace.rs. */
export type TabKind =
  | "scanner"
  | "device"
  | "terminal"
  | "simulator"
  | "sniffer"
  | "mesh"
  | "audio"
  | "direction_finding"
  | "channel_sounding"
  | "hci_console";

export type NewTab = {
  kind: "scanner" | "simulator";
  label: string;
  /** What it is for, under the label in the + menu. */
  detail: string;
  shortcut?: string;
};

/** The name of the scanner tab: there is one at most. */
export const SCANNER_NAME = "Scanner";

/** The name a new device simulator tab gets, with a number after it when it is taken. */
export const SIMULATOR_NAME = "Device simulator";

export const NEW_TABS: NewTab[] = [
  {
    kind: "scanner",
    label: SCANNER_NAME,
    detail: "The devices around, their adverts decoded, RSSI over time",
    shortcut: keys("Ctrl+Shift+N", "Cmd+N"),
  },
  {
    kind: "simulator",
    label: SIMULATOR_NAME,
    detail: "bleNIMA as a device: an advertiser and a GATT server",
  },
];
