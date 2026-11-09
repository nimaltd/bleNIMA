<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import About from "./lib/About.svelte";
  import Coming from "./lib/Coming.svelte";
  import DeviceView from "./lib/DeviceView.svelte";
  import ScannerView from "./lib/ScannerView.svelte";
  import BondsDialog from "./lib/BondsDialog.svelte";
  import SimulatorView from "./lib/SimulatorView.svelte";
  import CommandPalette from "./lib/CommandPalette.svelte";
  import MenuBar from "./lib/MenuBar.svelte";
  import SettingsDialog from "./lib/SettingsDialog.svelte";
  import StatusBar from "./lib/StatusBar.svelte";
  import TabBar from "./lib/TabBar.svelte";
  import UpdateDialog from "./lib/UpdateDialog.svelte";
  import Welcome from "./lib/Welcome.svelte";
  import { buildMenus, isEnabled, itemForKey, menuCommands, menuForAltKey } from "./lib/menus";
  import type { Command } from "./lib/palette";
  import { loadSettings, settings, updateSettings } from "./lib/settings.svelte";
  import { activeTab, loadWorkspace, newTab, showTab, tabs, view } from "./lib/tabs.svelte";
  import { appScheme } from "./lib/theme.svelte";
  import { checkForUpdate, type UpdateCheck } from "./lib/update";

  type AppInfo = { version: string; os: string; arch: string };

  let info = $state<AppInfo>();
  let ready = $state(false);
  let aboutOpen = $state(false);
  let settingsOpen = $state(false);
  let bondsOpen = $state(false);
  /** The command palette's commands while it is open. */
  let palette: Command[] | null = $state(null);
  /** What looking for a newer version found, while it is shown. */
  let updateCheck: UpdateCheck | null = $state(null);
  /** Whether one of the app's own dialogs is open, which has the keyboard. */
  const dialogOpen = $derived(aboutOpen || settingsOpen || bondsOpen || palette !== null || updateCheck !== null);
  let menuBar: MenuBar;

  const menus = buildMenus({
    about: () => (aboutOpen = true),
    exit: () => void getCurrentWindow().close(),
    newTab,
    settings: () => (settingsOpen = true),
    commandPalette: openPalette,
    checkUpdates: () => void lookForUpdate(true),
    bonds: () => (bondsOpen = true),
  });

  /**
   * Looks on GitHub for a newer bleNIMA. Asked from the Help menu, it says
   * what it found; when bleNIMA starts, it speaks only of a newer version
   * that was not skipped, and says nothing when GitHub cannot be reached.
   */
  async function lookForUpdate(asked: boolean) {
    if (!info) {
      return;
    }
    const check = await checkForUpdate(info.version);
    if (asked || (check.kind === "newer" && check.release.version !== settings.updates.skipped)) {
      updateCheck = check;
    }
  }

  /** Opens the command palette with what can be done now: the menus and the tabs. */
  function openPalette() {
    const commands = menuCommands(menus);
    tabs.forEach((tab, index) =>
      commands.push({
        label: `Go to ${tab.name}`,
        group: "Tab",
        checked: index === view.active,
        run: () => showTab(index),
      }),
    );
    palette = commands;
  }

  onMount(() => {
    const appInfo = invoke<AppInfo>("app_info").then((result) => (info = result));
    // The workspace waits for the settings, which say how much of it comes back.
    loadSettings()
      .catch((error) => console.error(error))
      .then(() => loadWorkspace())
      .catch((error) => console.error(error))
      .finally(() => (ready = true))
      // Once the tabs are back, if the settings let it.
      .then(() => appInfo)
      .then(() => (settings.updates.check ? lookForUpdate(false) : undefined))
      .catch((error) => console.error(error));
  });

  // The page and the window frame follow the theme.
  $effect(() => {
    document.documentElement.dataset.theme = appScheme();
  });
  $effect(() => {
    const theme = settings.theme;
    void getCurrentWindow().setTheme(theme === "system" ? null : theme);
  });

  function onkeydown(event: KeyboardEvent) {
    if (dialogOpen) {
      return;
    }
    const item = itemForKey(menus, event);
    if (item) {
      event.preventDefault();
      if (isEnabled(item)) {
        item.run?.();
      }
      return;
    }
    const menu = menuForAltKey(menus, event);
    if (menu !== null) {
      event.preventDefault();
      void menuBar.openMenu(menu);
    }
  }

  /** The web view's own menu, with its emoji and writing direction, only where text is typed. */
  function oncontextmenu(event: MouseEvent) {
    const target = event.target instanceof Element ? event.target : null;
    if (!target?.closest("input, textarea, [contenteditable]")) {
      event.preventDefault();
    }
  }
</script>

<svelte:window {onkeydown} {oncontextmenu} />

<div class="app">
  <MenuBar bind:this={menuBar} {menus} onclose={() => {}} />
  <TabBar onnew={newTab} />

  <main class="pane" style:--view-size={`${settings.font_size}px`}>
    {#if ready}
      {#if tabs.length === 0}
        <Welcome onscan={() => newTab("scanner")} />
      {/if}
      <!-- Every tab stays, hidden behind the one in front, so a terminal keeps what it showed. -->
      {#each tabs as tab, i (tab.id)}
        <div class="slot" class:hidden={i !== view.active}>
          {#if tab.scanner}
            <ScannerView {tab} />
          {:else if tab.device}
            <DeviceView {tab} active={i === view.active} />
          {:else if tab.server}
            <SimulatorView {tab} />
          {:else}
            <Coming {tab} onscan={() => newTab("scanner")} />
          {/if}
        </div>
      {/each}
    {/if}
  </main>

  <StatusBar tab={activeTab()} {info} />
</div>

{#if bondsOpen}
  <BondsDialog onclose={() => (bondsOpen = false)} />
{/if}

{#if settingsOpen}
  <SettingsDialog onclose={() => (settingsOpen = false)} />
{/if}

{#if palette}
  <CommandPalette commands={palette} onclose={() => (palette = null)} />
{/if}

{#if updateCheck && info}
  <UpdateDialog
    check={updateCheck}
    current={info.version}
    onskip={(version) => {
      updateSettings({ updates: { ...settings.updates, skipped: version } });
      updateCheck = null;
    }}
    onclose={() => (updateCheck = null)}
  />
{/if}

{#if aboutOpen && info}
  <About version={info.version} onclose={() => (aboutOpen = false)} />
{/if}

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .pane {
    position: relative;
    flex: 1;
    min-height: 0;
    background: var(--bg);
  }

  .slot {
    position: absolute;
    inset: 0;
  }

  .slot.hidden {
    display: none;
  }
</style>
