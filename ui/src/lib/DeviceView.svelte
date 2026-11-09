<script lang="ts">
  import { onMount } from "svelte";
  import { save } from "@tauri-apps/plugin-dialog";
  import { invoke } from "@tauri-apps/api/core";
  import DeviceCard from "./DeviceCard.svelte";
  import GattView from "./GattView.svelte";
  import LogView from "./LogView.svelte";
  import TerminalView from "./TerminalView.svelte";
  import ToolsView from "./ToolsView.svelte";
  import { radioInfo, type RadioInfo } from "./bluetooth";
  import { settings } from "./settings.svelte";
  import type { Tab } from "./tabs.svelte";

  let { tab, active }: { tab: Tab; active: boolean } = $props();
  const device = $derived(tab.device!);

  let radio: RadioInfo | null = $state(null);
  onMount(() => {
    radioInfo()
      .then((info) => (radio = info))
      .catch(() => {});
  });

  const VIEWS = [
    { view: "gatt", label: "GATT", title: "Services and characteristics: read, write, notify, plot" },
    { view: "terminal", label: "Terminal", title: "The device as a terminal: NUS or any write and notify pair" },
    { view: "log", label: "Log", title: "Everything that happened, with times" },
    { view: "tools", label: "Tests", title: "Throughput, latency, soak and send-and-expect tests" },
  ];

  function busy(): boolean {
    return device.state === "searching" || device.state === "connecting" || device.state === "discovering";
  }

  async function saveLog() {
    const path = await save({
      title: "Save the log",
      defaultPath: `${tab.name.replace(/[<>:"/\\|?*]/g, "_")}.log`,
      filters: [{ name: "Text", extensions: ["log", "txt"] }],
    });
    if (path) await invoke("save_text", { path, text: device.logText() }).catch((error) => console.error(error));
  }
</script>

<div class="device">
  <div class="toolbar">
    <span class="state {device.state}">
      <span class="dot"></span>
      {device.stateText}
    </span>
    {#if device.connected && device.link}
      <span class="separator"></span>
      <span class="fact" title="The ATT MTU the OS agreed with the device">MTU <b class="mono">{device.link.mtu}</b></span>
      {#if device.link.interval_ms !== null}
        <span class="fact" title="Connection interval, latency and supervision timeout">
          <b class="mono">{device.link.interval_ms} ms</b> / <b class="mono">{device.link.latency}</b> /
          <b class="mono">{device.link.timeout_ms} ms</b>
        </span>
      {/if}
      {#if radio?.connection_parameters}
        <label class="option" title="Ask the device for connection parameters, as one of the presets the OS offers">
          Parameters
          <select
            class="field small"
            onchange={(event) =>
              device.send({
                type: "parameters",
                preset: (event.currentTarget as HTMLSelectElement).value as "balanced" | "throughput" | "power",
              })}
          >
            <option value="balanced">Balanced</option>
            <option value="throughput">Throughput</option>
            <option value="power">Low power</option>
          </select>
        </label>
      {/if}
    {/if}
    <span class="spacer"></span>
    {#if radio?.pairing && device.connected}
      {#if device.paired}
        <span class="chip" title="The OS has a bond with this device: keys are kept">Bonded</span>
      {/if}
      <button
        class="btn small"
        disabled={device.pairing}
        title={device.paired ? "Take the bond away, so it pairs anew next time" : "Pair and bond; Windows asks for a passkey if the device needs one"}
        onclick={() => device.pair(!device.paired)}>{device.pairing ? "Pairing..." : device.paired ? "Unpair" : "Pair"}</button
      >
    {/if}
    {#if device.open}
      <button class="btn small" onclick={() => tab.stop()}>Disconnect</button>
    {:else}
      <button class="btn small primary" onclick={() => tab.start()}>Connect</button>
    {/if}
    <button class="btn small" onclick={saveLog} title="Save everything in the log to a file">Save log...</button>
    <div class="views" role="group" aria-label="View">
      {#each VIEWS as { view, label, title } (view)}
        <button class="view" class:on={tab.view === view} {title} onclick={() => tab.setView(view)}>{label}</button>
      {/each}
    </div>
  </div>

  {#if settings.device_card === "above_all"}
    <DeviceCard {device} />
  {/if}

  <div class="body">
    {#if busy() && device.services.length === 0}
      <p class="empty"><span class="spinner"></span>{device.stateText}</p>
    {/if}
    <div class="pane" class:hidden={tab.view !== "gatt"}>
      <GattView {device} />
    </div>
    <!-- The terminal stays while another view is shown, so nothing it showed is lost. -->
    <div class="pane" class:hidden={tab.view !== "terminal"}>
      <TerminalView {tab} active={active && tab.view === "terminal"} />
    </div>
    {#if tab.view === "log"}
      <div class="pane"><LogView source={device} /></div>
    {/if}
    {#if tab.view === "tools"}
      <div class="pane"><ToolsView {tab} /></div>
    {/if}
  </div>
</div>

<style>
  .device {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 12px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border);
  }

  .state {
    display: flex;
    align-items: center;
    gap: 7px;
    max-width: 420px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .dot {
    flex-shrink: 0;
    width: 8px;
    height: 8px;
    border: 1.5px solid var(--muted);
    border-radius: 50%;
  }

  .connected .dot {
    background: var(--ok);
    border-color: var(--ok);
  }

  .searching .dot,
  .connecting .dot,
  .discovering .dot,
  .waiting .dot {
    background: var(--warn);
    border-color: var(--warn);
    animation: pulse 1.4s ease-in-out infinite;
  }

  @keyframes pulse {
    50% {
      opacity: 0.35;
    }
  }

  .fact {
    color: var(--muted);
  }

  .chip {
    padding: 0 8px;
    font-size: 11px;
    font-weight: 600;
    line-height: 20px;
    color: var(--ok);
    border: 1px solid var(--ok);
    border-radius: 4px;
  }

  .fact b {
    font-weight: 500;
    color: var(--text);
  }

  .separator {
    width: 1px;
    height: 20px;
    background: var(--border);
  }

  .spacer {
    flex: 1;
  }

  .small {
    height: 26px;
    padding: 0 10px;
    font-size: 12px;
  }

  .option {
    display: flex;
    align-items: center;
    gap: 5px;
    color: var(--muted);
  }

  .views {
    display: flex;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: 6px;
  }

  .view {
    height: 24px;
    padding: 0 9px;
    font: inherit;
    font-size: 12px;
    color: var(--muted);
    background: none;
    border: 0;
  }

  .view + .view {
    border-left: 1px solid var(--border);
  }

  .view.on {
    color: var(--on-accent);
    background: var(--accent);
  }

  .body {
    position: relative;
    flex: 1;
    min-height: 0;
  }

  .pane {
    position: absolute;
    inset: 0;
  }

  .pane.hidden {
    display: none;
  }

  .empty {
    position: absolute;
    top: 40%;
    left: 0;
    right: 0;
    z-index: 2;
    display: flex;
    justify-content: center;
    gap: 10px;
    color: var(--muted);
    pointer-events: none;
  }

  .spinner {
    width: 14px;
    height: 14px;
    border: 2px solid var(--border);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
