<script lang="ts">
  import { NEW_TABS, type TabKind } from "./new-tab";
  import { isBuilt } from "./roadmap";
  import { look, stateText } from "./state";
  import { closeTab, showTab, tabs, view } from "./tabs.svelte";

  /** Opens a new tab of a kind. */
  let { onnew }: { onnew: (kind: TabKind) => void } = $props();

  let bar: HTMLDivElement;
  let add: HTMLButtonElement;
  /** Where the choice under the + button shows, while it is open. */
  let choice: { left: number } | null = $state(null);

  function toggle() {
    if (choice) {
      choice = null;
    } else {
      choice = { left: add.getBoundingClientRect().left - bar.getBoundingClientRect().left };
    }
  }

  function pick(kind: TabKind) {
    choice = null;
    onnew(kind);
  }

  function onWindowMouseDown(event: MouseEvent) {
    if (choice && !bar.contains(event.target as Node)) {
      choice = null;
    }
  }
</script>

<svelte:window
  onmousedown={onWindowMouseDown}
  onkeydown={(event) => {
    if (choice && event.key === "Escape") {
      choice = null;
    }
  }}
/>

<div class="bar" bind:this={bar}>
  <header class="tabs" role="tablist">
    {#if tabs.length === 0}
      <div class="tab active" role="tab" aria-selected="true">
        <span class="dot stopped"></span>
        Welcome
      </div>
    {/if}

    {#each tabs as tab, i (tab.id)}
      <div
        class="tab"
        class:active={i === view.active}
        role="tab"
        tabindex="-1"
        aria-selected={i === view.active}
        title={`${tab.name}: ${stateText(tab)}`}
        onmousedown={() => showTab(i)}
      >
        <span class="dot {look(tab)}"></span>
        <span class="name">{tab.name}</span>
        <button
          class="close"
          aria-label={`Close ${tab.name}`}
          title="Close tab"
          onmousedown={(event) => {
            event.stopPropagation();
            closeTab(tab);
          }}
        >
          ×
        </button>
      </div>
    {/each}

    <button
      class="add"
      aria-label="New tab"
      aria-haspopup="menu"
      aria-expanded={choice !== null}
      title="New tab"
      bind:this={add}
      onclick={toggle}
    >
      +
    </button>
  </header>

  {#if choice}
    <div class="choice" role="menu" style:left={`${choice.left}px`}>
      <!-- The same as File > New tab. -->
      {#each NEW_TABS as { kind, label, detail, phase } (kind)}
        <button role="menuitem" onclick={() => pick(kind)}>
          <span class="title">
            <strong>{label}</strong>
            {#if !isBuilt(phase)}
              <span class="phase" title={`Comes in Phase ${phase} of the roadmap`}>Phase {phase}</span>
            {/if}
          </span>
          <span class="detail">{detail}</span>
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .bar {
    position: relative;
  }

  .choice {
    position: absolute;
    top: calc(100% + 2px);
    z-index: 10;
    display: flex;
    flex-direction: column;
    width: 330px;
    max-height: calc(100vh - 90px);
    padding: 4px;
    overflow-y: auto;
    background: var(--menu-bg);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: var(--shadow);
  }

  .choice button {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 2px;
    padding: 7px 10px;
    font: inherit;
    color: var(--text);
    text-align: left;
    background: none;
    border: 0;
    border-radius: 6px;
    cursor: default;
  }

  .choice button:hover {
    background: var(--accent-soft);
  }

  .title {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .choice strong {
    font-weight: 600;
  }

  .detail {
    font-size: 12px;
    color: var(--muted);
  }

  .phase {
    flex-shrink: 0;
    padding: 0 6px;
    font-size: 11px;
    line-height: 18px;
    color: var(--muted);
    border: 1px solid var(--border);
    border-radius: 4px;
  }

  .tabs {
    display: flex;
    align-items: stretch;
    height: 36px;
    overflow-x: auto;
    background: var(--panel);
    border-bottom: 1px solid var(--border);
    scrollbar-width: none;
  }

  .tab {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 8px;
    max-width: 240px;
    padding: 0 8px 0 14px;
    color: var(--muted);
    border-right: 1px solid var(--border);
  }

  .tab:hover {
    color: var(--text);
  }

  .tab.active {
    color: var(--text);
    background: var(--bg);
    box-shadow: inset 0 2px 0 var(--accent);
    /* The active tab covers the bar's bottom border, so it joins the pane. */
    margin-bottom: -1px;
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .dot {
    flex-shrink: 0;
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }

  .dot.connected {
    background: var(--ok);
  }

  .dot.waiting {
    background: var(--warn);
    animation: pulse 1.4s ease-in-out infinite;
  }

  .dot.problem {
    background: var(--bad);
  }

  .dot.stopped {
    border: 1.5px solid var(--muted);
  }

  @keyframes pulse {
    50% {
      opacity: 0.35;
    }
  }

  button {
    font: inherit;
    color: var(--muted);
    background: none;
    border: 0;
    cursor: default;
  }

  .close {
    width: 20px;
    height: 20px;
    font-size: 15px;
    line-height: 1;
    border-radius: 4px;
    visibility: hidden;
  }

  .tab:hover .close,
  .tab.active .close {
    visibility: visible;
  }

  .close:hover {
    color: var(--text);
    background: var(--hover);
  }

  .add {
    flex-shrink: 0;
    width: 36px;
    font-size: 18px;
  }

  .add:hover {
    color: var(--text);
    background: var(--hover);
  }
</style>
