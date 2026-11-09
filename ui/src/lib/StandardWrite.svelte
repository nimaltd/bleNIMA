<script lang="ts">
  import { PROPERTY, type CharacteristicInfo } from "./bluetooth";
  import type { DeviceState } from "./device.svelte";
  import { SENSOR_LOCATIONS, characteristicUuid, shortUuid } from "./profiles";
  import {
    ANSWERED,
    RESET_ENERGY_EXPENDED,
    alertLevel,
    currentTime,
    cyclingPowerControlPoint,
    localTimeInformation,
    measurementInterval,
    pcTimeZone,
    recordAccess,
    scControlPoint,
  } from "./writes";

  // Buttons and boxes that write a standard characteristic's value, made from
  // what is picked: the time now, an alert level, a control point's request.
  let { device, characteristic }: { device: DeviceState; characteristic: CharacteristicInfo } = $props();

  const short = $derived(shortUuid(characteristicUuid(characteristic.key)));
  const withResponse = $derived((characteristic.properties & PROPERTY.write) !== 0);
  const needsIndicate = $derived(
    short !== null &&
      ANSWERED.has(short) &&
      (characteristic.properties & PROPERTY.indicate) !== 0 &&
      !device.subscribed.has(characteristic.key),
  );

  let number = $state("");
  let location = $state(0);
  let problem = $state("");
  let deleting = $state(false);

  function send(data: number[] | string) {
    problem = "";
    if (typeof data === "string") {
      problem = data;
      return;
    }
    device.write(characteristic.key, data, withResponse);
  }

  function timeZone() {
    const { standard, dst } = pcTimeZone(new Date());
    send(localTimeInformation(standard, dst));
  }

  const LOCATIONS = Object.entries(SENSOR_LOCATIONS).map(([value, name]) => ({ value: Number(value), name }));
</script>

<div class="standard">
  {#if short === 0x2a2b}
    <button class="btn small primary" disabled={!device.connected} onclick={() => send(currentTime(new Date()))}
      >Set to now</button
    >
    <span class="muted">This PC's date and time</span>
  {:else if short === 0x2a0f}
    <button class="btn small primary" disabled={!device.connected} onclick={timeZone}>Set to this PC's time zone</button>
  {:else if short === 0x2a06}
    <button class="btn small" disabled={!device.connected} onclick={() => send(alertLevel(0))}>No alert</button>
    <button class="btn small" disabled={!device.connected} onclick={() => send(alertLevel(1))}>Mild alert</button>
    <button class="btn small" disabled={!device.connected} onclick={() => send(alertLevel(2))}>High alert</button>
  {:else if short === 0x2a39}
    <button class="btn small" disabled={!device.connected} onclick={() => send(RESET_ENERGY_EXPENDED)}
      >Reset energy expended</button
    >
  {:else if short === 0x2a55}
    <span class="group">
      <input class="field small mono" placeholder="0" bind:value={number} spellcheck="false" />
      <button class="btn small" disabled={!device.connected} onclick={() => send(scControlPoint.setCumulativeValue(number))}
        >Set cumulative value</button
      >
    </span>
    <button class="btn small" disabled={!device.connected} onclick={() => send(scControlPoint.startCalibration)}
      >Start sensor calibration</button
    >
    <span class="group">
      <select class="field small" bind:value={location}>
        {#each LOCATIONS as { value, name } (value)}
          <option {value}>{name}</option>
        {/each}
      </select>
      <button class="btn small" disabled={!device.connected} onclick={() => send(scControlPoint.updateLocation(location))}
        >Update sensor location</button
      >
    </span>
    <button class="btn small" disabled={!device.connected} onclick={() => send(scControlPoint.requestLocations)}
      >Request supported sensor locations</button
    >
  {:else if short === 0x2a66}
    <button class="btn small" disabled={!device.connected} onclick={() => send(cyclingPowerControlPoint.startOffsetCompensation)}
      >Start offset compensation</button
    >
    <button class="btn small" disabled={!device.connected} onclick={() => send(cyclingPowerControlPoint.requestCrankLength)}
      >Request crank length</button
    >
    <span class="group">
      <input class="field small mono" placeholder="172.5" bind:value={number} spellcheck="false" />
      <span class="muted">mm</span>
      <button class="btn small" disabled={!device.connected} onclick={() => send(cyclingPowerControlPoint.setCrankLength(number))}
        >Set crank length</button
      >
    </span>
  {:else if short === 0x2a52}
    <button class="btn small" disabled={!device.connected} onclick={() => send(recordAccess.reportAll)}>Report all records</button>
    <button class="btn small" disabled={!device.connected} onclick={() => send(recordAccess.countAll)}>Number of records</button>
    {#if deleting}
      <span class="group">
        <span class="warn">Delete every record on the device?</span>
        <button
          class="btn small"
          disabled={!device.connected}
          onclick={() => {
            deleting = false;
            send(recordAccess.deleteAll);
          }}>Delete</button
        >
        <button class="btn small" onclick={() => (deleting = false)}>Cancel</button>
      </span>
    {:else}
      <button class="btn small" disabled={!device.connected} onclick={() => (deleting = true)}>Delete all records...</button>
    {/if}
    <button class="btn small" disabled={!device.connected} onclick={() => send(recordAccess.abort)}>Abort</button>
  {:else if short === 0x2a21}
    <span class="group">
      <input class="field small mono" placeholder="60" bind:value={number} spellcheck="false" />
      <span class="muted">seconds, 0 for none</span>
      <button class="btn small" disabled={!device.connected} onclick={() => send(measurementInterval(number))}>Set</button>
    </span>
  {/if}
</div>
{#if needsIndicate}
  <p class="muted hint">The answer comes as an indication: turn Indicate on first.</p>
{/if}
{#if problem}<p class="problem">{problem}</p>{/if}

<style>
  .standard {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-top: 8px;
  }

  .group {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .group input {
    width: 110px;
  }

  .small {
    height: 26px;
    padding: 0 10px;
    font-size: 12px;
  }

  .muted {
    color: var(--muted);
  }

  .warn {
    color: var(--warn);
  }

  .hint {
    margin: 6px 0 0;
  }

  .problem {
    margin: 6px 0 0;
    color: var(--bad);
  }
</style>
