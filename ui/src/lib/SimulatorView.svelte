<script lang="ts">
  import { onMount } from "svelte";
  import { save } from "@tauri-apps/plugin-dialog";
  import { invoke } from "@tauri-apps/api/core";
  import AdvertiserView from "./AdvertiserView.svelte";
  import LogView from "./LogView.svelte";
  import ServerView from "./ServerView.svelte";
  import { radioInfo, type RadioInfo } from "./bluetooth";
  import { loadNames } from "./server.svelte";
  import { settings } from "./settings.svelte";
  import type { Tab } from "./tabs.svelte";

  let { tab }: { tab: Tab } = $props();
  const server = $derived(tab.server!);
  /** One part at a time, all on one page, or in two columns: chosen in Settings. */
  const layout = $derived(settings.simulator.layout);

  let radio: RadioInfo | null = $state(null);
  let radioProblem = $state("");
  onMount(() => {
    loadNames();
    radioInfo()
      .then((info) => (radio = info))
      .catch((error) => (radioProblem = String(error)));
  });

  /** Why the GATT server cannot run here, in words; "" when it can. */
  const off = $derived.by(() => {
    const info = radio as RadioInfo | null;
    if (radioProblem) return radioProblem;
    if (info && !info.peripheral) return info.peripheral_problem ?? "This Bluetooth adapter cannot be a peripheral";
    return "";
  });

  const VIEWS = [
    { view: "advertiser", label: "Advertiser", title: "bleNIMA's advert: iBeacon, Eddystone, manufacturer data" },
    { view: "server", label: "GATT server", title: "Services and characteristics a phone or a central connects to" },
    { view: "log", label: "Log", title: "Every read, write, subscribe and notification, with times" },
  ];

  /** Whether a part is on screen: each one in its view, or all of them. */
  function shown(view: string): boolean {
    return layout !== "views" || tab.view === view;
  }

  async function saveLog() {
    const path = await save({
      title: "Save the log",
      defaultPath: `${tab.name.replace(/[<>:"/\\|?*]/g, "_")}.log`,
      filters: [{ name: "Text", extensions: ["log", "txt"] }],
    });
    if (path) await invoke("save_text", { path, text: server.logText() }).catch((error) => console.error(error));
  }
</script>

<div class="simulator layout-{layout}">
  <div class="toolbar">
    <span class="state" class:on={server.running} class:busy={server.starting} class:bad={!server.running && !!server.problem}>
      <span class="dot"></span>
      {server.stateText}
    </span>
    <span class="spacer"></span>
    {#if server.running}
      <button class="btn small" onclick={() => tab.stop()}>Stop server</button>
    {:else}
      <button
        class="btn small primary"
        disabled={server.starting || off !== "" || server.services.length === 0}
        title={off || (server.services.length === 0 ? "Add a service first" : "Serve the services and advertise them")}
        onclick={() => tab.start()}>Start server</button
      >
    {/if}
    <button class="btn small" onclick={saveLog} title="Save everything in the log to a file">Save log...</button>
    {#if layout === "views"}
      <div class="views" role="group" aria-label="View">
        {#each VIEWS as { view, label, title } (view)}
          <button class="view" class:on={tab.view === view} {title} onclick={() => tab.setView(view)}>{label}</button>
        {/each}
      </div>
    {/if}
  </div>

  <!-- The same parts in every layout, so a layout change does not stop the advert. -->
  <div class="body">
    <div class="left">
      <section class="part advertiser" class:hidden={!shown("advertiser")}>
        <AdvertiserView />
      </section>
      <section class="part server" class:hidden={!shown("server")}>
        <ServerView {server} {radio} {off} />
      </section>
    </div>
    <section class="part log" class:hidden={!shown("log")}>
      <LogView source={server} />
    </section>
  </div>
</div>

<style>
  .simulator {
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
    max-width: 520px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .state.bad {
    color: var(--bad);
  }

  .dot {
    flex-shrink: 0;
    width: 8px;
    height: 8px;
    border: 1.5px solid var(--muted);
    border-radius: 50%;
  }

  .on .dot {
    background: var(--ok);
    border-color: var(--ok);
  }

  .busy .dot {
    background: var(--warn);
    border-color: var(--warn);
    animation: pulse 1.4s ease-in-out infinite;
  }

  @keyframes pulse {
    50% {
      opacity: 0.35;
    }
  }

  .spacer {
    flex: 1;
  }

  .small {
    height: 26px;
    padding: 0 10px;
    font-size: 12px;
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
    flex: 1;
    min-height: 0;
  }

  .part.hidden {
    display: none;
  }

  /* One part at a time, filling the tab. */
  .layout-views .body {
    display: flex;
    flex-direction: column;
  }

  .layout-views .left {
    display: contents;
  }

  .layout-views .part {
    flex: 1;
    min-height: 0;
  }

  /* All on one page, one under the other. */
  .layout-page .body {
    overflow-y: auto;
  }

  .layout-page .server {
    height: 560px;
    border-top: 1px solid var(--border);
  }

  .layout-page .log {
    height: 360px;
    border-top: 1px solid var(--border);
  }

  /* The advertiser and the GATT server on the left, the log on the right. */
  .layout-columns .body {
    display: flex;
  }

  .layout-columns .left {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
  }

  .layout-columns .server {
    height: 560px;
    border-top: 1px solid var(--border);
  }

  .layout-columns .log {
    width: 40%;
    min-width: 320px;
    border-left: 1px solid var(--border);
  }
</style>
