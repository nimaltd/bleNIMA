// A device simulator tab's GATT server, as the UI keeps it: the services
// set up, their values while it runs, who is subscribed, the timers, and the log.

import { Channel, invoke } from "@tauri-apps/api/core";
import { SvelteMap } from "svelte/reactivity";
import { clock, parseFormat, type LogLine } from "./device.svelte";
import { show } from "./format";
import { decodeProfile } from "./profiles";
import {
  KNOWN_NAMES,
  describeServed,
  fullUuid,
  hexBytes,
  hexText,
  liveValue,
  newCharacteristic,
  newService,
  presetServices,
  sends,
  toServices,
  uuidText,
  type ServerEvent,
  type ServiceSetup,
} from "./server";

/** How many lines the log keeps. */
const LOG_KEPT = 5000;
/** The shortest timer, in ms: shorter ones are run at this. */
export const TIMER_MIN = 20;

/** A standard UUID and its name, from the names database on the Rust side. */
export type NamedUuid = { uuid: string; name: string };

/** The standard names, asked for once, by kind. */
let standard = $state.raw({
  services: [] as NamedUuid[],
  characteristics: [] as NamedUuid[],
  service: new Map<string, string>(),
  characteristic: new Map<string, string>(),
});
let asked = false;

/** Asks the Rust side for every standard service and characteristic name, once. */
export function loadNames(): void {
  if (asked) return;
  asked = true;
  Promise.all([
    invoke<NamedUuid[]>("standard_names", { characteristics: false }),
    invoke<NamedUuid[]>("standard_names", { characteristics: true }),
  ])
    .then(([services, characteristics]) => {
      standard = {
        services,
        characteristics,
        service: new Map(services.map((entry) => [entry.uuid, entry.name])),
        characteristic: new Map(characteristics.map((entry) => [entry.uuid, entry.name])),
      };
    })
    .catch(() => (asked = false));
}

/** Every standard service, or characteristic, to pick one by name. */
export function standardList(characteristics: boolean): NamedUuid[] {
  return characteristics ? standard.characteristics : standard.services;
}

/** A UUID's standard name, or one bleNIMA knows; "" when it has none. */
export function nameOf(uuid: string, characteristic: boolean): string {
  const full = fullUuid(uuid);
  if (!full) return "";
  return (characteristic ? standard.characteristic : standard.service).get(full) ?? KNOWN_NAMES[full] ?? "";
}

/** The full UUID of a standard name, typed in any case; null when it is not one. */
export function uuidOfName(name: string, characteristic: boolean): string | null {
  const wanted = name.trim().toLowerCase();
  const found = standardList(characteristic).find((entry) => entry.name.toLowerCase() === wanted);
  return found?.uuid ?? null;
}

function key(service: number, characteristic: number): string {
  return `${service}/${characteristic}`;
}

function count(n: number, thing: string): string {
  return `${n} ${thing}${n === 1 ? "" : "s"}`;
}

/**
 * The simulator's server that serves, or is starting. The simulators would
 * share the PC's one GATT table and its advert, so one serves at a time.
 */
let serving: ServerState | null = null;

export class ServerState {
  services: ServiceSetup[] = $state([]);
  private id: number | null = $state(null);
  starting = $state(false);
  problem = $state("");
  /** Each service's advert while it runs. */
  adverts: { on: boolean; text: string }[] = $state([]);
  /** The values now, by "service/characteristic", while it runs. */
  readonly values = new SvelteMap<string, number[]>();
  /** The clients subscribed to each characteristic, by "service/characteristic". */
  readonly subscribers = new SvelteMap<string, string[]>();
  log: LogLine[] = $state([]);
  /** The service picked in the tree, and its characteristic, or null for the service itself. */
  selected: { service: number; characteristic: number | null } | null = $state(null);
  private timers = new Map<string, ReturnType<typeof setInterval>>();
  private ticks = new Map<string, number>();
  /** Values sent and not answered yet, so a timer does not pile them up. */
  private busy = new Set<string>();
  private lineCount = 0;

  constructor(services: ServiceSetup[]) {
    this.services = structuredClone(services);
  }

  get running(): boolean {
    return this.id !== null;
  }

  /** Every client subscribed to something. */
  get clients(): string[] {
    return [...new Set([...this.subscribers.values()].flat())];
  }

  get stateText(): string {
    if (this.starting) return "Starting the GATT server...";
    if (!this.running) return this.problem || "GATT server stopped";
    const advertising = this.adverts.some((advert) => advert.on);
    return `Serving ${count(this.services.length, "service")}, ${advertising ? "advertising" : "not advertising"}; ${count(this.clients.length, "client")} subscribed`;
  }

  serviceName(service: number): string {
    const uuid = this.services[service]?.uuid ?? "";
    return nameOf(uuid, false) || uuidText(uuid);
  }

  characteristicName(service: number, at: number): string {
    const characteristic = this.services[service]?.characteristics[at];
    if (!characteristic) return "";
    return characteristic.description.trim() || nameOf(characteristic.uuid, true) || uuidText(characteristic.uuid);
  }

  /** A characteristic's value now: as it runs, or the one it starts with. */
  valueOf(service: number, at: number): number[] {
    if (this.running) return this.values.get(key(service, at)) ?? [];
    return hexBytes(this.services[service]?.characteristics[at]?.value ?? "") ?? [];
  }

  /** A value in words: in the characteristic's format, or what it means, or hex. */
  describe(service: number, at: number, data: number[]): string {
    const characteristic = this.services[service]?.characteristics[at];
    if (data.length === 0) return "(empty)";
    const choice = parseFormat(characteristic?.format);
    if (choice && choice.format !== "hex") return show(data, choice.format, choice.littleEndian);
    if (!choice) {
      const uuid = fullUuid(characteristic?.uuid ?? "") ?? "";
      const known = decodeProfile(uuid, data)?.text ?? describeServed(uuid, data);
      if (known) return known;
    }
    return hexText(data);
  }

  private addLine(line: Omit<LogLine, "n" | "time">): void {
    this.lineCount++;
    this.log.push({ ...line, n: this.lineCount, time: clock() });
    if (this.log.length > LOG_KEPT) this.log.splice(0, this.log.length - LOG_KEPT);
  }

  /** Makes the services on the radio and advertises them. */
  async start(): Promise<void> {
    if (this.running || this.starting) return;
    const services = toServices(this.services);
    if (typeof services === "string") {
      this.problem = services;
      this.addLine({ kind: "error", key: null, name: "GATT server", text: services, hex: "" });
      return;
    }
    this.problem = "";
    this.starting = true;
    // The one that serves stops first.
    const before = serving;
    serving = this;
    if (before && before !== this) await before.stop("another simulator started");
    this.values.clear();
    this.subscribers.clear();
    this.adverts = services.map(() => ({ on: false, text: "Starting" }));
    services.forEach((service, at) =>
      service.characteristics.forEach((characteristic, index) => this.values.set(key(at, index), characteristic.value)),
    );
    const channel = new Channel<ServerEvent>();
    channel.onmessage = (event) => this.take(event);
    try {
      const id = await invoke<number>("server_start", { services, events: channel });
      if (serving !== this) {
        // Another simulator started while this one was starting: that one serves.
        await invoke("server_stop", { id }).catch(() => {});
        this.adverts = [];
        this.addLine({ kind: "state", key: null, name: "GATT server", text: "Stopped, as another simulator started", hex: "" });
        return;
      }
      this.id = id;
      this.addLine({
        kind: "state",
        key: null,
        name: "GATT server",
        text: `Started with ${count(services.length, "service")}, advertised as connectable`,
        hex: "",
      });
      this.services.forEach((service, at) => service.characteristics.forEach((_, index) => this.startTimer(at, index)));
    } catch (error) {
      if (serving === this) serving = null;
      this.problem = String(error);
      this.adverts = [];
      this.addLine({ kind: "error", key: null, name: "GATT server", text: this.problem, hex: "" });
    } finally {
      this.starting = false;
    }
  }

  /** Takes the services off the radio, and their advert; `because` says why, in the log, when it was not asked. */
  async stop(because = ""): Promise<void> {
    if (serving === this) serving = null;
    for (const timer of this.timers.values()) clearInterval(timer);
    this.timers.clear();
    const id = this.id;
    this.id = null;
    this.subscribers.clear();
    this.adverts = [];
    if (id !== null) {
      await invoke("server_stop", { id }).catch(() => {});
      this.addLine({
        kind: "state",
        key: null,
        name: "GATT server",
        text: `Stopped${because ? `, as ${because}` : ""}: its services and their advert are gone`,
        hex: "",
      });
    }
  }

  /** Starts, restarts or stops a characteristic's timer, as it is set up now. */
  startTimer(service: number, at: number): void {
    const k = key(service, at);
    clearInterval(this.timers.get(k));
    this.timers.delete(k);
    const characteristic = this.services[service]?.characteristics[at];
    if (!this.running || !characteristic || !(characteristic.timer_ms > 0)) return;
    const ms = Math.max(TIMER_MIN, characteristic.timer_ms);
    this.timers.set(
      k,
      setInterval(() => void this.tick(service, at, ms), ms),
    );
  }

  /** One tick of a timer: the next live value, or the value as it is, sent again. */
  private async tick(service: number, at: number, ms: number): Promise<void> {
    const k = key(service, at);
    const characteristic = this.services[service]?.characteristics[at];
    if (!characteristic || this.busy.has(k) || this.id === null) return;
    const n = this.ticks.get(k) ?? 0;
    this.ticks.set(k, n + 1);
    const now = this.values.get(k) ?? [];
    const next = (characteristic.live ? liveValue(characteristic.uuid, now, { n, now: new Date(), ms }) : null) ?? now;
    this.busy.add(k);
    try {
      await this.send(service, at, next, sends(characteristic));
    } catch (error) {
      // Stopped while it was sending: nothing went wrong.
      if (this.id === null) return;
      clearInterval(this.timers.get(k));
      this.timers.delete(k);
      this.addLine({
        kind: "error",
        key: k,
        name: this.characteristicName(service, at),
        text: `The timer stopped: ${error}`,
        hex: "",
      });
    } finally {
      this.busy.delete(k);
    }
  }

  private async send(service: number, at: number, data: number[], notify: boolean): Promise<number> {
    this.values.set(key(service, at), data);
    return invoke<number>("server_value", { id: this.id, place: { service, characteristic: at }, data, notify });
  }

  /**
   * Sets a value by hand: the one it starts with, and the one it has now when the server runs. With `notify`, the
   * clients subscribed get it too.
   */
  async setValue(service: number, at: number, data: number[], notify: boolean): Promise<void> {
    const characteristic = this.services[service]?.characteristics[at];
    if (!characteristic) return;
    characteristic.value = hexText(data);
    if (!this.running) return;
    const name = this.characteristicName(service, at);
    try {
      const subscribed = await this.send(service, at, data, notify);
      if (!notify || subscribed === 0) {
        this.addLine({
          kind: "state",
          key: key(service, at),
          name,
          text: `${notify ? "Nobody is subscribed: set" : "Set"} to ${this.describe(service, at, data)}, for reads`,
          hex: hexText(data),
        });
      }
    } catch (error) {
      this.addLine({ kind: "error", key: key(service, at), name, text: String(error), hex: "" });
    }
  }

  private take(event: ServerEvent): void {
    if (this.id === null && !this.starting) return;
    switch (event.type) {
      case "advertising": {
        this.adverts[event.service] = { on: event.on, text: event.text };
        this.addLine({ kind: "state", key: null, name: this.serviceName(event.service), text: event.text, hex: "" });
        break;
      }
      case "read": {
        const { service, characteristic } = event.place;
        const from = event.offset ? ` from byte ${event.offset}` : "";
        this.addLine({
          kind: "read",
          key: key(service, characteristic),
          name: this.characteristicName(service, characteristic),
          text: `${event.client} read ${this.describe(service, characteristic, event.data)}${from}`,
          hex: hexText(event.data),
        });
        break;
      }
      case "written": {
        const { service, characteristic } = event.place;
        this.values.set(key(service, characteristic), event.value);
        const at = event.offset ? ` at byte ${event.offset}` : "";
        this.addLine({
          kind: event.with_response ? "write" : "write_without_response",
          key: key(service, characteristic),
          name: this.characteristicName(service, characteristic),
          text: `${event.client} wrote ${this.describe(service, characteristic, event.data)}${at}`,
          hex: hexText(event.data),
        });
        break;
      }
      case "subscribers": {
        const { service, characteristic } = event.place;
        const k = key(service, characteristic);
        const before = this.subscribers.get(k) ?? [];
        this.subscribers.set(k, event.clients);
        const name = this.characteristicName(service, characteristic);
        for (const client of event.clients.filter((c) => !before.includes(c))) {
          this.addLine({ kind: "subscribed", key: k, name, text: `${client} subscribed`, hex: "" });
        }
        for (const client of before.filter((c) => !event.clients.includes(c))) {
          this.addLine({ kind: "subscribed", key: k, name, text: `${client} unsubscribed`, hex: "" });
        }
        break;
      }
      case "notified": {
        const { service, characteristic } = event.place;
        const k = key(service, characteristic);
        this.values.set(k, event.data);
        const setup = this.services[service]?.characteristics[characteristic];
        const verb = setup && !setup.properties.includes("notify") ? "Indicated" : "Notified";
        const name = this.characteristicName(service, characteristic);
        if (event.sent.length > 0) {
          this.addLine({
            kind: "notify",
            key: k,
            name,
            text: `${event.echo ? "Echoed" : verb} ${this.describe(service, characteristic, event.data)} to ${event.sent.join(", ")}`,
            hex: hexText(event.data),
          });
        }
        if (event.failed.length > 0) {
          this.addLine({ kind: "error", key: k, name, text: `Not sent to ${event.failed.join("; ")}`, hex: hexText(event.data) });
        }
        break;
      }
      case "failed": {
        const place = event.place;
        this.addLine({
          kind: "error",
          key: place ? key(place.service, place.characteristic) : null,
          name: place ? this.characteristicName(place.service, place.characteristic) : "GATT server",
          text: `${event.action}: ${event.problem}`,
          hex: "",
        });
        break;
      }
    }
  }

  // ---- Changing the services; only while it is stopped ----

  /** Adds a preset's services, and picks the first one. */
  addPreset(id: string): void {
    if (this.running) return;
    const added = presetServices(id);
    if (added.length === 0) return;
    this.services.push(...added);
    this.selected = { service: this.services.length - added.length, characteristic: null };
  }

  /** Adds a service made by hand, with one characteristic. */
  addService(): void {
    if (this.running) return;
    this.services.push(newService());
    this.selected = { service: this.services.length - 1, characteristic: null };
  }

  removeService(service: number): void {
    if (this.running) return;
    this.services.splice(service, 1);
    this.selected = this.services.length > 0 ? { service: Math.min(service, this.services.length - 1), characteristic: null } : null;
  }

  addCharacteristic(service: number): void {
    const list = this.services[service]?.characteristics;
    if (this.running || !list) return;
    list.push(newCharacteristic());
    this.selected = { service, characteristic: list.length - 1 };
  }

  removeCharacteristic(service: number, at: number): void {
    const list = this.services[service]?.characteristics;
    if (this.running || !list) return;
    list.splice(at, 1);
    this.selected = { service, characteristic: null };
  }

  clearLog(): void {
    this.log.splice(0, this.log.length);
  }

  /** The log as text, for saving. */
  logText(): string {
    return this.log
      .map((line) => `${line.time}\t${line.kind}\t${line.name}\t${line.text}${line.hex && line.hex !== line.text ? `\t${line.hex}` : ""}`)
      .join("\n");
  }
}
