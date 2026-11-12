// Where bleNIMA is going, phase by phase, as docs/PLAN.md lists it, and
// where to read about it. Menu items that a later phase brings say which one.
// It has no imports, so its tests run in Node as they are.

export type Phase = { number: number; name: string };

/** The phase being built now. Everything in a later one is greyed out or says when it comes. */
export const CURRENT_PHASE = 9;

export const PHASES: Phase[] = [
  { number: 0, name: "Skeleton" },
  { number: 1, name: "Scanner" },
  { number: 2, name: "GATT" },
  { number: 3, name: "Terminals" },
  { number: 4, name: "Testing tools" },
  { number: 5, name: "MCUmgr firmware update" },
  { number: 6, name: "Pairing and bonds" },
  { number: 7, name: "Device simulator" },
  { number: 8, name: "Mesh over GATT proxy" },
  { number: 9, name: "1.0" },
];

/** A phase's name, such as "Scanner"; "" for a number that is not a phase. */
export function phaseName(number: number): string {
  return PHASES.find((phase) => phase.number === number)?.name ?? "";
}

/** Whether a phase is built, so what it brings can be used. */
export function isBuilt(number: number): boolean {
  return number <= CURRENT_PHASE;
}

export const GITHUB = "https://github.com/nimaltd/bleNIMA";
export const PLAN = `${GITHUB}/blob/main/docs/PLAN.md`;
export const HELP = "https://nimaltd.github.io/blenima/help/";
