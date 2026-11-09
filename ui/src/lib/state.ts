// How a tab's state is shown: a colored dot and a few words.

import { kindInfo } from "./new-tab";
import { phaseName } from "./roadmap";
import type { Tab } from "./tabs.svelte";

export type Look = "connected" | "waiting" | "problem" | "stopped";

export function look(tab: Tab): Look {
  if (tab.scanner) {
    if (tab.scanner.problem) return "problem";
    return tab.scanner.running ? "connected" : "stopped";
  }
  if (tab.server) {
    if (tab.server.starting) return "waiting";
    if (tab.server.running) return "connected";
    return tab.server.problem ? "problem" : "stopped";
  }
  if (tab.device) {
    switch (tab.device.state) {
      case "connected":
        return "connected";
      case "closed":
        return "stopped";
      default:
        return "waiting";
    }
  }
  return "stopped";
}

export function stateText(tab: Tab): string {
  if (tab.scanner) {
    const scanner = tab.scanner;
    if (scanner.problem) return scanner.problem;
    const heard = `${scanner.devices.size} devices, ${scanner.adverts} adverts`;
    return scanner.running ? `Scanning: ${heard}, ${scanner.rate}/s` : `Not scanning: ${heard}`;
  }
  if (tab.device) return tab.device.stateText;
  if (tab.server) return tab.server.stateText;
  if (tab.kind === "device" || tab.kind === "terminal") return "Open a device from a scanner tab";
  const { phase } = kindInfo(tab.kind);
  return `Comes in Phase ${phase} of the roadmap, ${phaseName(phase)}`;
}
