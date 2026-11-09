// The test tools' arithmetic and the send-and-expect script, apart from the
// UI. It has no imports, so its tests run in Node as they are.

/** min, max, mean, median and 95th percentile of some numbers. */
export type Stats = { count: number; min: number; max: number; mean: number; median: number; p95: number };

export function stats(values: number[]): Stats | null {
  if (values.length === 0) return null;
  const sorted = [...values].sort((a, b) => a - b);
  const at = (share: number) => sorted[Math.min(sorted.length - 1, Math.floor(share * sorted.length))];
  return {
    count: values.length,
    min: sorted[0],
    max: sorted[sorted.length - 1],
    mean: values.reduce((sum, value) => sum + value, 0) / values.length,
    median: at(0.5),
    p95: at(0.95),
  };
}

/**
 * Packets lost, from a counter at the start of each payload that goes up by one
 * a packet and wraps at `modulo` (256 for one byte, 65536 for two).
 */
export function lostPackets(counters: number[], modulo: number): number {
  let lost = 0;
  for (let at = 1; at < counters.length; at++) {
    const step = (counters[at] - counters[at - 1] + modulo) % modulo;
    if (step > 1) lost += step - 1;
  }
  return lost;
}

/** What a step names: a characteristic by short UUID, full UUID or name. */
export type Target = string;

export type Step =
  | { kind: "subscribe"; target: Target; line: number }
  | { kind: "write"; target: Target; data: number[]; withResponse: boolean; line: number }
  | { kind: "read"; target: Target; expect: Pattern | null; line: number }
  | { kind: "expect"; target: Target; expect: Pattern; within: number; line: number }
  | { kind: "wait"; ms: number; line: number };

/** What a value must look like: text in it, a regex on its text, or hex bytes at its start. */
export type Pattern = { text?: string; regex?: string; hex?: number[] };

const HELP =
  "Steps: subscribe <char>, write <char> <value>, write-nr <char> <value>, read <char> [expect <value>], expect <char> <value> [within <ms>], wait <ms>";

/** Reads a value: "text with \\r", /regex/ or hex AA 55. */
function value(rest: string): { text?: string; regex?: string; hex?: number[] } | string {
  const trimmed = rest.trim();
  if (trimmed.startsWith('"') && trimmed.endsWith('"') && trimmed.length >= 2) {
    return { text: trimmed.slice(1, -1) };
  }
  if (trimmed.startsWith("/") && trimmed.lastIndexOf("/") > 0) {
    const regex = trimmed.slice(1, trimmed.lastIndexOf("/"));
    try {
      new RegExp(regex);
    } catch {
      return `Not a regular expression: ${regex}`;
    }
    return { regex };
  }
  const hex = /^hex\s+(.*)$/i.exec(trimmed);
  if (hex) {
    const digits = hex[1].replace(/0x/gi, "").replace(/[\s,]/g, "");
    if (!/^([0-9a-f]{2})+$/i.test(digits)) return "Hex needs two digits a byte, such as hex AA 55";
    return { hex: digits.match(/../g)!.map((byte) => Number.parseInt(byte, 16)) };
  }
  return 'A value is "text", /regex/ or hex AA 55';
}

/** Bytes of text with \r, \n, \t and \xNN. */
export function escapes(text: string): number[] {
  const out: number[] = [];
  const encoder = new TextEncoder();
  for (let at = 0; at < text.length; at++) {
    if (text[at] === "\\" && at + 1 < text.length) {
      const next = text[at + 1];
      const simple: Record<string, number> = { r: 13, n: 10, t: 9, "\\": 92, '"': 34 };
      if (next in simple) {
        out.push(simple[next]);
        at++;
        continue;
      }
      if (next === "x" && /^[0-9a-f]{2}$/i.test(text.slice(at + 2, at + 4))) {
        out.push(Number.parseInt(text.slice(at + 2, at + 4), 16));
        at += 3;
        continue;
      }
    }
    out.push(...encoder.encode(text[at]));
  }
  return out;
}

/** Splits "target rest", where a target with spaces is in single quotes: 'Battery Level'. */
function target(rest: string): [string, string] | null {
  const trimmed = rest.trim();
  if (trimmed.startsWith("'")) {
    const end = trimmed.indexOf("'", 1);
    return end > 0 ? [trimmed.slice(1, end), trimmed.slice(end + 1)] : null;
  }
  const match = /^(\S+)(.*)$/.exec(trimmed);
  return match ? [match[1], match[2]] : null;
}

/** Reads a send-and-expect script, one step a line; # starts a comment. */
export function parseScript(script: string): { steps: Step[] } | { error: string; line: number } {
  const steps: Step[] = [];
  const lines = script.split(/\r?\n/);
  for (let index = 0; index < lines.length; index++) {
    const line = index + 1;
    const text = lines[index].replace(/(^|\s)#.*$/, "").trim();
    if (!text) continue;
    const [word, ...restWords] = text.split(/\s+/);
    const rest = text.slice(word.length);
    const fail = (error: string) => ({ error, line });
    switch (word.toLowerCase()) {
      case "wait": {
        const ms = Number(restWords[0]);
        if (!Number.isFinite(ms) || ms < 0) return fail("wait needs a time in ms, such as wait 500");
        steps.push({ kind: "wait", ms, line });
        break;
      }
      case "subscribe": {
        const found = target(rest);
        if (!found) return fail("subscribe needs a characteristic");
        steps.push({ kind: "subscribe", target: found[0], line });
        break;
      }
      case "write":
      case "write-nr": {
        const found = target(rest);
        if (!found) return fail(`${word} needs a characteristic and a value`);
        const parsed = value(found[1]);
        if (typeof parsed === "string") return fail(parsed);
        if (parsed.regex) return fail("A regex cannot be written");
        const data = parsed.hex ?? escapes(parsed.text ?? "");
        steps.push({ kind: "write", target: found[0], data, withResponse: word.toLowerCase() === "write", line });
        break;
      }
      case "read": {
        const found = target(rest);
        if (!found) return fail("read needs a characteristic");
        const after = found[1].trim();
        let expect: Pattern | null = null;
        if (after) {
          const match = /^expect\s+(.*)$/i.exec(after);
          if (!match) return fail("After read <char> only expect <value> can come");
          const parsed = value(match[1]);
          if (typeof parsed === "string") return fail(parsed);
          expect = parsed;
        }
        steps.push({ kind: "read", target: found[0], expect, line });
        break;
      }
      case "expect": {
        const found = target(rest);
        if (!found) return fail("expect needs a characteristic and a value");
        let after = found[1].trim();
        let within = 2000;
        const time = /\s+within\s+(\d+)\s*(ms)?$/i.exec(after);
        if (time) {
          within = Number(time[1]);
          after = after.slice(0, time.index);
        }
        const parsed = value(after);
        if (typeof parsed === "string") return fail(parsed);
        steps.push({ kind: "expect", target: found[0], expect: parsed, within, line });
        break;
      }
      default:
        return fail(`Unknown step "${word}". ${HELP}`);
    }
  }
  return { steps };
}

/** Whether a value's bytes look like the pattern. */
export function matches(pattern: Pattern, data: number[]): boolean {
  if (pattern.hex) return pattern.hex.every((byte, at) => data[at] === byte);
  const text = new TextDecoder().decode(Uint8Array.from(data));
  if (pattern.regex !== undefined) return new RegExp(pattern.regex).test(text);
  return text.includes(pattern.text ?? "");
}

/** Whether a characteristic is the one a step names: by short UUID, full UUID or name, any case. */
export function isTarget(name: string, uuidText: string, key: string, wanted: string): boolean {
  const want = wanted.toLowerCase().replace(/^0x/, "");
  const characteristic = key.split("/")[1]?.toLowerCase() ?? "";
  const short = /^0000([0-9a-f]{4})-0000-1000-8000-00805f9b34fb$/.exec(characteristic)?.[1];
  return (
    name.toLowerCase() === want ||
    uuidText.toLowerCase().replace(/^0x/, "") === want ||
    characteristic === want ||
    short === want ||
    characteristic.startsWith(want)
  );
}
