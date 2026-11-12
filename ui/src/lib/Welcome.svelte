<script lang="ts">
  import { onMount } from "svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { radioInfo, type RadioInfo } from "./bluetooth";
  import { logo } from "./logo";
  import { NEW_TABS } from "./new-tab";
  import { GITHUB, HELP } from "./roadmap";
  import { shortcutLabel } from "./shortcuts";

  // The page with no tab open, as libNIMA's and terNIMA's: the logo, what
  // bleNIMA is, where to read more, and how to start.
  let {
    version,
    onscan,
  }: {
    version: string;
    /** Opens the scanner. */
    onscan: () => void;
  } = $props();

  const scanner = NEW_TABS[0];
  const drawn = logo();

  let radio: RadioInfo | null = $state(null);
  let radioProblem = $state("");
  onMount(() => {
    radioInfo()
      .then((info) => (radio = info))
      .catch((error) => (radioProblem = String(error)));
  });
</script>

<div class="welcome">
  <svg
    class="logo"
    width={drawn.width}
    height={drawn.height}
    viewBox="0 0 {drawn.width} {drawn.height}"
    role="img"
    aria-label="bleNIMA"
  >
    <path class="ble-shadow" d={drawn.bleShadow} />
    <path class="ble-face" d={drawn.bleFace} />
    <path class="nima-shadow" d={drawn.nimaShadow} />
    <path class="nima-face" d={drawn.nimaFace} />
  </svg>

  <div class="lines">
    <p><span class="version">v{version}</span>&nbsp; a Bluetooth LE tester for embedded developers</p>
    <p><button class="link" onclick={() => void openUrl(GITHUB)}>{GITHUB}</button></p>
    <p><span class="dim">Help:</span> <button class="link" onclick={() => void openUrl(HELP)}>{HELP}</button></p>
  </div>

  <div class="table">
    <span class="label">Radio</span>
    <span class:dim={!radio} class:problem={radioProblem !== ""}>{radioProblem || radio?.name || "..."}</span>
    <span class="label">New tab</span>
    <span>{NEW_TABS.map((entry) => entry.label).join(", ")}</span>
    <span class="label">Devices</span>
    <span>from the scanner: Connect, Terminal, or a double-click</span>
  </div>

  <div class="lines">
    <p class="dim">Open the scanner with this button, or File &gt; New tab.</p>
    <p class="dim">The computer's own Bluetooth does it all: there is no dongle to buy.</p>
  </div>

  <button class="btn primary open" onclick={onscan}>
    New scanner
    {#if scanner.shortcut}<span class="key">{shortcutLabel(scanner.shortcut)}</span>{/if}
  </button>
</div>

<style>
  /* The same colors as the logo in terNIMA's welcome text and libNIMA's welcome page. */
  .welcome {
    --logo-teal: #2dd4bf;
    --logo-teal-shadow: #135950;
    --logo-orange: #fb923c;
    --logo-orange-shadow: #693d19;

    height: 100%;
    padding: 18px 2ch;
    overflow: auto;
    font-family: var(--font-mono);
    font-size: 14px;
    font-variant-ligatures: none;
    line-height: 18px;
    box-sizing: border-box;
  }

  :global(:root[data-theme="light"]) .welcome {
    --logo-teal: #0d9488;
    --logo-teal-shadow: #99f6e4;
    --logo-orange: #ea580c;
    --logo-orange-shadow: #fed7aa;
  }

  .logo {
    display: block;
    shape-rendering: crispEdges;
  }

  .ble-face {
    fill: var(--logo-teal);
  }

  .ble-shadow {
    fill: var(--logo-teal-shadow);
  }

  .nima-face {
    fill: var(--logo-orange);
  }

  .nima-shadow {
    fill: none;
    stroke: var(--logo-orange-shadow);
    stroke-width: 1;
  }

  .lines {
    margin-top: 18px;
  }

  p {
    margin: 0;
  }

  .version {
    color: var(--accent);
  }

  .dim,
  .link {
    color: var(--muted);
  }

  .problem {
    color: var(--bad);
  }

  .link {
    padding: 0;
    font: inherit;
    background: none;
    border: 0;
    cursor: pointer;
  }

  .link:hover {
    color: var(--accent);
    text-decoration: underline;
  }

  .table {
    display: grid;
    grid-template-columns: 14ch minmax(0, 1fr);
    margin-top: 18px;
    white-space: pre-wrap;
  }

  .label {
    color: var(--muted);
    white-space: nowrap;
  }

  .open {
    margin-top: 18px;
    font-family: var(--font-ui);
    font-size: 13px;
  }

  .key {
    margin-left: 8px;
    opacity: 0.7;
  }
</style>
