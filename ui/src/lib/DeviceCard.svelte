<script lang="ts">
  import type { DeviceState } from "./device.svelte";

  // The device information card: the name, appearance, battery and Device
  // Information the device tab reads by itself after connecting. In two
  // columns, or as one line with the rest under "More".
  let { device, strip = false }: { device: DeviceState; strip?: boolean } = $props();

  let open = $state(true);
  let more = $state(false);

  const rows = $derived(device.info);
  const left = $derived(rows.slice(0, Math.ceil(rows.length / 2)));
  const right = $derived(rows.slice(Math.ceil(rows.length / 2)));

  /** What the one line shows; the rest is under "More". */
  const FIRST = ["Name", "Appearance", "Battery", "Manufacturer", "Model", "Firmware"];
  /** In the one line, these say what they are; the others are clear by themselves. */
  const LABELLED = new Set(["Firmware", "Software", "Hardware", "Serial", "PnP ID"]);

  function item(row: { label: string; text: string }): string {
    // The one line shows an appearance by its name, without its number.
    const text = row.label === "Appearance" ? row.text.replace(/ \(0x[0-9A-F]{4}\)$/, "") : row.text;
    return LABELLED.has(row.label) ? `${row.label} ${text}` : text;
  }
</script>

{#if device.hasInfo}
  {#if strip}
    <div class="strip">
      <span class="line">
        {rows
          .filter((row) => FIRST.includes(row.label))
          .map(item)
          .join(" · ") || (device.connected ? "Reading the device information..." : "No device information read yet")}
      </span>
      <button class="link" aria-expanded={more} onclick={() => (more = !more)}>More {more ? "▴" : "▾"}</button>
    </div>
    {#if more}
      <div class="strip more">
        <span class="line">
          {rows
            .filter((row) => !FIRST.includes(row.label))
            .map(item)
            .join(" · ") || "Nothing more read"}
        </span>
        <button class="btn tiny" disabled={!device.connected} onclick={() => device.readInfo()}>Read again</button>
      </div>
    {/if}
  {:else}
    <section class="card" aria-label="Device information">
      <header>
        <h3>Device information</h3>
        <span class="spacer"></span>
        <button class="btn tiny" disabled={!device.connected} onclick={() => device.readInfo()}>Read again</button>
        <button class="fold" aria-expanded={open} title={open ? "Fold the card" : "Unfold the card"} onclick={() => (open = !open)}
          >{open ? "▴" : "▾"}</button
        >
      </header>
      {#if open}
        {#if rows.length === 0}
          <p class="muted">{device.connected ? "Reading the device information..." : "No device information read yet"}</p>
        {:else}
          <div class="columns">
            {#each [left, right] as half, at (at)}
              {#if half.length > 0}
                <dl>
                  {#each half as row (row.label)}
                    <dt>{row.label}</dt>
                    <dd>{row.text}</dd>
                  {/each}
                </dl>
              {/if}
            {/each}
          </div>
        {/if}
      {/if}
    </section>
  {/if}
{/if}

<style>
  .card {
    flex-shrink: 0;
    margin: 10px 12px 4px;
    padding: 8px 12px 10px;
    font-size: var(--view-size);
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 8px;
  }

  header {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  h3 {
    margin: 0;
    font-size: 11px;
    font-weight: 600;
    color: var(--muted);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .spacer {
    flex: 1;
  }

  .fold {
    width: 22px;
    height: 22px;
    padding: 0;
    font: inherit;
    color: var(--muted);
    background: none;
    border: 0;
    border-radius: 4px;
  }

  .fold:hover {
    background: var(--hover);
  }

  .columns {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 32px;
    margin-top: 6px;
  }

  dl {
    display: grid;
    flex: 1;
    grid-template-columns: max-content 1fr;
    gap: 3px 14px;
    min-width: 260px;
    margin: 0;
  }

  dt {
    color: var(--muted);
  }

  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .strip {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 10px;
    padding: 6px 12px;
    font-size: var(--view-size);
    border-bottom: 1px solid var(--border);
  }

  .strip.more {
    color: var(--muted);
    background: var(--panel);
  }

  .line {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .link {
    flex-shrink: 0;
    padding: 0 4px;
    font: inherit;
    font-size: 12px;
    color: var(--accent);
    background: none;
    border: 0;
  }

  .tiny {
    height: 22px;
    padding: 0 8px;
    font-size: 11px;
  }

  .muted {
    margin: 6px 0 0;
    color: var(--muted);
  }
</style>
