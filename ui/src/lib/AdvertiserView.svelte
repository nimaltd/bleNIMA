<script lang="ts">
  import { onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { LEGACY_MAX, advertLength, eddystoneUrl, ibeacon, type Structure } from "./advertise";
  import { hex, parseHex } from "./format";

  type Preset = "ibeacon" | "eddystone" | "manufacturer" | "custom";

  let preset: Preset = $state("ibeacon");
  let uuid = $state("E2C56DB5-DFFB-48D2-B060-D0F5A71096E0");
  let major = $state(1);
  let minor = $state(1);
  let power = $state(-59);
  let url = $state("https://github.com");
  let urlPower = $state(-20);
  let company = $state("FFFF");
  let manufacturerData = $state("01 02 03");
  let custom: { type: string; data: string }[] = $state([{ type: "FF", data: "FF FF 01 02" }]);

  let id: number | null = $state(null);
  let status = $state("Not advertising");
  let problem = $state("");
  let timer: ReturnType<typeof setInterval> | undefined;

  /** The AD structures to send, or what is wrong with what was typed. */
  const structures: Structure[] | string = $derived.by(() => {
    switch (preset) {
      case "ibeacon":
        return ibeacon(uuid, Number(major), Number(minor), Number(power));
      case "eddystone":
        return eddystoneUrl(url, Number(urlPower));
      case "manufacturer": {
        const code = Number.parseInt(company, 16);
        if (!/^[0-9a-f]{1,4}$/i.test(company) || Number.isNaN(code)) return "The company ID is up to 4 hex digits, such as 0059";
        const data = parseHex(manufacturerData);
        if (typeof data === "string") return data;
        return [{ ad_type: 0xff, data: [code & 0xff, code >> 8, ...data] }];
      }
      case "custom": {
        const list: Structure[] = [];
        for (const row of custom) {
          const type = Number.parseInt(row.type, 16);
          if (!/^[0-9a-f]{1,2}$/i.test(row.type)) return "Each type is one byte in hex, such as FF";
          const data = parseHex(row.data);
          if (typeof data === "string") return data;
          list.push({ ad_type: type, data });
        }
        return list;
      }
    }
  });

  const bytes = $derived(
    typeof structures === "string" ? [] : structures.flatMap((s) => [s.data.length + 1, s.ad_type, ...s.data]),
  );

  async function start() {
    if (typeof structures === "string") return;
    problem = "";
    try {
      id = await invoke<number>("advertise_start", { structures });
      timer = setInterval(async () => {
        if (id !== null) status = await invoke<string>("advertise_status", { id });
      }, 1000);
      status = "Starting...";
    } catch (error) {
      problem = String(error);
    }
  }

  async function stop() {
    clearInterval(timer);
    if (id !== null) await invoke("advertise_stop", { id }).catch(() => {});
    id = null;
    status = "Not advertising";
  }

  onDestroy(() => void stop());
</script>

<div class="advertiser">
  <h2>Advertiser</h2>
  <p class="muted">
    bleNIMA advertises as a peripheral, to test a phone app or a central's firmware. Windows puts manufacturer data,
    service UUIDs and service data in the advert; it keeps Flags for itself.
  </p>

  <div class="row">
    <label class="option">
      Advert
      <select class="field small" bind:value={preset} disabled={id !== null}>
        <option value="ibeacon">iBeacon</option>
        <option value="eddystone">Eddystone URL</option>
        <option value="manufacturer">Manufacturer data</option>
        <option value="custom">AD structures by hand</option>
      </select>
    </label>
    {#if id === null}
      <button class="btn small primary" disabled={typeof structures === "string"} onclick={start}>Advertise</button>
    {:else}
      <button class="btn small" onclick={stop}>Stop</button>
    {/if}
    <span class="status" class:on={id !== null}>{status}</span>
  </div>

  <fieldset disabled={id !== null}>
    {#if preset === "ibeacon"}
      <div class="grid">
        <label class="dlg-label wide">UUID <input class="field mono" bind:value={uuid} spellcheck="false" /></label>
        <label class="dlg-label">Major <input class="field mono" type="number" min="0" max="65535" bind:value={major} /></label>
        <label class="dlg-label">Minor <input class="field mono" type="number" min="0" max="65535" bind:value={minor} /></label>
        <label class="dlg-label">Power at 1 m, dBm <input class="field mono" type="number" bind:value={power} /></label>
      </div>
    {:else if preset === "eddystone"}
      <div class="grid">
        <label class="dlg-label wide">URL <input class="field mono" bind:value={url} spellcheck="false" /></label>
        <label class="dlg-label">Power at 0 m, dBm <input class="field mono" type="number" bind:value={urlPower} /></label>
      </div>
    {:else if preset === "manufacturer"}
      <div class="grid">
        <label class="dlg-label">Company ID (hex) <input class="field mono" bind:value={company} spellcheck="false" /></label>
        <label class="dlg-label wide">Data (hex) <input class="field mono" bind:value={manufacturerData} spellcheck="false" /></label>
      </div>
    {:else}
      {#each custom as row, at (at)}
        <div class="custom">
          <label class="dlg-label">Type <input class="field mono" bind:value={row.type} spellcheck="false" /></label>
          <label class="dlg-label wide">Data (hex) <input class="field mono" bind:value={row.data} spellcheck="false" /></label>
          <button class="btn small" onclick={() => custom.splice(at, 1)}>Remove</button>
        </div>
      {/each}
      <button class="btn small" onclick={() => custom.push({ type: "FF", data: "" })}>Add a structure</button>
    {/if}
  </fieldset>

  <h3>On the air</h3>
  {#if typeof structures === "string"}
    <p class="bad">{structures}</p>
  {:else}
    <p class="mono bytes">{hex(bytes)}</p>
    <p class="muted" class:bad={advertLength(structures) > LEGACY_MAX}>
      {advertLength(structures)} of {LEGACY_MAX} bytes{advertLength(structures) > LEGACY_MAX ? ": too long for a legacy advert" : ""}
    </p>
  {/if}
  {#if problem}<p class="bad">{problem}</p>{/if}

  <h3>Next</h3>
  <p class="muted">A GATT server, with services and characteristics notified on a timer, comes next in Phase 7.</p>
</div>

<style>
  .advertiser {
    height: 100%;
    padding: 16px 20px;
    overflow-y: auto;
    box-sizing: border-box;
    font-size: var(--view-size);
    user-select: text;
  }

  h2 {
    margin: 0 0 4px;
    font-size: 17px;
    font-weight: 600;
  }

  h3 {
    margin: 22px 0 6px;
    font-size: 11px;
    font-weight: 600;
    color: var(--muted);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .muted {
    margin: 0 0 10px;
    color: var(--muted);
  }

  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    margin: 12px 0;
  }

  .option {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--muted);
  }

  .small {
    height: 26px;
    padding: 0 10px;
    font-size: 12px;
  }

  .status {
    color: var(--muted);
  }

  .status.on {
    color: var(--ok);
  }

  fieldset {
    margin: 0;
    padding: 0;
    border: 0;
  }

  .grid,
  .custom {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 10px 14px;
    max-width: 760px;
  }

  .custom {
    grid-template-columns: 90px 1fr auto;
    align-items: end;
    margin-bottom: 8px;
  }

  .wide {
    grid-column: span 2;
  }

  .custom .wide {
    grid-column: auto;
  }

  .bytes {
    margin: 0 0 4px;
    overflow-wrap: anywhere;
  }

  .bad {
    color: var(--bad) !important;
  }
</style>
