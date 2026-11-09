<script lang="ts">
  import { onMount } from "svelte";
  import { save } from "@tauri-apps/plugin-dialog";
  import Chart, { align } from "./Chart.svelte";
  import { TERMINAL_PRESETS, addressKindText, advKindText, type DeviceDetails, type DeviceRow } from "./bluetooth";
  import { changed, openDevice, type Tab } from "./tabs.svelte";

  let { tab }: { tab: Tab } = $props();
  const scanner = $derived(tab.scanner!);

  let now = $state(Date.now());
  let details: DeviceDetails | null = $state(null);
  let csvProblem = $state("");

  onMount(() => {
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });

  // The selected device's details, read again every second while it is selected.
  $effect(() => {
    const id = scanner.selected;
    void now;
    if (!id) {
      details = null;
      return;
    }
    void scanner.details(id).then((found) => {
      if (scanner.selected === id) details = found;
    });
  });

  const rows = $derived.by(() => {
    void scanner.version;
    void scanner.devices.size;
    return scanner.shown();
  });

  /** "now", "4 s", "2 min" since a device was last heard. */
  function seen(row: DeviceRow): string {
    const seconds = Math.max(0, (now - scanner.startedAt - row.last_ms) / 1000);
    if (seconds < 2) return "now";
    if (seconds < 120) return `${Math.round(seconds)} s`;
    return `${Math.round(seconds / 60)} min`;
  }

  /** The RSSI as a share of the bar: -100 dBm is empty, -30 dBm full. */
  function bar(rssi: number | null): number {
    if (rssi === null) return 0;
    return Math.min(100, Math.max(0, ((rssi + 100) / 70) * 100));
  }

  function info(row: DeviceRow): string {
    return row.beacon ?? row.companies[0] ?? row.services[0] ?? row.appearance ?? "";
  }

  function hasTerminal(row: DeviceRow): boolean {
    return row.service_uuids.some((uuid) =>
      TERMINAL_PRESETS.some((preset) => preset.service.toUpperCase().includes(uuid.replace("0x", "").toUpperCase())),
    );
  }

  function interval(row: DeviceRow): string {
    if (row.interval_ms === null) return "";
    return row.interval_ms >= 1000 ? `${(row.interval_ms / 1000).toFixed(2)} s` : `${Math.round(row.interval_ms)} ms`;
  }

  function filterChanged() {
    changed();
  }

  async function exportCsv() {
    csvProblem = "";
    const path = await save({
      title: "Save the device list as CSV",
      defaultPath: `${tab.name.replace(/[<>:"/\\|?*]/g, "_")}.csv`,
      filters: [{ name: "CSV", extensions: ["csv"] }],
    });
    if (path) await scanner.saveCsv(path).catch((error) => (csvProblem = String(error)));
  }

  function connect(row: DeviceRow, kind: "device" | "terminal") {
    openDevice(row.id, row.name ?? row.address ?? row.id, kind);
  }

  /** The devices the plot shows: the favourites, or else the selected one. */
  const plotted = $derived.by(() => {
    void scanner.version;
    const ids = scanner.favourites.length > 0 ? scanner.favourites.slice(0, 8) : scanner.selected ? [scanner.selected] : [];
    return ids.filter((id) => scanner.rssi.has(id));
  });

  const plotData = $derived.by(() => {
    void scanner.version;
    return align(
      plotted.map((id) => {
        const series = scanner.rssi.get(id)!;
        return { at: series.at.map((ms) => ms / 1000), values: series.dbm };
      }),
    );
  });

  const plotLabels = $derived(
    plotted.map((id) => {
      const row = scanner.devices.get(id);
      return row?.name ? `${row.name} ${row.address ?? ""}`.trim() : (row?.address ?? id);
    }),
  );
</script>

<div class="scanner">
  <div class="toolbar">
    {#if scanner.running}
      <button class="btn small" onclick={() => tab.stop()} title="Stop hearing adverts; the list stays">Stop</button>
    {:else}
      <button class="btn small primary" onclick={() => tab.start()}>Scan</button>
    {/if}
    <button class="btn small" onclick={() => scanner.clear()} title="Forget every device heard">Clear</button>
    <span class="separator"></span>
    <input
      class="field small search"
      placeholder="Filter: name, address, company, service"
      bind:value={scanner.filter.text}
      oninput={filterChanged}
      spellcheck="false"
    />
    <label class="option" title="Hide devices weaker than this">
      RSSI ≥
      <input
        type="range"
        min="-100"
        max="-30"
        step="1"
        bind:value={scanner.filter.min_rssi}
        onchange={filterChanged}
      />
      <span class="mono number">{scanner.filter.min_rssi}</span>
    </label>
    <label class="check"><input type="checkbox" bind:checked={scanner.filter.only_named} onchange={filterChanged} /> Named</label>
    <label class="check"
      ><input type="checkbox" bind:checked={scanner.filter.only_connectable} onchange={filterChanged} /> Connectable</label
    >
    <label class="check"
      ><input type="checkbox" bind:checked={scanner.filter.only_favourites} onchange={filterChanged} /> ★ only</label
    >
    <label class="option">
      Sort
      <select class="field small" bind:value={scanner.filter.sort} onchange={filterChanged}>
        <option value="rssi">RSSI</option>
        <option value="name">Name</option>
        <option value="last">Last heard</option>
        <option value="first">First heard</option>
      </select>
    </label>
    <span class="spacer"></span>
    <button class="btn small" onclick={exportCsv} title="Save every device and its adverts as CSV">CSV...</button>
    <div class="views" role="group" aria-label="View">
      <button class="view" class:on={tab.view !== "plot"} onclick={() => tab.setView("list")}>List</button>
      <button
        class="view"
        class:on={tab.view === "plot"}
        onclick={() => tab.setView("plot")}
        title="RSSI over time of the favourites, or of the selected device">RSSI plot</button
      >
    </div>
  </div>

  {#if scanner.problem}
    <p class="problem">{scanner.problem}</p>
  {/if}
  {#if csvProblem}
    <p class="problem">{csvProblem}</p>
  {/if}

  {#if tab.view === "plot"}
    <div class="plot">
      {#if plotted.length === 0}
        <p class="empty">Star devices with ★, or select one, to plot their RSSI.</p>
      {:else}
        <Chart labels={plotLabels} data={plotData} version={scanner.version} unit="dBm" />
      {/if}
    </div>
  {:else}
    <div class="body">
      <div class="list" role="grid" aria-label="Devices">
        <div class="row head" role="row">
          <span></span>
          <span>Device</span>
          <span>RSSI</span>
          <span title="How often it advertises, as heard">Interval</span>
          <span>Adverts</span>
          <span>What it says</span>
          <span>Heard</span>
        </div>
        {#each rows as row (row.id)}
          {@const favourite = scanner.favourites.includes(row.id)}
          <div
            class="row"
            class:selected={scanner.selected === row.id}
            class:gone={now - scanner.startedAt - row.last_ms > 30_000}
            role="row"
            tabindex="-1"
            onmousedown={() => (scanner.selected = row.id)}
            ondblclick={() => row.connectable !== false && connect(row, "device")}
          >
            <button
              class="star"
              class:on={favourite}
              title={favourite ? "Take it off the favourites" : "Keep it at the top, and plot it"}
              onmousedown={(event) => event.stopPropagation()}
              onclick={() => {
                scanner.toggleFavourite(row.id);
                changed();
              }}>{favourite ? "★" : "☆"}</button
            >
            <span class="device">
              <span class="name">{row.name ?? "(no name)"}</span>
              <span class="address mono">{row.address ?? row.id}{row.connectable ? "" : " · not connectable"}</span>
            </span>
            <span class="rssi">
              <span class="mono">{row.rssi ?? ""}</span>
              <span class="meter"><span style:width={`${bar(row.rssi)}%`}></span></span>
            </span>
            <span class="mono">{interval(row)}</span>
            <span class="mono">{row.count}</span>
            <span class="info">{info(row)}</span>
            <span class="muted">{seen(row)}</span>
          </div>
        {:else}
          <p class="empty">
            {scanner.running ? "Listening for adverts..." : "Press Scan to hear the devices around."}
          </p>
        {/each}
      </div>

      <aside class="details">
        {#if details}
          {@const row = details.row}
          <h2>{row.name ?? "(no name)"}</h2>
          <p class="mono address">{row.address ?? row.id}</p>
          <div class="actions">
            <button class="btn small primary" disabled={row.connectable === false} onclick={() => connect(row, "device")}
              >Connect</button
            >
            <button
              class="btn small"
              disabled={row.connectable === false}
              title={hasTerminal(row) ? "It has a UART-like service" : "Pick its characteristics once connected"}
              onclick={() => connect(row, "terminal")}>Terminal</button
            >
          </div>
          <dl>
            {#if row.address_kind}<dt>Address</dt><dd>{addressKindText(row.address_kind)}</dd>{/if}
            {#if row.kind}<dt>PDU</dt><dd>{advKindText(row.kind)}</dd>{/if}
            <dt>RSSI</dt><dd>{row.rssi ?? "?"} dBm</dd>
            {#if row.tx_power !== null}<dt>TX power</dt><dd>{row.tx_power} dBm</dd>{/if}
            {#if row.interval_ms !== null}<dt>Interval</dt><dd>≈ {interval(row)} heard</dd>{/if}
            {#if row.advertised_interval_ms !== null}<dt>Says</dt><dd>{row.advertised_interval_ms} ms</dd>{/if}
            <dt>Adverts</dt><dd>{row.count}</dd>
            {#if row.appearance}<dt>Appearance</dt><dd>{row.appearance}</dd>{/if}
          </dl>
          {#if details.beacon}
            <h3>{details.beacon.kind}</h3>
            <dl>
              {#each details.beacon.fields as [name, value] (name)}
                <dt>{name}</dt>
                <dd class="mono">{value}</dd>
              {/each}
            </dl>
          {/if}
          <h3>Advert</h3>
          {#each details.advert as field, at (at)}
            <div class="adfield">
              <span class="type mono">0x{field.ad_type.toString(16).toUpperCase().padStart(2, "0")}</span>
              <span class="what">{field.name}</span>
              <span class="value">{field.value}</span>
            </div>
          {:else}
            <p class="muted">Nothing in it.</p>
          {/each}
          <p class="raw mono" title="The advert's bytes">{details.advert_hex}</p>
          {#if details.response.length > 0}
            <h3>Scan response</h3>
            {#each details.response as field, at (at)}
              <div class="adfield">
                <span class="type mono">0x{field.ad_type.toString(16).toUpperCase().padStart(2, "0")}</span>
                <span class="what">{field.name}</span>
                <span class="value">{field.value}</span>
              </div>
            {/each}
            <p class="raw mono">{details.response_hex}</p>
          {/if}
          {#if !row.raw}
            <p class="muted note">This OS gives only what it read from the adverts, so the bytes are made again from that.</p>
          {/if}
        {:else}
          <p class="empty">Select a device to see its adverts in words and in bytes.</p>
        {/if}
      </aside>
    </div>
  {/if}
</div>

<style>
  .scanner {
    display: flex;
    flex-direction: column;
    height: 100%;
    font-size: var(--view-size);
  }

  .toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 10px;
    padding: 6px 10px;
    font-size: 13px;
    border-bottom: 1px solid var(--border);
  }

  .small {
    height: 26px;
    padding: 0 10px;
    font-size: 12px;
  }

  .field.small {
    padding: 0 6px;
  }

  .search {
    width: 240px;
  }

  .separator {
    width: 1px;
    height: 20px;
    background: var(--border);
  }

  .spacer {
    flex: 1;
  }

  .option,
  .check {
    display: flex;
    align-items: center;
    gap: 5px;
    color: var(--muted);
  }

  .check:has(input:checked) {
    color: var(--text);
  }

  .number {
    min-width: 3ch;
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

  .problem {
    margin: 8px 10px 0;
    color: var(--bad);
  }

  .body {
    display: flex;
    flex: 1;
    min-height: 0;
  }

  .list {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
  }

  .row {
    display: grid;
    grid-template-columns: 28px minmax(160px, 1.4fr) 110px 70px 64px minmax(120px, 1.6fr) 52px;
    align-items: center;
    gap: 8px;
    min-height: 40px;
    padding: 0 10px;
    border-bottom: 1px solid var(--border);
  }

  .row.head {
    position: sticky;
    top: 0;
    z-index: 1;
    min-height: 28px;
    font-size: 11px;
    font-weight: 600;
    color: var(--muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    background: var(--bg);
  }

  .row:not(.head):hover {
    background: var(--hover);
  }

  .row.selected {
    background: var(--accent-soft);
  }

  .row.gone {
    opacity: 0.55;
  }

  .star {
    font-size: 15px;
    color: var(--muted);
    background: none;
    border: 0;
  }

  .star.on {
    color: var(--accent-2);
  }

  .device {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .name,
  .address,
  .info {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .address {
    font-size: 11px;
    color: var(--muted);
  }

  .rssi {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .rssi .mono {
    min-width: 3.5ch;
    text-align: right;
  }

  .meter {
    flex: 1;
    height: 6px;
    overflow: hidden;
    background: var(--border);
    border-radius: 3px;
  }

  .meter span {
    display: block;
    height: 100%;
    background: var(--accent);
  }

  .muted {
    color: var(--muted);
  }

  .empty {
    padding: 24px;
    color: var(--muted);
    text-align: center;
  }

  .details {
    width: clamp(260px, 32%, 400px);
    flex-shrink: 0;
    padding: 12px 14px;
    overflow-y: auto;
    border-left: 1px solid var(--border);
    user-select: text;
  }

  .details h2 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
  }

  .details h3 {
    margin: 16px 0 6px;
    font-size: 11px;
    font-weight: 600;
    color: var(--muted);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .details .address {
    margin: 2px 0 10px;
    font-size: 12px;
  }

  .actions {
    display: flex;
    gap: 8px;
  }

  dl {
    display: grid;
    grid-template-columns: 96px 1fr;
    gap: 4px 10px;
    margin: 12px 0 0;
  }

  dt {
    color: var(--muted);
  }

  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .adfield {
    display: grid;
    grid-template-columns: 40px 1fr;
    gap: 2px 8px;
    padding: 5px 0;
    border-bottom: 1px solid var(--border);
  }

  .type {
    font-size: 11px;
    color: var(--accent);
  }

  .what {
    color: var(--muted);
  }

  .value {
    grid-column: 2;
    overflow-wrap: anywhere;
  }

  .raw {
    margin: 6px 0 0;
    font-size: 11px;
    color: var(--muted);
    overflow-wrap: anywhere;
  }

  .note {
    margin-top: 12px;
    font-size: 12px;
  }

  /* A narrow window: the details under the list. */
  @media (max-width: 760px) {
    .body {
      flex-direction: column;
    }

    .details {
      width: auto;
      max-height: 45%;
      border-top: 1px solid var(--border);
      border-left: 0;
    }

    .row {
      grid-template-columns: 24px minmax(120px, 1fr) 90px 56px;
    }

    .row > :nth-child(n + 5) {
      display: none;
    }
  }

  .plot {
    flex: 1;
    min-height: 0;
    padding: 10px 14px;
  }
</style>
