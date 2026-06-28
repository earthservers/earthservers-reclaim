// Theme tokens - CSS variables for theming
// Adapted from EarthSocial for EarthServers Local

export type ThemeTokens = {
  name?: string;
  // Base colors
  textColor: string;
  accentColor: string;
  highlightColor?: string;
  cardBg: string;
  appBg?: string;
  backgroundColor?: string;
  navbarBg?: string;
  navbarOpacity?: number;
  dropdownColor?: string;
  dropdownOpacity?: number;
  primaryColor: string;
  secondaryColor: string;
  cardGradientColor1: string;
  cardGradientColor2: string;
  cardGradientEnabled?: boolean;
  cardGradientAngle?: number;
  cardOpacity?: number;
  // Animated background HSL
  bg1: string;
  bg2: string;
  auroraSpeed?: string;
  radius?: string;
};

// Default theme - EarthServers style
export const DEFAULT_THEME: ThemeTokens = {
  name: "EarthServers Default",
  textColor: "#f0f0f0",
  accentColor: "#0178C6",
  highlightColor: "#0fab89",
  cardBg: "rgba(26, 26, 46, 0.8)",
  appBg: "linear-gradient(135deg, #0a0a0f, #1a1a2e)",
  navbarBg: "#0a0a0f",
  navbarOpacity: 90,
  primaryColor: "#0fab89",
  secondaryColor: "#e91e63",
  cardGradientColor1: "#1a1a2e",
  cardGradientColor2: "#2a2a3e",
  cardGradientEnabled: false,
  cardGradientAngle: 135,
  cardOpacity: 80,
  bg1: "165 90% 40%",
  bg2: "330 80% 50%",
  auroraSpeed: "16s",
  radius: "12px",
};

// Preset themes matching EarthSocial
export const PRESET_THEMES = {
  "earthservers-default": {
    name: "EarthServers Default",
    textColor: "#f0f0f0",
    accentColor: "#0178C6",
    highlightColor: "#0fab89",
    cardBg: "rgba(26, 26, 46, 0.8)",
    appBg: "linear-gradient(135deg, #0a0a0f, #1a1a2e)",
    navbarBg: "#0a0a0f",
    navbarOpacity: 90,
    primaryColor: "#0fab89",
    secondaryColor: "#e91e63",
    cardGradientColor1: "#1a1a2e",
    cardGradientColor2: "#2a2a3e",
    cardGradientEnabled: false,
    cardOpacity: 80,
    bg1: "165 90% 40%",
    bg2: "330 80% 50%",
  },
  "ocean-turtle": {
    name: "Ocean Turtle",
    textColor: "#e0f7fa",
    accentColor: "#26c6da",
    highlightColor: "#4dd0e1",
    cardBg: "rgba(0, 77, 64, 0.28)",
    appBg: "linear-gradient(135deg, #006064, #00838f, #0097a7)",
    navbarBg: "#006064",
    navbarOpacity: 92,
    primaryColor: "#006064",
    secondaryColor: "#0097a7",
    cardGradientColor1: "#00695c",
    cardGradientColor2: "#00897b",
    cardGradientEnabled: true,
    cardGradientAngle: 120,
    cardOpacity: 80,
    bg1: "184 100% 38%",
    bg2: "186 85% 45%",
  },
  "mountain-eagle": {
    name: "Mountain Eagle",
    textColor: "#eceff1",
    accentColor: "#78909c",
    highlightColor: "#90a4ae",
    cardBg: "rgba(38, 50, 56, 0.45)",
    appBg: "linear-gradient(180deg, #263238, #37474f, #546e7a)",
    navbarBg: "#263238",
    navbarOpacity: 95,
    primaryColor: "#263238",
    secondaryColor: "#546e7a",
    cardGradientColor1: "#37474f",
    cardGradientColor2: "#455a64",
    cardGradientEnabled: true,
    cardGradientAngle: 180,
    cardOpacity: 85,
    bg1: "200 15% 22%",
    bg2: "200 18% 35%",
  },
  "sun-fire": {
    name: "Sun Fire",
    textColor: "#3e2723",
    accentColor: "#ff9800",
    highlightColor: "#ffb74d",
    cardBg: "rgba(255, 248, 225, 0.65)",
    appBg: "linear-gradient(135deg, #ffa726, #ffb74d, #ffca28, #ffd54f)",
    navbarBg: "#ffa726",
    navbarOpacity: 90,
    primaryColor: "#ffa726",
    secondaryColor: "#ffd54f",
    cardGradientColor1: "#ffb74d",
    cardGradientColor2: "#ffca28",
    cardGradientEnabled: true,
    cardGradientAngle: 45,
    cardOpacity: 90,
    bg1: "38 95% 55%",
    bg2: "48 98% 65%",
  },
  "lightning-bolt": {
    name: "Lightning Bolt",
    textColor: "#e8eaf6",
    accentColor: "#7c4dff",
    highlightColor: "#9575cd",
    cardBg: "rgba(49, 27, 146, 0.32)",
    appBg: "linear-gradient(135deg, #1a237e, #283593, #3949ab, #5c6bc0)",
    navbarBg: "#1a237e",
    navbarOpacity: 94,
    primaryColor: "#1a237e",
    secondaryColor: "#5c6bc0",
    cardGradientColor1: "#283593",
    cardGradientColor2: "#512da8",
    cardGradientEnabled: true,
    cardGradientAngle: 90,
    cardOpacity: 85,
    bg1: "250 85% 52%",
    bg2: "230 70% 62%",
  },
  "air-clouds": {
    name: "Air Clouds",
    textColor: "#37474f",
    accentColor: "#64b5f6",
    highlightColor: "#42a5f5",
    cardBg: "rgba(236, 239, 241, 0.75)",
    appBg: "linear-gradient(180deg, #81d4fa, #b3e5fc, #e1f5fe, #eceff1)",
    navbarBg: "#81d4fa",
    navbarOpacity: 92,
    primaryColor: "#81d4fa",
    secondaryColor: "#eceff1",
    cardGradientColor1: "#b3e5fc",
    cardGradientColor2: "#e1f5fe",
    cardGradientEnabled: true,
    cardGradientAngle: 160,
    cardOpacity: 90,
    bg1: "199 80% 85%",
    bg2: "200 50% 92%",
  },
} as const;

export type PresetThemeKey = keyof typeof PRESET_THEMES;

export function getThemeByKey(key: PresetThemeKey): ThemeTokens {
  return PRESET_THEMES[key];
}

export const THEME_DISPLAY_NAMES: Record<PresetThemeKey, string> = {
  'earthservers-default': 'EarthServers Default',
  'ocean-turtle': 'Ocean Turtle',
  'mountain-eagle': 'Mountain Eagle',
  'sun-fire': 'Sun Fire',
  'lightning-bolt': 'Lightning Bolt',
  'air-clouds': 'Air Clouds',
};
