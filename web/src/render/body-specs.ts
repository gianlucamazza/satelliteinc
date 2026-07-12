/** Per-body presentation: texture paths and offline fallback colors. */
export interface BodyStyle {
  textureUrl?: string;
  remoteFallbackUrl?: string;
  fallbackColor: number;
  emissive: number;
}

const STYLES: Record<string, BodyStyle> = {
  earth: {
    textureUrl: "/textures/earth_atmos_2048.jpg",
    remoteFallbackUrl:
      "https://raw.githubusercontent.com/mrdoob/three.js/r175/examples/textures/planets/earth_atmos_2048.jpg",
    fallbackColor: 0x4488cc,
    emissive: 0x0a1520,
  },
  moon: {
    textureUrl: "/textures/moon_1024.jpg",
    remoteFallbackUrl:
      "https://raw.githubusercontent.com/mrdoob/three.js/r175/examples/textures/planets/moon_1024.jpg",
    fallbackColor: 0x9a9a9a,
    emissive: 0x0c0c10,
  },
  // Mars texture: Solar System Scope (CC BY 4.0) — see README attribution.
  mars: {
    textureUrl: "/textures/mars_2048.jpg",
    fallbackColor: 0xb5532a,
    emissive: 0x140a06,
  },
  venus: { fallbackColor: 0xd9b078, emissive: 0x141006 },
  jupiter: { fallbackColor: 0xc8a678, emissive: 0x14100a },
};

const DEFAULT_STYLE: BodyStyle = {
  fallbackColor: 0x777788,
  emissive: 0x0a0a10,
};

export function bodyStyle(textureId: string | null): BodyStyle {
  return (textureId ? STYLES[textureId] : undefined) ?? DEFAULT_STYLE;
}
