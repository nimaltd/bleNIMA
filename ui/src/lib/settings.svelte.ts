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

/** The same fields as `Settings` in blenima-core's settings.rs. */
export type Settings = {
  theme: ThemeChoice;
  /** The text size of the views: lists, values and terminals. */
  font_size: number;
  startup: StartupSettings;
  updates: UpdateSettings;
};

/** Text size limits, the same as in settings.rs. */
export const FONT_SIZE_MIN = 9;
export const FONT_SIZE_MAX = 24;
export const FONT_SIZE_DEFAULT = 13;

/** The settings in use. Read them anywhere; change them only with `updateSettings`. */
export const settings: Settings = $state({
  theme: "system",
  font_size: FONT_SIZE_DEFAULT,
  startup: { restore_tabs: true, reconnect: true, restore_window: true },
  updates: { check: true, skipped: "" },
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
