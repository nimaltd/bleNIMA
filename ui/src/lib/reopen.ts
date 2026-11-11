// Which of the tabs saved last time open again. It has no imports, so its
// tests run in Node as they are.

/** A saved tab, as far as this needs: its kind, and its device for device and terminal tabs. */
export type Saved = { kind: string; device?: string };

/**
 * The saved tabs that open, by their place in the list, in their order: the
 * first scanner, the device and terminal tabs that have their device, and the
 * device simulators. A second scanner, a device tab with no device and the
 * kinds a later phase brings, which bleNIMA 0.2 and before could make, are
 * left out.
 */
export function reopened(saved: Saved[]): number[] {
  let scanner = false;
  return saved.flatMap((tab, at) => {
    if (tab.kind === "scanner") {
      if (scanner) return [];
      scanner = true;
      return [at];
    }
    if (tab.kind === "simulator") return [at];
    if ((tab.kind === "device" || tab.kind === "terminal") && tab.device) return [at];
    return [];
  });
}
