/**
 * Shared render palette for Three.js scenes.
 * Keep in sync with `:root` tokens in style.css (UI uses the CSS vars directly).
 */
export const RENDER_COLORS = {
  sceneBg: 0x050810,
  mapBg: 0x03060c,
  accent: 0x4da3ff,
  accentLight: 0x88bbff,
  success: 0x3dd68c,
  warn: 0xffb347,
  danger: 0xff6b6b,
  muted: 0x6a7a94,
  stationIdle: 0xff4466,
  satellite: 0xffdd44,
  satellitePass: 0x3dd68c,
  satelliteDownlink: 0xff8844,
  transfer: 0xff8844,
  refSat: 0xaa66ff,
  fleetDot: 0x88ddff,
  focusRing: 0xffdd44,
  cruise: 0xffaa44,
  sun: 0xffcc44,
  bodyDefault: 0x8899aa,
  orbitGuide: 0x334466,
  ambient: 0x334466,
  white: 0xffffff,
} as const;

/** Schematic body tints for the system map (texture-free). */
export const BODY_COLORS: Record<string, number> = {
  sun: RENDER_COLORS.sun,
  earth: RENDER_COLORS.accent,
  moon: 0xaaaaaa,
  venus: 0xd9b078,
  mars: 0xff6644,
  jupiter: 0xc8a678,
};

export function hexCss(hex: number): string {
  return `#${hex.toString(16).padStart(6, "0")}`;
}

export function prefersReducedMotion(): boolean {
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}