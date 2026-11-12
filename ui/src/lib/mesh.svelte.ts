// The Mesh tab's session, as the UI keeps it: the network and its nodes, the
// devices heard, the proxy, the device being provisioned, and the log of the
// messages sent and received. The types have the same fields as in
// blenima-core's mesh/session.rs.

import { Channel, invoke } from "@tauri-apps/api/core";
import { clock, type LogLine } from "./device.svelte";
import { hex } from "./format";
import { addressText, answerOf, describeMessage, parseComposition, splitOpcode, type Bytes, type Composition } from "./mesh";

/** How many lines the log keeps. */
const LOG_KEPT = 5000;
/** How long an answer is waited for. */
const ANSWER_WAIT = 10_000;

export type KeyRef = { type: "device" } | { type: "app"; index: number };
export type AppKey = { index: number; key: string };

export type MeshNode = {
  address: number;
  elements: number;
  device_key: string;
  uuid: string;
  name: string;
  /** Composition Data page 0 as hex, once it was read. */
  composition: string;
};

export type NetworkInfo = {
  file: string;
  net_key: string;
  net_key_index: number;
  app_keys: AppKey[];
  iv_index: number;
  iv_update: boolean;
  address: number;
  ttl: number;
  nodes: MeshNode[];
};

export type Unprovisioned = { id: string; name: string | null; rssi: number | null; uuid: string; oob: number };
export type ProxyOf = { type: "network" } | { type: "node"; address: number };
export type ProxyRow = { id: string; name: string | null; rssi: number | null; of: ProxyOf };
export type ProxyState = "off" | "looking" | "connecting" | "connected";

export type Capabilities = {
  elements: number;
  algorithms: number;
  public_key_type: number;
  oob_type: number;
  output_size: number;
  output_actions: number;
  input_size: number;
  input_actions: number;
};

export type Algorithm = "cmac" | "hmac_sha256";

export type AuthMethod =
  | { method: "no_oob" }
  | { method: "static" }
  | { method: "output"; action: number; size: number }
  | { method: "input"; action: number; size: number };

type MeshEvent =
  | { type: "network"; network: NetworkInfo }
  | { type: "heard"; unprovisioned: Unprovisioned[]; proxies: ProxyRow[] }
  | { type: "proxy"; state: ProxyState; id: string | null; text: string }
  | { type: "capabilities"; capabilities: Capabilities; algorithm: Algorithm; address: number }
  | { type: "ask_value"; method: AuthMethod }
  | { type: "show_value"; method: AuthMethod; value: string }
  | { type: "provisioned"; address: number; elements: number }
  | { type: "provision_failed"; problem: string }
  | { type: "sent"; id: number; dst: number; key: KeyRef; payload: Bytes; at_ms: number }
  | { type: "received"; src: number; dst: number; key: KeyRef; ttl: number; payload: Bytes; at_ms: number }
  | { type: "acked"; id: number; at_ms: number }
  | { type: "not_acked"; id: number; problem: string; at_ms: number }
  | { type: "log"; text: string; at_ms: number }
  | { type: "failed"; action: string; problem: string; at_ms: number };

type MeshCommand =
  | { type: "provision"; id: string; attention: number }
  | { type: "method"; method: AuthMethod }
  | { type: "value"; value: string }
  | { type: "cancel" }
  | { type: "connect"; id: string | null }
  | { type: "disconnect" }
  | { type: "send"; dst: number; key: KeyRef; ttl: number | null; payload: Bytes }
  | { type: "rename"; address: number; name: string }
  | { type: "forget"; address: number };

/** Where provisioning a device is. */
export type Provisioning = {
  id: string;
  name: string;
  step: "connecting" | "method" | "value" | "show" | "working" | "done" | "failed";
  capabilities: Capabilities | null;
  algorithm: Algorithm | null;
  /** The address the device gets. */
  address: number | null;
  method: AuthMethod | null;
  /** The value for the user to put into the device, for Input OOB. */
  value: string;
  problem: string;
};

/** A message waited for: the answer to one bleNIMA sent. */
type Waiting = {
  from: number;
  opcode: number;
  resolve: (params: Bytes) => void;
  reject: (problem: Error) => void;
  timer: ReturnType<typeof setTimeout>;
};

/** "device key", or "AppKey 0". */
export function keyText(key: KeyRef): string {
  return key.type === "device" ? "device key" : `AppKey ${key.index}`;
}

export class MeshState {
  private id: number | null = $state(null);
  private channel: Channel<MeshEvent> | null = null;
  network: NetworkInfo | null = $state(null);
  unprovisioned: Unprovisioned[] = $state([]);
  proxies: ProxyRow[] = $state([]);
  proxyState: ProxyState = $state("off");
  proxyId: string | null = $state(null);
  proxyText = $state("Not open");
  provisioning: Provisioning | null = $state(null);
  log: LogLine[] = $state([]);
  /** The last problem, for the status bar. */
  problem = $state("");
  /** Each node's composition, read from its hex once: a plain cache, as it is filled while the page is drawn. */
  private readonly compositions = new Map<string, Composition | null>();
  private waiting: Waiting[] = [];
  private lineCount = 0;
  private tidCount = 0;

  get open(): boolean {
    return this.id !== null;
  }

  get connected(): boolean {
    return this.proxyState === "connected";
  }

  /** A Transaction Identifier for the next Set message: one more each time. */
  tid(): number {
    this.tidCount = (this.tidCount + 1) & 0xff;
    return this.tidCount;
  }

  node(address: number): MeshNode | undefined {
    return this.network?.nodes.find((node) => address >= node.address && address < node.address + Math.max(1, node.elements));
  }

  composition(node: MeshNode): Composition | null {
    if (!node.composition) return null;
    let parsed = this.compositions.get(node.composition);
    if (parsed === undefined) {
      const bytes = node.composition.match(/../g)?.map((pair) => parseInt(pair, 16)) ?? [];
      parsed = parseComposition(bytes);
      this.compositions.set(node.composition, parsed);
    }
    return parsed;
  }

  /** What the state is, in words, for the status bar. */
  get stateText(): string {
    if (!this.open) return "Not open";
    if (this.provisioning && !["done", "failed"].includes(this.provisioning.step)) {
      return `Provisioning ${this.provisioning.name || this.provisioning.id}`;
    }
    const nodes = this.network ? `${this.network.nodes.length} nodes` : "";
    return [this.proxyText, nodes].filter(Boolean).join(", ");
  }

  async start(): Promise<void> {
    if (this.id !== null) return;
    const channel = new Channel<MeshEvent>();
    channel.onmessage = (event) => {
      if (channel === this.channel) this.take(event);
    };
    this.channel = channel;
    this.proxyText = "Opening the network";
    try {
      this.id = await invoke<number>("mesh_open", { events: channel });
      this.problem = "";
    } catch (error) {
      this.channel = null;
      this.problem = String(error);
      this.proxyText = String(error);
      this.addLine({ kind: "error", key: null, name: "Bluetooth", text: String(error), hex: "" });
    }
  }

  async stop(): Promise<void> {
    const id = this.id;
    this.id = null;
    this.channel = null;
    for (const waiting of this.waiting.splice(0)) {
      clearTimeout(waiting.timer);
      waiting.reject(new Error("The Mesh tab was closed"));
    }
    if (id !== null) await invoke("mesh_close", { id }).catch(() => {});
    this.proxyState = "off";
    this.proxyId = null;
    this.proxyText = "Not open";
    this.provisioning = null;
    this.unprovisioned = [];
    this.proxies = [];
  }

  private command(command: MeshCommand): void {
    if (this.id === null) return;
    invoke("mesh_command", { id: this.id, command }).catch((error) => this.failed("send a command", String(error)));
  }

  provision(device: Unprovisioned, attention = 5): void {
    this.provisioning = {
      id: device.id,
      name: device.name ?? "",
      step: "connecting",
      capabilities: null,
      algorithm: null,
      address: null,
      method: null,
      value: "",
      problem: "",
    };
    this.command({ type: "provision", id: device.id, attention });
  }

  chooseMethod(method: AuthMethod): void {
    if (!this.provisioning) return;
    this.provisioning.method = method;
    this.provisioning.step = "working";
    this.command({ type: "method", method });
  }

  giveValue(value: string): void {
    if (!this.provisioning) return;
    this.provisioning.step = "working";
    this.command({ type: "value", value });
  }

  /** Stops provisioning, or puts away what it ended with. */
  closeProvisioning(): void {
    if (this.provisioning && !["done", "failed"].includes(this.provisioning.step)) this.command({ type: "cancel" });
    this.provisioning = null;
  }

  connect(id: string | null = null): void {
    this.command({ type: "connect", id });
  }

  disconnect(): void {
    this.command({ type: "disconnect" });
  }

  rename(address: number, name: string): void {
    this.command({ type: "rename", address, name });
  }

  forget(address: number): void {
    this.command({ type: "forget", address });
  }

  /** Sends an access message: its opcode and parameters. */
  send(dst: number, key: KeyRef, payload: Bytes): void {
    this.command({ type: "send", dst, key, ttl: null, payload });
  }

  /**
   * Sends a message and waits for its answer from `dst`: the answer's
   * parameters. A message no answer comes to resolves at once, with none.
   */
  request(dst: number, key: KeyRef, payload: Bytes): Promise<Bytes | null> {
    const split = splitOpcode(payload);
    const answer = split ? answerOf(split.opcode) : null;
    this.send(dst, key, payload);
    if (answer === null) return Promise.resolve(null);
    return new Promise((resolve, reject) => {
      const waiting: Waiting = {
        from: dst,
        opcode: answer,
        resolve,
        reject,
        timer: setTimeout(() => {
          this.waiting = this.waiting.filter((other) => other !== waiting);
          reject(new Error(`No answer from ${addressText(dst)}`));
        }, ANSWER_WAIT),
      };
      this.waiting.push(waiting);
    });
  }

  clearLog(): void {
    this.log = [];
  }

  /** The log as text, a line each. */
  logText(): string {
    return this.log.map((line) => `${line.time}  ${line.kind.padEnd(8)}  ${line.name}  ${line.text}  ${line.hex}`).join("\n");
  }

  private addLine(line: Omit<LogLine, "n" | "time">): void {
    this.lineCount++;
    this.log.push({ ...line, n: this.lineCount, time: clock() });
    if (this.log.length > LOG_KEPT) this.log.splice(0, this.log.length - LOG_KEPT);
  }

  private failed(action: string, problem: string): void {
    this.problem = `Could not ${action}: ${problem}`;
    this.addLine({ kind: "error", key: null, name: action, text: problem, hex: "" });
  }

  private take(event: MeshEvent): void {
    switch (event.type) {
      case "network":
        this.network = event.network;
        break;
      case "heard":
        this.unprovisioned = event.unprovisioned;
        this.proxies = event.proxies;
        break;
      case "proxy": {
        const was = this.proxyState;
        this.proxyState = event.state;
        this.proxyId = event.id;
        this.proxyText = event.text;
        if (event.state !== was && (event.state === "connected" || was === "connected")) {
          this.addLine({ kind: "state", key: null, name: "Proxy", text: event.text, hex: "" });
        }
        if (event.state !== "connected") {
          for (const waiting of this.waiting.splice(0)) {
            clearTimeout(waiting.timer);
            waiting.reject(new Error("The proxy was disconnected"));
          }
        }
        break;
      }
      case "capabilities":
        if (this.provisioning) {
          this.provisioning.capabilities = event.capabilities;
          this.provisioning.algorithm = event.algorithm;
          this.provisioning.address = event.address;
          this.provisioning.step = "method";
        }
        break;
      case "ask_value":
        if (this.provisioning) {
          this.provisioning.method = event.method;
          this.provisioning.step = "value";
        }
        break;
      case "show_value":
        if (this.provisioning) {
          this.provisioning.method = event.method;
          this.provisioning.value = event.value;
          this.provisioning.step = "show";
        }
        break;
      case "provisioned":
        if (this.provisioning) {
          this.provisioning.address = event.address;
          this.provisioning.step = "done";
        }
        this.addLine({
          kind: "state",
          key: null,
          name: "Provisioning",
          text: `Node ${addressText(event.address)} joined the network, with ${event.elements} element${event.elements === 1 ? "" : "s"}`,
          hex: "",
        });
        break;
      case "provision_failed":
        if (this.provisioning) {
          this.provisioning.problem = event.problem;
          this.provisioning.step = "failed";
        }
        this.addLine({ kind: "error", key: null, name: "Provisioning", text: event.problem, hex: "" });
        break;
      case "sent": {
        const { name, text } = describeMessage(event.payload);
        this.addLine({
          kind: "sent",
          key: null,
          name,
          text: `to ${addressText(event.dst)}, ${keyText(event.key)}${text ? `: ${text}` : ""}`,
          hex: hex(event.payload),
        });
        break;
      }
      case "received": {
        const { name, text } = describeMessage(event.payload);
        this.addLine({
          kind: "received",
          key: null,
          name,
          text: `from ${addressText(event.src)}${event.dst === this.network?.address ? "" : ` to ${addressText(event.dst)}`}, ${keyText(event.key)}${text ? `: ${text}` : ""}`,
          hex: hex(event.payload),
        });
        const split = splitOpcode(event.payload);
        if (split) {
          const found = this.waiting.find((waiting) => waiting.from === event.src && waiting.opcode === split.opcode);
          if (found) {
            clearTimeout(found.timer);
            this.waiting = this.waiting.filter((waiting) => waiting !== found);
            found.resolve(split.params);
          }
        }
        break;
      }
      case "acked":
        this.addLine({ kind: "acked", key: null, name: "Acknowledged", text: "Every segment arrived", hex: "" });
        break;
      case "not_acked":
        this.addLine({ kind: "error", key: null, name: "Not acknowledged", text: event.problem, hex: "" });
        break;
      case "log":
        this.addLine({ kind: "mark", key: null, name: "Mesh", text: event.text, hex: "" });
        break;
      case "failed":
        this.failed(event.action, event.problem);
        break;
    }
  }
}
