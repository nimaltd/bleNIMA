// The open tabs, which one is in front, and saving them to workspace.toml.

import { invoke } from "@tauri-apps/api/core";
import { DeviceState } from "./device.svelte";
import { kindInfo, type TabKind } from "./new-tab";
import { ScannerState, type ScanFilter } from "./scanner.svelte";
import type { ServiceSetup } from "./server";
import { ServerState } from "./server.svelte";
import { settings } from "./settings.svelte";

export type LineEnding = "cr" | "lf" | "crlf";

/** How a terminal talks to its device: the same fields as `TerminalSetup` in workspace.rs. */
export type TerminalSetup = {
  write: string;
  notify: string;
  enter: LineEnding;
  local_echo: boolean;
};

/** One tab, as it is saved: the same fields as `TabSetup` in workspace.rs. */
export type TabSetup = {
  name: string;
  kind: TabKind;
  /** Whether it was connected, scanning or serving when it was saved. */
  connected: boolean;
  device?: string;
  view?: string;
  filter?: ScanFilter | null;
  favourites?: string[];
  terminal?: TerminalSetup | null;
  formats?: Record<string, string>;
  /** A Peripheral simulator's GATT server: the same fields as `ServerSetup` in workspace.rs. */
  server?: { services: ServiceSetup[] } | null;
};

/** What the UI saves of `Workspace` in workspace.rs; the window's place comes from the window. */
type Workspace = {
  active: number;
  tabs: TabSetup[];
};

let nextId = 1;

/** The views a device tab has. */
export type DeviceView = "gatt" | "terminal" | "log" | "tools";

/** The views a Peripheral simulator tab has, when its parts are shown one at a time. */
export type SimulatorView = "advertiser" | "server" | "log";

export class Tab {
  /** Tells tabs apart while they are open, even with the same name. */
  readonly id = nextId++;
  readonly kind: TabKind;
  name = $state("");
  view = $state("");
  readonly scanner: ScannerState | null = null;
  readonly device: DeviceState | null = null;
  readonly server: ServerState | null = null;
  terminal: TerminalSetup = $state({ write: "", notify: "", enter: "cr", local_echo: false });

  constructor(setup: TabSetup) {
    this.kind = setup.kind;
    this.name = setup.name;
    this.view = setup.view ?? "";
    if (setup.kind === "scanner") {
      this.scanner = new ScannerState(setup.filter ?? null, setup.favourites ?? []);
      this.view ||= "list";
    }
    if ((setup.kind === "device" || setup.kind === "terminal") && setup.device) {
      this.device = new DeviceState(setup.device, setup.formats ?? {});
      this.view ||= setup.kind === "terminal" ? "terminal" : "gatt";
    }
    if (setup.kind === "simulator") {
      this.server = new ServerState(setup.server?.services ?? []);
      this.view ||= "server";
    }
    if (setup.terminal) this.terminal = { ...setup.terminal };
  }

  /** Scanning, connected or trying to be, or serving. */
  get connected(): boolean {
    if (this.scanner) return this.scanner.running;
    if (this.device) return this.device.open;
    if (this.server) return this.server.running;
    return false;
  }

  /** Whether this tab does anything yet, or only says when it comes. */
  get built(): boolean {
    return this.scanner !== null || this.device !== null || this.server !== null;
  }

  /** Starts scanning, connecting, or serving. */
  start(): void {
    void this.scanner?.start();
    void this.device?.connect();
    // Saved again once it serves, so it serves again next time.
    void this.server?.start().then(changed);
    changed();
  }

  stop(): void {
    void this.scanner?.stop();
    void this.device?.disconnect();
    void this.server?.stop();
    changed();
  }

  setView(view: string): void {
    this.view = view;
    changed();
  }

  /** The tab as it is saved. */
  setup(): TabSetup {
    return {
      name: this.name,
      kind: this.kind,
      connected: this.connected,
      device: this.device?.device ?? "",
      view: this.view,
      filter: this.scanner ? $state.snapshot(this.scanner.filter) : null,
      favourites: this.scanner ? [...this.scanner.favourites] : [],
      terminal: this.device ? $state.snapshot(this.terminal) : null,
      formats: this.device ? $state.snapshot(this.device.formats) : {},
      server: this.server ? { services: $state.snapshot(this.server.services) } : null,
    };
  }
}

/** The open tabs. */
export const tabs: Tab[] = $state([]);

/** What is on screen: `active` is the tab in front, with the keyboard. */
export const view: { active: number } = $state({ active: 0 });

let loaded = false;
let saveTimer: ReturnType<typeof setTimeout> | undefined;

export function activeTab(): Tab | undefined {
  return tabs[view.active];
}

/** Opens the tabs saved last time, if the Startup settings say so. The settings must be loaded first. */
export async function loadWorkspace(): Promise<void> {
  const workspace = await invoke<Workspace>("get_workspace");
  loaded = true;
  if (settings.startup.restore_tabs) {
    for (const setup of workspace.tabs) {
      const tab = new Tab(setup);
      tabs.push(tab);
      // A scanner scans when it opens; a device tab connects if it was connected.
      if (tab.scanner && setup.connected) void tab.scanner.start();
      if (tab.device && setup.connected && settings.startup.reconnect) void tab.device.connect();
      if (tab.server && setup.connected && settings.startup.reconnect) void tab.start();
    }
    view.active = Math.min(workspace.active, Math.max(0, tabs.length - 1));
  } else {
    // The tabs that do not come back are taken out of the file too.
    changed();
  }
}

/** Opens a new tab of a kind, named after it, and brings it in front. A scanner starts scanning. */
export function newTab(kind: TabKind): Tab {
  const tab = new Tab({ name: freeName(kindInfo(kind).label), kind, connected: false });
  tabs.push(tab);
  showTab(tabs.length - 1);
  if (tab.scanner) tab.start();
  return tab;
}

/** Opens a device or terminal tab for a device the scanner heard, and connects. */
export function openDevice(device: string, name: string, kind: "device" | "terminal"): Tab {
  const tab = new Tab({ name: freeName(name), kind, connected: false, device });
  tabs.push(tab);
  showTab(tabs.length - 1);
  tab.start();
  return tab;
}

/** "Scanner", or "Scanner 2" when there is one already. */
export function freeName(label: string): string {
  const taken = new Set(tabs.map((tab) => tab.name));
  let name = label;
  for (let number = 2; taken.has(name); number++) {
    name = `${label} ${number}`;
  }
  return name;
}

export function closeTab(tab: Tab): void {
  const index = tabs.indexOf(tab);
  if (index < 0) {
    return;
  }
  void tab.scanner?.close();
  void tab.device?.disconnect();
  void tab.server?.stop();
  tabs.splice(index, 1);
  const after = index < view.active ? view.active - 1 : view.active;
  view.active = Math.min(after, Math.max(0, tabs.length - 1));
  changed();
}

/** Brings a tab in front, going round past the ends. */
export function showTab(index: number): void {
  if (tabs.length === 0) {
    return;
  }
  view.active = (index + tabs.length) % tabs.length;
  changed();
}

/** Saves the workspace a moment after the last change. */
export function changed(): void {
  if (!loaded) {
    return;
  }
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    const workspace: Workspace = { active: view.active, tabs: tabs.map((tab) => tab.setup()) };
    invoke("set_workspace", { workspace }).catch((error) => console.error(error));
  }, 300);
}
