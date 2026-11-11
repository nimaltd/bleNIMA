// Values to write to standard characteristics, made from what a person picks
// instead of typed hex: the time now, an alert level, a control point's
// request. It has no imports, so its tests run in Node as they are.

/** The characteristics that have a form, by their 16-bit UUIDs. */
export const FORMS = new Set([0x2a2b, 0x2a0f, 0x2a06, 0x2a39, 0x2a55, 0x2a66, 0x2a52, 0x2a21]);

/** The control points, whose answers come as indications. */
export const ANSWERED = new Set([0x2a55, 0x2a66, 0x2a52]);

function le16(value: number): number[] {
  return [value & 0xff, (value >> 8) & 0xff];
}

function le32(value: number): number[] {
  return [value & 0xff, (value >>> 8) & 0xff, (value >>> 16) & 0xff, (value >>> 24) & 0xff];
}

/** A whole number typed, from `min` to `max`; an error in words if it is not one. */
function whole(input: string, min: number, max: number): number | string {
  const value = Number(input.trim());
  if (input.trim() === "" || !Number.isInteger(value) || value < min || value > max) {
    return `A whole number from ${min} to ${max}`;
  }
  return value;
}

/**
 * Current Time, 0x2A2B: the date and time of `now`, its day of the week, the
 * 1/256 seconds, and "manual time update" as the reason it changed.
 */
export function currentTime(now: Date): number[] {
  // The day of the week counts from Monday, 1, to Sunday, 7.
  const day = now.getDay() === 0 ? 7 : now.getDay();
  return [
    ...le16(now.getFullYear()),
    now.getMonth() + 1,
    now.getDate(),
    now.getHours(),
    now.getMinutes(),
    now.getSeconds(),
    day,
    Math.floor((now.getMilliseconds() * 256) / 1000),
    0x01,
  ];
}

/** The PC's time zone at `now`: minutes ahead of UTC in standard time, and the minutes DST adds now. */
export function pcTimeZone(now: Date): { standard: number; dst: number } {
  // getTimezoneOffset is minutes behind UTC; the larger of January's and July's is standard time.
  const january = new Date(now.getFullYear(), 0, 1).getTimezoneOffset();
  const july = new Date(now.getFullYear(), 6, 1).getTimezoneOffset();
  // "+ 0" makes UTC's -0 a plain 0.
  const standard = -Math.max(january, july) + 0;
  return { standard, dst: -now.getTimezoneOffset() - standard + 0 };
}

/**
 * Local Time Information, 0x2A0F: the time zone in 15-minute steps, and the
 * DST offset as its code: 0 for none, 2 for half an hour, 4 for an hour, 8
 * for two, and 255 for one the standard has no code for.
 */
export function localTimeInformation(standard: number, dst: number): number[] {
  const zone = Math.round(standard / 15);
  const codes: Record<number, number> = { 0: 0, 30: 2, 60: 4, 120: 8 };
  return [zone & 0xff, codes[dst] ?? 255];
}

/** Alert Level, 0x2A06: 0 no alert, 1 mild, 2 high. */
export function alertLevel(level: 0 | 1 | 2): number[] {
  return [level];
}

/** Heart Rate Control Point, 0x2A39: reset the energy expended. */
export const RESET_ENERGY_EXPENDED = [0x01];

/** SC Control Point, 0x2A55, of Cycling and Running Speed and Cadence. */
export const scControlPoint = {
  /** The wheel revolutions, or the total distance, to count from. */
  setCumulativeValue(input: string): number[] | string {
    const value = whole(input, 0, 0xffffffff);
    return typeof value === "string" ? value : [0x01, ...le32(value)];
  },
  startCalibration: [0x02],
  updateLocation(location: number): number[] {
    return [0x03, location];
  },
  requestLocations: [0x04],
};

/** Cycling Power Control Point, 0x2A66. */
export const cyclingPowerControlPoint = {
  /** The crank length in millimetres, in steps of half a millimetre. */
  setCrankLength(input: string): number[] | string {
    const millimetres = Number(input.trim());
    const halves = millimetres * 2;
    if (input.trim() === "" || !Number.isInteger(halves) || halves < 0 || halves > 0xffff) {
      return "Millimetres from 0 to 32767.5, in steps of 0.5";
    }
    return [0x04, ...le16(halves)];
  },
  requestCrankLength: [0x05],
  startOffsetCompensation: [0x0c],
};

/** Record Access Control Point, 0x2A52: op code, operator, operand. */
export const recordAccess = {
  reportAll: [0x01, 0x01],
  deleteAll: [0x02, 0x01],
  abort: [0x03, 0x00],
  countAll: [0x04, 0x01],
};

/** Measurement Interval, 0x2A21: seconds between measurements; 0 for none. */
export function measurementInterval(input: string): number[] | string {
  const value = whole(input, 0, 0xffff);
  return typeof value === "string" ? value : le16(value);
}
