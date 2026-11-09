// A scanner tab: the devices it hears, filtered and sorted, and their RSSI over time.

import { Channel, invoke } from "@tauri-apps/api/core";
import { SvelteMap } from "svelte/reactivity";
import type { Changes, DeviceDetails, DeviceRow } from "./bluetooth";

/** How many RSSI readings a device keeps in the UI, for its plot. */
const RSSI_KEPT = 3000;

export type ScanFilter = {
  text: string;
  min_rssi: number;
  only_named: boolean;
  only_connectable: boolean;
  only_favourites: boolean;
  sort: "rssi" | "name" | "last" | "first";
};

export const DEFAULT_FILTER: ScanFilter = {
  text: "",
  min_rssi: -100,
  only_named: false,
  only_connectable: false,
  only_favourites: false,
  sort: "rssi",
};

export class ScannerState {
  /** The Rust side's id for this scanner, once it is open. */
  private id: number | null = null;
  private channel: Channel<Changes> | null = null;
  readonly devices = new SvelteMap<string, DeviceRow>();
  /** Each device's RSSI over time, [ms] and [dBm]; `version` says when it grew. */
  readonly rssi = new Map<string, { at: number[]; dbm: number[] }>();
  version = $state(0);
  running = $state(false);
  adverts = $state(0);
  /** Adverts a second, over the last batches. */
  rate = $state(0);
  problem: string | null = $state(null);
  selected: string | null = $state(null);
  filter: ScanFilter = $state({ ...DEFAULT_FILTER });
  favourites: string[] = $state([]);
  /** When the scan started, by the wall clock, to say how long ago a device was heard. */
  startedAt = Date.now();
  private lastCount = 0;
  private lastTime = Date.now();

  constructor(filter: Partial<ScanFilter> | null, favourites: string[]) {
    Object.assign(this.filter, filter ?? {});
    this.favourites = [...favourites];
  }

  private newChannel(): Channel<Changes> {
    const channel = new Channel<Changes>();
    channel.onmessage = (changes) => {
      if (channel === this.channel) this.take(changes);
    };
    this.channel = channel;
    return channel;
  }

  private take(changes: Changes): void {
    if (changes.problem) {
      this.problem = changes.problem;
      this.running = false;
      return;
    }
    for (const row of changes.rows) this.devices.set(row.id, row);
    for (const [id, points] of changes.points) {
      let series = this.rssi.get(id);
      if (!series) {
        series = { at: [], dbm: [] };
        this.rssi.set(id, series);
      }
      for (const [at, dbm] of points) {
        series.at.push(at);
        series.dbm.push(dbm);
      }
      if (series.at.length > RSSI_KEPT) {
        series.at.splice(0, series.at.length - RSSI_KEPT);
        series.dbm.splice(0, series.dbm.length - RSSI_KEPT);
      }
    }
    const now = Date.now();
    if (now - this.lastTime >= 1000) {
      this.rate = Math.round(((changes.adverts - this.lastCount) * 1000) / (now - this.lastTime));
      this.lastCount = changes.adverts;
      this.lastTime = now;
    }
    this.adverts = changes.adverts;
    if (changes.points.length > 0) this.version++;
  }

  /** Starts scanning, or goes on after a pause. */
  async start(): Promise<void> {
    this.problem = null;
    try {
      if (this.id === null) {
        this.startedAt = Date.now();
        this.id = await invoke<number>("scan_open", { events: this.newChannel() });
      } else {
        await invoke("scan_run", { id: this.id, on: true, events: this.newChannel() });
      }
      this.running = true;
    } catch (error) {
      this.problem = String(error);
      this.running = false;
    }
  }

  async stop(): Promise<void> {
    if (this.id !== null) {
      await invoke("scan_run", { id: this.id, on: false, events: new Channel<Changes>() }).catch(() => {});
    }
    this.running = false;
  }

  async clear(): Promise<void> {
    if (this.id !== null) await invoke("scan_clear", { id: this.id });
    this.devices.clear();
    this.rssi.clear();
    this.selected = null;
    this.version++;
  }

  async close(): Promise<void> {
    if (this.id !== null) await invoke("scan_close", { id: this.id }).catch(() => {});
    this.id = null;
    this.channel = null;
    this.running = false;
  }

  details(device: string): Promise<DeviceDetails | null> {
    if (this.id === null) return Promise.resolve(null);
    return invoke<DeviceDetails | null>("scan_details", { id: this.id, device });
  }

  async saveCsv(path: string): Promise<void> {
    if (this.id === null) throw new Error("Nothing scanned yet");
    await invoke("scan_csv", { id: this.id, path });
  }

  toggleFavourite(id: string): void {
    const at = this.favourites.indexOf(id);
    if (at >= 0) this.favourites.splice(at, 1);
    else this.favourites.push(id);
  }

  /** The rows that pass the filters, sorted, favourites first. */
  shown(): DeviceRow[] {
    const filter = this.filter;
    const text = filter.text.trim().toLowerCase();
    const rows = [...this.devices.values()].filter((row) => {
      if (filter.only_named && !row.name) return false;
      if (filter.only_connectable && row.connectable === false) return false;
      if (filter.only_favourites && !this.favourites.includes(row.id)) return false;
      if ((row.rssi ?? -127) < filter.min_rssi) return false;
      if (text) {
        const haystack = [row.name, row.address, row.id, row.beacon, row.appearance, ...row.companies, ...row.services]
          .filter(Boolean)
          .join(" ")
          .toLowerCase();
        if (!haystack.includes(text)) return false;
      }
      return true;
    });
    const by: Record<ScanFilter["sort"], (a: DeviceRow, b: DeviceRow) => number> = {
      rssi: (a, b) => (b.rssi ?? -127) - (a.rssi ?? -127),
      name: (a, b) => (a.name ?? "￿").localeCompare(b.name ?? "￿"),
      last: (a, b) => b.last_ms - a.last_ms,
      first: (a, b) => a.first_ms - b.first_ms,
    };
    const favourite = (row: DeviceRow) => (this.favourites.includes(row.id) ? 0 : 1);
    return rows.sort((a, b) => favourite(a) - favourite(b) || by[filter.sort](a, b));
  }
}
