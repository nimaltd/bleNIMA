// A device tab's connection, as the UI keeps it: the GATT table, each
// characteristic's values, the log, and what is subscribed.

import { Channel, invoke } from "@tauri-apps/api/core";
import { SvelteMap, SvelteSet } from "svelte/reactivity";
import { appearanceName } from "./appearance";
import { PROPERTY } from "./bluetooth";
import type { CharacteristicInfo, Command, DeviceEvent, Link, LinkState, ServiceInfo, Source } from "./bluetooth";
import { decodeDescriptor, descriptorUuid, isCccd, valueFormat, type ValueFormat } from "./descriptors";
import { hex, show, width, type Format } from "./format";
import { DEVICE_INFO, characteristicUuid, decodeProfile, infoText, shortUuid, type Names } from "./profiles";
import { settings } from "./settings.svelte";

/** The names the decoders take from the tables. */
const NAMES: Names = { appearance: appearanceName };

/** How many lines the log keeps. */
const LOG_KEPT = 5000;
/** How many values each characteristic keeps, for its list and its plot. */
const HISTORY_KEPT = 2000;

export type Value = { data: number[]; source: Source; at: number };

export type LogLine = {
  n: number;
  /** The time of day it happened: "14:02:11.204". */
  time: string;
  kind: Source | "error" | "mark" | "state" | "subscribed";
  key: string | null;
  /** The characteristic's name, or what the line is about. */
  name: string;
  text: string;
  hex: string;
};

/** "14:02:11.204" */
export function clock(at = new Date()): string {
  const two = (n: number) => String(n).padStart(2, "0");
  return `${two(at.getHours())}:${two(at.getMinutes())}:${two(at.getSeconds())}.${String(at.getMilliseconds()).padStart(3, "0")}`;
}

/** A format as saved: "u16", or "u16:be" for big-endian. */
export type FormatChoice = { format: Format; littleEndian: boolean };

export function parseFormat(saved: string | undefined): FormatChoice | null {
  if (!saved) return null;
  const [format, endian] = saved.split(":");
  return { format: format as Format, littleEndian: endian !== "be" };
}

export function formatText(choice: FormatChoice): string {
  return choice.littleEndian ? choice.format : `${choice.format}:be`;
}

export class DeviceState {
  private id: number | null = $state(null);
  private channel: Channel<DeviceEvent> | null = null;
  /** The radio's id for the device. */
  readonly device: string;
  state: LinkState = $state("closed");
  stateText = $state("Not connected");
  services: ServiceInfo[] = $state([]);
  link: Link | null = $state(null);
  /** The PHY the connection sends and receives on: "1M", "2M" or "Coded"; null until the OS tells it. */
  phy: { tx: string; rx: string } | null = $state(null);
  readonly values = new SvelteMap<string, Value>();
  readonly history = new Map<string, Value[]>();
  /** Grows as values come, so plots and lists know to draw again. */
  version = $state(0);
  readonly subscribed = new SvelteSet<string>();
  log: LogLine[] = $state([]);
  selected: string | null = $state(null);
  formats: Record<string, string> = $state({});
  /** Bytes received and sent, for the status bar and the throughput test. */
  rx = $state(0);
  tx = $state(0);
  connectedAt: number | null = $state(null);
  /** Whether the OS has a bond with the device; null where it cannot tell. */
  paired: boolean | null = $state(null);
  pairing = $state(false);
  /** Called with every notification, for the terminal and the tests. */
  readonly listeners = new Set<(key: string, data: number[], source: Source) => void>();
  private lineCount = 0;
  /** Subscriptions asked for and not answered yet, so none is asked twice. */
  private pending = new Set<string>();
  /** The CCCDs shown in words, read again when their characteristic is subscribed or not. */
  private shownCccds = new Set<string>();

  constructor(device: string, formats: Record<string, string>) {
    this.device = device;
    this.formats = { ...formats };
  }

  get open(): boolean {
    return this.id !== null;
  }

  get connected(): boolean {
    return this.state === "connected";
  }

  /** Every characteristic, by key. */
  characteristic(key: string | null): CharacteristicInfo | undefined {
    if (!key) return undefined;
    for (const service of this.services) {
      const found = service.characteristics.find((c) => c.key === key);
      if (found) return found;
    }
    return undefined;
  }

  /** A characteristic's name: its user description, its standard name, or its UUID. */
  nameOf(key: string | null): string {
    const characteristic = this.characteristic(key);
    if (!characteristic) return key ?? "";
    return characteristic.user_description || characteristic.name || characteristic.uuid;
  }

  /** The format a characteristic's value is shown in: as chosen, from its 0x2904, or hex. */
  formatOf(key: string): FormatChoice & { exponent: number; unit: string } {
    const characteristic = this.characteristic(key);
    const chosen = parseFormat(this.formats[key]);
    const described = characteristic?.format;
    return {
      format: chosen?.format ?? ((described?.format as Format | undefined) ?? "hex"),
      littleEndian: chosen?.littleEndian ?? true,
      exponent: chosen ? 0 : (described?.exponent ?? 0),
      unit: chosen ? "" : (described?.unit_symbol ?? ""),
    };
  }

  /** A value in words: a standard characteristic's meaning, or the chosen format. */
  describe(key: string, data: number[]): string {
    if (!this.formats[key]) {
      const known = decodeProfile(characteristicUuid(key), data, NAMES);
      if (known) return known.text;
    }
    const { format, littleEndian, exponent, unit } = this.formatOf(key);
    return show(data, format, littleEndian, exponent, unit);
  }

  /** A standard characteristic's value decoded, with its numbers by name for plotting; null for others. */
  decode(key: string, data: number[]): ReturnType<typeof decodeProfile> {
    return decodeProfile(characteristicUuid(key), data, NAMES);
  }

  /** How a characteristic's value is read, for its Valid Range and trigger descriptors. */
  private valueFormat(key: string): ValueFormat | null {
    const chosen = parseFormat(this.formats[key]);
    if (chosen && width(chosen.format) > 0) return { format: chosen.format, exponent: 0, unit: "" };
    return valueFormat(characteristicUuid(key), this.characteristic(key)?.format ?? null);
  }

  /** A descriptor's value in words, or in hex when it is not a standard one. */
  describeDescriptor(key: string, data: number[]): string {
    const characteristic = key.split("/").slice(0, 2).join("/");
    return decodeDescriptor(descriptorUuid(key), data, this.valueFormat(characteristic)) ?? hex(data);
  }

  /** The key of the first characteristic with this 16-bit UUID, or null when the device has none. */
  private standardKey(short: number): string | null {
    for (const service of this.services) {
      for (const characteristic of service.characteristics) {
        if (shortUuid(characteristicUuid(characteristic.key)) === short) return characteristic.key;
      }
    }
    return null;
  }

  /** Whether the device has any of what the device information card shows. */
  get hasInfo(): boolean {
    return DEVICE_INFO.some(({ uuid }) => this.standardKey(uuid) !== null);
  }

  /** The device information card's rows: those the device has, and that have been read. */
  get info(): { label: string; text: string }[] {
    const rows: { label: string; text: string }[] = [];
    for (const { uuid, label } of DEVICE_INFO) {
      const key = this.standardKey(uuid);
      const value = key ? this.values.get(key) : undefined;
      if (value && value.data.length > 0) rows.push({ label, text: infoText(uuid, value.data, NAMES) });
    }
    return rows;
  }

  /** Reads what the device information card shows: each one the device has and lets be read. */
  readInfo(): void {
    for (const { uuid } of DEVICE_INFO) {
      const key = this.standardKey(uuid);
      if (key && ((this.characteristic(key)?.properties ?? 0) & PROPERTY.read) !== 0) this.read(key);
    }
  }

  private addLine(line: Omit<LogLine, "n" | "time">): void {
    this.lineCount++;
    this.log.push({ ...line, n: this.lineCount, time: clock() });
    if (this.log.length > LOG_KEPT) this.log.splice(0, this.log.length - LOG_KEPT);
  }

  private take(event: DeviceEvent): void {
    switch (event.type) {
      case "state": {
        const was = this.state;
        this.state = event.state;
        this.stateText = event.text;
        if (event.state === "connected" && was !== "connected") {
          this.connectedAt = Date.now();
          void this.checkPaired();
          if (settings.device_card !== "off") this.readInfo();
        }
        if (event.state !== "connected") {
          this.connectedAt = null;
          this.phy = null;
        }
        if (event.state === "waiting" && was === "connected") {
          this.subscribed.clear();
          this.pending.clear();
        }
        if (event.state === "connected" || event.state === "waiting" || event.state === "closed") {
          this.addLine({ kind: "state", key: null, name: "Connection", text: event.text, hex: "" });
        }
        break;
      }
      case "services":
        this.services = event.services;
        break;
      case "link":
        this.link = { mtu: event.mtu, interval_ms: event.interval_ms, latency: event.latency, timeout_ms: event.timeout_ms };
        break;
      case "phy":
        this.phy = { tx: event.tx, rx: event.rx };
        break;
      case "value": {
        const value: Value = { data: event.data, source: event.source, at: Date.now() };
        this.values.set(event.key, value);
        let list = this.history.get(event.key);
        if (!list) {
          list = [];
          this.history.set(event.key, list);
        }
        list.push(value);
        if (list.length > HISTORY_KEPT) list.splice(0, list.length - HISTORY_KEPT);
        if (event.source === "write" || event.source === "write_without_response") this.tx += event.data.length;
        else this.rx += event.data.length;
        this.version++;
        for (const listener of this.listeners) listener(event.key, event.data, event.source);
        this.addLine({
          kind: event.source,
          key: event.key,
          name: this.nameOf(event.source === "descriptor" ? event.key.split("/").slice(0, 2).join("/") : event.key),
          text:
            event.source === "descriptor"
              ? this.describeDescriptor(event.key, event.data)
              : this.describe(event.key, event.data),
          hex: hex(event.data),
        });
        break;
      }
      case "subscribed":
        this.pending.delete(`${event.key}:${event.on}`);
        if (event.on) this.subscribed.add(event.key);
        else this.subscribed.delete(event.key);
        this.addLine({
          kind: "subscribed",
          key: event.key,
          name: this.nameOf(event.key),
          text: event.on ? "Notifications on" : "Notifications off",
          hex: "",
        });
        // A CCCD shown in words shows its new value.
        for (const descriptor of this.characteristic(event.key)?.descriptors ?? []) {
          if (this.shownCccds.has(descriptor.key)) this.readDescriptor(descriptor.key);
        }
        break;
      case "failed":
        if (event.key) {
          this.pending.delete(`${event.key}:true`);
          this.pending.delete(`${event.key}:false`);
        }
        this.addLine({
          kind: "error",
          key: event.key,
          name: event.key ? this.nameOf(event.key) : "Connection",
          text: `Could not ${event.action}: ${event.problem}`,
          hex: "",
        });
        break;
      case "mark":
        this.addLine({ kind: "mark", key: null, name: "Connection", text: event.text, hex: "" });
        break;
    }
  }

  /** Starts looking for the device and connecting, and keeps it connected. */
  async connect(): Promise<void> {
    if (this.id !== null) return;
    const channel = new Channel<DeviceEvent>();
    channel.onmessage = (event) => {
      if (channel === this.channel) this.take(event);
    };
    this.channel = channel;
    try {
      this.state = "searching";
      this.id = await invoke<number>("device_open", { device: this.device, events: channel });
    } catch (error) {
      this.state = "closed";
      this.stateText = String(error);
      this.addLine({ kind: "error", key: null, name: "Bluetooth", text: String(error), hex: "" });
    }
  }

  async disconnect(): Promise<void> {
    const id = this.id;
    this.id = null;
    if (id !== null) await invoke("device_close", { id }).catch(() => {});
    this.channel = null;
    this.state = "closed";
    this.stateText = "Not connected";
    this.connectedAt = null;
    this.phy = null;
    this.subscribed.clear();
    this.pending.clear();
  }

  send(command: Command): void {
    if (this.id !== null) void invoke("device_command", { id: this.id, command });
  }

  read(key: string): void {
    this.send({ type: "read", key });
  }

  write(key: string, data: number[], withResponse: boolean): void {
    this.send({ type: "write", key, data, with_response: withResponse });
  }

  /** Writes in pieces that fit the MTU, as a terminal's typing or a pasted text needs. */
  writeChunked(key: string, data: number[], withResponse: boolean): void {
    const size = Math.max(20, (this.link?.mtu ?? 23) - 3);
    for (let at = 0; at < data.length; at += size) this.write(key, data.slice(at, at + size), withResponse);
  }

  subscribe(key: string, on: boolean): void {
    const pendingKey = `${key}:${on}`;
    if (this.pending.has(pendingKey) || (on && this.subscribed.has(key))) return;
    this.pending.add(pendingKey);
    this.send({ type: "subscribe", key, on });
  }

  readDescriptor(key: string): void {
    if (isCccd(key)) this.shownCccds.add(key);
    this.send({ type: "read_descriptor", key });
  }

  /** Turns notifications or indications on or off from a CCCD's row, which then shows its new value. */
  configure(cccd: string, on: boolean): void {
    this.shownCccds.add(cccd);
    this.subscribe(cccd.split("/").slice(0, 2).join("/"), on);
  }

  async checkPaired(): Promise<void> {
    this.paired = await invoke<boolean | null>("device_paired", { device: this.device }).catch(() => null);
  }

  /** Pairs and bonds through the OS, which asks for a passkey itself when one is needed. */
  async pair(on: boolean): Promise<void> {
    this.pairing = true;
    try {
      const text = await invoke<string>(on ? "device_pair" : "device_unpair", { device: this.device });
      this.addLine({ kind: "state", key: null, name: on ? "Pairing" : "Unpairing", text, hex: "" });
    } catch (error) {
      this.addLine({ kind: "error", key: null, name: on ? "Pairing" : "Unpairing", text: String(error), hex: "" });
    } finally {
      this.pairing = false;
      await this.checkPaired();
    }
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
