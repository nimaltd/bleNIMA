<script lang="ts">
  import type { DeviceState } from "./device.svelte";

  let { device }: { device: DeviceState } = $props();

  /** The lines drawn at most: the newest. */
  const SHOWN = 1500;

  let filter = $state("");
  let list: HTMLDivElement;
  let follow = $state(true);

  const lines = $derived.by(() => {
    const text = filter.trim().toLowerCase();
    const matching = text
      ? device.log.filter((line) => `${line.name} ${line.text} ${line.hex} ${line.kind}`.toLowerCase().includes(text))
      : device.log;
    return matching.slice(-SHOWN);
  });

  // Keeps the newest line in view, unless the list was scrolled up to read.
  $effect(() => {
    void lines.length;
    if (follow && list) queueMicrotask(() => (list.scrollTop = list.scrollHeight));
  });

  function onscroll() {
    follow = list.scrollHeight - list.scrollTop - list.clientHeight < 24;
  }
</script>

<div class="log">
  <div class="bar">
    <input class="field small" placeholder="Filter the log" bind:value={filter} spellcheck="false" />
    <span class="muted">{device.log.length} lines</span>
    <span class="spacer"></span>
    <button class="btn small" onclick={() => device.clearLog()}>Clear</button>
  </div>
  <div class="lines" bind:this={list} {onscroll}>
    {#each lines as line (line.n)}
      <div class="line {line.kind}">
        <span class="time mono">{line.time}</span>
        <span class="kind">{line.kind.replace(/_/g, " ")}</span>
        <span class="name">{line.name}</span>
        <span class="text">{line.text}{#if line.hex && line.hex !== line.text}<span class="hex mono"> {line.hex}</span>{/if}</span>
      </div>
    {:else}
      <p class="empty">Nothing yet.</p>
    {/each}
  </div>
</div>

<style>
  .log {
    display: flex;
    flex-direction: column;
    height: 100%;
    font-size: var(--view-size);
  }

  .bar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border);
  }

  .bar input {
    width: 260px;
  }

  .small {
    height: 26px;
    padding: 0 10px;
    font-size: 12px;
  }

  .spacer {
    flex: 1;
  }

  .muted {
    color: var(--muted);
  }

  .lines {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    user-select: text;
  }

  .line {
    display: grid;
    grid-template-columns: 96px 110px minmax(120px, 220px) 1fr;
    gap: 10px;
    padding: 3px 10px;
    border-bottom: 1px solid var(--border);
  }

  .time,
  .kind {
    color: var(--muted);
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .text {
    overflow-wrap: anywhere;
  }

  .hex {
    margin-left: 10px;
    font-size: 0.9em;
    color: var(--muted);
  }

  .line.error .text {
    color: var(--bad);
  }

  .line.mark .text,
  .line.state .text {
    color: var(--warn);
  }

  .line.notify .kind {
    color: var(--accent);
  }

  .line.write .kind,
  .line.write_without_response .kind {
    color: var(--accent-2);
  }

  .empty {
    padding: 24px;
    color: var(--muted);
    text-align: center;
  }
</style>
