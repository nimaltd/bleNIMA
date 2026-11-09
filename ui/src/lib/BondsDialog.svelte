<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Modal from "./Modal.svelte";
  import { openDevice } from "./tabs.svelte";

  let { onclose }: { onclose: () => void } = $props();

  type Bond = { name: string; address: string };

  let bonds: Bond[] = $state([]);
  let loading = $state(true);
  let problem = $state("");
  let note = $state("");

  async function load() {
    loading = true;
    problem = "";
    try {
      bonds = await invoke<Bond[]>("bonds");
    } catch (error) {
      problem = String(error);
    } finally {
      loading = false;
    }
  }

  onMount(() => void load());

  async function unpair(bond: Bond) {
    note = "";
    try {
      note = `${bond.name || bond.address}: ${await invoke<string>("device_unpair", { device: bond.address })}`;
    } catch (error) {
      problem = String(error);
    }
    await load();
  }

  function connect(bond: Bond) {
    openDevice(bond.address, bond.name || bond.address, "device");
    onclose();
  }
</script>

<Modal title="Bonded devices" {onclose} onsubmit={onclose}>
  <p class="dlg-hint">The Bluetooth LE devices this computer has a bond with: their keys are kept, so they connect encrypted.</p>
  <div class="dlg-list">
    {#if loading}
      <p class="dlg-empty">Asking the OS...</p>
    {:else}
      {#each bonds as bond (bond.address)}
        <div class="dlg-row bond">
          <span>{bond.name || "(no name)"}</span>
          <span class="mono dlg-muted">{bond.address}</span>
          <button class="btn small" onclick={() => connect(bond)}>Connect</button>
          <button class="btn small" onclick={() => unpair(bond)}>Unpair</button>
        </div>
      {:else}
        <p class="dlg-empty">{problem ? "" : "No Bluetooth LE device is bonded."}</p>
      {/each}
    {/if}
  </div>
  {#if note}<p class="dlg-hint">{note}</p>{/if}
  {#if problem}<p class="dlg-error">{problem}</p>{/if}

  {#snippet buttons()}
    <button class="btn" onclick={() => void load()}>Refresh</button>
    <button class="btn primary" onclick={onclose}>Close</button>
  {/snippet}
</Modal>

<style>
  .bond {
    grid-template-columns: 1fr auto auto auto;
  }

  .small {
    height: 24px;
    padding: 0 8px;
    font-size: 12px;
  }
</style>
