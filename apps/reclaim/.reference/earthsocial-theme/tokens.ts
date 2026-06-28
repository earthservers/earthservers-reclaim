// The CSS vars you actually use in globals.css and components.
// Keep this small and brand-focused; everything else derives from these.
export type ThemeTokens = {
  name?: string
  // base colors
  textColor: string           // e.g. "#fff"
  accentColor: string         // e.g. "#f97316"
  highlightColor?: string     // for input focus, dropdown highlights, etc.
  cardBg: string              // rgba OK
  appBg?: string              // gradient or solid; optional if you use animated layer
  backgroundColor?: string    // fallback background color
  navbarBg?: string           // navbar background color
  navbarOpacity?: number      // 0-100, navbar opacity
  tabBarBg?: string           // tab bar background color
  tabBarOpacity?: number      // 0-100, tab bar opacity
  dropdownColor?: string      // dropdown background color
  dropdownOpacity?: number    // 70-100, dropdown menu opacity
  dropdownButtonOpacity?: number // 0-100, dropdown button opacity
  primaryColor: string        // gradient start color (required)
  secondaryColor: string      // gradient end color (required)
  cardGradientColor1: string  // card gradient color 1 (required - all presets define this)
  cardGradientColor2: string  // card gradient color 2 (required - all presets define this)
  cardGradientEnabled?: boolean // card gradient enabled
  cardGradientAngle?: number  // card gradient angle
  // animated bg hues for aurora (HSL components)
  bg1: string                 // "198 90% 40%"
  bg2: string                 // "187 76% 35%"
  auroraSpeed?: string        // "16s"
  radius?: string             // "12px"
  // Advanced controls (Midnight Lizard-inspired) - HSV
  backgroundSaturationLimit?: number  // 0-100, default 70
  backgroundContrast?: number         // 0-100, default 50
  backgroundBrightnessLimit?: number  // 0-100, default 14
  backgroundGraySaturation?: number   // 0-100, default 5
  backgroundHueGravity?: number       // 0-100, default 0
  backgroundDefaultHue?: number       // 0-360, default 165
  // TMI (Temperature, Magenta, Intensity) controls
  temperatureLimit?: number           // 0-100, default 50 (cool to warm)
  magentaLimit?: number               // 0-100, default 50 (green to magenta)
  intensityLimit?: number             // 0-100, default 50 (dark to bright)
}

// Minimal defaults (site fallback) - using Ocean Turtle colors
export const DEFAULT_THEME: ThemeTokens = {
  name: "Default",
  textColor: "#e0f7fa",
  accentColor: "#26c6da",
  highlightColor: "#4dd0e1", // Bright cyan-blue for focus states
  cardBg: "rgba(0,77,64,0.28)",
  appBg: "linear-gradient(135deg, #006064, #00838f, #0097a7)",
  navbarBg: "#006064",
  navbarOpacity: 92,
  tabBarBg: "#00838f",
  tabBarOpacity: 88,
  dropdownColor: "#004d56", // Deep ocean blue-green
  dropdownOpacity: 95,
  dropdownButtonOpacity: 90,
  primaryColor: "#006064",
  secondaryColor: "#0097a7",
  cardGradientColor1: "#00695c",
  cardGradientColor2: "#00897b",
  cardGradientEnabled: true,
  cardGradientAngle: 120,
  bg1: "184 100% 38%",
  bg2: "186 85% 45%",
  auroraSpeed: "16s",
  radius: "12px",
  // HSV controls
  backgroundSaturationLimit: 65, // Vibrant ocean colors - boost saturation
  backgroundContrast: 55, // Moderate contrast for underwater depth
  backgroundBrightnessLimit: 52, // Slightly brighter for sunlight through water
  backgroundGraySaturation: 15, // Add aquatic tint to grays
  backgroundHueGravity: 25, // Pull colors toward cyan/teal
  backgroundDefaultHue: 184, // Teal/cyan (ocean color)
  // TMI controls - Cool ocean depths
  temperatureLimit: 50, // Cool like deep ocean water
  magentaLimit: 50, // Slight magenta for ocean blues
  intensityLimit: 50, // Slightly darker for underwater feel
}

// Free presets
export const PRESET_THEMES = {
  "ocean-turtle": {
    name: "Ocean Turtle",
    textColor: "#e0f7fa", // Bright cyan-white for ocean foam
    accentColor: "#26c6da", // Vibrant turquoise for sea life
    highlightColor: "#4dd0e1", // Bright cyan-blue for focus states
    cardBg: "rgba(0,77,64,0.28)", // Deep sea green with transparency
    appBg: "linear-gradient(135deg, #006064, #00838f, #0097a7)", // Deep ocean to shallow water
    navbarBg: "#006064", // Deep ocean navbar
    navbarOpacity: 92,
    tabBarBg: "#00838f", // Mid-ocean tab bar
    tabBarOpacity: 88,
    dropdownColor: "#004d56", // Deep ocean blue-green
    dropdownOpacity: 95,
    dropdownButtonOpacity: 90,
    primaryColor: "#006064",
    secondaryColor: "#0097a7",
    cardGradientColor1: "#00695c",
    cardGradientColor2: "#00897b",
    cardGradientEnabled: true,
    cardGradientAngle: 120, // Wave-like angle
    bg1: "184 100% 38%", // Teal ocean
    bg2: "186 85% 45%", // Lighter aqua
    // HSV controls
    backgroundSaturationLimit: 65, // Vibrant ocean colors - boost saturation
    backgroundContrast: 55, // Moderate contrast for underwater depth
    backgroundBrightnessLimit: 52, // Slightly brighter for sunlight through water
    backgroundGraySaturation: 15, // Add aquatic tint to grays
    backgroundHueGravity: 25, // Pull colors toward cyan/teal
    backgroundDefaultHue: 184, // Teal/cyan (ocean color)
    // TMI controls - Cool, slightly magenta-shifted ocean depths
    temperatureLimit: 50, // Cool like deep ocean water
    magentaLimit: 50, // Slight magenta for ocean blues
    intensityLimit: 50, // Slightly darker for underwater feel
  },
  "mountain-eagle": {
    name: "Mountain Eagle",
    textColor: "#eceff1", // Light gray like mountain snow
    accentColor: "#78909c", // Stone gray-blue
    highlightColor: "#90a4ae", // Lighter stone blue for highlights
    cardBg: "rgba(38,50,56,0.45)", // Dark slate with depth
    appBg: "linear-gradient(180deg, #263238, #37474f, #546e7a)", // Peak to valley gradient (vertical)
    navbarBg: "#263238", // Dark mountain peak
    navbarOpacity: 95,
    tabBarBg: "#37474f", // Mid-mountain
    tabBarOpacity: 90,
    dropdownColor: "#1c2a30", // Dark stone gray
    dropdownOpacity: 95,
    dropdownButtonOpacity: 92,
    primaryColor: "#263238",
    secondaryColor: "#546e7a",
    cardGradientColor1: "#37474f",
    cardGradientColor2: "#455a64",
    cardGradientEnabled: true,
    cardGradientAngle: 180, // Top to bottom like mountains
    bg1: "200 15% 22%", // Dark slate blue
    bg2: "200 18% 35%", // Lighter slate
    // HSV controls
    backgroundSaturationLimit: 35, // Desaturated for stone/rock feel
    backgroundContrast: 60, // High contrast for dramatic peaks/valleys
    backgroundBrightnessLimit: 48, // Slightly darker for mountain shadows
    backgroundGraySaturation: 5, // Minimal - mountains are naturally gray
    backgroundHueGravity: 10, // Subtle pull toward cool gray-blue
    backgroundDefaultHue: 200, // Cool blue-gray (mountain stone)
    // TMI controls - Cool, neutral stone with depth
    temperatureLimit: 50, // Cool like mountain air
    magentaLimit: 50, // Slightly green-shifted for natural stone
    intensityLimit: 50, // Darker for dramatic shadows and peaks
  },
  "sun-fire": {
    name: "Sun Fire",
    textColor: "#3e2723", // Softer dark brown
    accentColor: "#ff9800", // Softer orange
    highlightColor: "#ffb74d", // Golden amber for highlights
    cardBg: "rgba(255,248,225,0.65)", // Gentle warm glow
    appBg: "linear-gradient(135deg, #ffa726, #ffb74d, #ffca28, #ffd54f)", // Gentler warm gradient
    navbarBg: "#ffa726", // Softer orange navbar
    navbarOpacity: 90,
    tabBarBg: "#ffb74d", // Gentle warm tab
    tabBarOpacity: 85,
    dropdownColor: "#ff8f00", // Bright amber-orange
    dropdownOpacity: 90,
    dropdownButtonOpacity: 85,
    primaryColor: "#ffa726",
    secondaryColor: "#ffd54f",
    cardGradientColor1: "#ffb74d",
    cardGradientColor2: "#ffca28",
    cardGradientEnabled: true,
    cardGradientAngle: 45, // Flame licking upward angle
    bg1: "38 95% 55%", // Softer orange
    bg2: "48 98% 65%", // Warm golden
    // HSV controls
    backgroundSaturationLimit: 75, // High saturation for vibrant fire colors
    backgroundContrast: 48, // Lower contrast for soft radiant glow
    backgroundBrightnessLimit: 58, // Brighter for sun/fire radiance
    backgroundGraySaturation: 20, // Warm glow even in grays
    backgroundHueGravity: 35, // Strong pull toward warm orange/gold
    backgroundDefaultHue: 40, // Orange/golden (sun/fire color)
    // TMI controls - Warm, golden radiance
    temperatureLimit: 50, // Very warm like fire and sunlight
    magentaLimit: 50, // Shift toward golden/yellow (away from magenta)
    intensityLimit: 50, // Bright and radiant
  },
  "lightning-bolt": {
    name: "Lightning Bolt",
    textColor: "#e8eaf6", // Electric white-purple
    accentColor: "#7c4dff", // Electric violet
    highlightColor: "#9575cd", // Bright electric purple for highlights
    cardBg: "rgba(49,27,146,0.32)", // Deep electric purple
    appBg: "linear-gradient(135deg, #1a237e, #283593, #3949ab, #5c6bc0)", // Storm to electric sky
    navbarBg: "#1a237e", // Deep storm navbar
    navbarOpacity: 94,
    tabBarBg: "#3949ab", // Electric sky tab
    tabBarOpacity: 89,
    dropdownColor: "#311b92", // Deep electric purple
    dropdownOpacity: 95,
    dropdownButtonOpacity: 88,
    primaryColor: "#1a237e",
    secondaryColor: "#5c6bc0",
    cardGradientColor1: "#283593",
    cardGradientColor2: "#512da8",
    cardGradientEnabled: true,
    cardGradientAngle: 90, // Vertical lightning strike
    bg1: "250 85% 52%", // Electric purple
    bg2: "230 70% 62%", // Lighter electric blue
    // HSV controls
    backgroundSaturationLimit: 70, // High saturation for electric vibrancy
    backgroundContrast: 62, // Very high contrast for dramatic strikes
    backgroundBrightnessLimit: 46, // Darker for stormy atmosphere
    backgroundGraySaturation: 18, // Electric tint in grays
    backgroundHueGravity: 30, // Pull toward electric purple
    backgroundDefaultHue: 250, // Electric purple (lightning color)
    // TMI controls - Cool, electric purple energy
    temperatureLimit: 50, // Very cool like electric storms
    magentaLimit: 50, // Strong magenta shift for purple/violet
    intensityLimit: 50, // Dark and dramatic like storm clouds
  },
  "air-clouds": {
    name: "Air Clouds",
    textColor: "#37474f", // Lighter gray for better readability
    accentColor: "#64b5f6", // Softer, lighter sky blue
    highlightColor: "#42a5f5", // Vibrant sky blue for highlights
    cardBg: "rgba(236,239,241,0.75)", // Light gray clouds
    appBg: "linear-gradient(180deg, #81d4fa, #b3e5fc, #e1f5fe, #eceff1)", // Lighter blue gradient to gray
    navbarBg: "#81d4fa", // Lighter sky blue navbar
    navbarOpacity: 92,
    tabBarBg: "#b3e5fc", // Very light blue tab
    tabBarOpacity: 88,
    dropdownColor: "#4fc3f7", // Bright sky blue
    dropdownOpacity: 92,
    dropdownButtonOpacity: 88,
    primaryColor: "#81d4fa",
    secondaryColor: "#eceff1",
    cardGradientColor1: "#b3e5fc",
    cardGradientColor2: "#e1f5fe",
    cardGradientEnabled: true,
    cardGradientAngle: 160, // Gentle cloud drift
    bg1: "199 80% 85%", // Lighter sky blue
    bg2: "200 50% 92%", // Very light gray-blue
    // HSV controls
    backgroundSaturationLimit: 55, // Moderate saturation for soft airy feel
    backgroundContrast: 42, // Low contrast for gentle cloud softness
    backgroundBrightnessLimit: 56, // Brighter for open daylight sky
    backgroundGraySaturation: 12, // Soft sky tint in grays
    backgroundHueGravity: 15, // Gentle pull toward sky blue
    backgroundDefaultHue: 199, // Sky blue (cloud/air color)
    // TMI controls - Light, airy, soft and bright
    temperatureLimit: 50, // Slightly cool like fresh air
    magentaLimit: 50, // Neutral magenta for pure sky blue
    intensityLimit: 50, // Bright like open sky and daylight
  },
} as const;

export type PresetThemeKey = keyof typeof PRESET_THEMES;

// Helper to get theme by key
export function getThemeByKey(key: PresetThemeKey): ThemeTokens {
  return PRESET_THEMES[key];
}

// Theme display names
export const THEME_DISPLAY_NAMES: Record<PresetThemeKey, string> = {
  'ocean-turtle': 'Ocean Turtle',
  'mountain-eagle': 'Mountain Eagle',
  'sun-fire': 'Sun Fire',
  'lightning-bolt': 'Lightning Bolt',
  'air-clouds': 'Air Clouds',
};
