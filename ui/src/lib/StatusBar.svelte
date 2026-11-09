<script lang="ts">
  import { onMount } from "svelte";
  import { look, stateText } from "./state";
  import type { Tab } from "./tabs.svelte";

  type AppInfo = { version: string; os: string; arch: string };

  let { tab, info }: { tab: Tab | undefined; info: AppInfo | undefined } = $props();

  // A clock for the time connected, ticking once a second.
  let now = $state(Date.now());
  onMount(() => {
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });

  function bytes(count: number): string {
    if (count < 1024) return `${count} B`;
    if (count < 1024 * 1024) return `${(count / 1024).toFixed(1)} KB`;
    return `${(count / 1024 / 1024).toFixed(1)} MB`;
  }

  function elapsed(ms: number): string {
    const seconds = Math.max(0, Math.floor(ms / 1000));
    const two = (n: number) => String(n).padStart(2, "0");
    return `${two(Math.floor(seconds / 3600))}:${two(Math.floor(seconds / 60) % 60)}:${two(seconds % 60)}`;
  }
</script>

<footer class="status">
  {#if tab}
    <span class="state {look(tab)}" title={stateText(tab)}>{stateText(tab)}</span>
    {#if tab.device}
      {#if tab.device.connectedAt !== null}
        <span class="mono">{elapsed(now - tab.device.connectedAt)}</span>
      {/if}
      <span class="divider"></span>
      <span class="number" title="Bytes received: reads and notifications">RX {bytes(tab.device.rx)}</span>
      <span class="number" title="Bytes written">TX {bytes(tab.device.tx)}</span>
    {/if}
  {:else}
    <span>No tab open</span>
  {/if}
  <span class="spacer"></span>
  {#if info}
    <span>bleNIMA v{info.version}</span>
    <span class="divider"></span>
    <span>{info.os} {info.arch}</span>
  {/if}
</footer>

<style>
  .status {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 24px;
    padding: 0 12px;
    overflow: hidden;
    font-size: 12px;
    color: var(--muted);
    white-space: nowrap;
    background: var(--panel);
    border-top: 1px solid var(--border);
  }

  .state {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .state.connected {
    color: var(--ok);
  }

  .state.waiting {
    color: var(--warn);
  }

  .state.problem {
    color: var(--bad);
  }

  .number {
    min-width: 9ch;
    font-variant-numeric: tabular-nums;
  }

  .spacer {
    flex: 1;
  }

  .divider {
    flex-shrink: 0;
    width: 1px;
    height: 12px;
    background: var(--border);
  }
</style>
