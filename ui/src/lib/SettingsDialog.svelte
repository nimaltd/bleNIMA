<script lang="ts">
  import Modal from "./Modal.svelte";
  import {
    FONT_SIZE_MAX,
    FONT_SIZE_MIN,
    settings,
    updateSettings,
    type StartupSettings,
    type ThemeChoice,
    type UpdateSettings,
  } from "./settings.svelte";
  import type { SimulatorSettings } from "./settings.svelte";

  let { onclose }: { onclose: () => void } = $props();

  // A copy of the settings: nothing changes until Save.
  let theme: ThemeChoice = $state(settings.theme);
  let fontSize = $state(settings.font_size);
  let startup: StartupSettings = $state({ ...settings.startup });
  let updates: UpdateSettings = $state({ ...settings.updates });
  let simulator: SimulatorSettings = $state({ ...settings.simulator });
  let error = $state("");

  function submit() {
    if (!Number.isInteger(fontSize) || fontSize < FONT_SIZE_MIN || fontSize > FONT_SIZE_MAX) {
      error = `The text size must be a whole number from ${FONT_SIZE_MIN} to ${FONT_SIZE_MAX}.`;
      return;
    }
    updateSettings({
      theme,
      font_size: fontSize,
      startup: { ...startup },
      updates: { ...updates },
      simulator: { ...simulator },
    });
    onclose();
  }
</script>

<Modal title="Settings" {onclose} onsubmit={submit}>
  <h2 class="dlg-section">Appearance</h2>
  <div class="dlg-grid">
    <label class="dlg-label">
      Theme
      <select class="field" bind:value={theme}>
        <option value="system">Follow the OS</option>
        <option value="dark">Dark</option>
        <option value="light">Light</option>
      </select>
    </label>
    <label class="dlg-label">
      Text size
      <input class="field mono" type="number" min={FONT_SIZE_MIN} max={FONT_SIZE_MAX} bind:value={fontSize} />
    </label>
  </div>
  <p class="dlg-hint">The text size is for the views: lists, values and terminals.</p>

  <h2 class="dlg-section">Startup</h2>
  <p class="dlg-hint">What bleNIMA brings back from last time when it starts.</p>
  <div class="dlg-checks startup">
    <label class="dlg-check">
      <input type="checkbox" bind:checked={startup.restore_tabs} /> Reopen the tabs
    </label>
    <label class="dlg-check under" class:dlg-muted={!startup.restore_tabs}>
      <input type="checkbox" bind:checked={startup.reconnect} disabled={!startup.restore_tabs} /> Connect the tabs that
      were connected
    </label>
    <label class="dlg-check">
      <input type="checkbox" bind:checked={startup.restore_window} /> Open the window where it was, at its size
    </label>
  </div>

  <h2 class="dlg-section">Peripheral simulator</h2>
  <div class="dlg-grid">
    <label class="dlg-label">
      Layout
      <select class="field" bind:value={simulator.layout}>
        <option value="views">One at a time, as views</option>
        <option value="page">All on one page</option>
        <option value="columns">Two columns, log right</option>
      </select>
    </label>
  </div>
  <p class="dlg-hint">
    How a Peripheral simulator tab shows its advertiser, GATT server and log. A GATT server that ran when bleNIMA closed
    starts again with "Connect the tabs that were connected".
  </p>

  <h2 class="dlg-section">Updates</h2>
  <div class="dlg-checks">
    <label class="dlg-check">
      <input type="checkbox" bind:checked={updates.check} /> Look on GitHub for a new version when bleNIMA starts
    </label>
  </div>

  {#if error}
    <p class="dlg-error">{error}</p>
  {/if}

  {#snippet buttons()}
    <button class="btn" onclick={onclose}>Cancel</button>
    <button class="btn primary" onclick={submit}>Save</button>
  {/snippet}
</Modal>

<style>
  .startup {
    flex-direction: column;
  }

  /* Connecting the tabs again needs them reopened first. */
  .under {
    padding-left: 24px;
  }
</style>
