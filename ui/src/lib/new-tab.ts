// The kinds of tab that File > New tab and the + button open: one list, so
// both offer the same, in the same words and order.

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
  kind: TabKind;
  label: string;
  /** What it is for, under the label in the + menu. */
  detail: string;
  /** The roadmap phase that brings it. */
  phase: number;
  shortcut?: string;
};

/** In the order the roadmap brings them; all through the computer's own Bluetooth. */
export const NEW_TABS: NewTab[] = [
  {
    kind: "scanner",
    label: "Scanner",
    detail: "The devices around, their adverts decoded, RSSI over time",
    phase: 1,
    shortcut: keys("Ctrl+Shift+N", "Cmd+N"),
  },
  {
    kind: "device",
    label: "Device",
    detail: "One device: its GATT table, its values read, written and plotted",
    phase: 2,
  },
  {
    kind: "terminal",
    label: "Terminal",
    detail: "NUS or any write and notify pair as a terminal, like a UART",
    phase: 3,
  },
  {
    kind: "simulator",
    label: "Peripheral simulator",
    detail: "bleNIMA as a device: an advertiser and a GATT server",
    phase: 7,
  },
  {
    kind: "mesh",
    label: "Mesh",
    detail: "Provision and configure mesh nodes through a GATT proxy",
    phase: 8,
  },
];

/** What the list says about a kind of tab. */
export function kindInfo(kind: TabKind): NewTab {
  return NEW_TABS.find((entry) => entry.kind === kind) ?? NEW_TABS[0];
}
