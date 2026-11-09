<script lang="ts">
  import Chart, { align } from "./Chart.svelte";
  import DeviceCard from "./DeviceCard.svelte";
  import StandardWrite from "./StandardWrite.svelte";
  import { PROPERTY, type CharacteristicInfo } from "./bluetooth";
  import { isCccd } from "./descriptors";
  import { formatText, type DeviceState } from "./device.svelte";
  import { FORMATS, encodeNumber, hex, numbers, parseHex, parseText, width, type Format } from "./format";
  import { characteristicUuid, isUartText, shortUuid } from "./profiles";
  import { settings } from "./settings.svelte";
  import { changed } from "./tabs.svelte";
  import { FORMS } from "./writes";

  let { device }: { device: DeviceState } = $props();

  let input = $state("");
  let mode: "text" | "hex" | "number" = $state("text");
  let problem = $state("");
  let showPlot = $state(false);

  const selected = $derived(device.characteristic(device.selected));
  const has = (c: CharacteristicInfo | undefined, bit: number) => c !== undefined && (c.properties & bit) !== 0;

  // The Nordic UART Service takes text: writing to it starts in Text.
  $effect(() => {
    if (selected && isUartText(characteristicUuid(selected.key))) mode = "text";
  });

  const BADGES: [number, string, string][] = [
    [PROPERTY.read, "R", "read"],
    [PROPERTY.write, "W", "write"],
    [PROPERTY.writeWithoutResponse, "WnR", "write without response"],
    [PROPERTY.notify, "N", "notify"],
    [PROPERTY.indicate, "I", "indicate"],
  ];

  function preview(key: string): string {
    const value = device.values.get(key);
    return value ? device.describe(key, value.data) : "";
  }

  function setFormat(key: string, format: string, littleEndian: boolean) {
    if (format === "auto") delete device.formats[key];
    else device.formats[key] = formatText({ format: format as Format, littleEndian });
    device.version++;
    changed();
  }

  function bytes(): number[] | null {
    problem = "";
    if (mode === "hex") {
      const parsed = parseHex(input);
      if (typeof parsed === "string") return ((problem = parsed), null);
      return parsed;
    }
    if (mode === "number" && selected) {
      const { format, littleEndian } = device.formatOf(selected.key);
      const parsed = encodeNumber(input, format, littleEndian);
      if (typeof parsed === "string") return ((problem = parsed), null);
      return parsed;
    }
    return parseText(input);
  }

  function write(withResponse: boolean) {
    const data = bytes();
    if (data && selected) device.write(selected.key, data, withResponse);
  }

  /** The values plotted: a standard characteristic's numbers by name, or the chosen format's. */
  const plot = $derived.by(() => {
    void device.version;
    if (!selected || !showPlot) return null;
    const history = device.history.get(selected.key) ?? [];
    const lines = new Map<string, { at: number[]; values: number[] }>();
    const start = history[0]?.at ?? 0;
    const { format, littleEndian, exponent } = device.formatOf(selected.key);
    for (const value of history) {
      const known = device.formats[selected.key] ? null : device.decode(selected.key, value.data);
      const named: [string, number][] = known
        ? Object.entries(known.values)
        : numbers(value.data, format, littleEndian, exponent).map((v, at) => [`value ${at + 1}`, v]);
      for (const [name, number] of named.slice(0, 8)) {
        let line = lines.get(name);
        if (!line) lines.set(name, (line = { at: [], values: [] }));
        line.at.push((value.at - start) / 1000);
        line.values.push(number);
      }
    }
    return { labels: [...lines.keys()], data: align([...lines.values()]) };
  });

  const recent = $derived.by(() => {
    void device.version;
    if (!selected) return [];
    return (device.history.get(selected.key) ?? []).slice(-60).reverse();
  });

  function time(at: number): string {
    const date = new Date(at);
    const two = (n: number) => String(n).padStart(2, "0");
    return `${two(date.getHours())}:${two(date.getMinutes())}:${two(date.getSeconds())}.${String(date.getMilliseconds()).padStart(3, "0")}`;
  }
</script>

<div class="gatt-view">
  {#if settings.device_card === "above_gatt" || settings.device_card === "strip"}
    <DeviceCard {device} strip={settings.device_card === "strip"} />
  {/if}
  <div class="gatt">
    <div class="tree" role="tree" aria-label="GATT table">
      {#each device.services as service (service.key)}
        <div class="service" role="treeitem" aria-selected="false" aria-expanded="true">
          <span class="title">{service.name ?? "Unknown service"}</span>
          <span class="uuid mono">{service.uuid}</span>
        </div>
        {#each service.characteristics as characteristic (characteristic.key)}
          <button
            class="characteristic"
            class:selected={device.selected === characteristic.key}
            role="treeitem"
            aria-selected={device.selected === characteristic.key}
            onclick={() => (device.selected = characteristic.key)}
          >
            <span class="line">
              <span class="title">{characteristic.user_description || characteristic.name || "Unknown characteristic"}</span>
              <span class="badges">
                {#each BADGES as [bit, label, title] (label)}
                  {#if characteristic.properties & bit}<span class="badge" {title}>{label}</span>{/if}
                {/each}
                {#if device.subscribed.has(characteristic.key)}<span class="badge on" title="Notifications on">●</span>{/if}
              </span>
            </span>
            <span class="uuid mono">{characteristic.uuid}</span>
            {#if device.values.has(characteristic.key)}
              <span class="preview">{preview(characteristic.key)}</span>
            {/if}
          </button>
        {/each}
      {:else}
        <p class="empty">{device.connected ? "The device has no services." : "Its services show once it is connected."}</p>
      {/each}
    </div>

    <div class="panel">
      {#if selected}
        {@const choice = device.formatOf(selected.key)}
        {@const value = device.values.get(selected.key)}
        <h2>{selected.user_description || selected.name || "Unknown characteristic"}</h2>
        <p class="uuid mono">{selected.uuid}</p>
        <p class="muted">{selected.property_names.join(", ")}</p>

        <div class="value">
          <div class="shown">{value ? device.describe(selected.key, value.data) : "No value read yet"}</div>
          {#if value}
            <div class="hex mono">{hex(value.data)} · {value.data.length} bytes · {value.source.replace(/_/g, " ")} at {time(value.at)}</div>
          {/if}
        </div>

        <div class="actions">
          <button class="btn small" disabled={!device.connected || !has(selected, PROPERTY.read)} onclick={() => device.read(selected.key)}
            >Read</button
          >
          {#if has(selected, PROPERTY.notify) || has(selected, PROPERTY.indicate)}
            {@const on = device.subscribed.has(selected.key)}
            <button
              class="btn small"
              class:primary={on}
              disabled={!device.connected}
              onclick={() => device.subscribe(selected.key, !on)}
              >{on ? "Stop" : has(selected, PROPERTY.notify) ? "Notify" : "Indicate"}</button
            >
          {/if}
          <label class="option">
            Show as
            <select
              class="field small"
              value={device.formats[selected.key]?.split(":")[0] ?? "auto"}
              onchange={(event) => setFormat(selected.key, (event.currentTarget as HTMLSelectElement).value, choice.littleEndian)}
            >
              <option value="auto">Automatic{selected.format ? ` (${selected.format.format})` : ""}</option>
              {#each FORMATS as { format, label } (format)}
                <option value={format}>{label}</option>
              {/each}
            </select>
          </label>
          {#if width(choice.format) > 1}
            <label class="check">
              <input
                type="checkbox"
                checked={!choice.littleEndian}
                onchange={(event) => setFormat(selected.key, choice.format, !(event.currentTarget as HTMLInputElement).checked)}
              /> Big-endian
            </label>
          {/if}
          <label class="check"><input type="checkbox" bind:checked={showPlot} /> Plot</label>
        </div>

        {#if has(selected, PROPERTY.write) || has(selected, PROPERTY.writeWithoutResponse)}
          {@const short = shortUuid(characteristicUuid(selected.key))}
          {#if short !== null && FORMS.has(short)}
            {#key selected.key}
              <StandardWrite {device} characteristic={selected} />
            {/key}
          {/if}
          <div class="write">
            <select class="field small" bind:value={mode} title="How what is typed becomes bytes">
              <option value="text">Text</option>
              <option value="hex">Hex</option>
              <option value="number" disabled={width(choice.format) === 0}>Number ({choice.format})</option>
            </select>
            <input
              class="field small mono"
              placeholder={mode === "hex" ? "AA 55 01" : mode === "text" ? "text, \\r \\n \\x03" : "42"}
              bind:value={input}
              onkeydown={(event) => event.key === "Enter" && write(has(selected, PROPERTY.write))}
              spellcheck="false"
            />
            {#if has(selected, PROPERTY.write)}
              <button class="btn small primary" disabled={!device.connected} onclick={() => write(true)}>Write</button>
            {/if}
            {#if has(selected, PROPERTY.writeWithoutResponse)}
              <button class="btn small" disabled={!device.connected} onclick={() => write(false)} title="Write without response"
                >Write, no response</button
              >
            {/if}
          </div>
          {#if problem}<p class="problem">{problem}</p>{/if}
        {/if}

        {#if selected.descriptors.length > 0}
          <h3>Descriptors</h3>
          {#each selected.descriptors as descriptor (descriptor.key)}
            {@const read = device.values.get(descriptor.key)}
            <div class="descriptor">
              <span>{descriptor.name ?? "Unknown descriptor"} <span class="uuid mono">{descriptor.uuid}</span></span>
              <span class="said" title={read ? hex(read.data) : ""}>{read ? device.describeDescriptor(descriptor.key, read.data) : ""}</span>
              <span class="buttons">
                {#if isCccd(descriptor.key) && (has(selected, PROPERTY.notify) || has(selected, PROPERTY.indicate))}
                  {@const on = device.subscribed.has(selected.key)}
                  <button class="btn tiny" disabled={!device.connected} onclick={() => device.configure(descriptor.key, !on)}
                    >{on ? "Stop" : has(selected, PROPERTY.notify) ? "Notify" : "Indicate"}</button
                  >
                {/if}
                <button class="btn tiny" disabled={!device.connected} onclick={() => device.readDescriptor(descriptor.key)}>Read</button>
              </span>
            </div>
          {/each}
        {/if}

        {#if plot}
          <div class="chart">
            {#if plot.labels.length > 0}
              <Chart labels={plot.labels} data={plot.data} version={device.version} unit={choice.unit} />
            {:else}
              <p class="muted">No numbers to plot yet: pick a number format, and read or notify.</p>
            {/if}
          </div>
        {/if}

        {#if recent.length > 0}
          <h3>Values</h3>
          <div class="history">
            {#each recent as entry, at (at)}
              <div class="entry">
                <span class="muted mono">{time(entry.at)}</span>
                <span class="kind">{entry.source.replace(/_/g, " ")}</span>
                <span>{device.describe(selected.key, entry.data)}</span>
              </div>
            {/each}
          </div>
        {/if}
      {:else}
        <p class="empty">Select a characteristic to read, write or subscribe to it.</p>
      {/if}
    </div>
  </div>
</div>

<style>
  .gatt-view {
    display: flex;
    flex-direction: column;
    height: 100%;
    font-size: var(--view-size);
  }

  .gatt {
    display: flex;
    flex: 1;
    min-height: 0;
  }

  .tree {
    width: 380px;
    flex-shrink: 0;
    padding: 6px 0;
    overflow-y: auto;
    border-right: 1px solid var(--border);
  }

  .service {
    display: flex;
    flex-direction: column;
    padding: 10px 12px 4px;
  }

  .service .title {
    font-weight: 600;
  }

  .characteristic {
    display: flex;
    flex-direction: column;
    gap: 1px;
    width: 100%;
    padding: 6px 12px 6px 24px;
    font: inherit;
    color: var(--text);
    text-align: left;
    background: none;
    border: 0;
  }

  .characteristic:hover {
    background: var(--hover);
  }

  .characteristic.selected {
    background: var(--accent-soft);
  }

  .line {
    display: flex;
    justify-content: space-between;
    gap: 8px;
  }

  .badges {
    display: flex;
    gap: 3px;
  }

  .badge {
    padding: 0 4px;
    font-size: 10px;
    line-height: 16px;
    color: var(--muted);
    border: 1px solid var(--border);
    border-radius: 3px;
  }

  .badge.on {
    color: var(--ok);
    border-color: var(--ok);
  }

  .uuid {
    font-size: 11px;
    color: var(--muted);
    overflow-wrap: anywhere;
  }

  .preview {
    overflow: hidden;
    font-size: 12px;
    color: var(--accent);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .panel {
    flex: 1;
    min-width: 0;
    padding: 14px 18px;
    overflow-y: auto;
    user-select: text;
  }

  h2 {
    margin: 0;
    font-size: 17px;
    font-weight: 600;
  }

  h3 {
    margin: 18px 0 6px;
    font-size: 11px;
    font-weight: 600;
    color: var(--muted);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .muted {
    color: var(--muted);
  }

  .value {
    margin: 12px 0;
    padding: 10px 12px;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 8px;
  }

  .shown {
    font-size: 1.25em;
    overflow-wrap: anywhere;
  }

  .hex {
    margin-top: 4px;
    font-size: 12px;
    color: var(--muted);
    overflow-wrap: anywhere;
  }

  .actions,
  .write {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-top: 8px;
  }

  .write input {
    flex: 1;
    min-width: 160px;
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

  .option,
  .check {
    display: flex;
    align-items: center;
    gap: 5px;
    color: var(--muted);
  }

  .problem {
    margin: 6px 0 0;
    color: var(--bad);
  }

  .descriptor {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1.4fr) auto;
    align-items: center;
    gap: 10px;
    padding: 4px 0;
    border-bottom: 1px solid var(--border);
  }

  .said {
    overflow-wrap: anywhere;
  }

  .buttons {
    display: flex;
    gap: 6px;
  }

  .chart {
    height: 260px;
    margin-top: 14px;
  }

  .history {
    max-height: 280px;
    overflow-y: auto;
  }

  .entry {
    display: grid;
    grid-template-columns: 96px 90px 1fr;
    gap: 8px;
    padding: 3px 0;
    border-bottom: 1px solid var(--border);
    overflow-wrap: anywhere;
  }

  .kind {
    color: var(--muted);
  }

  .empty {
    padding: 24px;
    color: var(--muted);
    text-align: center;
  }
</style>
