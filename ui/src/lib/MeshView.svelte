<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { save } from "@tauri-apps/plugin-dialog";
  import LogView from "./LogView.svelte";
  import MeshNode from "./MeshNode.svelte";
  import MeshProvision from "./MeshProvision.svelte";
  import { addressText, oobInfoText } from "./mesh";
  import type { ProxyRow } from "./mesh.svelte";
  import type { Tab } from "./tabs.svelte";

  // The Mesh tab: the network and its nodes, the devices waiting to be
  // provisioned, the proxies heard, and the log of messages.
  let { tab }: { tab: Tab } = $props();
  const mesh = $derived(tab.mesh!);

  const VIEWS = [
    { view: "network", label: "Network", title: "The nodes, the devices waiting to be provisioned, and the keys" },
    { view: "log", label: "Log", title: "Every message sent and received, with times" },
  ];

  /** What is shown on the right: the network's keys, or a node. */
  let selected: "network" | number = $state("network");
  const node = $derived(typeof selected === "number" ? mesh.node(selected) : undefined);
  let copied = $state("");

  function proxyOf(proxy: ProxyRow): string {
    return proxy.of.type === "network" ? "the network" : `node ${addressText(proxy.of.address)}`;
  }

  async function copy(text: string, what: string) {
    await navigator.clipboard.writeText(text).catch(() => {});
    copied = what;
    setTimeout(() => (copied = ""), 1500);
  }

  async function saveLog() {
    const path = await save({
      title: "Save the log",
      defaultPath: "mesh.log",
      filters: [{ name: "Text", extensions: ["log", "txt"] }],
    });
    if (path) await invoke("save_text", { path, text: mesh.logText() }).catch((error) => console.error(error));
  }

  const rssi = (value: number | null) => (value === null ? "" : `${value} dBm`);
</script>

<div class="mesh">
  <div class="toolbar">
    <span class="state {mesh.proxyState}" title={mesh.proxyText}>
      <span class="dot"></span>
      {mesh.open ? mesh.proxyText : "Not open"}
    </span>
    {#if mesh.network}
      <span class="separator"></span>
      <span class="fact" title="The IV index the network is at, as its proxy's beacon says">
        IV index <b class="mono">{mesh.network.iv_index}</b>{#if mesh.network.iv_update}&nbsp;(updating){/if}
      </span>
      <span class="fact" title="bleNIMA's own address in the network, as the provisioner and the client">
        bleNIMA <b class="mono">{addressText(mesh.network.address)}</b>
      </span>
    {/if}
    <span class="spacer"></span>
    {#if mesh.open}
      {#if mesh.proxyState === "off"}
        <button class="btn small primary" onclick={() => mesh.connect()}>Connect to a proxy</button>
      {:else}
        <button class="btn small" onclick={() => mesh.disconnect()}>Disconnect</button>
      {/if}
      <button class="btn small" onclick={() => tab.stop()}>Close the network</button>
    {:else}
      <button class="btn small primary" onclick={() => tab.start()}>Open the network</button>
    {/if}
    <button class="btn small" onclick={saveLog} title="Save everything in the log to a file">Save log...</button>
    <div class="views" role="group" aria-label="View">
      {#each VIEWS as { view, label, title } (view)}
        <button class="view" class:on={tab.view === view} {title} onclick={() => tab.setView(view)}>{label}</button>
      {/each}
    </div>
  </div>

  <div class="body">
    {#if tab.view === "log"}
      <div class="pane"><LogView source={mesh} /></div>
    {:else}
      <div class="pane split">
        <aside>
          <button class="item" class:on={selected === "network"} onclick={() => (selected = "network")}>
            <b>Network</b>
            <span class="muted">keys, IV index, {mesh.network?.nodes.length ?? 0} nodes</span>
          </button>

          <h3>Nodes</h3>
          {#each mesh.network?.nodes ?? [] as each (each.address)}
            <button class="item" class:on={selected === each.address} onclick={() => (selected = each.address)}>
              <span><b class="mono">{addressText(each.address)}</b> {each.name}</span>
              <span class="muted">
                {each.elements} element{each.elements === 1 ? "" : "s"}{#if mesh.proxies.some((p) => p.of.type === "node" && p.of.address === each.address)}, heard as a proxy{/if}
              </span>
            </button>
          {:else}
            <p class="hint">None yet: provision a device below.</p>
          {/each}

          <h3>Waiting to be provisioned</h3>
          {#each mesh.unprovisioned as device (device.id)}
            <div class="row-item">
              <span class="grow">
                <b>{device.name || device.id}</b>
                <span class="muted mono uuid" title="Device UUID">{device.uuid}</span>
                <span class="muted">{rssi(device.rssi)}{#if device.oob}, OOB: {oobInfoText(device.oob)}{/if}</span>
              </span>
              <button class="btn tiny primary" disabled={mesh.provisioning !== null} onclick={() => mesh.provision(device)}
                >Provision</button
              >
            </div>
          {:else}
            <p class="hint">
              {mesh.open ? "None heard. A device shows here while it advertises the Mesh Provisioning Service." : "Open the network to listen."}
            </p>
          {/each}

          <h3>Proxies</h3>
          {#each mesh.proxies as proxy (proxy.id)}
            <div class="row-item">
              <span class="grow">
                <b>{proxy.name || proxy.id}</b>
                <span class="muted">of {proxyOf(proxy)}, {rssi(proxy.rssi)}</span>
              </span>
              {#if mesh.proxyId === proxy.id && mesh.proxyState === "connected"}
                <span class="chip">Connected</span>
              {:else}
                <button class="btn tiny" onclick={() => mesh.connect(proxy.id)}>Connect</button>
              {/if}
            </div>
          {:else}
            <p class="hint">None of this network's heard yet.</p>
          {/each}
        </aside>

        <section>
          {#if node}
            {#key node.address}
              <MeshNode {mesh} {node} />
            {/key}
          {:else if mesh.network}
            {@const network = mesh.network}
            <h2>Network</h2>
            <p class="muted">
              bleNIMA keeps this network in <span class="mono">{network.file}</span>. Keep that file to yourself: its keys let
              anyone into the network.
            </p>
            <dl>
              <dt>Network key</dt>
              <dd>
                <span class="mono">{network.net_key}</span>
                <button class="btn tiny" onclick={() => copy(network.net_key, "net")}>{copied === "net" ? "Copied" : "Copy"}</button>
                <span class="muted">index {network.net_key_index}</span>
              </dd>
              {#each network.app_keys as key (key.index)}
                <dt>AppKey {key.index}</dt>
                <dd>
                  <span class="mono">{key.key}</span>
                  <button class="btn tiny" onclick={() => copy(key.key, `app${key.index}`)}>{copied === `app${key.index}` ? "Copied" : "Copy"}</button>
                </dd>
              {/each}
              <dt>IV index</dt>
              <dd class="mono">{network.iv_index}{network.iv_update ? ", in an IV update" : ""}</dd>
              <dt>bleNIMA</dt>
              <dd><span class="mono">{addressText(network.address)}</span> <span class="muted">sends with TTL {network.ttl}</span></dd>
              <dt>Nodes</dt>
              <dd>{network.nodes.length}</dd>
            </dl>
            <p class="muted">
              To start, provision a device on the left: bleNIMA then connects to a proxy of the network by itself. On the node's
              page, read its composition, give it an application key, and bind its models to the key.
            </p>
          {:else}
            <p class="hint">{mesh.open ? "Opening the network..." : "The network is closed."}</p>
          {/if}
        </section>
      </div>
    {/if}
  </div>
</div>

{#if mesh.provisioning}
  <MeshProvision {mesh} provisioning={mesh.provisioning} />
{/if}

<style>
  .mesh {
    display: flex;
    flex-direction: column;
    height: 100%;
    font-size: var(--view-size);
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

  .looking .dot,
  .connecting .dot {
    background: var(--warn);
    border-color: var(--warn);
  }

  .fact {
    color: var(--muted);
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

  .tiny {
    height: 22px;
    padding: 0 8px;
    font-size: 11px;
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

  .split {
    display: grid;
    grid-template-columns: clamp(220px, 28%, 300px) minmax(0, 1fr);
  }

  /* In a narrow window the lists go above the page. */
  @media (max-width: 760px) {
    .split {
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: minmax(0, 40%) minmax(0, 1fr);
    }

    aside {
      border-right: 0;
      border-bottom: 1px solid var(--border);
    }
  }

  aside {
    padding: 8px;
    overflow-y: auto;
    border-right: 1px solid var(--border);
  }

  section {
    padding: 14px 18px;
    overflow-y: auto;
    user-select: text;
  }

  h2 {
    margin: 0 0 6px;
    font-size: 17px;
    font-weight: 600;
  }

  h3 {
    margin: 16px 4px 6px;
    font-size: 11px;
    font-weight: 600;
    color: var(--muted);
    text-transform: uppercase;
  }

  .item {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
    padding: 6px 8px;
    font: inherit;
    color: var(--text);
    text-align: left;
    background: none;
    border: 0;
    border-radius: 6px;
    cursor: pointer;
  }

  .item:hover {
    background: var(--hover);
  }

  .item.on {
    background: var(--selected, var(--hover));
    box-shadow: inset 2px 0 0 var(--accent);
  }

  .row-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
  }

  .grow {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .uuid {
    overflow: hidden;
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .muted {
    color: var(--muted);
  }

  .hint {
    margin: 4px 8px;
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

  dl {
    display: grid;
    grid-template-columns: 110px 1fr;
    gap: 6px 12px;
    margin: 12px 0;
  }

  dt {
    color: var(--muted);
  }

  dd {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin: 0;
    overflow-wrap: anywhere;
  }

  p {
    margin: 0 0 8px;
  }
</style>
