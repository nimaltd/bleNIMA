// Run with npm test: Node runs TypeScript as it is.

import assert from "node:assert/strict";
import { test } from "node:test";
import { reopened } from "./reopen.ts";

test("the tabs that can open, open as they were, with one scanner", () => {
  const saved = [
    { kind: "device", device: "" },
    { kind: "scanner" },
    { kind: "device", device: "AA:BB" },
    { kind: "scanner" },
    { kind: "mesh" },
    { kind: "simulator" },
    { kind: "terminal", device: "CC:DD" },
    { kind: "terminal" },
  ];
  assert.deepEqual(reopened(saved), [1, 2, 5, 6]);
});

test("the scanner keeps its place among the others", () => {
  assert.deepEqual(reopened([{ kind: "device", device: "AA:BB" }, { kind: "scanner" }]), [0, 1]);
  assert.deepEqual(reopened([{ kind: "simulator" }]), [0]);
  assert.deepEqual(reopened([]), []);
});
