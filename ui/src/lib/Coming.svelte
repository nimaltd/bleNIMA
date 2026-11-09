<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { kindInfo } from "./new-tab";
  import { CURRENT_PHASE, PLAN, isBuilt, phaseName } from "./roadmap";
  import type { Tab } from "./tabs.svelte";

  /** A tab whose kind a later phase of the roadmap brings, or a device tab with no device yet. */
  let { tab, onscan }: { tab: Tab; onscan: () => void } = $props();

  const info = $derived(kindInfo(tab.kind));
  const next = $derived(info.phase === CURRENT_PHASE + 1);
  const needsDevice = $derived((tab.kind === "device" || tab.kind === "terminal") && isBuilt(info.phase));
</script>

<div class="coming">
  <div class="card">
    {#if !needsDevice}<span class="phase">Phase {info.phase} · {phaseName(info.phase)}</span>{/if}
    <h1>{info.label}</h1>
    <p>{info.detail}.</p>
    {#if needsDevice}
      <p class="when">
        Pick a device in a scanner tab, then press Connect or Terminal there, or double-click it. This tab can be closed.
      </p>
      <button class="btn primary" onclick={onscan}>New scanner</button>
    {:else}
      <p class="when">
        This tab comes in Phase {info.phase} of the roadmap{next ? ", the next one" : ""}. Until then it keeps its place, and
        opens again with bleNIMA.
      </p>
      <button class="btn" onclick={() => openUrl(PLAN)}>Read the plan</button>
    {/if}
  </div>
</div>

<style>
  .coming {
    display: grid;
    place-items: center;
    height: 100%;
    padding: 24px;
    overflow-y: auto;
    font-size: var(--view-size);
    box-sizing: border-box;
  }

  .card {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    width: min(480px, 100%);
    padding: 22px 24px;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 12px;
  }

  .phase {
    padding: 0 8px;
    font-size: 11px;
    font-weight: 600;
    line-height: 20px;
    color: var(--accent);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    background: var(--accent-soft);
    border-radius: 4px;
  }

  h1 {
    margin: 12px 0 4px;
    font-size: 20px;
    font-weight: 600;
  }

  p {
    margin: 0 0 10px;
    line-height: 1.5;
  }

  .when {
    color: var(--muted);
  }

  .btn {
    margin-top: 6px;
  }
</style>
