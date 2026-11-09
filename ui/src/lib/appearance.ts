// Appearance names, such as "Heart Rate Sensor: Heart Rate Belt", from the
// same Nordic Bluetooth numbers database the Rust side names them from.

import table from "../../../crates/blenima-core/data/gap_appearance.json?raw";

type Category = { category: number; name: string; subcategory?: { value: number; name: string }[] };

/** The full 16-bit value: the category in the top 10 bits, the subcategory in the low 6. */
const NAMES = new Map<number, string>();
for (const category of JSON.parse(table) as Category[]) {
  NAMES.set(category.category << 6, category.name);
  for (const sub of category.subcategory ?? []) NAMES.set((category.category << 6) | sub.value, `${category.name}: ${sub.name}`);
}

/** What an appearance value says the device is, by its subcategory or else its category. */
export function appearanceName(value: number): string | null {
  return NAMES.get(value) ?? NAMES.get(value & ~0x3f) ?? null;
}
