<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import icon from "../../../src-tauri/icons/icon.svg";
  import { NEW_TABS } from "./new-tab";
  import { CURRENT_PHASE, PHASES, PLAN } from "./roadmap";
  import { shortcutLabel } from "./shortcuts";

  /** Opens a new scanner tab. */
  let { onscan }: { onscan: () => void } = $props();

  const scanner = NEW_TABS[0];
</script>

<div class="welcome">
  <div class="head">
    <img src={icon} alt="" width="72" height="72" />
    <h1>ble<span class="nima">NIMA</span></h1>
    <p class="tagline">A Bluetooth LE tester for embedded developers, the same on Windows, macOS and Linux.</p>
    <div class="buttons">
      <button class="btn primary" onclick={onscan}>
        New scanner
        {#if scanner.shortcut}<span class="key">{shortcutLabel(scanner.shortcut)}</span>{/if}
      </button>
      <button class="btn" onclick={() => openUrl(PLAN)}>The plan</button>
    </div>
  </div>

  <section class="roadmap" aria-labelledby="roadmap-title">
    <h2 id="roadmap-title" class="dlg-section">Roadmap</h2>
    <ol>
      {#each PHASES as phase (phase.number)}
        <li class:done={phase.number < CURRENT_PHASE} class:now={phase.number === CURRENT_PHASE}>
          <span class="number">{phase.number}</span>
          <span class="name">{phase.name}</span>
          {#if phase.number === CURRENT_PHASE}
            <span class="badge">now</span>
          {/if}
        </li>
      {/each}
    </ol>
  </section>
</div>

<style>
  .welcome {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 28px;
    height: 100%;
    padding: 48px 24px 24px;
    overflow-y: auto;
    font-size: var(--view-size);
    box-sizing: border-box;
  }

  .head {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
  }

  /* "ble" in teal and "NIMA" in orange, as the icon's two colors. */
  h1 {
    margin: 10px 0 4px;
    font-size: 36px;
    font-weight: 600;
    letter-spacing: -0.01em;
    color: var(--accent);
  }

  /* The same orange as NIMA in terNIMA's logo. */
  .nima {
    color: #fb923c;
  }

  :global([data-theme="light"]) .nima {
    color: #ea580c;
  }

  .tagline {
    max-width: 460px;
    margin: 0;
    line-height: 1.5;
    color: var(--muted);
  }

  .buttons {
    display: flex;
    gap: 8px;
    margin-top: 20px;
  }

  .key {
    font-size: 12px;
    opacity: 0.75;
  }

  .roadmap {
    width: min(560px, 100%);
  }

  .roadmap h2 {
    margin-top: 0;
  }

  ol {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    grid-auto-flow: column;
    grid-template-rows: repeat(5, auto);
    gap: 2px 16px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 30px;
    padding: 0 8px;
    color: var(--muted);
    border-radius: 6px;
  }

  li.now {
    color: var(--text);
    background: var(--accent-soft);
  }

  li.done {
    color: var(--text);
  }

  .number {
    display: grid;
    flex-shrink: 0;
    place-items: center;
    width: 20px;
    height: 20px;
    font-family: var(--font-mono);
    font-size: 11px;
    border: 1.5px solid var(--border);
    border-radius: 50%;
  }

  li.now .number {
    color: var(--on-accent);
    background: var(--accent);
    border-color: var(--accent);
  }

  li.done .number {
    color: var(--ok);
    border-color: var(--ok);
  }

  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .badge {
    font-size: 11px;
    font-weight: 600;
    color: var(--accent);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  @media (max-width: 560px) {
    ol {
      grid-template-columns: minmax(0, 1fr);
      grid-auto-flow: row;
      grid-template-rows: none;
    }
  }
</style>
