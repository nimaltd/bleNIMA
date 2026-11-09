<script lang="ts">
  import { untrack } from "svelte";
  import type { RadioInfo } from "./bluetooth";
  import { parseFormat } from "./device.svelte";
  import { FORMATS, encodeNumber, numbers, parseHex, parseText } from "./format";
  import { PRESETS, PROPERTIES, UART_PRESETS, fullUuid, hasLive, hexText, sends, uuidText, type Property } from "./server";
  import { TIMER_MIN, standardList, uuidOfName, type ServerState } from "./server.svelte";
  import { changed } from "./tabs.svelte";

  let { server, radio, off }: { server: ServerState; radio: RadioInfo | null; off: string } = $props();

  /** Each tab's own lists of names, so their ids do not meet. */
  const uid = $props.id();

  const selected = $derived(server.selected);
  const service = $derived(selected ? server.services[selected.service] : undefined);
  const characteristic = $derived(
    selected && selected.characteristic !== null ? service?.characteristics[selected.characteristic] : undefined,
  );

  let preset = $state("");
  /** The value being typed, and what is wrong with it. */
  let draft = $state("");
  let problem = $state("");
  /** The UUID or name being typed, and what is wrong with it. */
  let uuidDraft = $state("");
  let uuidProblem = $state("");

  /** A value as it is typed in a format: hex, text with escapes, or a number. */
  function typed(bytes: number[], format: string): string {
    const choice = parseFormat(format);
    if (!choice || choice.format === "hex") return hexText(bytes);
    if (choice.format === "utf8") {
      return new TextDecoder()
        .decode(Uint8Array.from(bytes))
        .replace(/\\/g, "\\\\")
        .replace(/\r/g, "\\r")
        .replace(/\n/g, "\\n")
        .replace(/\t/g, "\\t")
        .replace(/[\x00-\x1f]/g, (c) => `\\x${c.charCodeAt(0).toString(16).padStart(2, "0")}`);
    }
    return String(numbers(bytes, choice.format, choice.littleEndian)[0] ?? "");
  }

  /** What is typed as bytes, in the characteristic's format; or what is wrong, in words. */
  function bytesOf(input: string, format: string): number[] | string {
    const choice = parseFormat(format);
    if (!choice || choice.format === "hex") return parseHex(input);
    if (choice.format === "utf8") return parseText(input);
    return encodeNumber(input, choice.format, choice.littleEndian);
  }

  // When another service or characteristic is picked, the inputs show its own.
  $effect(() => {
    void selected?.service;
    void selected?.characteristic;
    untrack(() => {
      problem = "";
      uuidProblem = "";
      if (selected && selected.characteristic !== null && characteristic) {
        draft = typed(server.valueOf(selected.service, selected.characteristic), characteristic.format);
        uuidDraft = characteristic.uuid;
      } else if (service) {
        uuidDraft = service.uuid;
      }
    });
  });

  function pick(service: number, characteristic: number | null) {
    server.selected = { service, characteristic };
  }

  function addPreset() {
    if (!preset) return;
    server.addPreset(preset);
    preset = "";
    changed();
  }

  async function apply(notify: boolean) {
    if (!selected || selected.characteristic === null || !characteristic) return;
    const data = bytesOf(draft, characteristic.format);
    if (typeof data === "string") {
      problem = data;
      return;
    }
    if (data.length > 512) {
      problem = `A value is 512 bytes at most; this one has ${data.length}`;
      return;
    }
    problem = "";
    await server.setValue(selected.service, selected.characteristic, data, notify);
    changed();
  }

  function setFormat(format: string) {
    if (!selected || selected.characteristic === null || !characteristic) return;
    characteristic.format = format;
    draft = typed(server.valueOf(selected.service, selected.characteristic), format);
    problem = "";
    changed();
  }

  /** Takes the UUID typed, or the UUID of the standard name typed. */
  function applyUuid(ofCharacteristic: boolean) {
    const target = ofCharacteristic ? characteristic : service;
    if (!target) return;
    const text = uuidDraft.trim();
    const named = uuidOfName(text, ofCharacteristic);
    if (!named && !fullUuid(text)) {
      uuidProblem = "Not a UUID nor a standard name: type 180F, all 32 hex digits, or pick a name from the list";
      return;
    }
    uuidProblem = "";
    target.uuid = named ? uuidText(named).replace(/^0x/, "") : text;
    uuidDraft = target.uuid;
    changed();
  }

  function toggle(property: Property, on: boolean) {
    if (!characteristic) return;
    const kept = characteristic.properties.filter((p) => p !== property);
    if (on) kept.push(property);
    characteristic.properties = PROPERTIES.map((entry) => entry.property).filter((p) => kept.includes(p));
    changed();
  }

  function setTimer(input: string) {
    if (!selected || selected.characteristic === null || !characteristic) return;
    const ms = Math.round(Number(input));
    characteristic.timer_ms = Number.isFinite(ms) && ms > 0 ? Math.max(TIMER_MIN, ms) : 0;
    server.startTimer(selected.service, selected.characteristic);
    changed();
  }
</script>

<div class="server">
  <div class="head">
    <h2>GATT server</h2>
    <p class="muted">
      Services that a phone app or a central's firmware connects to. While the server runs, Windows advertises each service
      as connectable, apart from the Advertiser's advert.
    </p>
    {#if off}
      <p class="bad">{off}</p>
    {:else if radio?.peripheral}
      <p class="muted">This Bluetooth adapter can be a peripheral.</p>
    {/if}
  </div>

  <fieldset class="split" class:off={off !== ""} disabled={off !== ""}>
    <div class="side">
      <div class="tree" role="tree" aria-label="GATT server">
        {#each server.services as entry, s (s)}
          {@const advert = server.running ? server.adverts[s] : undefined}
          <button
            class="service"
            class:selected={selected?.service === s && selected.characteristic === null}
            role="treeitem"
            aria-selected={selected?.service === s && selected.characteristic === null}
            onclick={() => pick(s, null)}
          >
            <span class="line">
              <span class="title">{server.serviceName(s)}</span>
              {#if advert}<span class="badge" class:on={advert.on} title={advert.text}>{advert.on ? "Advertised" : "Not advertised"}</span>{/if}
            </span>
            <span class="uuid mono">{uuidText(entry.uuid)}</span>
          </button>
          {#each entry.characteristics as item, c (c)}
            {@const clients = server.subscribers.get(`${s}/${c}`) ?? []}
            <button
              class="characteristic"
              class:selected={selected?.service === s && selected.characteristic === c}
              role="treeitem"
              aria-selected={selected?.service === s && selected.characteristic === c}
              onclick={() => pick(s, c)}
            >
              <span class="line">
                <span class="title">{server.characteristicName(s, c)}</span>
                <span class="badges">
                  {#each PROPERTIES as { property, badge, label } (property)}
                    {#if item.properties.includes(property)}<span class="badge" title={label}>{badge}</span>{/if}
                  {/each}
                  {#if clients.length > 0}<span class="badge on" title={`Subscribed: ${clients.join(", ")}`}>● {clients.length}</span>{/if}
                </span>
              </span>
              <span class="uuid mono">{uuidText(item.uuid)}</span>
              <span class="preview">
                {server.describe(s, c, server.valueOf(s, c))}{#if item.timer_ms > 0}<span class="every">{` · every ${item.timer_ms} ms`}</span>{/if}
              </span>
            </button>
          {/each}
        {:else}
          <p class="empty">No services yet: add a preset, or a service by hand.</p>
        {/each}
      </div>
      <div class="add">
        <select class="field small" bind:value={preset} disabled={server.running} onchange={addPreset} title="Add a preset's services">
          <option value="">Add a preset...</option>
          {#each PRESETS as entry (entry.id)}
            <option value={entry.id} title={entry.detail}>{entry.name}</option>
          {/each}
        </select>
        <button
          class="btn small"
          disabled={server.running}
          onclick={() => {
            server.addService();
            changed();
          }}>Add a service</button
        >
      </div>
    </div>

    <div class="panel">
      {#if selected && selected.characteristic !== null && characteristic}
        {@const s = selected.service}
        {@const c = selected.characteristic}
        {@const value = server.valueOf(s, c)}
        {@const clients = server.subscribers.get(`${s}/${c}`) ?? []}
        <h2>{server.characteristicName(s, c)}</h2>
        <p class="uuid mono">{uuidText(characteristic.uuid)} in {server.serviceName(s)}</p>

        <div class="value">
          <div class="shown">{server.describe(s, c, value)}</div>
          <div class="hex mono">
            {hexText(value) || "No bytes"} · {value.length} bytes{server.running ? "" : " · the value it starts with"}
          </div>
        </div>

        <div class="write">
          <select class="field small" value={characteristic.format} onchange={(event) => setFormat(event.currentTarget.value)} title="How the value is typed and shown">
            <option value="">Hex, read out</option>
            {#each FORMATS as { format, label } (format)}
              <option value={format}>{label}</option>
            {/each}
          </select>
          <input
            class="field small mono"
            bind:value={draft}
            spellcheck="false"
            placeholder="The value"
            onkeydown={(event) => event.key === "Enter" && void apply(false)}
          />
          <button class="btn small" onclick={() => apply(false)} title="The value reads get, and the one the server starts with">Set</button>
          {#if sends(characteristic)}
            <button
              class="btn small primary"
              disabled={!server.running}
              title={server.running ? "Set it, and send it to every client subscribed" : "Start the server first"}
              onclick={() => apply(true)}>{characteristic.properties.includes("notify") ? "Notify" : "Indicate"}</button
            >
          {/if}
        </div>
        {#if problem}<p class="problem">{problem}</p>{/if}

        <h3>Timer</h3>
        <div class="row">
          <label class="option">
            {sends(characteristic) ? "Notify every" : "Change every"}
            <input
              class="field small mono timer"
              type="number"
              min="0"
              step="100"
              value={characteristic.timer_ms}
              onchange={(event) => setTimer(event.currentTarget.value)}
            />
            ms
          </label>
          <span class="muted">0 for by hand only</span>
        </div>
        {#if hasLive(characteristic.uuid)}
          <label class="check">
            <input
              type="checkbox"
              checked={characteristic.live}
              onchange={(event) => {
                characteristic.live = event.currentTarget.checked;
                changed();
              }}
            />
            Live values: {characteristic.live ? "made up by bleNIMA, as a real device sends them" : "the value above, again and again"}
          </label>
        {/if}

        {#if sends(characteristic)}
          <h3>Subscribed</h3>
          <p class="muted">
            {clients.length > 0 ? clients.join(", ") : server.running ? "Nobody yet" : "Start the server, then subscribe from the phone"}
          </p>
        {/if}

        <h3>Characteristic</h3>
        <fieldset disabled={server.running}>
          <div class="grid">
            <label class="dlg-label">
              UUID or name
              <input
                class="field mono"
                list={`${uid}-characteristics`}
                bind:value={uuidDraft}
                onchange={() => applyUuid(true)}
                spellcheck="false"
              />
            </label>
            <label class="dlg-label">
              User description (0x2901)
              <input
                class="field"
                value={characteristic.description}
                placeholder="None"
                onchange={(event) => {
                  characteristic.description = event.currentTarget.value;
                  changed();
                }}
              />
            </label>
          </div>
          {#if uuidProblem}<p class="problem">{uuidProblem}</p>{/if}
          <div class="properties">
            {#each PROPERTIES as { property, label } (property)}
              <label class="check">
                <input
                  type="checkbox"
                  checked={characteristic.properties.includes(property)}
                  onchange={(event) => toggle(property, event.currentTarget.checked)}
                />
                {label}
              </label>
            {/each}
          </div>
          {#if characteristic.properties.length === 0}<p class="problem">It needs one of these at least.</p>{/if}
          <div class="row">
            <button
              class="btn small"
              onclick={() => {
                server.removeCharacteristic(s, c);
                changed();
              }}>Remove the characteristic</button
            >
          </div>
        </fieldset>
        {#if server.running}<p class="muted">Stop the server to change its UUID, description or properties.</p>{/if}
        {#if !server.running}
          <datalist id={`${uid}-characteristics`}>
            {#each standardList(true) as entry, at (at)}
              <option value={entry.name}>{uuidText(entry.uuid)}</option>
            {/each}
          </datalist>
        {/if}
      {:else if selected && service}
        {@const s = selected.service}
        <h2>{server.serviceName(s)}</h2>
        <p class="uuid mono">{uuidText(service.uuid)}</p>
        <p class="muted">
          A primary service with {service.characteristics.length} characteristic{service.characteristics.length === 1 ? "" : "s"}{server.running &&
          server.adverts[s]
            ? `. ${server.adverts[s].text}`
            : ""}.
        </p>
        {#if UART_PRESETS.includes(service.preset)}
          <label class="check">
            <input
              type="checkbox"
              checked={service.echo}
              disabled={server.running}
              onchange={(event) => {
                service.echo = event.currentTarget.checked;
                changed();
              }}
            />
            Echo: what a client writes comes back to it as a notification
          </label>
        {/if}

        <h3>Service</h3>
        <fieldset disabled={server.running}>
          <label class="dlg-label">
            UUID or name
            <input class="field mono" list={`${uid}-services`} bind:value={uuidDraft} onchange={() => applyUuid(false)} spellcheck="false" />
          </label>
          {#if uuidProblem}<p class="problem">{uuidProblem}</p>{/if}
          <div class="row">
            <button
              class="btn small"
              onclick={() => {
                server.addCharacteristic(s);
                changed();
              }}>Add a characteristic</button
            >
            <button
              class="btn small"
              onclick={() => {
                server.removeService(s);
                changed();
              }}>Remove the service</button
            >
          </div>
        </fieldset>
        {#if server.running}<p class="muted">Stop the server to change its services.</p>{/if}
        {#if !server.running}
          <datalist id={`${uid}-services`}>
            {#each standardList(false) as entry, at (at)}
              <option value={entry.name}>{uuidText(entry.uuid)}</option>
            {/each}
          </datalist>
        {/if}
      {:else}
        <p class="empty">Pick a service or a characteristic to change it, or add a preset.</p>
      {/if}
    </div>
  </fieldset>
</div>

<style>
  .server {
    display: flex;
    flex-direction: column;
    height: 100%;
    font-size: var(--view-size);
  }

  .head {
    padding: 14px 20px 4px;
    border-bottom: 1px solid var(--border);
  }

  h2 {
    margin: 0 0 4px;
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

  p {
    margin: 0 0 10px;
  }

  .muted {
    color: var(--muted);
  }

  .bad,
  .problem {
    color: var(--bad);
  }

  fieldset {
    min-width: 0;
    margin: 0;
    padding: 0;
    border: 0;
  }

  .split {
    display: flex;
    flex: 1;
    min-height: 0;
  }

  .split.off {
    opacity: 0.5;
  }

  .side {
    display: flex;
    flex-direction: column;
    width: 380px;
    flex-shrink: 0;
    border-right: 1px solid var(--border);
  }

  .tree {
    flex: 1;
    min-height: 0;
    padding: 6px 0;
    overflow-y: auto;
  }

  .add {
    display: flex;
    gap: 8px;
    padding: 8px 12px;
    border-top: 1px solid var(--border);
  }

  .add select {
    flex: 1;
    min-width: 0;
  }

  .service,
  .characteristic {
    display: flex;
    flex-direction: column;
    gap: 1px;
    width: 100%;
    font: inherit;
    color: var(--text);
    text-align: left;
    background: none;
    border: 0;
  }

  .service {
    padding: 10px 12px 4px;
  }

  .service .title {
    font-weight: 600;
  }

  .characteristic {
    padding: 6px 12px 6px 24px;
  }

  .service:hover,
  .characteristic:hover {
    background: var(--hover);
  }

  .service.selected,
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
    white-space: nowrap;
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

  .every {
    color: var(--muted);
  }

  .panel {
    flex: 1;
    min-width: 0;
    padding: 14px 18px;
    overflow-y: auto;
    user-select: text;
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

  .write,
  .row,
  .properties {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 14px;
    margin-top: 8px;
  }

  .write {
    gap: 8px;
  }

  .write input {
    flex: 1;
    min-width: 160px;
  }

  .timer {
    width: 90px;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px 14px;
    max-width: 760px;
  }

  .small {
    height: 26px;
    padding: 0 10px;
    font-size: 12px;
  }

  .option,
  .check {
    display: flex;
    align-items: center;
    gap: 5px;
    color: var(--muted);
  }

  .check {
    margin-top: 8px;
  }

  .properties .check {
    margin-top: 0;
  }

  .empty {
    padding: 24px;
    color: var(--muted);
    text-align: center;
  }
</style>
