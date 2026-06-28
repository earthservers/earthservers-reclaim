/**
 * Theme Cache Utilities
 *
 * Manages caching of transformed theme data in sessionStorage to prevent
 * flash of default theme on page load and navigation.
 *
 * Three types of themes:
 * 1. Site theme - User's global preference (key: "siteTheme")
 * 2. Profile themes - Per-user (key: "profileTheme_{userId}")
 * 3. Community themes - Per-community (key: "communityTheme_{communityId}")
 */

import type { ThemeAnimations } from '@theme/animation-config';

export type ThemeType = 'site' | 'profile' | 'community';

export type CachedThemeData = {
  // Transformed colors (after color spacing is applied)
  textColor: string;
  accentColor: string;
  cardBg: string;
  appBg: string;
  navbarBg: string;
  tabBarBg: string;
  primaryColor: string;
  secondaryColor: string;

  // Optional fields
  dropdownColor?: string;
  cardGradientColor1?: string;
  cardGradientColor2?: string;
  navbarTextColor?: string;

  // Opacity settings
  navbarOpacity?: number;
  tabBarOpacity?: number;
  cardOpacity?: number;

  // Background gradient settings (for LoadingScreen continuity)
  gradientEnabled?: boolean;
  gradientAngle?: number;
  gradientFavorability?: number;
  gradientStrength?: number;

  // Card gradient settings
  cardGradientEnabled?: boolean;
  cardGradientAngle?: number;
  cardGradientFavorability?: number;
  cardGradientStrength?: number;

  // Color space setting and transformation limits
  colorSpace?: 'Off' | 'RGB' | 'HSV' | 'TMI';
  temperatureLimit?: number;
  magentaLimit?: number;
  intensityLimit?: number;
  backgroundSaturationLimit?: number;
  backgroundBrightnessLimit?: number;
  redLimit?: number;
  greenLimit?: number;
  blueLimit?: number;
  backgroundContrast?: number;
  backgroundHueGravity?: number;
  backgroundGraySaturation?: number;
  backgroundDefaultHue?: number;

  // Animations (new format with characters, bubbles, decorations)
  animations?: ThemeAnimations;
  animationsEnabled?: boolean;

  // Metadata
  timestamp: number;
  themeType: ThemeType;
  userId?: string;
  communityId?: string;
  pageType?: 'feed' | 'profile' | 'community' | 'friends' | 'ads' | 'marketplace';
};

/**
 * Get the cache key for a specific theme type
 */
function getThemeCacheKey(type: ThemeType, id?: string): string {
  switch (type) {
    case 'site':
      return 'siteTheme';
    case 'profile':
      return id ? `profileTheme_${id}` : 'profileTheme';
    case 'community':
      return id ? `communityTheme_${id}` : 'communityTheme';
    default:
      return 'siteTheme';
  }
}

/**
 * Cache a theme based on its type
 * @param theme - The theme data to cache
 * @param type - Type of theme (site, profile, or community)
 * @param id - userId for profile themes, communityId for community themes
 */
export function cacheTheme(
  theme: Omit<CachedThemeData, 'timestamp' | 'themeType'>,
  type: ThemeType,
  id?: string
): void {
  try {
    const cacheData: CachedThemeData = {
      ...theme,
      themeType: type,
      timestamp: Date.now()
    };

    const key = getThemeCacheKey(type, id);
    sessionStorage.setItem(key, JSON.stringify(cacheData));

    // NOTE: DO NOT write to 'currentPageTheme' here - that's managed by NavigationContext
    // NavigationContext is the single source of truth for LoadingScreen theme

      } catch (e) {
    console.warn('⚠️ [ThemeCache] Failed to cache theme:', e);
  }
}

/**
 * Get a cached theme by type and id
 * @param type - Type of theme to retrieve
 * @param id - userId for profile themes, communityId for community themes
 * @returns Cached theme data or null if not found/expired
 */
export function getCachedTheme(type: ThemeType, id?: string): CachedThemeData | null {
  try {
    const key = getThemeCacheKey(type, id);
    const cached = sessionStorage.getItem(key);
    if (!cached) return null;

    const data = JSON.parse(cached) as any;

    // Check cache version - invalidate if stale (missing animations field)
    if (!data.animations && CACHE_VERSION >= 3) {
      sessionStorage.removeItem(key);
      return null;
    }

    // Check if cache is less than 1 hour old
    const age = Date.now() - data.timestamp;
    const maxAge = 60 * 60 * 1000; // 1 hour

    if (age > maxAge) {
      sessionStorage.removeItem(key);
      return null;
    }

    return data as CachedThemeData;
  } catch (e) {
    console.warn('⚠️ [ThemeCache] Failed to read cached theme:', e);
    return null;
  }
}

/**
 * Get the cached theme for the current page (from currentPageTheme key)
 * Used by loading screens to match the theme of the page being navigated from
 */
export function getCachedPageTheme(): CachedThemeData | null {
  try {
    const cached = sessionStorage.getItem('currentPageTheme');
    if (!cached) return null;

    const data = JSON.parse(cached) as CachedThemeData;

    // Check if cache is less than 1 hour old
    const age = Date.now() - data.timestamp;
    const maxAge = 60 * 60 * 1000; // 1 hour

    if (age > maxAge) {
      sessionStorage.removeItem('currentPageTheme');
      return null;
    }

    return data;
  } catch (e) {
    console.warn('⚠️ [ThemeCache] Failed to read cached theme:', e);
    return null;
  }
}

/**
 * Legacy function for backward compatibility
 * @deprecated Use cacheTheme() instead
 */
export function cacheCurrentPageTheme(theme: Omit<CachedThemeData, 'timestamp'>): void {
  // Determine type from the theme data
  const type: ThemeType = theme.communityId ? 'community' : theme.userId ? 'profile' : 'site';
  const id = theme.userId || theme.communityId;
  cacheTheme(theme, type, id);
}

/**
 * Clear a specific cached theme
 * @param type - Type of theme to clear
 * @param id - userId for profile themes, communityId for community themes
 */
export function clearCachedTheme(type: ThemeType, id?: string): void {
  try {
    const key = getThemeCacheKey(type, id);
    sessionStorage.removeItem(key);
      } catch (e) {
    console.warn('⚠️ [ThemeCache] Failed to clear cached theme:', e);
  }
}

/**
 * Clear all cached themes (e.g., on logout)
 */
export function clearAllCachedThemes(): void {
  try {
    // Clear all theme-related keys
    const keys = Object.keys(sessionStorage);
    keys.forEach(key => {
      if (key.startsWith('siteTheme') ||
          key.startsWith('profileTheme') ||
          key.startsWith('communityTheme') ||
          key === 'currentPageTheme' ||
          key === 'siteCustomization') {
        sessionStorage.removeItem(key);
      }
    });
      } catch (e) {
    console.warn('⚠️ [ThemeCache] Failed to clear cached themes:', e);
  }
}

/**
 * Migrate old caches to new version - called on app startup
 * Clears all theme caches that don't match current CACHE_VERSION
 */
export function migrateThemeCaches(): void {
  try {
    const migrationKey = 'themeCacheVersion';
    const storedVersion = sessionStorage.getItem(migrationKey);

    // If stored version doesn't match current version, clear all caches
    if (storedVersion !== CACHE_VERSION.toString()) {
      console.log('🔄 [ThemeCache] Migrating theme caches from version', storedVersion, 'to', CACHE_VERSION);
      clearAllCachedThemes();
      sessionStorage.setItem(migrationKey, CACHE_VERSION.toString());
    }
  } catch (e) {
    console.warn('⚠️ [ThemeCache] Failed to migrate theme caches:', e);
  }
}

/**
 * Cache version - increment when cache structure or transformation logic changes
 * This allows automatic invalidation of stale cached data
 */
const CACHE_VERSION = 3; // Bumped to 3 - added animations field to CachedThemeData

/**
 * Cache site customization with BOTH base and computed colors
 * This allows instant rendering without recalculation
 */
export function cacheSiteCustomization(data: {
  baseColors: any;
  computedColors?: any;
  settings: any;
  animations?: any;
}): void {
  try {
    const cacheData = {
      version: CACHE_VERSION, // Add version for cache invalidation

      // Base colors (untransformed, for editing)
      baseColors: {
        primaryColor: data.baseColors.primaryColor,
        secondaryColor: data.baseColors.secondaryColor,
        textColor: data.baseColors.textColor,
        accentColor: data.baseColors.accentColor,
        navbarBg: data.baseColors.navbarBg,
        tabBarBg: data.baseColors.tabBarBg,
        cardBg: data.baseColors.cardBg,
        appBg: data.baseColors.appBg,
        navbarTextColor: data.baseColors.navbarTextColor,
      },

      // Computed colors (after color space transformation, for rendering)
      computedColors: data.computedColors ? {
        primaryColor: data.computedColors.primaryColor,
        secondaryColor: data.computedColors.secondaryColor,
        textColor: data.computedColors.textColor,
        accentColor: data.computedColors.accentColor,
        navbarBg: data.computedColors.navbarBg,
        tabBarBg: data.computedColors.tabBarBg,
        cardBg: data.computedColors.cardBg,
        appBg: data.computedColors.appBg,
        navbarTextColor: data.computedColors.navbarTextColor,
        cardGradientColor1: data.computedColors.cardGradientColor1,
        cardGradientColor2: data.computedColors.cardGradientColor2,
      } : null,

      // Settings and transformation parameters
      settings: data.settings,

      // Animations data (CRITICAL: Include animations so they persist across page loads)
      animations: data.animations || null
    };

    sessionStorage.setItem('siteCustomization', JSON.stringify(cacheData));
      } catch (e) {
    console.warn('⚠️ [ThemeCache] Failed to cache site customization:', e);
  }
}

/**
 * Backward compatible version - accepts old format
 */
export function cacheSiteCustomizationLegacy(customization: any): void {
  try {
    cacheSiteCustomization({
      baseColors: customization,
      settings: customization
    });
  } catch (e) {
    console.warn('⚠️ [ThemeCache] Failed to cache site customization (legacy):', e);
  }
}

/**
 * Get cached site customization for ThemeContext to load synchronously
 */
export function getCachedSiteCustomization(): any | null {
  try {
    const cached = sessionStorage.getItem('siteCustomization');
    if (!cached) return null;

    const parsed = JSON.parse(cached);

    // Check cache version - invalidate if stale
    if (parsed.version !== CACHE_VERSION) {
            sessionStorage.removeItem('siteCustomization');
      sessionStorage.removeItem('currentPageTheme'); // Also clear navigation cache
      return null;
    }

    // Check if it's the new format with baseColors/computedColors
    if (parsed.baseColors && parsed.settings) {
            return parsed;
    }

    // Backward compatibility - old format (shouldn't happen with version check)
    return {
      baseColors: parsed,
      settings: parsed,
      computedColors: null
    };
  } catch (e) {
    console.warn('⚠️ [ThemeCache] Failed to read cached site customization:', e);
    return null;
  }
}

/**
 * Create optimized cache object for profiles/communities (view-only)
 * Only stores final display colors, not base colors (smaller footprint)
 */
export function createViewOnlyCache(data: {
  baseColors: any;
  computedColors?: any;
  colorSpace: string;
  userId?: string;
  communityId?: string;
}) {
  // If colorSpace is "Off", use base colors directly (no transformation needed)
  // Otherwise use computed colors (already transformed)
  const colors = data.colorSpace !== 'Off' && data.computedColors
    ? data.computedColors
    : data.baseColors;

  return {
    colors: {
      primaryColor: colors.primaryColor,
      secondaryColor: colors.secondaryColor,
      textColor: colors.textColor,
      accentColor: colors.accentColor,
      navbarBg: colors.navbarBg,
      tabBarBg: colors.tabBarBg,
      cardBg: colors.cardBg,
      appBg: colors.appBg,
      navbarTextColor: colors.navbarTextColor
    },
    colorSpace: data.colorSpace,
    userId: data.userId,
    communityId: data.communityId
  };
}

/**
 * Create full cache object for site theme and own profile (editable)
 * Stores both base and computed colors for theme editor
 */
export function createEditableCache(data: {
  baseColors: any;
  computedColors?: any;
  settings: any;
}) {
  return {
    baseColors: {
      primaryColor: data.baseColors.primaryColor,
      secondaryColor: data.baseColors.secondaryColor,
      textColor: data.baseColors.textColor,
      accentColor: data.baseColors.accentColor,
      navbarBg: data.baseColors.navbarBg,
      tabBarBg: data.baseColors.tabBarBg,
      cardBg: data.baseColors.cardBg,
      appBg: data.baseColors.appBg,
      navbarTextColor: data.baseColors.navbarTextColor
    },
    // Only include computed colors if colorSpace is not "Off"
    computedColors: data.settings.colorSpace !== 'Off' && data.computedColors ? {
      primaryColor: data.computedColors.primaryColor,
      secondaryColor: data.computedColors.secondaryColor,
      textColor: data.computedColors.textColor,
      accentColor: data.computedColors.accentColor,
      navbarBg: data.computedColors.navbarBg,
      tabBarBg: data.computedColors.tabBarBg,
      cardBg: data.computedColors.cardBg,
      appBg: data.computedColors.appBg,
      navbarTextColor: data.computedColors.navbarTextColor
    } : undefined,
    settings: data.settings
  };
}
