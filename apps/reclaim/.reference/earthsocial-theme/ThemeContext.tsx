import { createContext, useContext, useState, ReactNode, useEffect, useRef } from 'react';
import {
  PRESET_THEMES,
  THEME_DISPLAY_NAMES,
  getThemeByKey,
  type PresetThemeKey,
  type ThemeTokens
} from '@theme/tokens';
import { getThemeAnimations } from '@theme/animation-config';
import { useAuth } from './AuthContext';
import { transformColor, type ColorTransformOptions } from '../utils/colorTransform';
import { getCachedSiteCustomization, cacheSiteCustomization } from '../utils/themeCache';

type ThemeContextType = {
  // Current theme preset
  currentPreset: PresetThemeKey;
  setCurrentPreset: (preset: PresetThemeKey) => void;

  // Theme tokens for current preset
  theme: ThemeTokens;

  // Site theme customization (includes colorSpace, limits, etc.)
  siteCustomization: any | null;

  // Animations from saved theme (or null if using preset defaults)
  siteAnimations: any | null;

  // Loading state (false when cache is available)
  isLoading: boolean;

  // All available presets
  availablePresets: typeof PRESET_THEMES;
  presetDisplayNames: typeof THEME_DISPLAY_NAMES;

  // Legacy compatibility
  setTheme?: (theme: 'light' | 'dark') => void;
};

const ThemeContext = createContext<ThemeContextType | undefined>(undefined);

// Memoize getInitialPreset to prevent redundant localStorage reads
// This cache persists across component remounts (including StrictMode double-rendering)
let cachedPreset: PresetThemeKey | null = null;
const getInitialPreset = (): PresetThemeKey => {
  // Return cached value if available
  if (cachedPreset) {
    return cachedPreset;
  }

  const savedPreset = localStorage.getItem('earth-social-theme') || localStorage.getItem('themePreset');

  if (savedPreset && savedPreset in PRESET_THEMES) {
    cachedPreset = savedPreset as PresetThemeKey;
    return cachedPreset;
  }

  cachedPreset = 'ocean-turtle';
  return cachedPreset;
};

export function ThemeProvider({ children }: { children: ReactNode }) {
  const { user, token } = useAuth(); // Get user and token from AuthContext

  // Track if we've already initialized to prevent redundant theme applications
  const hasInitialized = useRef(false);

  const initialPreset = getInitialPreset();
  const [currentPreset, setCurrentPreset] = useState<PresetThemeKey>(initialPreset);

  // STEP 1: Load from cache SYNCHRONOUSLY on mount (before first render)
  // UNIFIED CACHE: Always load site theme by default
  // ProfileClient and CommunityClient will override when they mount
  const cachedData = (() => {
    try {
      // Load site theme cache (siteCustomization)
      return getCachedSiteCustomization();
    } catch (error) {
      console.error('[ThemeContext] ❌ Failed to load site theme on mount:', error);
      return null;
    }
  })();
  const [siteCustomization, setSiteCustomization] = useState<any | null>(() => {
    // This runs once, synchronously, before first render
    if (cachedData) {
      // Use baseColors for siteCustomization (this is what pages expect)
      const customization = cachedData.baseColors || cachedData.settings;
            return customization;
    }
    return null;
  });

  const [siteAnimations, setSiteAnimations] = useState<any | null>(() => {
    // Load animations from cache on mount
    if (cachedData?.animations) {
      console.log('[ThemeContext] 🎬 Initial animations from cache (partial):', cachedData.animations);
      // CRITICAL: Merge cached partial animations with preset defaults
      // Cache may only contain customized fields (e.g., enabled, speed)
      // But we need full animation configs with count, size, type, positions
      const fullAnimations = getThemeAnimations(initialPreset);
      const mergedAnimations = {
        characters: fullAnimations.characters.map((defaultChar) => {
          const customChar = cachedData.animations.characters?.find((c: any) => c.id === defaultChar.id);
          return customChar ? { ...defaultChar, ...customChar } : defaultChar;
        }),
        bubbles: cachedData.animations.bubbles && fullAnimations.bubbles
          ? { ...fullAnimations.bubbles, ...cachedData.animations.bubbles }
          : fullAnimations.bubbles,
        decorations: fullAnimations.decorations.map((defaultDeco) => {
          const customDeco = cachedData.animations.decorations?.find((d: any) => d.id === defaultDeco.id);
          return customDeco ? { ...defaultDeco, ...customDeco } : defaultDeco;
        }),
      };
      console.log('[ThemeContext] 🎬 Initial animations merged with preset defaults:', mergedAnimations);
      return mergedAnimations;
    }
    // Fallback: Use preset animations if no cached animations
    // This ensures animations always work even without customization
    const fallback = getThemeAnimations(initialPreset);
    console.log('[ThemeContext] 🎬 Initial animations from fallback (preset:', initialPreset, '):', fallback);
    return fallback;
  });

  const [isLoading, setIsLoading] = useState(!siteCustomization); // Not loading if we have cache
  const [theme, setTheme] = useState<ThemeTokens>(() => {
    // Debug: Log what we have in cachedData
    
    // If we have cached COMPUTED colors, use them directly (no transformation needed!)
    if (cachedData?.computedColors && cachedData.computedColors.primaryColor) {
            const baseTheme = PRESET_THEMES[initialPreset];

      // Use the PRE-COMPUTED theme (already has color spacing applied)
      const customizedTheme: ThemeTokens = {
        ...baseTheme,
        primaryColor: cachedData.computedColors.primaryColor,
        secondaryColor: cachedData.computedColors.secondaryColor,
        textColor: cachedData.computedColors.textColor || baseTheme.textColor,
        accentColor: cachedData.computedColors.accentColor || baseTheme.accentColor,
        navbarBg: cachedData.computedColors.navbarBg || baseTheme.navbarBg,
        tabBarBg: cachedData.computedColors.tabBarBg || baseTheme.tabBarBg,
        cardBg: cachedData.computedColors.cardBg || baseTheme.cardBg,
        appBg: cachedData.computedColors.appBg || baseTheme.appBg,
        cardGradientColor1: cachedData.computedColors.cardGradientColor1 || baseTheme.cardGradientColor1,
        cardGradientColor2: cachedData.computedColors.cardGradientColor2 || baseTheme.cardGradientColor2,
      };

      // Apply cached theme to DOM immediately
      const root = document.documentElement;
      root.style.setProperty('--text-color', customizedTheme.textColor);
      root.style.setProperty('--color-accent', customizedTheme.accentColor);
      root.style.setProperty('--card-bg', customizedTheme.cardBg);
      if (customizedTheme.appBg) {
        root.style.setProperty('--app-bg', customizedTheme.appBg);
      }
      if (customizedTheme.navbarBg) {
        root.style.setProperty('--navbar-bg', customizedTheme.navbarBg);
      }
      if (customizedTheme.tabBarBg) {
        root.style.setProperty('--tab-bar-bg', customizedTheme.tabBarBg);
      }

      hasInitialized.current = true;
      return customizedTheme;
    }

    // Fallback: If we only have base colors, build theme (backward compatibility)
    if (cachedData?.baseColors && cachedData.baseColors.primaryColor) {
            const baseTheme = PRESET_THEMES[initialPreset];

      // Build theme with cached customization
      const customizedTheme: ThemeTokens = {
        ...baseTheme,
        primaryColor: cachedData.baseColors.primaryColor,
        secondaryColor: cachedData.baseColors.secondaryColor,
        textColor: cachedData.baseColors.textColor || baseTheme.textColor,
        accentColor: cachedData.baseColors.accentColor || baseTheme.accentColor,
        navbarBg: cachedData.baseColors.navbarBg || baseTheme.navbarBg,
        tabBarBg: cachedData.baseColors.tabBarBg || baseTheme.tabBarBg,
        cardBg: cachedData.baseColors.cardBg || baseTheme.cardBg,
        appBg: cachedData.baseColors.appBg || baseTheme.appBg,
      };

      // Apply cached theme to DOM immediately
      const root = document.documentElement;
      root.style.setProperty('--text-color', customizedTheme.textColor);
      root.style.setProperty('--color-accent', customizedTheme.accentColor);
      root.style.setProperty('--card-bg', customizedTheme.cardBg);
      if (customizedTheme.appBg) {
        root.style.setProperty('--app-bg', customizedTheme.appBg);
      }
      if (customizedTheme.navbarBg) {
        root.style.setProperty('--navbar-bg', customizedTheme.navbarBg);
      }
      if (customizedTheme.tabBarBg) {
        root.style.setProperty('--tab-bar-bg', customizedTheme.tabBarBg);
      }

      hasInitialized.current = true;
      return customizedTheme;
    }

    // No cache - use default preset theme
    const initialTheme = PRESET_THEMES[initialPreset];
    // Apply theme to DOM immediately, before first render
    const root = document.documentElement;
    root.style.setProperty('--text-color', initialTheme.textColor);
    root.style.setProperty('--color-accent', initialTheme.accentColor);
    root.style.setProperty('--card-bg', initialTheme.cardBg);
    if (initialTheme.appBg) {
      root.style.setProperty('--app-bg', initialTheme.appBg);
    }
    if (initialTheme.navbarBg) {
      root.style.setProperty('--navbar-bg', initialTheme.navbarBg);
    }
    if (initialTheme.tabBarBg) {
      root.style.setProperty('--tab-bar-bg', initialTheme.tabBarBg);
    }
    if ((initialTheme as any).radius) {
      root.style.setProperty('--radius', (initialTheme as any).radius);
    }

    // Mark as initialized since we just applied the theme
    hasInitialized.current = true;

    return initialTheme;
  });

  // Apply advanced color transformations to a theme
  // Helper to transform all colors in a gradient string
  const transformGradient = (gradient: string, options: ColorTransformOptions): string => {
    // Match all hex colors in the gradient string
    const hexColorRegex = /#[0-9a-fA-F]{6}/g;

    return gradient.replace(hexColorRegex, (hexColor) => {
      return transformColor(hexColor, options);
    });
  };

  const applyColorTransformations = (baseTheme: ThemeTokens, options: ColorTransformOptions): ThemeTokens => {
    // Transform all color properties in the theme
    const transformedTheme = { ...baseTheme };

    // Transform hex colors
    if (baseTheme.textColor?.startsWith('#')) {
      transformedTheme.textColor = transformColor(baseTheme.textColor, options);
    }
    if (baseTheme.accentColor?.startsWith('#')) {
      transformedTheme.accentColor = transformColor(baseTheme.accentColor, options);
    }
    if (baseTheme.highlightColor?.startsWith('#')) {
      transformedTheme.highlightColor = transformColor(baseTheme.highlightColor, options);
    }
    if (baseTheme.primaryColor?.startsWith('#')) {
      transformedTheme.primaryColor = transformColor(baseTheme.primaryColor, options);
    }
    if (baseTheme.secondaryColor?.startsWith('#')) {
      transformedTheme.secondaryColor = transformColor(baseTheme.secondaryColor, options);
    }
    if (baseTheme.cardGradientColor1?.startsWith('#')) {
      transformedTheme.cardGradientColor1 = transformColor(baseTheme.cardGradientColor1, options);
    }
    if (baseTheme.cardGradientColor2?.startsWith('#')) {
      transformedTheme.cardGradientColor2 = transformColor(baseTheme.cardGradientColor2, options);
    }
    // Transform navigation/UI colors
    if (baseTheme.navbarBg?.startsWith('#')) {
      transformedTheme.navbarBg = transformColor(baseTheme.navbarBg, options);
    }
    if (baseTheme.tabBarBg?.startsWith('#')) {
      transformedTheme.tabBarBg = transformColor(baseTheme.tabBarBg, options);
    }
    if (baseTheme.dropdownColor?.startsWith('#')) {
      transformedTheme.dropdownColor = transformColor(baseTheme.dropdownColor, options);
    }
    // Transform gradients by transforming each hex color in the gradient string
    if (baseTheme.appBg?.includes('gradient')) {
      transformedTheme.appBg = transformGradient(baseTheme.appBg, options);
    } else if (baseTheme.appBg?.startsWith('#')) {
      transformedTheme.appBg = transformColor(baseTheme.appBg, options);
    }
    if (baseTheme.cardBg?.includes('gradient')) {
      transformedTheme.cardBg = transformGradient(baseTheme.cardBg, options);
    } else if (baseTheme.cardBg?.startsWith('#')) {
      transformedTheme.cardBg = transformColor(baseTheme.cardBg, options);
    }

    return transformedTheme;
  };

  // Helper to build theme from customization
  const buildThemeFromCustomization = (customization: any, preset?: PresetThemeKey): ThemeTokens => {
    // CRITICAL: If computedColors already exist (from cache), use them directly!
    // This prevents re-transformation and ensures first render has correct colors
    if (customization.computedColors) {
      
      // Return theme built from pre-computed colors
      const baseTheme = { ...PRESET_THEMES[preset || currentPreset] };
      return {
        ...baseTheme,
        ...customization.computedColors, // Use ALL pre-computed colors
      };
    }

    // If no computedColors, transform from baseColors
    
    let baseTheme: ThemeTokens = { ...PRESET_THEMES[preset || currentPreset] };

    // STEP 1: Override preset colors with user's custom colors BEFORE transformation
    if (customization.primaryColor) baseTheme.primaryColor = customization.primaryColor;
    if (customization.secondaryColor) baseTheme.secondaryColor = customization.secondaryColor;
    if (customization.textColor) baseTheme.textColor = customization.textColor;
    if (customization.accentColor) baseTheme.accentColor = customization.accentColor;
    // CRITICAL: Also set cardGradientColor1/2 on baseTheme so they get transformed
    if (customization.cardGradientColor1) baseTheme.cardGradientColor1 = customization.cardGradientColor1;
    if (customization.cardGradientColor2) baseTheme.cardGradientColor2 = customization.cardGradientColor2;
    // Also set navigation/UI colors on baseTheme so they get transformed
    if (customization.navbarBg) baseTheme.navbarBg = customization.navbarBg;
    if (customization.tabBarBg) baseTheme.tabBarBg = customization.tabBarBg;
    if (customization.dropdownColor) baseTheme.dropdownColor = customization.dropdownColor;

    
    const colorSpace = customization.colorSpace || 'Off';

    // STEP 2: Apply color space transformation if enabled
    if (colorSpace !== 'Off') {
      const transformOptions: ColorTransformOptions = {
        colorSpace,
        temperatureLimit: customization.temperatureLimit ?? 50,
        magentaLimit: customization.magentaLimit ?? 50,
        intensityLimit: customization.intensityLimit ?? 50,
        saturationLimit: customization.backgroundSaturationLimit ?? 50,
        brightnessLimit: customization.backgroundBrightnessLimit ?? 50,
        redLimit: customization.redLimit ?? 50,
        greenLimit: customization.greenLimit ?? 50,
        blueLimit: customization.blueLimit ?? 50,
        contrast: customization.backgroundContrast ?? 50,
        hueGravity: customization.backgroundHueGravity ?? 0,
        graySaturation: customization.backgroundGraySaturation ?? 0,
        targetHue: customization.backgroundDefaultHue ?? 184,
      };

      
      // Apply color transformations when colorSpace is TMI/HSV/RGB
      baseTheme = applyColorTransformations(baseTheme, transformOptions);

          }

    // Helper to convert hex to rgba
    const hexToRgba = (hex: string, alpha: number) => {
      const r = parseInt(hex.slice(1, 3), 16);
      const g = parseInt(hex.slice(3, 5), 16);
      const b = parseInt(hex.slice(5, 7), 16);
      return `rgba(${r}, ${g}, ${b}, ${alpha})`;
    };

    // Helper to blend two colors (returns hex)
    const blendColors = (color1: string, color2: string, ratio: number) => {
      const r1 = parseInt(color1.slice(1, 3), 16);
      const g1 = parseInt(color1.slice(3, 5), 16);
      const b1 = parseInt(color1.slice(5, 7), 16);

      const r2 = parseInt(color2.slice(1, 3), 16);
      const g2 = parseInt(color2.slice(3, 5), 16);
      const b2 = parseInt(color2.slice(5, 7), 16);

      const r = Math.round(r1 + (r2 - r1) * ratio);
      const g = Math.round(g1 + (g2 - g1) * ratio);
      const b = Math.round(b1 + (b2 - b1) * ratio);

      return `#${r.toString(16).padStart(2, '0')}${g.toString(16).padStart(2, '0')}${b.toString(16).padStart(2, '0')}`;
    };

    // Generate app background gradient from primary/secondary colors with favorability and strength
    const appBg = customization.gradientEnabled
      ? (() => {
          const fav = customization.gradientFavorability ?? 50;
          const strength = (customization.gradientStrength ?? 100) / 100;

          const color1 = baseTheme.primaryColor || PRESET_THEMES['ocean-turtle'].primaryColor;
          const color2 = baseTheme.secondaryColor || PRESET_THEMES['ocean-turtle'].secondaryColor;

          // Gradient strength: blend color2 towards color1
          const blendedColor2 = strength === 1 ? color2 : blendColors(color1, color2, strength);

          return `linear-gradient(${customization.gradientAngle || 135}deg, ${color1} 0%, ${blendedColor2} ${fav * 2}%)`;
        })()
      : baseTheme.appBg; // Use transformed appBg from baseTheme (already transformed by applyColorTransformations)

    // Calculate navbarBg with opacity (color already transformed in baseTheme)
    const navbarBg = (() => {
      const opacity = (customization.navbarOpacity ?? 92) / 100;
      // Use transformed navbarBg from baseTheme if set, otherwise use transformed primaryColor
      const baseColor = baseTheme.navbarBg || baseTheme.primaryColor || PRESET_THEMES['ocean-turtle'].primaryColor;
      const r = parseInt(baseColor.slice(1, 3), 16);
      const g = parseInt(baseColor.slice(3, 5), 16);
      const b = parseInt(baseColor.slice(5, 7), 16);
      return `rgba(${r}, ${g}, ${b}, ${opacity})`;
    })();

    // Calculate tabBarBg with opacity (color already transformed in baseTheme)
    const tabBarBg = (() => {
      const opacity = (customization.tabBarOpacity ?? 88) / 100;
      // Use transformed tabBarBg from baseTheme if set, otherwise use transformed primaryColor
      const baseColor = baseTheme.tabBarBg || baseTheme.primaryColor || PRESET_THEMES['ocean-turtle'].primaryColor;
      const r = parseInt(baseColor.slice(1, 3), 16);
      const g = parseInt(baseColor.slice(3, 5), 16);
      const b = parseInt(baseColor.slice(5, 7), 16);
      return `rgba(${r}, ${g}, ${b}, ${opacity})`;
    })();

    // Card background with gradient support, favorability, opacity, and gradient strength
    const cardBg = (() => {
      const opacity = (customization.cardOpacity ?? 100) / 100;
      const gradientEnabled = customization.cardGradientEnabled ?? baseTheme.cardGradientEnabled ?? true;

      if (gradientEnabled) {
        const fav = customization.cardGradientFavorability ?? 50;
        const strength = (customization.cardGradientStrength ?? 100) / 100;

        const color1 = baseTheme.cardGradientColor1 || baseTheme.accentColor;
        const color2 = baseTheme.cardGradientColor2 || baseTheme.accentColor;

        // Gradient strength: blend color2 towards color1
        // 0 = color2 becomes color1 (no gradient), 1 = full gradient
        const blendedColor2 = strength === 1 ? color2 : blendColors(color1, color2, strength);

        // Create gradient with rgba for opacity
        const rgba1 = hexToRgba(color1, opacity);
        const rgba2 = hexToRgba(blendedColor2, opacity);
        const angle = customization.cardGradientAngle ?? (baseTheme as any).cardGradientAngle ?? 135;

        return `linear-gradient(${angle}deg, ${rgba1} 0%, ${rgba2} ${fav * 2}%)`;
      } else {
        // Single color with opacity - use cardGradientColor1 as the primary card color
        const baseColor = customization.cardGradientColor1 || customization.cardBg || baseTheme.cardBg;
        if (baseColor.startsWith('#')) {
          return hexToRgba(baseColor, opacity);
        }
        // If it's already rgba, multiply opacity with original alpha
        if (baseColor.startsWith('rgba')) {
          // Extract RGB and alpha values
          const rgbaMatch = baseColor.match(/rgba?\((\d+),\s*(\d+),\s*(\d+),?\s*([\d.]+)?\)/);
          if (rgbaMatch) {
            const [, r, g, b, a] = rgbaMatch;
            const originalAlpha = a ? parseFloat(a) : 1.0;
            const finalAlpha = originalAlpha * opacity;
            return `rgba(${r}, ${g}, ${b}, ${finalAlpha})`;
          }
        }
        return baseColor;
      }
    })();

    // Use dropdownColor from baseTheme (already transformed)
    const dropdownColor = baseTheme.dropdownColor || baseTheme.primaryColor || baseTheme.accentColor;

    // CRITICAL: Use transformed baseTheme colors, NOT original customization colors
    // The baseTheme was already transformed by applyColorTransformations() above
    const result = {
      ...baseTheme,
      appBg,
      navbarBg,
      tabBarBg,
      cardBg,
      dropdownColor,
      // Use baseTheme colors (already transformed) instead of customization (untransformed)
      textColor: baseTheme.textColor,
      accentColor: baseTheme.accentColor,
      primaryColor: baseTheme.primaryColor || baseTheme.accentColor,
      secondaryColor: baseTheme.secondaryColor || baseTheme.accentColor,
      // CRITICAL FIX: Use baseTheme.cardGradientColor1/2 (transformed) not customization (untransformed)
      cardGradientColor1: baseTheme.cardGradientColor1 || customization.cardGradientColor1,
      cardGradientColor2: baseTheme.cardGradientColor2 || customization.cardGradientColor2,
      cardGradientEnabled: customization.cardGradientEnabled,
      cardGradientAngle: customization.cardGradientAngle,
    };

    // Debug: Verify transformation worked
    if (colorSpace !== 'Off') {
      
      if (customization.primaryColor && customization.primaryColor === result.primaryColor) {
        console.error('❌ [buildThemeFromCustomization] TRANSFORMATION FAILED - colors are identical!');
      }
    }

    return result;
  };

  // Apply theme to DOM and state
  const applyTheme = (newTheme: ThemeTokens) => {
    // DEBUG: Log every theme update with caller info
    
    // Update state normally (removed flushSync to fix React rendering warning)
    setTheme(newTheme);

    // Apply theme to CSS variables
    // IMPORTANT: Variable names must match what's used in CSS!
    // CSS uses: var(--text-color) and var(--color-accent)
    const root = document.documentElement;
    root.style.setProperty('--text-color', newTheme.textColor);
    root.style.setProperty('--color-accent', newTheme.accentColor);
    root.style.setProperty('--card-bg', newTheme.cardBg);

    if (newTheme.appBg) {
      root.style.setProperty('--app-bg', newTheme.appBg);
    }
    if (newTheme.navbarBg) {
      root.style.setProperty('--navbar-bg', newTheme.navbarBg);
    }
    if (newTheme.tabBarBg) {
      root.style.setProperty('--tab-bar-bg', newTheme.tabBarBg);
    }
    if (newTheme.radius) {
      root.style.setProperty('--radius', newTheme.radius);
    }
  };

  // Update theme when preset changes (skip first run since already initialized)
  useEffect(() => {
    // Skip first run - theme was already applied in useState initializer
    if (!hasInitialized.current) {
      hasInitialized.current = true;
      return;
    }

    // CRITICAL: Don't apply preset theme if we have customization loaded!
    // Customization takes priority over preset theme
    if (siteCustomization && Object.keys(siteCustomization).length > 0) {
            return;
    }

        const newTheme = getThemeByKey(currentPreset);
    applyTheme(newTheme);

    // Save to localStorage
    localStorage.setItem('earth-social-theme', currentPreset);
  }, [currentPreset, siteCustomization]);

  // Listen for site theme updates from customizer
  useEffect(() => {
    const handleSiteThemeUpdate = (event: any) => {
      const { customization, animations, baseTheme } = event.detail;
      if (customization) {
        const newTheme = buildThemeFromCustomization(customization, baseTheme as PresetThemeKey);
        applyTheme(newTheme);
        setSiteCustomization(customization);

        // UNIFIED CACHE: Update animations when theme customizer saves
        if (animations) {
          setSiteAnimations(animations);
        }
      }
    };

    window.addEventListener('siteThemeUpdated', handleSiteThemeUpdate);
    return () => window.removeEventListener('siteThemeUpdated', handleSiteThemeUpdate);
  }, [currentPreset]);

  // Track the last user ID to prevent reload on token refresh
  const lastUserIdRef = useRef<number | null>(null);

  // Load saved theme when user is available
  useEffect(() => {
    // Only try to load theme if user is logged in
    if (!user) {
      // User is not logged in
      // CRITICAL: Don't reset theme if we already loaded it from cache!
      if (hasInitialized.current) {
                return;
      }

      // Load theme from localStorage or use default (only if not already initialized)
      const savedPreset = localStorage.getItem('themePreset');
      if (savedPreset && savedPreset in PRESET_THEMES) {
        setCurrentPreset(savedPreset as PresetThemeKey);
        applyTheme(getThemeByKey(savedPreset as PresetThemeKey));
      }
      // Reset lastUserIdRef when user logs out
      lastUserIdRef.current = null;
      return;
    }

    // CRITICAL: Prevent theme reload on token refresh
    // Only reload theme if the user ID actually changed (login/logout/switch user)
    if (lastUserIdRef.current === user.id) {
      // Same user, just a token refresh - don't reload theme
      return;
    }

    // Update the last user ID
    lastUserIdRef.current = user.id;

    const loadSavedTheme = async () => {
            try {
        const userId = user.id;

        if (userId && token) {
          // Load site theme customization
                    const themeResponse = await fetch(`/api/users/${userId}/site-theme`, {
            credentials: 'include',
            headers: {
              'Authorization': `Bearer ${token}`
            }
          });

          if (themeResponse.ok) {
            const themeData = await themeResponse.json();
            console.log('[ThemeContext] 🎨 Backend theme data:', themeData);
            console.log('[ThemeContext] 🎬 Backend animations:', themeData.animations);
                        // themeData IS the customization object (backend returns tokens directly)
            const customization = themeData.customization || themeData;

            // If we have a saved theme (even if it's the default), use it
            if (customization && typeof customization === 'object' && Object.keys(customization).length > 0) {

              // Determine which base theme to use for animations (needed for both cached and non-cached paths)
              const base = (themeData.baseTheme && themeData.baseTheme in PRESET_THEMES)
                ? themeData.baseTheme
                : 'ocean-turtle';

              // Check if we already have cached computedColors for this exact customization
              const cached = getCachedSiteCustomization();
              const cachedIsIdentical = cached?.baseColors &&
                cached.baseColors.primaryColor === customization.primaryColor &&
                cached.baseColors.colorSpace === customization.colorSpace;

              if (cachedIsIdentical && cached.computedColors) {

                // Just update siteCustomization state to include computedColors
                // Theme is already correct from initial load
                setSiteCustomization({
                  ...customization,
                  computedColors: cached.computedColors
                });

                // CRITICAL: Also update animations from backend, even if theme is cached
                // Merge backend animations with preset defaults (backend may have partial data)
                const backendAnimations = themeData.animations || customization.animations;
                if (backendAnimations) {
                  const fullAnimations = getThemeAnimations(base as PresetThemeKey);
                  const mergedAnimations = {
                    characters: fullAnimations.characters.map((defaultChar) => {
                      const customChar = backendAnimations.characters?.find((c: any) => c.id === defaultChar.id);
                      return customChar ? { ...defaultChar, ...customChar } : defaultChar;
                    }),
                    bubbles: backendAnimations.bubbles && fullAnimations.bubbles
                      ? { ...fullAnimations.bubbles, ...backendAnimations.bubbles }
                      : fullAnimations.bubbles,
                    decorations: fullAnimations.decorations.map((defaultDeco) => {
                      const customDeco = backendAnimations.decorations?.find((d: any) => d.id === defaultDeco.id);
                      return customDeco ? { ...defaultDeco, ...customDeco } : defaultDeco;
                    }),
                  };
                  console.log('[ThemeContext] 🎬 Setting merged animations (cached path):', mergedAnimations);
                  setSiteAnimations(mergedAnimations);
                } else {
                  console.log('[ThemeContext] 🎬 Setting preset animations (cached path):', getThemeAnimations(base as PresetThemeKey));
                  setSiteAnimations(getThemeAnimations(base as PresetThemeKey));
                }

                // No need to call applyTheme - we're already using the right colors from cache
                return;
              }


              // CRITICAL: Check if we're on a profile/community page before applying site theme
              // This prevents the site theme from overriding profile/community theme mid-render
              const shouldApplySiteTheme = (() => {
                try {
                  const currentPageTheme = sessionStorage.getItem('currentPageTheme');
                  if (currentPageTheme) {
                    const parsed = JSON.parse(currentPageTheme);
                    const isOnProfileOrCommunityPage =
                      parsed.type === 'profile' || parsed.type === 'community';

                    if (isOnProfileOrCommunityPage) {
                                            return false; // Don't apply
                    }
                  }
                } catch (error) {
                  console.error('[ThemeContext] ❌ Failed to check page type:', error);
                }
                return true; // Apply site theme (default)
              })();

              // Base theme already determined above at line 548-551

              // Build the theme (needed for caching even if not applying)
              const newTheme = buildThemeFromCustomization(customization, base as PresetThemeKey);

              // STEP 3: Always cache site theme (even if not applying) for when user navigates to site pages
              cacheSiteCustomization({
                baseColors: customization,
                computedColors: newTheme, // The transformed theme with color spacing applied
                settings: customization,
                animations: themeData.animations || customization.animations || null // CRITICAL: Include animations from backend
              });

              // Only apply theme and update state if we're on a site page
              if (shouldApplySiteTheme) {
                                setSiteCustomization(customization);
                // CRITICAL: Merge backend animations with preset defaults (backend may have partial data)
                const backendAnimations = themeData.animations || customization.animations;
                if (backendAnimations) {
                  const fullAnimations = getThemeAnimations(base as PresetThemeKey);
                  const mergedAnimations = {
                    characters: fullAnimations.characters.map((defaultChar) => {
                      const customChar = backendAnimations.characters?.find((c: any) => c.id === defaultChar.id);
                      return customChar ? { ...defaultChar, ...customChar } : defaultChar;
                    }),
                    bubbles: backendAnimations.bubbles && fullAnimations.bubbles
                      ? { ...fullAnimations.bubbles, ...backendAnimations.bubbles }
                      : fullAnimations.bubbles,
                    decorations: fullAnimations.decorations.map((defaultDeco) => {
                      const customDeco = backendAnimations.decorations?.find((d: any) => d.id === defaultDeco.id);
                      return customDeco ? { ...defaultDeco, ...customDeco } : defaultDeco;
                    }),
                  };
                  console.log('[ThemeContext] 🎬 Setting merged animations (non-cached path):', mergedAnimations);
                  setSiteAnimations(mergedAnimations);
                } else {
                  console.log('[ThemeContext] 🎬 Setting preset animations (non-cached path):', getThemeAnimations(base as PresetThemeKey));
                  setSiteAnimations(getThemeAnimations(base as PresetThemeKey));
                }
                applyTheme(newTheme);

                // Set the currentPreset to match the baseTheme so animations work correctly
                if (base in PRESET_THEMES) {
                  setCurrentPreset(base as PresetThemeKey);
                }
              } else {
                              }

              return; // Skip the localStorage fallback
            } else {
                          }
          }
        }
      } catch (error) {
        // console.error('Failed to load saved site theme:', error);
      } finally {
        // STEP 4: Mark loading as complete
        setIsLoading(false);
      }

      // Fallback to localStorage preset
      const savedTheme = localStorage.getItem('earth-social-theme');
      if (savedTheme && savedTheme in PRESET_THEMES) {
        setCurrentPreset(savedTheme as PresetThemeKey);
      }
    };

    loadSavedTheme();
  }, [user]); // Re-run when user changes (login/logout)

  // DEBUG: Log what we're actually providing to children
  
  return (
    <ThemeContext.Provider
      value={{
        currentPreset,
        setCurrentPreset,
        theme,
        siteCustomization,
        siteAnimations,
        isLoading,
        availablePresets: PRESET_THEMES,
        presetDisplayNames: THEME_DISPLAY_NAMES,
      }}
    >
      {children}
    </ThemeContext.Provider>
  );
}

export function useTheme() {
  const context = useContext(ThemeContext);
  if (!context) {
    throw new Error('useTheme must be used within ThemeProvider');
  }
  return context;
}

// Helper hook to get current theme colors
export function useThemeColors() {
  const { theme } = useTheme();
  return theme;
}
