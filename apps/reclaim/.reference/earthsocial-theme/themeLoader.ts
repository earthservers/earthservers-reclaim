import { PRESET_THEMES } from '@theme/tokens';
import { getThemeAnimations } from '@theme/animation-config';

/**
 * Load and apply the user's site theme from the API
 *
 * Note: This loads the RAW customization data (before color transformation).
 * The transformed theme (after color spacing) is cached separately by each page
 * component using themeCache.ts for instant page loads.
 */
export async function loadAndApplySiteTheme(userId: string, token?: string) {
  try {
    const headers: HeadersInit = {
      ...(token ? { 'Authorization': `Bearer ${token}` } : {})
    };

    const response = await fetch(`/api/users/${userId}/site-theme`, {
      credentials: 'include',
      headers
    });


    if (response.ok) {
      const data = await response.json();

      if (data.customization) {
        // Dispatch event to apply the saved theme
        // Ensure animationsEnabled defaults to true if not specified
        const animationsEnabled = typeof data.animationsEnabled === 'boolean'
          ? data.animationsEnabled
          : true;

        // Ensure animations are provided, fallback to generating from baseTheme or preset
        const animations = data.animations ||
          getThemeAnimations(data.baseTheme || data.customization.presetTheme || 'ocean-turtle');

        window.dispatchEvent(new CustomEvent('siteThemeUpdated', {
          detail: {
            customization: data.customization,
            animations,
            animationsEnabled,
            baseTheme: data.baseTheme
          }
        }));
        return;
      }
    }

    // No saved theme or error - dispatch default Ocean Turtle theme
    const defaultCustomization = {
      ...PRESET_THEMES['ocean-turtle'],
      navbarOpacity: 92,
      cardGradientFavorability: 50,
      cardOpacity: 100,
      cardGradientStrength: 100,
      gradientEnabled: true,
      gradientAngle: 135,
      gradientFavorability: 50,
      gradientStrength: 100,
    };

    window.dispatchEvent(new CustomEvent('siteThemeUpdated', {
      detail: {
        customization: defaultCustomization,
        animations: getThemeAnimations('ocean-turtle'),
        animationsEnabled: true
      }
    }));
  } catch (error) {

    // On error, dispatch default Ocean Turtle theme
    const defaultCustomization = {
      ...PRESET_THEMES['ocean-turtle'],
      navbarOpacity: 92,
      cardGradientFavorability: 50,
      cardOpacity: 100,
      cardGradientStrength: 100,
      gradientEnabled: true,
      gradientAngle: 135,
      gradientFavorability: 50,
      gradientStrength: 100,
    };

    window.dispatchEvent(new CustomEvent('siteThemeUpdated', {
      detail: {
        customization: defaultCustomization,
        animations: getThemeAnimations('ocean-turtle'),
        animationsEnabled: true
      }
    }));
  }
}
