<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import { PROPERTY, TERMINAL_PRESETS, type CharacteristicInfo } from "./bluetooth";
  import { changed, type LineEnding, type Tab } from "./tabs.svelte";
  import { appScheme } from "./theme.svelte";
  import { settings } from "./settings.svelte";

  let { tab, active }: { tab: Tab; active: boolean } = $props();
  const device = $derived(tab.device!);

  let box: HTMLDivElement;
  let term: Terminal | null = null;
  let fit: FitAddon | null = null;

  const all = $derived(device.services.flatMap((service) => service.characteristics));
  const writable = $derived(all.filter((c) => c.properties & (PROPERTY.write | PROPERTY.writeWithoutResponse)));
  const notifying = $derived(all.filter((c) => c.properties & (PROPERTY.notify | PROPERTY.indicate)));

  /** The preset whose service the device has, if any. */
  const preset = $derived(
    TERMINAL_PRESETS.find((p) => all.some((c) => c.key === `${p.service}/${p.write}`)) ?? null,
  );

  // Picks the characteristics by itself the first time: a known UART-like service, else the first that fit.
  $effect(() => {
    if (all.length === 0 || (tab.terminal.write && tab.terminal.notify)) return;
    const write = preset ? `${preset.service}/${preset.write}` : (writable[0]?.key ?? "");
    const notify = preset ? `${preset.service}/${preset.notify}` : (notifying[0]?.key ?? "");
    if (write || notify) {
      tab.terminal.write ||= write;
      tab.terminal.notify ||= notify;
      changed();
    }
  });

  /** The characteristic asked for notifications, until the device is lost, so it is asked once. */
  let asked = "";
  /** Whether the terminal has been used: only then does it ask the device for notifications. */
  let used = $state(untrack(() => tab.kind === "terminal" || tab.view === "terminal"));
  $effect(() => {
    if (active) used = true;
  });

  // Listens on the notify characteristic whenever the device is connected.
  $effect(() => {
    const key = tab.terminal.notify;
    if (!device.connected || !used) {
      asked = "";
      return;
    }
    if (key && key !== asked && all.some((c) => c.key === key) && !device.subscribed.has(key)) {
      asked = key;
      device.subscribe(key, true);
    }
  });

  function label(c: CharacteristicInfo): string {
    return `${c.user_description || c.name || "Characteristic"} (${c.uuid})`;
  }

  const ENDINGS: Record<LineEnding, string> = { cr: "\r", lf: "\n", crlf: "\r\n" };

  function type(text: string) {
    const key = tab.terminal.write;
    if (!device.connected || !key) return;
    const data = Array.from(new TextEncoder().encode(text.replace(/\r/g, ENDINGS[tab.terminal.enter])));
    if (tab.terminal.local_echo) term?.write(text.replace(/\r/g, "\r\n"));
    const target = device.characteristic(key);
    // Without response where it can: faster, and what UART services expect.
    const withResponse = !target || (target.properties & PROPERTY.writeWithoutResponse) === 0;
    device.writeChunked(key, data, withResponse);
  }

  function theme() {
    const css = (name: string) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();
    return {
      background: css("--bg"),
      foreground: css("--text"),
      cursor: css("--accent"),
      selectionBackground: `${css("--accent")}44`,
    };
  }

  onMount(() => {
    term = new Terminal({
      fontFamily: '"JetBrains Mono", ui-monospace, monospace',
      fontSize: settings.font_size,
      scrollback: 10000,
      convertEol: false,
      theme: theme(),
    });
    fit = new FitAddon();
    term.loadAddon(fit);
    term.open(box);
    term.onData(type);
    term.write("\x1b[2mThe device's notifications show here; what you type is written to it.\x1b[0m\r\n");
    const listener = (key: string, data: number[], source: string) => {
      if (key === tab.terminal.notify && source === "notify") term?.write(Uint8Array.from(data));
    };
    device.listeners.add(listener);
    const observer = new ResizeObserver(() => {
      if (box.offsetWidth > 0) fit?.fit();
    });
    observer.observe(box);
    return () => {
      device.listeners.delete(listener);
      observer.disconnect();
      term?.dispose();
      term = null;
    };
  });

  $effect(() => {
    void appScheme();
    if (term) term.options.theme = theme();
  });

  $effect(() => {
    if (term) term.options.fontSize = settings.font_size;
    fit?.fit();
  });

  $effect(() => {
    if (active) setTimeout(() => (fit?.fit(), term?.focus()), 0);
  });
</script>

<div class="terminal">
  <div class="bar">
    {#if preset}<span class="preset" title="A known UART-like service">{preset.name}</span>{/if}
    <label class="option">
      Write to
      <select class="field small" bind:value={tab.terminal.write} onchange={changed}>
        <option value="">(none)</option>
        {#each writable as c (c.key)}<option value={c.key}>{label(c)}</option>{/each}
      </select>
    </label>
    <label class="option">
      Show
      <select
        class="field small"
        value={tab.terminal.notify}
        onchange={(event) => {
          const old = tab.terminal.notify;
          if (old && device.subscribed.has(old)) device.subscribe(old, false);
          tab.terminal.notify = (event.currentTarget as HTMLSelectElement).value;
          changed();
        }}
      >
        <option value="">(none)</option>
        {#each notifying as c (c.key)}<option value={c.key}>{label(c)}</option>{/each}
      </select>
    </label>
    <label class="option">
      Enter sends
      <select class="field small" bind:value={tab.terminal.enter} onchange={changed}>
        <option value="cr">CR</option>
        <option value="lf">LF</option>
        <option value="crlf">CR LF</option>
      </select>
    </label>
    <label class="check"><input type="checkbox" bind:checked={tab.terminal.local_echo} onchange={changed} /> Local echo</label>
    <span class="spacer"></span>
    <button class="btn small" onclick={() => term?.clear()}>Clear</button>
  </div>
  <div class="screen" bind:this={box}></div>
</div>

<style>
  .terminal {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 12px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border);
  }

  .bar select {
    max-width: 280px;
  }

  .preset {
    padding: 0 8px;
    font-size: 11px;
    font-weight: 600;
    line-height: 20px;
    color: var(--accent);
    background: var(--accent-soft);
    border-radius: 4px;
  }

  .option,
  .check {
    display: flex;
    align-items: center;
    gap: 5px;
    color: var(--muted);
  }

  .spacer {
    flex: 1;
  }

  .small {
    height: 26px;
    padding: 0 10px;
    font-size: 12px;
  }

  .screen {
    flex: 1;
    min-height: 0;
    padding: 6px 4px 4px 10px;
  }
</style>
