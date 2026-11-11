// What is in the menu bar. Items for later phases of the roadmap are listed
// already, greyed out with their phase, so the menus show where the app is going.

import { openUrl } from "@tauri-apps/plugin-opener";
import { NEW_TABS, SIMULATOR_NAME, type NewTab } from "./new-tab";
import type { Command } from "./palette";
import { GITHUB, PLAN } from "./roadmap";
import {
  FONT_SIZE_DEFAULT,
  FONT_SIZE_MAX,
  FONT_SIZE_MIN,
  MENU_FONT_SIZE_DEFAULT,
  MENU_FONT_SIZE_MAX,
  MENU_FONT_SIZE_MIN,
  settings,
  updateSettings,
  type ThemeChoice,
} from "./settings.svelte";
import { isMac, keys, matches } from "./shortcuts";
import { activeTab, closeTab, hasScanner, showTab, tabs, view } from "./tabs.svelte";

export type Item = {
  label: string;
  shortcut?: string;
  /** Shown on the right where a shortcut would be. */
  hint?: string;
  /** The roadmap phase that brings this item. Until then it is greyed out. */
  phase?: number;
  enabled?: () => boolean;
  checked?: () => boolean;
  run?: () => void;
};

/** A menu's items, with `null` for a separator line and a string for a heading over the items after it. */
export type Menu = { title: string; items: (Item | string | null)[] };

/** What the menus ask of the app around them. */
export type AppActions = {
  about(): void;
  exit(): void;
  /** Opens a new scanner or device simulator tab. */
  newTab(kind: NewTab["kind"]): void;
  /** Opens the app's settings. */
  settings(): void;
  /** Opens the command palette. */
  commandPalette(): void;
  /** Looks on GitHub for a newer bleNIMA, and says what it found. */
  checkUpdates(): void;
  /** Shows the devices the OS has a bond with. */
  bonds(): void;
};

export function buildMenus(app: AppActions): Menu[] {
  const theme = (label: string, choice: ThemeChoice): Item => ({
    label,
    checked: () => settings.theme === choice,
    run: () => updateSettings({ theme: choice }),
  });
  const textSize = (size: number) =>
    updateSettings({ font_size: Math.min(FONT_SIZE_MAX, Math.max(FONT_SIZE_MIN, size)) });
  const menuTextSize = (size: number) =>
    updateSettings({ menu_font_size: Math.min(MENU_FONT_SIZE_MAX, Math.max(MENU_FONT_SIZE_MIN, size)) });

  const hasTab = () => activeTab() !== undefined;
  const hasTabs = () => tabs.length > 1;
  const scanner = () => activeTab()?.scanner ?? null;
  const device = () => activeTab()?.device ?? null;
  const deviceView = (label: string, view: string, shortcut?: string): Item => ({
    label,
    shortcut,
    enabled: () => device() !== null,
    checked: () => device() !== null && activeTab()?.view === view,
    run: () => activeTab()?.setView(view),
  });
  /** Puts the keyboard in the filter of what is on screen. */
  const find = () => {
    const field = document.querySelector<HTMLInputElement>(".slot:not(.hidden) input.search, .slot:not(.hidden) .log input");
    field?.focus();
    field?.select();
  };

  return [
    {
      title: "File",
      items: [
        // The same as the + button's menu. Device and terminal tabs open from the scanner.
        "New tab",
        ...NEW_TABS.map(({ kind, label, shortcut }) => ({
          label,
          shortcut,
          // There is one scanner at most.
          enabled: () => kind !== "scanner" || !hasScanner(),
          run: () => app.newTab(kind),
        })),
        null,
        {
          label: "Close tab",
          shortcut: keys("Ctrl+Shift+W", "Cmd+W"),
          enabled: hasTab,
          run: () => {
            const tab = activeTab();
            if (tab) closeTab(tab);
          },
        },
        null,
        { label: "Settings...", shortcut: keys("Ctrl+,", "Cmd+,"), run: app.settings },
        null,
        isMac ? { label: "Quit bleNIMA", shortcut: "Cmd+Q", run: app.exit } : { label: "Exit", run: app.exit },
      ],
    },
    {
      title: "Edit",
      items: [
        { label: "Copy", shortcut: keys("Ctrl+C", "Cmd+C"), run: () => document.execCommand("copy") },
        null,
        {
          label: "Find...",
          shortcut: keys("Ctrl+Shift+F", "Cmd+F"),
          enabled: () => scanner() !== null || (device() !== null && activeTab()?.view === "log"),
          run: find,
        },
        {
          label: "Clear",
          shortcut: keys("Ctrl+Shift+K", "Cmd+K"),
          enabled: () => scanner() !== null || device() !== null,
          run: () => {
            void scanner()?.clear();
            device()?.clearLog();
          },
        },
      ],
    },
    {
      title: "View",
      items: [
        {
          label: "Command palette...",
          shortcut: keys("Ctrl+Shift+P", "Cmd+Shift+P"),
          run: app.commandPalette,
        },
        null,
        "Scanner",
        {
          label: "Device list",
          enabled: () => scanner() !== null,
          checked: () => scanner() !== null && activeTab()?.view !== "plot",
          run: () => activeTab()?.setView("list"),
        },
        {
          label: "RSSI plot",
          shortcut: keys("Ctrl+Shift+G", "Cmd+Shift+G"),
          enabled: () => scanner() !== null,
          checked: () => scanner() !== null && activeTab()?.view === "plot",
          run: () => activeTab()?.setView(activeTab()?.view === "plot" ? "list" : "plot"),
        },
        "Device",
        deviceView("GATT", "gatt"),
        deviceView("Terminal", "terminal", keys("Ctrl+Shift+T", "Cmd+Shift+T")),
        deviceView("Log", "log", keys("Ctrl+Shift+J", "Cmd+Shift+J")),
        deviceView("Tests", "tools"),
        null,
        {
          label: "Next tab",
          shortcut: "Ctrl+Tab",
          enabled: hasTabs,
          run: () => showTab(view.active + 1),
        },
        {
          label: "Previous tab",
          shortcut: "Ctrl+Shift+Tab",
          enabled: hasTabs,
          run: () => showTab(view.active - 1),
        },
        null,
        {
          label: "Bigger text",
          shortcut: keys("Ctrl+=", "Cmd+="),
          run: () => textSize(settings.font_size + 1),
        },
        {
          label: "Smaller text",
          shortcut: keys("Ctrl+-", "Cmd+-"),
          run: () => textSize(settings.font_size - 1),
        },
        {
          label: "Normal text size",
          shortcut: keys("Ctrl+0", "Cmd+0"),
          run: () => textSize(FONT_SIZE_DEFAULT),
        },
        null,
        { label: "Bigger menu text", run: () => menuTextSize(settings.menu_font_size + 1) },
        { label: "Smaller menu text", run: () => menuTextSize(settings.menu_font_size - 1) },
        { label: "Normal menu text size", run: () => menuTextSize(MENU_FONT_SIZE_DEFAULT) },
        null,
        theme("System theme", "system"),
        theme("Dark theme", "dark"),
        theme("Light theme", "light"),
      ],
    },
    {
      title: "Device",
      items: [
        {
          label: "Start scanning",
          shortcut: "F5",
          enabled: () => scanner() !== null && !scanner()!.running,
          run: () => activeTab()?.start(),
        },
        {
          label: "Stop scanning",
          shortcut: "Shift+F5",
          enabled: () => scanner()?.running === true,
          run: () => activeTab()?.stop(),
        },
        null,
        {
          label: "Connect",
          enabled: () => device() !== null && !device()!.open,
          run: () => activeTab()?.start(),
        },
        {
          label: "Disconnect",
          enabled: () => device()?.open === true,
          run: () => activeTab()?.stop(),
        },
        null,
        {
          label: "Pair",
          enabled: () => device()?.connected === true && device()?.paired !== true && !device()!.pairing,
          run: () => void device()?.pair(true),
        },
        {
          label: "Unpair",
          enabled: () => device()?.paired === true && !device()!.pairing,
          run: () => void device()?.pair(false),
        },
        { label: "Bonded devices...", run: app.bonds },
      ],
    },
    {
      title: "Tools",
      items: [
        {
          label: "Tests: throughput, latency, soak, send and expect",
          enabled: () => device() !== null,
          run: () => activeTab()?.setView("tools"),
        },
        {
          label: "Firmware update (MCUmgr)",
          enabled: () => device() !== null,
          run: () => activeTab()?.setView("tools"),
        },
        null,
        { label: SIMULATOR_NAME, run: () => app.newTab("simulator") },
        { label: "Mesh", phase: 8 },
      ],
    },
    {
      title: "Help",
      items: [
        { label: "bleNIMA help", phase: 9 },
        { label: "The plan", run: () => void openUrl(PLAN) },
        { label: "bleNIMA on GitHub", run: () => void openUrl(GITHUB) },
        { label: "Report a bug...", run: () => void openUrl(`${GITHUB}/issues/new`) },
        { label: "Check for updates...", run: app.checkUpdates },
        null,
        { label: "About bleNIMA", run: app.about },
      ],
    },
  ];
}

/**
 * Every menu item that can be used now, as commands for the palette: the
 * palette's own item and the items of later phases left out.
 */
export function menuCommands(menus: Menu[]): Command[] {
  const commands: Command[] = [];
  for (const menu of menus) {
    // Items under a heading are named with it: "New tab: Scanner".
    let heading = "";
    for (const item of menu.items) {
      if (typeof item === "string" || item === null) {
        heading = item ?? "";
      } else if (isEnabled(item) && item.run && !item.label.startsWith("Command palette")) {
        commands.push({
          label: `${heading ? `${heading}: ` : ""}${item.label.replace(/\.\.\.$/, "")}`,
          group: menu.title,
          shortcut: item.shortcut,
          checked: item.checked?.() ?? false,
          run: item.run,
        });
      }
    }
  }
  return commands;
}

/** Whether an item can be used right now. */
export function isEnabled(item: Item): boolean {
  return item.phase === undefined && item.run !== undefined && (item.enabled?.() ?? true);
}

/** The item whose shortcut this key press is, if any. */
export function itemForKey(menus: Menu[], event: KeyboardEvent): Item | null {
  for (const menu of menus) {
    for (const item of menu.items) {
      if (item && typeof item !== "string" && item.shortcut && matches(item.shortcut, event)) {
        return item;
      }
    }
  }
  return null;
}

/** Alt+F opens File, Alt+E opens Edit and so on; not on macOS, where Option types characters. */
export function menuForAltKey(menus: Menu[], event: KeyboardEvent): number | null {
  if (isMac || !event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) {
    return null;
  }
  const index = menus.findIndex((menu) => event.code === `Key${menu.title[0]}`);
  return index >= 0 ? index : null;
}
