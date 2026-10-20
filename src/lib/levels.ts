import type { Dpi, ProfileLevel } from "@/lib/api";

// DPI levels live in the active profile (see src-tauri/src/profiles.rs).

export const MAX_LEVELS = 5;

export type Level = ProfileLevel;

export function nearest(list: number[], value: number): number {
  if (!list.length) return value;
  return list.reduce((best, v) => (Math.abs(v - value) < Math.abs(best - value) ? v : best), list[0]);
}

export function supportedValues(dpi: Dpi): number[] {
  return dpi.supported.length ? dpi.supported : [dpi.x];
}

export function levelMatches(l: Level, dpi: Dpi): boolean {
  return l.x === dpi.x && (!dpi.supports_y || (l.y ?? l.x) === (dpi.y ?? dpi.x));
}

export function levelLabel(l: Level): string {
  return l.y != null && l.y !== l.x ? `${l.x}×${l.y}` : String(l.x);
}
