<script lang="ts">
  import Modal from "./Modal.svelte";
  import { addressText } from "./mesh";
  import type { AuthMethod, MeshState, Provisioning } from "./mesh.svelte";

  // Provisioning one device, step by step: what it can do, the OOB method,
  // the value, and how it ended.
  let { mesh, provisioning }: { mesh: MeshState; provisioning: Provisioning } = $props();

  const OUTPUT_ACTIONS = ["Blink", "Beep", "Vibrate", "Output Numeric", "Output Alphanumeric"];
  const INPUT_ACTIONS = ["Push", "Twist", "Input Numeric", "Input Alphanumeric"];

  type Choice = "no_oob" | "static" | "output" | "input";

  const capabilities = $derived(provisioning.capabilities);
  const oobOnly = $derived(((capabilities?.oob_type ?? 0) & 0b10) !== 0);
  const hasStatic = $derived(((capabilities?.oob_type ?? 0) & 0b01) !== 0);
  const outputs = $derived(OUTPUT_ACTIONS.map((_, bit) => bit).filter((bit) => ((capabilities?.output_actions ?? 0) & (1 << bit)) !== 0));
  const inputs = $derived(INPUT_ACTIONS.map((_, bit) => bit).filter((bit) => ((capabilities?.input_actions ?? 0) & (1 << bit)) !== 0));
  const canOutput = $derived((capabilities?.output_size ?? 0) > 0 && outputs.length > 0);
  const canInput = $derived((capabilities?.input_size ?? 0) > 0 && inputs.length > 0);

  let choice: Choice | null = $state(null);
  let outputAction = $state(0);
  let outputSize = $state(1);
  let inputAction = $state(0);
  let inputSize = $state(1);
  let typed = $state("");

  // The first method the device has, once its capabilities are known: No OOB
  // where it is allowed, as most boards in development are provisioned.
  $effect(() => {
    if (!capabilities || choice !== null) return;
    choice = !oobOnly ? "no_oob" : canOutput ? "output" : canInput ? "input" : hasStatic ? "static" : null;
    outputAction = outputs[0] ?? 0;
    outputSize = capabilities.output_size || 1;
    inputAction = inputs[0] ?? 0;
    inputSize = capabilities.input_size || 1;
  });

  function method(): AuthMethod | null {
    switch (choice) {
      case "no_oob":
        return { method: "no_oob" };
      case "static":
        return { method: "static" };
      case "output":
        return { method: "output", action: outputAction, size: outputSize };
      case "input":
        return { method: "input", action: inputAction, size: inputSize };
      default:
        return null;
    }
  }

  function submit() {
    if (provisioning.step === "method") {
      const chosen = method();
      if (chosen) mesh.chooseMethod(chosen);
    } else if (provisioning.step === "value" && typed.trim()) {
      mesh.giveValue(typed);
      typed = "";
    } else if (provisioning.step === "done" || provisioning.step === "failed") {
      mesh.closeProvisioning();
    }
  }

  /** What the user does with the value bleNIMA shows, for Input OOB. */
  function showText(chosen: AuthMethod | null, value: string): string {
    if (chosen?.method !== "input") return value;
    if (chosen.action === 0) return `Push the device's button ${value} times.`;
    if (chosen.action === 1) return `Twist the device's knob ${value} times.`;
    return `Put ${value} into the device.`;
  }

  /** What the value typed is, for Static and Output OOB. */
  function askText(chosen: AuthMethod | null): string {
    if (chosen?.method === "static") return "The device's static OOB value, as hex: 16 bytes, or 32 for the HMAC-SHA256 algorithm.";
    if (chosen?.method === "output") {
      if (chosen.action <= 2) return `How many times did the device ${["blink", "beep", "vibrate"][chosen.action]}?`;
      return chosen.action === 3 ? "The number the device shows." : "The letters and digits the device shows.";
    }
    return "";
  }

  const name = $derived(provisioning.name || provisioning.id);
  const busy = $derived(provisioning.step === "connecting" || provisioning.step === "working");
</script>

<Modal title={`Provision ${name}`} onclose={() => mesh.closeProvisioning()} onsubmit={submit}>
  {#if busy}
    <p class="waiting">
      <span class="spinner"></span>
      {provisioning.step === "connecting" ? "Connecting, and asking what the device can do" : "Provisioning: keys are swapped and checked"}
    </p>
  {/if}

  {#if capabilities && provisioning.step === "method"}
    <p class="muted">
      {capabilities.elements} element{capabilities.elements === 1 ? "" : "s"}, so
      {capabilities.elements === 1 ? "address" : "addresses from"}
      <b class="mono">{addressText(provisioning.address ?? 0)}</b>. Algorithm:
      <b class="mono">{provisioning.algorithm === "hmac_sha256" ? "BTM_ECDH_P256_HMAC_SHA256_AES_CCM" : "BTM_ECDH_P256_CMAC_AES128_AES_CCM"}</b>.
    </p>
    <div class="methods" role="radiogroup" aria-label="OOB method">
      <label class:off={oobOnly}>
        <input type="radio" bind:group={choice} value="no_oob" disabled={oobOnly} />
        <span>
          <b>No OOB</b>
          <span class="muted">{oobOnly ? "This device takes only OOB authentication" : "Nothing is checked: any device in range could have answered"}</span>
        </span>
      </label>
      <label class:off={!hasStatic}>
        <input type="radio" bind:group={choice} value="static" disabled={!hasStatic} />
        <span>
          <b>Static OOB</b>
          <span class="muted">{hasStatic ? "A value the device has from the start, such as one printed on it" : "This device has no static value"}</span>
        </span>
      </label>
      <label class:off={!canOutput}>
        <input type="radio" bind:group={choice} value="output" disabled={!canOutput} />
        <span>
          <b>Output OOB</b>
          <span class="muted">{canOutput ? "The device puts out a value, and you type it here" : "This device puts out no value"}</span>
          {#if canOutput && choice === "output"}
            <span class="row">
              <select class="field small" bind:value={outputAction}>
                {#each outputs as action (action)}<option value={action}>{OUTPUT_ACTIONS[action]}</option>{/each}
              </select>
              <select class="field small" bind:value={outputSize} title="How many digits or letters">
                {#each Array.from({ length: capabilities.output_size }, (_, n) => n + 1) as size (size)}
                  <option value={size}>{size} {size === 1 ? "digit" : "digits"}</option>
                {/each}
              </select>
            </span>
          {/if}
        </span>
      </label>
      <label class:off={!canInput}>
        <input type="radio" bind:group={choice} value="input" disabled={!canInput} />
        <span>
          <b>Input OOB</b>
          <span class="muted">{canInput ? "bleNIMA shows a value, and you put it into the device" : "This device takes no value"}</span>
          {#if canInput && choice === "input"}
            <span class="row">
              <select class="field small" bind:value={inputAction}>
                {#each inputs as action (action)}<option value={action}>{INPUT_ACTIONS[action]}</option>{/each}
              </select>
              <select class="field small" bind:value={inputSize} title="How many digits or letters">
                {#each Array.from({ length: capabilities.input_size }, (_, n) => n + 1) as size (size)}
                  <option value={size}>{size} {size === 1 ? "digit" : "digits"}</option>
                {/each}
              </select>
            </span>
          {/if}
        </span>
      </label>
    </div>
  {/if}

  {#if provisioning.step === "value"}
    <p>{askText(provisioning.method)}</p>
    <!-- svelte-ignore a11y_autofocus -->
    <input class="field value mono" bind:value={typed} spellcheck="false" autofocus />
  {/if}

  {#if provisioning.step === "show"}
    <p class="shown mono">{provisioning.value}</p>
    <p>{showText(provisioning.method, provisioning.value)}</p>
    <p class="muted">bleNIMA goes on once the device says the value was put in.</p>
  {/if}

  {#if provisioning.step === "done"}
    <p class="ok">
      {name} is node <b class="mono">{addressText(provisioning.address ?? 0)}</b> now.
    </p>
    <p class="muted">
      It comes back as a proxy, most often, and bleNIMA connects to it by itself. Then read its composition, and give it an
      application key.
    </p>
  {/if}

  {#if provisioning.step === "failed"}
    <p class="bad">{provisioning.problem}</p>
  {/if}

  {#snippet buttons()}
    {#if provisioning.step === "done" || provisioning.step === "failed"}
      <button class="btn primary" onclick={() => mesh.closeProvisioning()}>Close</button>
    {:else}
      <button class="btn" onclick={() => mesh.closeProvisioning()}>Cancel</button>
      {#if provisioning.step === "method"}
        <button class="btn primary" disabled={choice === null} onclick={submit}>Provision</button>
      {:else if provisioning.step === "value"}
        <button class="btn primary" disabled={!typed.trim()} onclick={submit}>OK</button>
      {/if}
    {/if}
  {/snippet}
</Modal>

<style>
  p {
    margin: 0 0 10px;
  }

  .muted {
    color: var(--muted);
  }

  .waiting {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--muted);
  }

  .spinner {
    width: 14px;
    height: 14px;
    border: 2px solid var(--border);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .methods {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .methods label {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
  }

  .methods label.off {
    opacity: 0.55;
  }

  .methods label > span {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .row {
    display: flex;
    gap: 8px;
    margin-top: 6px;
  }

  .small {
    height: 26px;
    font-size: 12px;
  }

  .value {
    width: 100%;
    box-sizing: border-box;
  }

  .shown {
    margin: 6px 0 12px;
    font-size: 32px;
    font-weight: 600;
    color: var(--accent);
    letter-spacing: 0.08em;
  }

  .ok {
    color: var(--ok);
  }

  .bad {
    color: var(--bad);
  }
</style>
