<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { SMP_CHARACTERISTIC, SMP_SERVICE, type Source } from "./bluetooth";
  import type { DeviceState } from "./device.svelte";
  import { hex } from "./format";
  import {
    GROUP,
    IMAGE,
    OP,
    OS,
    Reassembler,
    activeImage,
    chunkSize,
    eraseBody,
    frame,
    imageInfo,
    mcumgrParams,
    sameHash,
    slotImages,
    slotInfo,
    smpError,
    standbyImage,
    uploadBody,
    type Cbor,
    type ImageInfo,
    type ImageSlots,
    type McumgrParams,
    type SlotImage,
  } from "./smp";

  // MCUmgr over Bluetooth, the image group: the slots and their sizes, test,
  // confirm, erase, upload, and an update that does it all in a row.
  let { device }: { device: DeviceState } = $props();

  const key = `${SMP_SERVICE}/${SMP_CHARACTERISTIC}`;
  const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

  let seq = 0;
  const joiner = new Reassembler();
  const waiting = new Map<number, (body: Cbor) => void>();
  const listener = (from: string, data: number[], source: Source) => {
    if (from !== key || source !== "notify") return;
    for (const { header, body } of joiner.push(data)) waiting.get(header.seq)?.(body);
  };
  $effect(() => {
    const listening = device;
    listening.listeners.add(listener);
    return () => listening.listeners.delete(listener);
  });

  let images: SlotImage[] = $state([]);
  let sizes: ImageSlots[] = $state([]);
  /** The device's SMP buffer, once asked; null where its firmware does not tell. */
  let params: McumgrParams | null = $state(null);
  let paramsAsked = false;
  let status = $state("");
  let problem = $state("");
  let busy = $state(false);
  let file: { path: string; bytes: Uint8Array; info: ImageInfo | null } | null = $state(null);
  let target = $state(0);
  let upgrade = $state(false);
  let sent = $state(0);
  let speed = $state(0);
  let uploading = $state(false);
  let cancelled = $state(false);
  let echoText = $state("hello");
  /** The slot an erase waits to be confirmed for. */
  let erasing: SlotImage | null = $state(null);

  type Step = { text: string; state: "todo" | "doing" | "done" | "failed" };
  let steps: Step[] = $state([]);

  /** The image numbers the device has, or 0 before it is asked. */
  const numbers = $derived([...new Set([0, ...images.map((image) => image.image), ...sizes.map((slots) => slots.image)])]);
  /** How big an image the slot an upload goes to takes, where the device says. */
  const room = $derived.by(() => {
    const slots = sizes.find((entry) => entry.image === target);
    if (!slots) return null;
    return slots.maxImageSize ?? slots.slots.find((slot) => slot.slot === 1)?.size ?? null;
  });
  const tooBig = $derived.by(() => {
    const chosen = file as { bytes: Uint8Array } | null;
    return chosen !== null && room !== null && chosen.bytes.length > room;
  });

  /** Sends one SMP request and waits for its answer. Throws with MCUmgr's error in words. */
  async function request(op: number, group: number, id: number, body: Cbor, timeout = 5000): Promise<Record<string, Cbor>> {
    if (!device.subscribed.has(key)) {
      device.subscribe(key, true);
      await sleep(400);
    }
    seq = (seq + 1) & 0xff;
    const mine = seq;
    const bytes = Array.from(frame(op, group, id, mine, body));
    const answer = new Promise<Cbor>((resolve, reject) => {
      const timer = setTimeout(() => {
        waiting.delete(mine);
        joiner.clear();
        reject(new Error("The device did not answer in time. Does its firmware have MCUmgr over Bluetooth?"));
      }, timeout);
      waiting.set(mine, (value) => {
        clearTimeout(timer);
        waiting.delete(mine);
        resolve(value);
      });
    });
    device.writeChunked(key, bytes, false);
    const result = (await answer) as Record<string, Cbor>;
    const error = smpError(result);
    if (error) throw new Error(`MCUmgr says: ${error}`);
    return result;
  }

  async function act(what: string, work: () => Promise<void>) {
    problem = "";
    status = what;
    busy = true;
    try {
      await work();
    } catch (error) {
      problem = error instanceof Error ? error.message : String(error);
      status = "";
    } finally {
      busy = false;
      uploading = false;
    }
  }

  /** Asks the device how big its SMP buffer is, once a connection: older firmware does not say. */
  async function askParams() {
    if (paramsAsked) return;
    paramsAsked = true;
    params = await request(OP.read, GROUP.os, OS.params, {}, 3000)
      .then(mcumgrParams)
      .catch(() => null);
  }

  /** Reads the slots, and their sizes where the firmware tells them (Zephyr 3.7 and later). */
  async function readSlots() {
    images = slotImages(await request(OP.read, GROUP.image, IMAGE.state, {}));
    sizes = await request(OP.read, GROUP.image, IMAGE.slotInfo, {}, 3000)
      .then(slotInfo)
      .catch(() => []);
  }

  function readImages() {
    return act("Reading the image slots...", async () => {
      await askParams();
      await readSlots();
      status = images.length ? "" : "The device lists no images.";
    });
  }

  function echo() {
    return act("Echo...", async () => {
      const started = performance.now();
      const answer = await request(OP.write, GROUP.os, OS.echo, { d: echoText });
      status = `The device answered "${String(answer.r)}" in ${Math.round(performance.now() - started)} ms.`;
    });
  }

  function reset() {
    return act("Resetting the device...", async () => {
      await request(OP.write, GROUP.os, OS.reset, {});
      status = "The device is resetting. bleNIMA connects again when it is back.";
    });
  }

  function setState(hash: Uint8Array | null, confirm: boolean) {
    return act(confirm ? "Confirming the image..." : "Marking the image to test...", async () => {
      const body: Record<string, Cbor> = { confirm };
      if (hash) body.hash = hash;
      images = slotImages(await request(OP.write, GROUP.image, IMAGE.state, body));
      status = confirm
        ? "Confirmed: it stays after the next reset."
        : "It runs at the next reset, once. Confirm it after, or the old one comes back.";
    });
  }

  function erase(image: SlotImage) {
    erasing = null;
    return act(`Erasing image ${image.image}, slot ${image.slot}...`, async () => {
      // Erasing a whole slot can take a while on a big flash.
      await request(OP.write, GROUP.image, IMAGE.erase, eraseBody(image.image, image.slot), 30_000);
      await readSlots();
      status = `Image ${image.image}, slot ${image.slot} is erased.`;
    });
  }

  async function pick() {
    const path = await open({
      title: "Pick a firmware image",
      multiple: false,
      directory: false,
      filters: [
        { name: "MCUboot images", extensions: ["bin"] },
        { name: "All files", extensions: ["*"] },
      ],
    });
    if (typeof path !== "string") return;
    const buffer = await invoke<ArrayBuffer>("read_file", { path });
    const bytes = new Uint8Array(buffer);
    file = { path, bytes, info: imageInfo(bytes) };
    sent = 0;
    steps = [];
  }

  /** Uploads the image to the slot that is not active, in requests that fit one write and the device's buffer. */
  async function send(image: Uint8Array) {
    await askParams();
    const sha = new Uint8Array(await crypto.subtle.digest("SHA-256", new Uint8Array(image)));
    const mtu = device.link?.mtu ?? 23;
    const bufSize = params?.bufSize ?? Number.POSITIVE_INFINITY;
    const started = performance.now();
    uploading = true;
    sent = 0;
    let offset = 0;
    let answer: Record<string, Cbor> = {};
    while (offset < image.length) {
      if (cancelled) throw new Error("Upload stopped.");
      const first = offset === 0;
      const size = Math.min(chunkSize(mtu, first, image.length, bufSize, target, upgrade), image.length - offset);
      const data = image.slice(offset, offset + size);
      const body = uploadBody(offset, data, first ? { image: target, len: image.length, sha, upgrade } : null);
      // The first request waits while the device erases the slot.
      answer = await request(OP.write, GROUP.image, IMAGE.upload, body, first ? 30_000 : 10_000);
      // The device says where it wants the next piece from.
      offset = typeof answer.off === "number" ? answer.off : offset + size;
      sent = offset;
      speed = offset / 1024 / ((performance.now() - started) / 1000);
    }
    uploading = false;
    if (answer.match === false) throw new Error("The device says the image it got does not match the one sent: upload it again.");
    return (performance.now() - started) / 1000;
  }

  function uploadOnly() {
    if (!file) return Promise.resolve();
    const image = file.bytes;
    cancelled = false;
    steps = [];
    return act("Uploading...", async () => {
      const seconds = await send(image);
      status = `Uploaded ${(image.length / 1024).toFixed(1)} kB in ${seconds.toFixed(1)} s. Test it, then reset.`;
      await readSlots();
    });
  }

  /** Waits until the device has gone and come back with its SMP service. */
  async function cameBack(timeout: number): Promise<void> {
    const until = Date.now() + timeout;
    // A reset drops the link; Windows notices within the supervision timeout.
    const goneBy = Date.now() + 20_000;
    while (device.connected && Date.now() < goneBy) await sleep(250);
    if (device.connected) throw new Error("The device did not reset: it is still connected.");
    joiner.clear();
    waiting.clear();
    paramsAsked = false;
    while (!(device.connected && device.hasSmp)) {
      if (cancelled) throw new Error("Update stopped.");
      if (Date.now() > until) throw new Error(`The device did not come back in ${Math.round(timeout / 1000)} s.`);
      await sleep(500);
    }
    // Its notifications are turned on again by then; give it a moment to settle.
    await sleep(1000);
  }

  async function step<T>(at: number, work: () => Promise<T>): Promise<T> {
    if (cancelled) throw new Error("Update stopped.");
    steps[at].state = "doing";
    try {
      const result = await work();
      steps[at].state = "done";
      return result;
    } catch (error) {
      steps[at].state = "failed";
      throw error;
    }
  }

  /** Uploads, marks the new image to test, resets, waits for the device, checks the new image runs, and confirms it. */
  function update() {
    if (!file) return Promise.resolve();
    const image = file.bytes;
    const number = target;
    cancelled = false;
    steps = [
      "Upload the image",
      "Mark it to test",
      "Reset the device",
      "Wait for it to come back: MCUboot swaps the images first",
      "Check that the new image runs",
      "Confirm it",
    ].map((text) => ({ text, state: "todo" }));
    return act("Updating...", async () => {
      await step(0, () => send(image));
      const next = await step(1, async () => {
        await readSlots();
        const uploaded = standbyImage(images, number);
        if (!uploaded?.hash) throw new Error("After the upload the device lists no image in its other slot.");
        images = slotImages(await request(OP.write, GROUP.image, IMAGE.state, { hash: uploaded.hash, confirm: false }));
        return uploaded;
      });
      // The device may reset before its answer goes out.
      await step(2, () => request(OP.write, GROUP.os, OS.reset, {}, 3000).catch(() => ({})));
      await step(3, () => cameBack(180_000));
      await step(4, async () => {
        images = slotImages(await request(OP.read, GROUP.image, IMAGE.state, {}));
        const running = activeImage(images, number);
        if (!sameHash(running?.hash ?? null, next.hash)) {
          throw new Error(
            `The device came back with version ${running?.version ?? "?"}, not ${next.version}: MCUboot put the old image back, as the new one did not start, or it was not marked to test.`,
          );
        }
      });
      await step(5, async () => {
        images = slotImages(await request(OP.write, GROUP.image, IMAGE.state, { confirm: true }));
      });
      status = `Updated to version ${next.version}: it stays after the next reset.`;
    });
  }

  const flagNames = (image: SlotImage) =>
    [
      image.active && "active",
      image.confirmed && "confirmed",
      image.pending && "pending",
      image.bootable && "bootable",
      image.permanent && "permanent",
    ]
      .filter(Boolean)
      .join(", ");

  /** "480.0 kB", or "" where the device does not say. */
  function sizeOf(image: SlotImage): string {
    const size = sizes.find((entry) => entry.image === image.image)?.slots.find((slot) => slot.slot === image.slot)?.size;
    return size ? `${(size / 1024).toFixed(1)} kB` : "";
  }

  const STEP_MARKS = { todo: "○", doing: "…", done: "✓", failed: "✗" };
</script>

<div class="mcumgr">
  <h2>MCUmgr</h2>
  <p class="muted">
    Firmware update for Zephyr and nRF Connect SDK devices with MCUmgr's SMP service over Bluetooth, and MCUboot.
  </p>

  <!-- While an update runs, its steps stay on screen, the reset's reconnection too. -->
  {#if !device.connected && !busy}
    <p class="note">Connect to the device first.</p>
  {:else if !device.hasSmp && !busy}
    <p class="note">
      This device has no SMP service (8D53DC1D-1DB7-4CD3-868B-8A527460AA84). Build its firmware with
      CONFIG_MCUMGR_TRANSPORT_BT.
    </p>
  {:else}
    <div class="row">
      <button class="btn small" disabled={busy} onclick={readImages}>Read image slots</button>
      <button class="btn small" disabled={busy} onclick={reset}>Reset device</button>
      <input class="field small" bind:value={echoText} />
      <button class="btn small" disabled={busy} onclick={echo}>Echo</button>
      {#if params}
        <span class="muted inline" title="From the device's MCUmgr parameters: the upload's pieces fit it"
          >SMP buffer {params.bufSize} bytes × {params.bufCount}</span
        >
      {/if}
    </div>

    {#if images.length > 0}
      <table>
        <thead><tr><th>Image</th><th>Slot</th><th>Version</th><th>State</th><th>Size</th><th>Hash</th><th></th></tr></thead>
        <tbody>
          {#each images as image (`${image.image}-${image.slot}`)}
            <tr>
              <td>{image.image}</td>
              <td>{image.slot}</td>
              <td class="mono">{image.version}</td>
              <td>{flagNames(image)}</td>
              <td class="mono">{sizeOf(image)}</td>
              <td class="mono hash" title={image.hash ? hex(image.hash) : ""}>{image.hash ? hex(image.hash.slice(0, 6)) : ""}…</td>
              <td class="buttons">
                {#if !image.active && !image.pending}
                  <button class="btn tiny" disabled={busy} onclick={() => setState(image.hash, false)}>Test</button>
                {/if}
                {#if image.active && !image.confirmed}
                  <button class="btn tiny primary" disabled={busy} onclick={() => setState(null, true)}>Confirm</button>
                {/if}
                {#if !image.active}
                  {#if erasing === image}
                    <span class="warn">Erase it?</span>
                    <button class="btn tiny" disabled={busy} onclick={() => erase(image)}>Erase</button>
                    <button class="btn tiny" onclick={() => (erasing = null)}>Cancel</button>
                  {:else}
                    <button class="btn tiny" disabled={busy} onclick={() => (erasing = image)}>Erase...</button>
                  {/if}
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}

    <h3>Update</h3>
    <div class="row">
      <button class="btn small" disabled={busy} onclick={pick}>Pick an image...</button>
      {#if file}
        <span class="mono path">{file.path.split(/[\\/]/).pop()}</span>
        <span class="muted inline">
          {(file.bytes.length / 1024).toFixed(1)} kB{file.info ? `, version ${file.info.version}` : ", no MCUboot header"}
        </span>
      {/if}
    </div>
    {#if file}
      <div class="row">
        <label class="option">
          Image
          <select class="field small" bind:value={target} disabled={busy}>
            {#each numbers as number (number)}
              <option value={number}>{number}</option>
            {/each}
          </select>
        </label>
        <label class="check" title="The device takes the image only if it is newer than the one that runs">
          <input type="checkbox" bind:checked={upgrade} disabled={busy} /> Only a newer version
        </label>
        {#if busy && (uploading || steps.some((entry) => entry.state === "doing"))}
          <button class="btn small" disabled={cancelled} onclick={() => (cancelled = true)}>{cancelled ? "Stopping..." : "Stop"}</button>
        {:else}
          <button
            class="btn small primary"
            disabled={busy || tooBig}
            title="Upload, mark it to test, reset, wait for the device, check the new image runs, and confirm it"
            onclick={update}>Update</button
          >
          <button class="btn small" disabled={busy || tooBig} title="Only upload it; test, reset and confirm by hand" onclick={uploadOnly}
            >Upload only</button
          >
        {/if}
      </div>
      {#if tooBig && room !== null}
        <p class="bad">
          The image is {(file.bytes.length / 1024).toFixed(1)} kB, but image {target}'s slot takes {(room / 1024).toFixed(1)} kB.
        </p>
      {/if}
    {/if}
    {#if file && sent > 0}
      <div class="progress"><span style:width={`${(sent / file.bytes.length) * 100}%`}></span></div>
      <p class="muted">
        {(sent / 1024).toFixed(1)} of {(file.bytes.length / 1024).toFixed(1)} kB, {speed.toFixed(1)} kB/s{speed > 0 &&
        sent < file.bytes.length
          ? `, ${Math.ceil((file.bytes.length - sent) / 1024 / speed)} s left`
          : ""}
      </p>
    {/if}
    {#if steps.length > 0}
      <ol class="steps">
        {#each steps as entry, at (at)}
          <li class={entry.state}><b>{STEP_MARKS[entry.state]}</b> {entry.text}</li>
        {/each}
      </ol>
    {/if}
  {/if}

  {#if status}<p class="status">{status}</p>{/if}
  {#if problem}<p class="bad">{problem}</p>{/if}
</div>

<style>
  .mcumgr {
    height: 100%;
    padding: 14px 18px;
    overflow-y: auto;
    box-sizing: border-box;
    font-size: var(--view-size);
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

  .muted.inline {
    margin: 0;
  }

  .note,
  .warn {
    color: var(--warn);
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    margin: 8px 0;
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

  table {
    width: 100%;
    margin-top: 10px;
    border-collapse: collapse;
  }

  th,
  td {
    padding: 5px 8px;
    text-align: left;
    border-bottom: 1px solid var(--border);
  }

  th {
    font-size: 11px;
    font-weight: 600;
    color: var(--muted);
    text-transform: uppercase;
  }

  .buttons {
    white-space: nowrap;
  }

  .hash {
    color: var(--muted);
  }

  .path {
    max-width: 260px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .progress {
    height: 8px;
    margin: 10px 0 6px;
    overflow: hidden;
    background: var(--border);
    border-radius: 4px;
  }

  .progress span {
    display: block;
    height: 100%;
    background: var(--accent);
  }

  .steps {
    margin: 10px 0 0;
    padding: 0;
    list-style: none;
  }

  .steps li {
    padding: 2px 0;
    color: var(--muted);
  }

  .steps b {
    display: inline-block;
    width: 18px;
  }

  .steps li.doing {
    color: var(--text);
  }

  .steps li.done b {
    color: var(--ok);
  }

  .steps li.failed {
    color: var(--bad);
  }

  .status {
    margin-top: 12px;
    color: var(--ok);
  }

  .bad {
    margin-top: 12px;
    color: var(--bad);
  }
</style>
