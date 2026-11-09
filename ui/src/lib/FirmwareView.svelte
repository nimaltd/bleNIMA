<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { SMP_CHARACTERISTIC, SMP_SERVICE, type Source } from "./bluetooth";
  import type { DeviceState } from "./device.svelte";
  import { hex } from "./format";
  import { GROUP, OP, Reassembler, chunkSize, frame, imageInfo, rcText, slotImages, type Cbor, type ImageInfo, type SlotImage } from "./smp";

  let { device }: { device: DeviceState } = $props();

  const key = `${SMP_SERVICE}/${SMP_CHARACTERISTIC}`;
  const hasSmp = $derived(device.services.some((s) => s.characteristics.some((c) => c.key === key)));

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
  let status = $state("");
  let problem = $state("");
  let busy = $state(false);
  let file: { path: string; bytes: Uint8Array; info: ImageInfo | null } | null = $state(null);
  let sent = $state(0);
  let speed = $state(0);
  let cancelled = false;
  let echoText = $state("hello");

  /** Sends one SMP request and waits for its answer. Throws with MCUmgr's error in words. */
  async function request(op: number, group: number, id: number, body: Cbor, timeout = 5000): Promise<Record<string, Cbor>> {
    if (!device.subscribed.has(key)) {
      device.subscribe(key, true);
      await new Promise((resolve) => setTimeout(resolve, 400));
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
    const rc = typeof result.rc === "number" ? result.rc : 0;
    if (rc !== 0) throw new Error(`MCUmgr says: ${rcText(rc)} (rc ${rc})`);
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
    }
  }

  function readImages() {
    return act("Reading the image slots...", async () => {
      images = slotImages(await request(OP.read, GROUP.image, 0, {}));
      status = images.length ? "" : "The device lists no images.";
    });
  }

  function echo() {
    return act("Echo...", async () => {
      const started = performance.now();
      const answer = await request(OP.write, GROUP.os, 0, { d: echoText });
      status = `The device answered "${String(answer.r)}" in ${Math.round(performance.now() - started)} ms.`;
    });
  }

  function reset() {
    return act("Resetting the device...", async () => {
      await request(OP.write, GROUP.os, 5, {});
      status = "The device is resetting. bleNIMA connects again when it is back.";
    });
  }

  function setState(hash: Uint8Array | null, confirm: boolean) {
    return act(confirm ? "Confirming the image..." : "Marking the image to test...", async () => {
      const body: Record<string, Cbor> = { confirm };
      if (hash) body.hash = hash;
      images = slotImages(await request(OP.write, GROUP.image, 0, body));
      status = confirm
        ? "Confirmed: it stays after the next reset."
        : "It runs at the next reset, once. Confirm it after, or the old one comes back.";
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
  }

  function upload() {
    if (!file) return Promise.resolve();
    const image = file.bytes;
    cancelled = false;
    return act("Uploading...", async () => {
      const sha = new Uint8Array(await crypto.subtle.digest("SHA-256", new Uint8Array(image)));
      const mtu = device.link?.mtu ?? 23;
      const started = performance.now();
      sent = 0;
      let offset = 0;
      while (offset < image.length) {
        if (cancelled) throw new Error("Upload stopped.");
        const first = offset === 0;
        const size = Math.min(chunkSize(mtu, first, image.length), image.length - offset);
        const data = image.slice(offset, offset + size);
        const body: Record<string, Cbor> = first ? { image: 0, len: image.length, off: 0, sha, data } : { off: offset, data };
        const answer = await request(OP.write, GROUP.image, 1, body, 10_000);
        // The device says where it wants the next piece from.
        offset = typeof answer.off === "number" ? answer.off : offset + size;
        sent = offset;
        speed = offset / 1024 / ((performance.now() - started) / 1000);
      }
      status = `Uploaded ${(image.length / 1024).toFixed(1)} kB in ${((performance.now() - started) / 1000).toFixed(1)} s. Test it, then reset.`;
      images = slotImages(await request(OP.read, GROUP.image, 0, {}));
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
</script>

<h2>Firmware update (MCUmgr)</h2>
<p class="muted">For Zephyr and nRF Connect SDK devices with MCUmgr's SMP service over Bluetooth, and MCUboot.</p>

{#if !device.connected}
  <p class="note">Connect to the device first.</p>
{:else if !hasSmp}
  <p class="note">This device has no SMP service (8D53DC1D-1DB7-4CD3-868B-8A527460AA84). Build its firmware with CONFIG_MCUMGR_TRANSPORT_BT.</p>
{:else}
  <div class="row">
    <button class="btn small" disabled={busy} onclick={readImages}>Read image slots</button>
    <button class="btn small" disabled={busy} onclick={reset}>Reset device</button>
    <input class="field small" bind:value={echoText} />
    <button class="btn small" disabled={busy} onclick={echo}>Echo</button>
  </div>

  {#if images.length > 0}
    <table>
      <thead><tr><th>Image</th><th>Slot</th><th>Version</th><th>State</th><th>Hash</th><th></th></tr></thead>
      <tbody>
        {#each images as image (`${image.image}-${image.slot}`)}
          <tr>
            <td>{image.image}</td>
            <td>{image.slot}</td>
            <td class="mono">{image.version}</td>
            <td>{flagNames(image)}</td>
            <td class="mono hash" title={image.hash ? hex(image.hash) : ""}>{image.hash ? hex(image.hash.slice(0, 6)) : ""}…</td>
            <td>
              {#if !image.active && !image.pending}
                <button class="btn tiny" disabled={busy} onclick={() => setState(image.hash, false)}>Test</button>
              {/if}
              {#if image.active && !image.confirmed}
                <button class="btn tiny primary" disabled={busy} onclick={() => setState(null, true)}>Confirm</button>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}

  <h3>Upload</h3>
  <div class="row">
    <button class="btn small" disabled={busy} onclick={pick}>Pick an image...</button>
    {#if file}
      <span class="mono path">{file.path.split(/[\\/]/).pop()}</span>
      <span class="muted">{(file.bytes.length / 1024).toFixed(1)} kB{file.info ? `, version ${file.info.version}` : ", no MCUboot header"}</span>
      {#if busy && status === "Uploading..."}
        <button class="btn small" onclick={() => (cancelled = true)}>Stop</button>
      {:else}
        <button class="btn small primary" disabled={busy} onclick={upload}>Upload</button>
      {/if}
    {/if}
  </div>
  {#if file && sent > 0}
    <div class="progress"><span style:width={`${(sent / file.bytes.length) * 100}%`}></span></div>
    <p class="muted">
      {(sent / 1024).toFixed(1)} of {(file.bytes.length / 1024).toFixed(1)} kB, {speed.toFixed(1)} kB/s{speed > 0 && sent < file.bytes.length
        ? `, ${Math.ceil((file.bytes.length - sent) / 1024 / speed)} s left`
        : ""}
    </p>
  {/if}
{/if}

{#if status}<p class="status">{status}</p>{/if}
{#if problem}<p class="bad">{problem}</p>{/if}

<style>
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
    color: var(--warn);
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    margin: 8px 0;
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

  .status {
    margin-top: 12px;
    color: var(--ok);
  }

  .bad {
    margin-top: 12px;
    color: var(--bad);
  }
</style>
