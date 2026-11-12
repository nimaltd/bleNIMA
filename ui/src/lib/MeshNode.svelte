<script lang="ts">
  import {
    SERVER,
    addressText,
    answerOf,
    client,
    config,
    describeMessage,
    featuresText,
    modelName,
    opcodeBytes,
    parseAddress,
    parseHex,
    splitOpcode,
    type Bytes,
    type Model,
  } from "./mesh";
  import { keyText, type KeyRef, type MeshNode, type MeshState } from "./mesh.svelte";

  // One node: its keys, what it is made of, its models' configuration, and
  // the clients that talk to its models.
  let { mesh, node }: { mesh: MeshState; node: MeshNode } = $props();

  const composition = $derived(mesh.composition(node));
  const appKeys = $derived(mesh.network?.app_keys ?? []);

  /** The last answer, or why there was none, in words. */
  let result = $state("");
  let resultBad = $state(false);
  let busy = $state(false);
  let confirming: "reset" | "forget" | null = $state(null);

  /** The application key that models are bound to, publish with, and the clients send with. */
  let appKey = $state(0);
  /** Where the clients send: "" for the element itself. */
  let sendTo = $state("");
  let acknowledged = $state(true);
  /** The model whose configuration is open, as "element/model". */
  let openModel: string | null = $state(null);
  let publishTo = $state("");
  let subscribeTo = $state("");

  let level = $state(0);
  let lightness = $state(65535);
  let temperature = $state(6500);
  let deltaUv = $state(0);
  let hue = $state(0);
  let saturation = $state(100);
  let property = $state("");
  let company = $state("");
  let testId = $state(0);
  let attention = $state(5);
  let divisor = $state(0);

  let rawTo = $state("");
  let rawKey = $state("device");
  let rawOpcode = $state("");
  let rawParams = $state("");

  let name = $state("");
  $effect(() => {
    name = node.name;
  });

  /** Sends a message and shows its answer; one that needs none shows it was sent. */
  async function run(dst: number, key: KeyRef, payload: Bytes) {
    busy = true;
    resultBad = false;
    result = `${describeMessage(payload).name} sent to ${addressText(dst)}`;
    try {
      const answer = await mesh.request(dst, key, payload);
      const split = splitOpcode(payload);
      const answerOpcode = split ? answerOf(split.opcode) : null;
      if (answer !== null && answerOpcode !== null) {
        const { name, text } = describeMessage([...opcodeBytes(answerOpcode), ...answer]);
        result = `${name} from ${addressText(dst)}${text ? `: ${text}` : ""}`;
        resultBad = /^(Invalid|Insufficient|Key Index|Not a|Storage|Feature|Cannot|Temporarily|Unspecified)/.test(text);
      }
    } catch (error) {
      result = (error as Error).message;
      resultBad = true;
    } finally {
      busy = false;
    }
  }

  const device: KeyRef = { type: "device" };
  const app = (): KeyRef => ({ type: "app", index: appKey });

  function configure(payload: Bytes) {
    void run(node.address, device, payload);
  }

  /** Where a client sends: the field above, or the element. */
  function target(element: number): number {
    return parseAddress(sendTo) ?? element;
  }

  function talk(element: number, payload: Bytes) {
    void run(target(element), app(), payload);
  }

  function addAppKey(index: number) {
    const key = appKeys.find((entry) => entry.index === index);
    const bytes = key ? parseHex(key.key) : null;
    if (bytes) configure(config.appKeyAdd(mesh.network?.net_key_index ?? 0, index, bytes));
  }

  function publish(element: number, model: Model, clear: boolean) {
    const address = clear ? 0 : parseAddress(publishTo);
    if (address === null) {
      result = "Publish to: a group or unicast address, such as 0xC000";
      resultBad = true;
      return;
    }
    configure(
      config.publicationSet(element, model, {
        address,
        appKey,
        credentials: false,
        ttl: 0xff,
        period: 0,
        retransmitCount: 0,
        retransmitSteps: 0,
      }),
    );
  }

  function subscription(element: number, model: Model, add: boolean) {
    const address = parseAddress(subscribeTo);
    if (address === null || address < 0xc000) {
      result = "Subscribe to: a group address, from 0xC000";
      resultBad = true;
      return;
    }
    configure(add ? config.subscribe(element, address, model) : config.unsubscribe(element, address, model));
  }

  function sendRaw() {
    const to = parseAddress(rawTo) ?? node.address;
    const opcode = parseHex(rawOpcode);
    const params = parseHex(rawParams);
    if (!opcode || opcode.length === 0 || !splitOpcode(opcode) || splitOpcode(opcode)?.params.length !== 0) {
      result = "Opcode: 1, 2 or 3 bytes of hex, such as 8201";
      resultBad = true;
      return;
    }
    if (!params) {
      result = "Parameters: hex bytes, such as 01 05";
      resultBad = true;
      return;
    }
    const key: KeyRef = rawKey === "device" ? device : { type: "app", index: Number(rawKey) };
    mesh.send(to, key, [...opcode, ...params]);
    resultBad = false;
    result = `Sent to ${addressText(to)} with the ${keyText(key)}: the answers are in the log`;
  }

  /** The element's address, from the node's first. */
  const elementAddress = (index: number) => node.address + index;
  const has = (models: Model[], id: number) => models.some((model) => model.vendor === null && model.id === id);
  const configurable = (model: Model) => !(model.vendor === null && (model.id === SERVER.config || model.id === 0x0001));
  const companyId = () => parseAddress(company) ?? composition?.cid ?? 0;
  const propertyId = () => (property.trim() ? (parseAddress(property) ?? undefined) : undefined);
</script>

<div class="node">
  <header>
    <input
      class="field name"
      bind:value={name}
      placeholder="Name this node"
      spellcheck="false"
      onchange={() => mesh.rename(node.address, name)}
    />
    <span class="muted">
      <b class="mono">{addressText(node.address)}</b>{#if node.elements > 1}&nbsp;to <b class="mono">{addressText(node.address + node.elements - 1)}</b>{/if},
      {node.elements} element{node.elements === 1 ? "" : "s"}
    </span>
  </header>
  <dl>
    <dt>Device UUID</dt>
    <dd class="mono">{node.uuid || "—"}</dd>
    <dt>Device key</dt>
    <dd class="mono">{node.device_key}</dd>
  </dl>

  {#if !mesh.connected}
    <p class="note">No proxy is connected: what is sent cannot reach the node.</p>
  {/if}

  <div class="row">
    <button class="btn small" class:primary={!composition} disabled={busy} onclick={() => configure(config.compositionGet(0))}
      >Read composition</button
    >
    {#if confirming === "reset"}
      <span class="warn">Reset it? It leaves the network and forgets its keys.</span>
      <button
        class="btn small"
        onclick={() => {
          confirming = null;
          configure(config.nodeReset());
        }}>Reset</button
      >
      <button class="btn small" onclick={() => (confirming = null)}>Cancel</button>
    {:else if confirming === "forget"}
      <span class="warn">Forget it here, without telling it? Do this for a node that is gone.</span>
      <button
        class="btn small"
        onclick={() => {
          confirming = null;
          mesh.forget(node.address);
        }}>Forget</button
      >
      <button class="btn small" onclick={() => (confirming = null)}>Cancel</button>
    {:else}
      <button class="btn small" disabled={busy} onclick={() => (confirming = "reset")}>Reset node...</button>
      <button class="btn small" onclick={() => (confirming = "forget")}>Forget...</button>
    {/if}
  </div>

  {#if result}
    <p class="result" class:bad={resultBad}>{result}</p>
  {/if}

  <h3>Application keys</h3>
  <div class="row">
    {#each appKeys as key (key.index)}
      <span class="muted">AppKey {key.index}</span>
      <button class="btn small" disabled={busy} onclick={() => addAppKey(key.index)}>Add to the node</button>
      <button
        class="btn small"
        disabled={busy}
        onclick={() => configure(config.appKeyDelete(mesh.network?.net_key_index ?? 0, key.index))}>Delete from it</button
      >
    {/each}
    <button class="btn small" disabled={busy} onclick={() => configure(config.appKeyGet(mesh.network?.net_key_index ?? 0))}
      >Which has it?</button
    >
  </div>

  <h3>Elements and models</h3>
  {#if !composition}
    <p class="muted">Read the node's composition to see its elements and models.</p>
  {:else}
    <p class="muted">
      Company <b class="mono">0x{composition.cid.toString(16).toUpperCase().padStart(4, "0")}</b>, product
      <b class="mono">0x{composition.pid.toString(16).toUpperCase().padStart(4, "0")}</b>, version
      <b class="mono">0x{composition.vid.toString(16).toUpperCase().padStart(4, "0")}</b>, {composition.crpl} replay entries,
      features: {featuresText(composition.features)}
    </p>
    <div class="row">
      <label class="option">
        Application key
        <select class="field small" bind:value={appKey}>
          {#each appKeys as key (key.index)}<option value={key.index}>AppKey {key.index}</option>{/each}
        </select>
      </label>
      <span class="muted">for binding, publishing, and the clients below</span>
    </div>
    {#each composition.elements as element, index (index)}
      <div class="element">
        <h4>Element {index + 1} <span class="mono muted">{addressText(elementAddress(index))}</span></h4>
        {#each element.models as model (`${model.vendor}/${model.id}`)}
          {@const key = `${index}/${model.vendor}/${model.id}`}
          <div class="model">
            {#if configurable(model)}
              <button class="link" onclick={() => (openModel = openModel === key ? null : key)}>
                {openModel === key ? "▾" : "▸"}
                {modelName(model)}
              </button>
            {:else}
              <span class="plain">{modelName(model)}</span>
            {/if}
          </div>
          {#if openModel === key}
            {@const at = elementAddress(index)}
            <div class="settings">
              <div class="row">
                <button class="btn tiny" disabled={busy} onclick={() => configure(config.bind(at, appKey, model))}>Bind AppKey {appKey}</button>
                <button class="btn tiny" disabled={busy} onclick={() => configure(config.unbind(at, appKey, model))}>Unbind it</button>
                <button class="btn tiny" disabled={busy} onclick={() => configure(config.bindingsGet(at, model))}>Bound keys</button>
              </div>
              <div class="row">
                <input class="field tiny address mono" bind:value={publishTo} placeholder="0xC000" spellcheck="false" />
                <button class="btn tiny" disabled={busy} onclick={() => publish(at, model, false)}>Publish there</button>
                <button class="btn tiny" disabled={busy} onclick={() => publish(at, model, true)}>Stop publishing</button>
                <button class="btn tiny" disabled={busy} onclick={() => configure(config.publicationGet(at, model))}>Where?</button>
              </div>
              <div class="row">
                <input class="field tiny address mono" bind:value={subscribeTo} placeholder="0xC000" spellcheck="false" />
                <button class="btn tiny" disabled={busy} onclick={() => subscription(at, model, true)}>Subscribe</button>
                <button class="btn tiny" disabled={busy} onclick={() => subscription(at, model, false)}>Unsubscribe</button>
                <button class="btn tiny" disabled={busy} onclick={() => configure(config.subscriptionsGet(at, model))}>Which?</button>
                <button class="btn tiny" disabled={busy} onclick={() => configure(config.subscriptionsClear(at, model))}>Clear all</button>
              </div>
            </div>
          {/if}
        {/each}
      </div>
    {/each}

    <h3>Clients</h3>
    <div class="row">
      <label class="option">
        Send to
        <input class="field small address mono" bind:value={sendTo} placeholder="the element" spellcheck="false" />
      </label>
      <label class="check"><input type="checkbox" bind:checked={acknowledged} /> Acknowledged</label>
      <span class="muted">with AppKey {appKey}: bind it to the model first</span>
    </div>
    {#each composition.elements as element, index (index)}
      {@const at = elementAddress(index)}
      {@const models = element.models}
      {#if [SERVER.onOff, SERVER.level, SERVER.lightness, SERVER.ctl, SERVER.hsl, SERVER.sensor, SERVER.health].some((id) => has(models, id))}
        <div class="element">
          <h4>Element {index + 1} <span class="mono muted">{addressText(at)}</span></h4>
          {#if has(models, SERVER.onOff)}
            <div class="client">
              <span class="label">Generic OnOff</span>
              <button class="btn tiny" disabled={busy} onclick={() => talk(at, client.onOffSet(true, mesh.tid(), acknowledged))}>On</button>
              <button class="btn tiny" disabled={busy} onclick={() => talk(at, client.onOffSet(false, mesh.tid(), acknowledged))}>Off</button>
              <button class="btn tiny" disabled={busy} onclick={() => talk(at, client.onOffGet())}>Get</button>
            </div>
          {/if}
          {#if has(models, SERVER.level)}
            <div class="client">
              <span class="label">Generic Level</span>
              <input class="field tiny number" type="number" min="-32768" max="32767" bind:value={level} />
              <button class="btn tiny" disabled={busy} onclick={() => talk(at, client.levelSet(level, mesh.tid(), acknowledged))}>Set</button>
              <button class="btn tiny" disabled={busy} onclick={() => talk(at, client.levelGet())}>Get</button>
            </div>
          {/if}
          {#if has(models, SERVER.lightness)}
            <div class="client">
              <span class="label">Light Lightness</span>
              <input class="field tiny number" type="number" min="0" max="65535" bind:value={lightness} />
              <button class="btn tiny" disabled={busy} onclick={() => talk(at, client.lightnessSet(lightness, mesh.tid(), acknowledged))}>Set</button>
              <button class="btn tiny" disabled={busy} onclick={() => talk(at, client.lightnessGet())}>Get</button>
            </div>
          {/if}
          {#if has(models, SERVER.ctl)}
            <div class="client">
              <span class="label">Light CTL</span>
              <input class="field tiny number" type="number" min="0" max="65535" bind:value={lightness} title="Lightness" />
              <input class="field tiny number" type="number" min="800" max="20000" bind:value={temperature} title="Temperature, K" />
              <span class="muted">K</span>
              <input class="field tiny number" type="number" min="-32768" max="32767" bind:value={deltaUv} title="Delta UV" />
              <button
                class="btn tiny"
                disabled={busy}
                onclick={() => talk(at, client.ctlSet(lightness, temperature, deltaUv, mesh.tid(), acknowledged))}>Set</button
              >
              <button class="btn tiny" disabled={busy} onclick={() => talk(at, client.ctlGet())}>Get</button>
            </div>
          {/if}
          {#if has(models, SERVER.hsl)}
            <div class="client">
              <span class="label">Light HSL</span>
              <input class="field tiny number" type="number" min="0" max="65535" bind:value={lightness} title="Lightness" />
              <input class="field tiny number" type="number" min="0" max="360" bind:value={hue} title="Hue, degrees" />
              <span class="muted">°</span>
              <input class="field tiny number" type="number" min="0" max="100" bind:value={saturation} title="Saturation, %" />
              <span class="muted">%</span>
              <button
                class="btn tiny"
                disabled={busy}
                onclick={() =>
                  talk(
                    at,
                    client.hslSet(
                      lightness,
                      Math.round((((hue % 360) + 360) % 360) * (65536 / 360)) & 0xffff,
                      Math.round((Math.min(100, Math.max(0, saturation)) / 100) * 65535),
                      mesh.tid(),
                      acknowledged,
                    ),
                  )}>Set</button
              >
              <button class="btn tiny" disabled={busy} onclick={() => talk(at, client.hslGet())}>Get</button>
            </div>
          {/if}
          {#if has(models, SERVER.sensor)}
            <div class="client">
              <span class="label">Sensor</span>
              <input class="field tiny address mono" bind:value={property} placeholder="any property" spellcheck="false" />
              <button class="btn tiny" disabled={busy} onclick={() => talk(at, client.sensorDescriptorGet(propertyId()))}>Descriptors</button>
              <button class="btn tiny" disabled={busy} onclick={() => talk(at, client.sensorGet(propertyId()))}>Get</button>
            </div>
          {/if}
          {#if has(models, SERVER.health)}
            <div class="client">
              <span class="label">Health</span>
              <input class="field tiny address mono" bind:value={company} placeholder={`0x${(composition.cid ?? 0).toString(16).toUpperCase().padStart(4, "0")}`} spellcheck="false" title="Company ID" />
              <button class="btn tiny" disabled={busy} onclick={() => talk(at, client.faultGet(companyId()))}>Faults</button>
              <button class="btn tiny" disabled={busy} onclick={() => talk(at, client.faultClear(companyId(), acknowledged))}>Clear</button>
              <input class="field tiny small-number" type="number" min="0" max="255" bind:value={testId} title="Test ID" />
              <button class="btn tiny" disabled={busy} onclick={() => talk(at, client.faultTest(testId, companyId(), acknowledged))}>Test</button>
            </div>
            <div class="client">
              <span class="label"></span>
              <input class="field tiny small-number" type="number" min="0" max="255" bind:value={attention} title="Seconds" />
              <span class="muted">s</span>
              <button class="btn tiny" disabled={busy} onclick={() => talk(at, client.attentionSet(attention, acknowledged))}>Attention</button>
              <button class="btn tiny" disabled={busy} onclick={() => talk(at, client.attentionGet())}>Get</button>
              <input class="field tiny small-number" type="number" min="0" max="15" bind:value={divisor} title="Fast period divisor" />
              <button class="btn tiny" disabled={busy} onclick={() => talk(at, client.periodSet(divisor, acknowledged))}>Period</button>
              <button class="btn tiny" disabled={busy} onclick={() => talk(at, client.periodGet())}>Get</button>
            </div>
          {/if}
        </div>
      {/if}
    {/each}
  {/if}

  <h3>Any message</h3>
  <div class="row">
    <label class="option">
      To
      <input class="field small address mono" bind:value={rawTo} placeholder={addressText(node.address)} spellcheck="false" />
    </label>
    <label class="option">
      Key
      <select class="field small" bind:value={rawKey}>
        <option value="device">Device key</option>
        {#each appKeys as key (key.index)}<option value={String(key.index)}>AppKey {key.index}</option>{/each}
      </select>
    </label>
    <label class="option">
      Opcode
      <input class="field small opcode mono" bind:value={rawOpcode} placeholder="8201" spellcheck="false" />
    </label>
    <label class="option">
      Parameters
      <input class="field small params mono" bind:value={rawParams} placeholder="hex" spellcheck="false" />
    </label>
    <button class="btn small" onclick={sendRaw}>Send</button>
  </div>
</div>

<style>
  .node {
    max-width: 920px;
  }

  header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 14px;
  }

  .name {
    width: min(240px, 100%);
    font-size: 15px;
    font-weight: 600;
  }

  dl {
    display: grid;
    grid-template-columns: 110px 1fr;
    gap: 2px 10px;
    margin: 10px 0;
  }

  dt {
    color: var(--muted);
  }

  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  h3 {
    margin: 20px 0 6px;
    font-size: 14px;
    font-weight: 600;
  }

  h4 {
    margin: 10px 0 4px;
    font-size: 13px;
    font-weight: 600;
  }

  p {
    margin: 0 0 8px;
  }

  .muted {
    color: var(--muted);
  }

  .note,
  .warn {
    color: var(--warn);
  }

  .result {
    padding: 6px 10px;
    color: var(--ok);
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow-wrap: anywhere;
  }

  .result.bad {
    color: var(--bad);
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 10px;
    margin: 6px 0;
  }

  .option,
  .check {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--muted);
  }

  .small {
    height: 26px;
    padding: 0 10px;
    font-size: 12px;
  }

  .tiny {
    height: 22px;
    padding: 0 8px;
    font-size: 11px;
  }

  .address {
    width: 90px;
  }

  .opcode {
    width: 80px;
  }

  .params {
    width: 200px;
  }

  .number {
    width: 80px;
  }

  .small-number {
    width: 56px;
  }

  .element {
    margin: 6px 0 10px;
    padding: 4px 12px 8px;
    border: 1px solid var(--border);
    border-radius: 8px;
  }

  .model {
    padding: 2px 0;
  }

  .link {
    padding: 0;
    font: inherit;
    color: var(--text);
    background: none;
    border: 0;
    cursor: pointer;
  }

  .link:hover {
    color: var(--accent);
  }

  .plain {
    padding-left: 1.1em;
    color: var(--muted);
  }

  .settings {
    margin: 2px 0 8px 18px;
    padding-left: 10px;
    border-left: 2px solid var(--border);
  }

  .client {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin: 4px 0;
  }

  .label {
    width: 120px;
    color: var(--muted);
  }
</style>
