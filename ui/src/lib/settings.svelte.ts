import { invoke } from "@tauri-apps/api/core";

export type ThemeChoice = "system" | "dark" | "light";

/** What comes back when bleNIMA starts. */
export type StartupSettings = {
  /** Opens the tabs from last time. */
  restore_tabs: boolean;
  /** Connects the tabs that were connected when bleNIMA closed. */
  reconnect: boolean;
  /** Opens the window where it was, at the size it had. */
  restore_window: boolean;
};

/** Looking on GitHub for a newer bleNIMA. */
export type UpdateSettings = {
  /** Looks when bleNIMA starts. */
  check: boolean;
  /** A version not to tell about again, such as "1.0.2"; empty for none. */
  skipped: string;
};

/** How a Peripheral simulator tab lays out its advertiser, GATT server and log. */
export type SimulatorLayout = "views" | "page" | "columns";

/** Peripheral simulator tabs. */
export type SimulatorSettings = {
  layout: SimulatorLayout;
};

/**
 * Where a device tab shows its device information card: two columns over the GATT view, one line over it, two
 * columns over every view, or none, with nothing read for it.
 */
export type DeviceCard = "above_gatt" | "strip" | "above_all" | "off";

/** The same fields as `Settings` in blenima-core's settings.rs. */
export type Settings = {
  theme: ThemeChoice;
  /** The text size of the menus, tabs, buttons and dialogs, which grow with it. */
  menu_font_size: number;
  /** The text size of the views: lists, values and terminals. */
  font_size: number;
  device_card: DeviceCard;
  startup: StartupSettings;
  updates: UpdateSettings;
  simulator: SimulatorSettings;
};

/** Text size limits, the same as in settings.rs. */
export const FONT_SIZE_MIN = 9;
export const FONT_SIZE_MAX = 24;
export const FONT_SIZE_DEFAULT = 13;

/** Menu text size limits, the same as in settings.rs. */
export const MENU_FONT_SIZE_MIN = 10;
export const MENU_FONT_SIZE_MAX = 26;
export const MENU_FONT_SIZE_DEFAULT = 16;
/** The menu text size app.css and the components are written for; above it, the Rust side zooms the window. */
const MENU_FONT_SIZE_DRAWN = 13;

/**
 * The CSS size that shows as `size` pixels on screen. The window is zoomed to make the menus' text its size, so the
 * views' text is made smaller by as much, and stays at the text size.
 */
export function unzoomed(size: number): number {
  return (size * MENU_FONT_SIZE_DRAWN) / settings.menu_font_size;
}

/** The settings in use. Read them anywhere; change them only with `updateSettings`. */
export const settings: Settings = $state({
  theme: "system",
  menu_font_size: MENU_FONT_SIZE_DEFAULT,
  font_size: FONT_SIZE_DEFAULT,
  device_card: "above_gatt",
  startup: { restore_tabs: true, reconnect: true, restore_window: true },
  updates: { check: true, skipped: "" },
  simulator: { layout: "views" },
});

/** Reads the settings from settings.toml, through the Rust side. */
export async function loadSettings(): Promise<void> {
  Object.assign(settings, await invoke<Settings>("get_settings"));
}

/** Changes settings at once, and saves them to settings.toml. */
export function updateSettings(change: Partial<Settings>): void {
  Object.assign(settings, change);
  invoke<Settings>("set_settings", { settings: $state.snapshot(settings) })
    .then((kept) => Object.assign(settings, kept))
    .catch((error) => console.error(error));
}
