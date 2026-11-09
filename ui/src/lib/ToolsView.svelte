<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import Chart, { align } from "./Chart.svelte";
  import FirmwareView from "./FirmwareView.svelte";
  import { PROPERTY, type CharacteristicInfo, type Source } from "./bluetooth";
  import { parseText } from "./format";
  import { isTarget, lostPackets, matches, parseScript, stats, type Stats, type Step } from "./tests";
  import type { Tab } from "./tabs.svelte";

  let { tab }: { tab: Tab } = $props();
  const device = $derived(tab.device!);

  type Tool = "throughput" | "latency" | "soak" | "script" | "firmware";
  let tool: Tool = $state("throughput");

  const all = $derived(device.services.flatMap((service) => service.characteristics));
  const writable = $derived(all.filter((c) => c.properties & (PROPERTY.write | PROPERTY.writeWithoutResponse)));
  const notifying = $derived(all.filter((c) => c.properties & (PROPERTY.notify | PROPERTY.indicate)));
  const label = (c: CharacteristicInfo) => `${c.user_description || c.name || "Characteristic"} (${c.uuid})`;
  const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

  /** The next value of a characteristic from these sources, or null after `timeout` ms. */
  function nextValue(key: string, sources: Source[], timeout: number): Promise<number[] | null> {
    return new Promise((resolve) => {
      const listener = (from: string, data: number[], source: Source) => {
        if (from === key && sources.includes(source)) {
          done(data);
        }
      };
      const timer = setTimeout(() => done(null), timeout);
      function done(data: number[] | null) {
        clearTimeout(timer);
        device.listeners.delete(listener);
        resolve(data);
      }
      device.listeners.add(listener);
    });
  }

  function withResponse(key: string): boolean {
    const target = device.characteristic(key);
    return !target || (target.properties & PROPERTY.writeWithoutResponse) === 0;
  }

  // ---- Throughput ----
  // The terminal's characteristics are where each test starts.
  let tpNotify = $state(untrack(() => tab.terminal.notify));
  let tpCounter: 0 | 1 | 2 = $state(0);
  let tpRunning = $state(false);
  let tpBytes = $state(0);
  let tpPackets = $state(0);
  let tpStarted = 0;
  let tpCounters: number[] = [];
  let tpLost = $state(0);
  let tpSeries = $state({ at: [] as number[], values: [] as number[] });
  let tpVersion = $state(0);
  let tpWindow = 0;
  let tpTimer: ReturnType<typeof setInterval> | undefined;
  let tpListener: ((key: string, data: number[], source: Source) => void) | null = null;

  function throughputStart() {
    if (!tpNotify) return;
    tpBytes = 0;
    tpPackets = 0;
    tpLost = 0;
    tpCounters = [];
    tpSeries = { at: [], values: [] };
    tpWindow = 0;
    tpStarted = performance.now();
    tpListener = (key, data, source) => {
      if (key !== tpNotify || source !== "notify") return;
      tpBytes += data.length;
      tpPackets++;
      tpWindow += data.length;
      if (tpCounter > 0 && data.length >= tpCounter) tpCounters.push(tpCounter === 1 ? data[0] : data[0] | (data[1] << 8));
    };
    device.listeners.add(tpListener);
    if (!device.subscribed.has(tpNotify)) device.subscribe(tpNotify, true);
    tpTimer = setInterval(() => {
      tpSeries.at.push((performance.now() - tpStarted) / 1000);
      tpSeries.values.push(tpWindow / 1024);
      tpWindow = 0;
      if (tpCounter > 0) tpLost = lostPackets(tpCounters, tpCounter === 1 ? 256 : 65536);
      tpVersion++;
    }, 1000);
    tpRunning = true;
  }

  function throughputStop() {
    if (tpListener) device.listeners.delete(tpListener);
    tpListener = null;
    clearInterval(tpTimer);
    tpRunning = false;
  }

  const tpSeconds = $derived(tpVersion >= 0 && tpRunning ? (performance.now() - tpStarted) / 1000 : tpSeries.at.at(-1) ?? 0);

  // ---- Write flood ----
  let floodWrite = $state(untrack(() => tab.terminal.write));
  let floodCount = $state(100);
  let floodResult = $state("");
  let floodRunning = $state(false);

  async function flood() {
    if (!floodWrite || !device.connected) return;
    floodRunning = true;
    floodResult = "";
    const size = Math.max(20, (device.link?.mtu ?? 23) - 3);
    const response = withResponse(floodWrite);
    let confirmed = 0;
    const all = new Promise<void>((resolve) => {
      const listener = (key: string, _data: number[], source: Source) => {
        if (key === floodWrite && (source === "write" || source === "write_without_response") && ++confirmed >= floodCount) {
          device.listeners.delete(listener);
          resolve();
        }
      };
      device.listeners.add(listener);
    });
    const started = performance.now();
    for (let at = 0; at < floodCount; at++) {
      const data = Array.from({ length: size }, (_, i) => (at + i) & 0xff);
      device.write(floodWrite, data, response);
    }
    const timeout = sleep(60_000).then(() => "timeout");
    const outcome = await Promise.race([all.then(() => "done"), timeout]);
    const seconds = (performance.now() - started) / 1000;
    floodResult =
      outcome === "done"
        ? `${floodCount} writes of ${size} bytes ${response ? "with" : "without"} response in ${seconds.toFixed(2)} s: ${((floodCount * size) / 1024 / seconds).toFixed(1)} kB/s`
        : `Only ${confirmed} of ${floodCount} writes went through in a minute`;
    floodRunning = false;
  }

  // ---- Latency ----
  let latWrite = $state(untrack(() => tab.terminal.write));
  let latNotify = $state(untrack(() => tab.terminal.notify));
  let latPayload = $state("ping\\r");
  let latRounds = $state(20);
  let latRunning = $state(false);
  let latTimes: number[] = $state([]);
  let latMissed = $state(0);
  const latStats: Stats | null = $derived(stats(latTimes));

  async function latency() {
    if (!latWrite || !latNotify || !device.connected) return;
    latRunning = true;
    latTimes = [];
    latMissed = 0;
    if (!device.subscribed.has(latNotify)) {
      device.subscribe(latNotify, true);
      await sleep(500);
    }
    const data = parseText(latPayload);
    for (let round = 0; round < latRounds && latRunning; round++) {
      const answer = nextValue(latNotify, ["notify"], 3000);
      const started = performance.now();
      device.write(latWrite, data, withResponse(latWrite));
      if ((await answer) === null) latMissed++;
      else latTimes = [...latTimes, performance.now() - started];
      await sleep(50);
    }
    latRunning = false;
  }

  // ---- Soak ----
  let soakCycles = $state(10);
  let soakHold = $state(1000);
  let soakRunning = $state(false);
  let soakDone = $state(0);
  let soakFailed = $state(0);
  let soakTimes: number[] = $state([]);
  const soakStats: Stats | null = $derived(stats(soakTimes));

  async function waitConnected(timeout: number): Promise<boolean> {
    const until = performance.now() + timeout;
    while (performance.now() < until) {
      if (device.connected && device.services.length > 0) return true;
      await sleep(100);
    }
    return false;
  }

  async function soak() {
    soakRunning = true;
    soakDone = 0;
    soakFailed = 0;
    soakTimes = [];
    for (let cycle = 0; cycle < soakCycles && soakRunning; cycle++) {
      await device.disconnect();
      device.services = [];
      await sleep(800);
      const started = performance.now();
      await device.connect();
      if (await waitConnected(45_000)) soakTimes = [...soakTimes, performance.now() - started];
      else soakFailed++;
      soakDone++;
      await sleep(soakHold);
    }
    soakRunning = false;
  }

  // ---- Send and expect ----
  let script = $state(`# Steps, one a line. Characteristics by short UUID, full UUID or 'Name'.
# subscribe 6E400003
# write-nr 6E400002 "kernel version\\r"
# expect 6E400003 /Zephyr version/ within 3000
read 'Device Name'
`);
  let scriptRepeat = $state(1);
  let scriptRunning = $state(false);
  let report: { line: number; ok: boolean; text: string }[] = $state([]);
  let scriptProblem = $state("");

  function keyFor(name: string): string | null {
    const found = all.find((c) => isTarget(c.user_description || c.name || "", c.uuid, c.key, name));
    return found?.key ?? null;
  }

  async function runStep(step: Step): Promise<string | null> {
    if (step.kind === "wait") {
      await sleep(step.ms);
      return null;
    }
    const key = keyFor(step.target);
    if (!key) return `No characteristic ${step.target}`;
    switch (step.kind) {
      case "subscribe": {
        if (!device.subscribed.has(key)) device.subscribe(key, true);
        await sleep(300);
        return device.subscribed.has(key) ? null : "Could not subscribe";
      }
      case "write": {
        const confirmed = nextValue(key, ["write", "write_without_response"], 5000);
        device.write(key, step.data, step.withResponse);
        return (await confirmed) === null ? "The write was not confirmed in 5 s" : null;
      }
      case "read": {
        const value = nextValue(key, ["read"], 5000);
        device.read(key);
        const data = await value;
        if (data === null) return "No answer to the read in 5 s";
        if (step.expect && !matches(step.expect, data)) return `Read ${device.describe(key, data)}`;
        return null;
      }
      case "expect": {
        const until = performance.now() + step.within;
        while (performance.now() < until) {
          const data = await nextValue(key, ["notify", "read"], until - performance.now());
          if (data === null) break;
          if (matches(step.expect, data)) return null;
        }
        return `Nothing matching came in ${step.within} ms`;
      }
    }
  }

  async function runScript() {
    scriptProblem = "";
    const parsed = parseScript(script);
    if ("error" in parsed) {
      scriptProblem = `Line ${parsed.line}: ${parsed.error}`;
      return;
    }
    scriptRunning = true;
    report = [];
    for (let round = 1; round <= scriptRepeat && scriptRunning; round++) {
      for (const step of parsed.steps) {
        if (!scriptRunning) break;
        const started = performance.now();
        const problem = await runStep(step);
        const ms = Math.round(performance.now() - started);
        const source = script.split(/\r?\n/)[step.line - 1].trim();
        report = [...report, { line: step.line, ok: problem === null, text: `${source}  (${ms} ms)${problem ? `: ${problem}` : ""}` }];
      }
    }
    scriptRunning = false;
  }

  const passed = $derived(report.filter((entry) => entry.ok).length);

  function reportText(): string {
    return [
      `bleNIMA send and expect: ${tab.name} (${device.device})`,
      `${passed} of ${report.length} steps passed`,
      ...report.map((entry) => `${entry.ok ? "PASS" : "FAIL"} line ${entry.line}: ${entry.text}`),
    ].join("\n");
  }

  onDestroy(() => {
    throughputStop();
    latRunning = false;
    soakRunning = false;
    scriptRunning = false;
  });

  const ms = (value: number) => `${value.toFixed(1)} ms`;
</script>

<div class="tools">
  <nav class="menu">
    {#each [["throughput", "Throughput"], ["latency", "Latency"], ["soak", "Connection soak"], ["script", "Send and expect"], ["firmware", "Firmware update"]] as [id, name] (id)}
      <button class:on={tool === id} onclick={() => (tool = id as Tool)}>{name}</button>
    {/each}
  </nav>

  <div class="page">
    {#if !device.connected && tool !== "firmware"}
      <p class="note">Connect to the device to run a test.</p>
    {/if}

    {#if tool === "throughput"}
      <h2>Throughput</h2>
      <p class="muted">Counts the bytes the device notifies, second by second.</p>
      <div class="row">
        <label class="option">Notifications of
          <select class="field small" bind:value={tpNotify}>
            <option value="">(pick one)</option>
            {#each notifying as c (c.key)}<option value={c.key}>{label(c)}</option>{/each}
          </select>
        </label>
        <label class="option" title="A counter at the start of each payload finds lost packets">Counter
          <select class="field small" bind:value={tpCounter}>
            <option value={0}>None</option>
            <option value={1}>1 byte</option>
            <option value={2}>2 bytes</option>
          </select>
        </label>
        {#if tpRunning}
          <button class="btn small" onclick={throughputStop}>Stop</button>
        {:else}
          <button class="btn small primary" disabled={!device.connected || !tpNotify} onclick={throughputStart}>Start</button>
        {/if}
      </div>
      <div class="numbers">
        <div><b>{(tpBytes / 1024).toFixed(1)}</b> kB</div>
        <div><b>{tpPackets}</b> notifications</div>
        <div><b>{tpSeconds > 0 ? (tpBytes / 1024 / tpSeconds).toFixed(2) : "0"}</b> kB/s on average</div>
        {#if tpCounter > 0}<div><b>{tpLost}</b> lost</div>{/if}
      </div>
      {#if tpSeries.at.length > 0}
        <div class="chart">
          <Chart labels={["kB/s"]} data={align([tpSeries])} version={tpVersion} unit="kB/s" />
        </div>
      {/if}

      <h3>Write flood</h3>
      <p class="muted">Writes packets the size of the MTU as fast as the OS takes them.</p>
      <div class="row">
        <label class="option">Write to
          <select class="field small" bind:value={floodWrite}>
            <option value="">(pick one)</option>
            {#each writable as c (c.key)}<option value={c.key}>{label(c)}</option>{/each}
          </select>
        </label>
        <label class="option">Packets <input class="field small number" type="number" min="1" max="100000" bind:value={floodCount} /></label>
        <button class="btn small primary" disabled={!device.connected || !floodWrite || floodRunning} onclick={flood}>Send</button>
      </div>
      {#if floodResult}<p class="result">{floodResult}</p>{/if}
    {:else if tool === "latency"}
      <h2>Latency</h2>
      <p class="muted">Writes, and times the notification that answers, round after round.</p>
      <div class="row">
        <label class="option">Write to
          <select class="field small" bind:value={latWrite}>
            <option value="">(pick one)</option>
            {#each writable as c (c.key)}<option value={c.key}>{label(c)}</option>{/each}
          </select>
        </label>
        <label class="option">Answer on
          <select class="field small" bind:value={latNotify}>
            <option value="">(pick one)</option>
            {#each notifying as c (c.key)}<option value={c.key}>{label(c)}</option>{/each}
          </select>
        </label>
      </div>
      <div class="row">
        <label class="option">Payload <input class="field small mono" bind:value={latPayload} /></label>
        <label class="option">Rounds <input class="field small number" type="number" min="1" max="10000" bind:value={latRounds} /></label>
        {#if latRunning}
          <button class="btn small" onclick={() => (latRunning = false)}>Stop</button>
        {:else}
          <button class="btn small primary" disabled={!device.connected || !latWrite || !latNotify} onclick={latency}>Start</button>
        {/if}
      </div>
      {#if latStats}
        <div class="numbers">
          <div><b>{ms(latStats.min)}</b> min</div>
          <div><b>{ms(latStats.median)}</b> median</div>
          <div><b>{ms(latStats.mean)}</b> mean</div>
          <div><b>{ms(latStats.p95)}</b> 95%</div>
          <div><b>{ms(latStats.max)}</b> max</div>
          <div><b>{latMissed}</b> without answer</div>
        </div>
        <div class="chart">
          <Chart
            labels={["round trip"]}
            data={align([{ at: latTimes.map((_, at) => at + 1), values: latTimes }])}
            version={latTimes.length}
            unit="ms"
            timeLabel="round"
          />
        </div>
      {/if}
    {:else if tool === "soak"}
      <h2>Connection soak test</h2>
      <p class="muted">Disconnects and connects again, cycle after cycle, and times each connection to the end of discovery.</p>
      <div class="row">
        <label class="option">Cycles <input class="field small number" type="number" min="1" max="10000" bind:value={soakCycles} /></label>
        <label class="option">Stay connected <input class="field small number" type="number" min="0" max="600000" bind:value={soakHold} /> ms</label>
        {#if soakRunning}
          <button class="btn small" onclick={() => (soakRunning = false)}>Stop</button>
        {:else}
          <button class="btn small primary" onclick={soak}>Start</button>
        {/if}
      </div>
      {#if soakDone > 0}
        <div class="numbers">
          <div><b>{soakDone}</b> of {soakCycles} cycles</div>
          <div><b class:bad={soakFailed > 0}>{soakFailed}</b> failed</div>
          {#if soakStats}
            <div><b>{(soakStats.median / 1000).toFixed(2)} s</b> median to connect</div>
            <div><b>{(soakStats.max / 1000).toFixed(2)} s</b> slowest</div>
          {/if}
        </div>
      {/if}
    {:else if tool === "script"}
      <h2>Send and expect</h2>
      <p class="muted">
        Steps: <code>subscribe</code>, <code>write</code>, <code>write-nr</code>, <code>read … expect</code>,
        <code>expect … within ms</code>, <code>wait</code>. Values are "text", /regex/ or hex AA 55.
      </p>
      <textarea class="field script mono" bind:value={script} spellcheck="false"></textarea>
      <div class="row">
        <label class="option">Run <input class="field small number" type="number" min="1" max="10000" bind:value={scriptRepeat} /> times</label>
        {#if scriptRunning}
          <button class="btn small" onclick={() => (scriptRunning = false)}>Stop</button>
        {:else}
          <button class="btn small primary" disabled={!device.connected} onclick={runScript}>Run</button>
        {/if}
        {#if report.length > 0}
          <button class="btn small" onclick={() => navigator.clipboard.writeText(reportText())}>Copy report</button>
        {/if}
      </div>
      {#if scriptProblem}<p class="bad">{scriptProblem}</p>{/if}
      {#if report.length > 0}
        <p class="result" class:bad={passed < report.length}>
          {passed === report.length ? "PASS" : "FAIL"}: {passed} of {report.length} steps passed
        </p>
        <div class="report">
          {#each report as entry, at (at)}
            <div class="step" class:fail={!entry.ok}><b>{entry.ok ? "✓" : "✗"}</b> <span class="mono">{entry.text}</span></div>
          {/each}
        </div>
      {/if}
    {:else}
      <FirmwareView {device} />
    {/if}
  </div>
</div>

<style>
  .tools {
    display: flex;
    height: 100%;
    font-size: var(--view-size);
  }

  .menu {
    display: flex;
    flex-direction: column;
    width: 190px;
    flex-shrink: 0;
    padding: 8px;
    border-right: 1px solid var(--border);
  }

  .menu button {
    padding: 8px 10px;
    font: inherit;
    color: var(--text);
    text-align: left;
    background: none;
    border: 0;
    border-radius: 6px;
  }

  .menu button:hover {
    background: var(--hover);
  }

  .menu button.on {
    background: var(--accent-soft);
  }

  .page {
    flex: 1;
    min-width: 0;
    padding: 14px 18px;
    overflow-y: auto;
    user-select: text;
  }

  h2 {
    margin: 0 0 4px;
    font-size: 17px;
    font-weight: 600;
  }

  h3 {
    margin: 22px 0 4px;
    font-size: 14px;
    font-weight: 600;
  }

  .muted {
    margin: 0 0 10px;
    color: var(--muted);
  }

  .note {
    margin: 0 0 12px;
    color: var(--warn);
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 14px;
    margin: 8px 0;
  }

  .option {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--muted);
  }

  .option select {
    max-width: 300px;
  }

  .small {
    height: 26px;
    padding: 0 10px;
    font-size: 12px;
  }

  .number {
    width: 90px;
  }

  .numbers {
    display: flex;
    flex-wrap: wrap;
    gap: 10px 28px;
    margin: 14px 0;
    color: var(--muted);
  }

  .numbers b {
    font-size: 1.4em;
    font-weight: 600;
    color: var(--text);
  }

  .bad {
    color: var(--bad) !important;
  }

  .chart {
    height: 240px;
  }

  .result {
    margin: 10px 0;
    font-weight: 600;
    color: var(--ok);
  }

  .script {
    width: 100%;
    height: 180px;
    padding: 8px;
    resize: vertical;
    box-sizing: border-box;
  }

  .report {
    max-height: 300px;
    overflow-y: auto;
  }

  .step {
    padding: 3px 0;
    color: var(--ok);
    border-bottom: 1px solid var(--border);
  }

  .step.fail {
    color: var(--bad);
  }

  code {
    font-family: var(--font-mono);
    font-size: 0.92em;
  }
</style>
