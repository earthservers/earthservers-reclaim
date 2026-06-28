// Site-wide theme customizer
// Controls the global app theme (navigation, feed, community list, etc.)

import React, { useState, useEffect } from 'react';
import ThemeCustomizerModal, { ThemeCustomization } from './ThemeCustomizerModal';
import { getThemeAnimations } from '@theme/animation-config';
import type { ThemeAnimations } from '@theme/animation-config';
import { PRESET_THEMES } from '@theme/tokens';
import { useTheme } from '../../../context/ThemeContext';
import { normalizeCustomization } from '../../../utils/colorNormalization';
import { cacheSiteCustomization } from '../../../utils/themeCache';
import { transformColor } from '../../../utils/colorTransform';
import { calculateNavbarBg } from '../../../utils/backgroundHelpers';

type Props = {
  isOpen: boolean;
  onClose: () => void;
  userId: string | number | undefined;
  initialCustomization?: ThemeCustomization;
  initialAnimations?: ThemeAnimations;
  initialAnimationsEnabled?: boolean;
};

// Use ocean-turtle preset as the default
const DEFAULT_CUSTOMIZATION: ThemeCustomization = {
  ...PRESET_THEMES['ocean-turtle'],
  navbarOpacity: 92,
  tabBarOpacity: 88,
  cardGradientFavorability: 50,
  cardOpacity: 100,
  cardGradientStrength: 100,
  gradientEnabled: true,
  gradientAngle: 135,
  gradientFavorability: 50,
  gradientStrength: 100,
  bubbleColor: PRESET_THEMES['ocean-turtle'].accentColor,
  turtleColor: PRESET_THEMES['ocean-turtle'].cardGradientColor2 || '#00897b',
  coralColors: [
    PRESET_THEMES['ocean-turtle'].accentColor,
    PRESET_THEMES['ocean-turtle'].secondaryColor || '#00acc1',
    PRESET_THEMES['ocean-turtle'].cardGradientColor2 || '#00897b',
    PRESET_THEMES['ocean-turtle'].cardGradientColor1 || '#00695c',
    PRESET_THEMES['ocean-turtle'].primaryColor || '#006064'
  ],
  profileFrameColor: PRESET_THEMES['ocean-turtle'].accentColor,
  profileFrameShape: 'square',
  profileNameAlign: 'left',
  profileFrameDesign: 'coral',
  animationsEnabled: true,
  selectedCharacter: 'turtle',
  selectedDecoration: 'coral',
  focusAnimationEnabled: false,
  colorSpace: 'Off',
};

export default function SiteThemeCustomizer({ isOpen, onClose, userId, initialCustomization, initialAnimations, initialAnimationsEnabled }: Props) {
  const { theme, currentPreset } = useTheme();

  // Initialize with initial customization if provided, otherwise use current theme from context
  const [customization, setCustomizationRaw] = useState<ThemeCustomization>(() =>
    initialCustomization || {
      ...DEFAULT_CUSTOMIZATION,
      primaryColor: theme.appBg?.includes('gradient') ? '#0891b2' : theme.appBg || '#0891b2',
      secondaryColor: '#0e7490',
      accentColor: theme.accentColor || '#14B8A6',
      textColor: theme.textColor || '#ffffff',
      navbarBg: theme.navbarBg || 'rgba(6, 182, 212, 0.95)',
      tabBarBg: theme.tabBarBg || 'rgba(30, 58, 95, 1)',
      cardBg: theme.cardBg || 'rgba(255, 255, 255, 0.1)',
    }
  );

  // Wrapped setter with logging
  const setCustomization = (newValue: ThemeCustomization | ((prev: ThemeCustomization) => ThemeCustomization)) => {
    setCustomizationRaw(newValue);
  };

  const [animations, setAnimations] = useState<ThemeAnimations>(initialAnimations || getThemeAnimations(currentPreset || 'ocean-turtle'));
  const [animationsEnabled, setAnimationsEnabled] = useState(initialAnimationsEnabled ?? true);
  const [baseTheme, setBaseTheme] = useState<string>(currentPreset || 'ocean-turtle');
  const [isReady, setIsReady] = useState(false); // Track if theme data is loaded and ready - start as false

  // Use refs to track the latest values for debounced dispatch
  const customizationRef = React.useRef(customization);
  const animationsRef = React.useRef(animations);
  const animationsEnabledRef = React.useRef(animationsEnabled);
  const baseThemeRef = React.useRef(baseTheme);

  // Update refs whenever state changes
  React.useEffect(() => {
    customizationRef.current = customization;
  }, [customization]);

  React.useEffect(() => {
    animationsRef.current = animations;
  }, [animations]);

  React.useEffect(() => {
    animationsEnabledRef.current = animationsEnabled;
  }, [animationsEnabled]);

  React.useEffect(() => {
    baseThemeRef.current = baseTheme;
  }, [baseTheme]);

  // Pre-calculate and load theme data BEFORE showing the modal
  useEffect(() => {
    const loadSiteTheme = async () => {
      // Reset ready state when modal closes
      if (!isOpen) {
        setIsReady(false);
        return;
      }

      // Don't load if userId is undefined
      if (!userId) {
        // Use defaults and mark as ready
        setCustomization(DEFAULT_CUSTOMIZATION);
        setAnimations(getThemeAnimations('ocean-turtle'));
        setAnimationsEnabled(true);
        setBaseTheme('ocean-turtle');
        setIsReady(true);
        return;
      }

      try {
        const response = await fetch(`/api/users/${userId}/site-theme`, {
          credentials: 'include'
        });

        if (response.ok) {
          const data = await response.json();

          if (data.customization) {
            // Normalize customization to ensure colors are hex (not rgba)
            const normalized = normalizeCustomization(data.customization);
            setCustomization(normalized);
          } else {
            setCustomization(DEFAULT_CUSTOMIZATION);
          }

          // Get full default animations for the theme
          const themeToUse = data.baseTheme || 'ocean-turtle';
          const fullAnimations = getThemeAnimations(themeToUse);

          // If backend sent partial animation customizations, merge them with defaults
          if (data.animations) {

            const mergedAnimations = {
              // Merge characters - preserve full structure, apply customizations
              characters: fullAnimations.characters.map((defaultChar) => {
                const customChar = data.animations.characters?.find((c: any) => c.id === defaultChar.id);
                return customChar ? { ...defaultChar, ...customChar } : defaultChar;
              }),

              // Merge bubbles - preserve full structure, apply customizations
              bubbles: data.animations.bubbles && fullAnimations.bubbles
                ? { ...fullAnimations.bubbles, ...data.animations.bubbles }
                : fullAnimations.bubbles,

              // Merge decorations - preserve full structure, apply customizations
              decorations: fullAnimations.decorations.map((defaultDeco) => {
                const customDeco = data.animations.decorations?.find((d: any) => d.id === defaultDeco.id);
                return customDeco ? { ...defaultDeco, ...customDeco } : defaultDeco;
              }),
            };

            setAnimations(mergedAnimations);
          } else {
            // No customizations, use full defaults
            setAnimations(fullAnimations);
          }

          if (typeof data.animationsEnabled === 'boolean') setAnimationsEnabled(data.animationsEnabled);
          if (data.baseTheme) {
            setBaseTheme(data.baseTheme);
          } else {
            setBaseTheme('ocean-turtle');
          }
        } else {
          // No saved theme found (404) - use defaults
          setCustomization(DEFAULT_CUSTOMIZATION);
          setAnimations(getThemeAnimations('ocean-turtle'));
          setAnimationsEnabled(true);
          setBaseTheme('ocean-turtle');
        }
      } catch (error) {
        console.error('[SiteThemeCustomizer] Error loading theme:', error);
        // On error, use defaults
        setCustomization(DEFAULT_CUSTOMIZATION);
        setAnimations(getThemeAnimations('ocean-turtle'));
        setAnimationsEnabled(true);
        setBaseTheme('ocean-turtle');
      }

      // Mark as ready to show the modal
      setIsReady(true);
    };

    loadSiteTheme();
  }, [isOpen, userId]);

  // Live preview: dispatch theme update event whenever customization changes
  // Debounce to prevent excessive re-renders while dragging sliders
  useEffect(() => {
    if (isOpen && isReady) {
      const timeoutId = setTimeout(() => {
        // Use refs to get the latest values, not the stale closure values
        const latestCustomization = customizationRef.current;
        const latestAnimations = animationsRef.current;
        const latestAnimationsEnabled = animationsEnabledRef.current;
        const latestBaseTheme = baseThemeRef.current;

        window.dispatchEvent(new CustomEvent('siteThemeUpdated', {
          detail: {
            customization: latestCustomization,
            animations: latestAnimations,
            animationsEnabled: latestAnimationsEnabled,
            baseTheme: latestBaseTheme
          }
        }));
      }, 50); // 50ms debounce - fast enough to feel instant, slow enough to reduce re-renders

      return () => clearTimeout(timeoutId);
    }
  }, [customization, animations, animationsEnabled, baseTheme, isOpen, isReady]);

  const handleSave = async () => {
    if (!userId) {
      alert('User ID is not available. Please try logging in again.');
      return;
    }

    // Normalize customization before saving to ensure colors are hex (not rgba)
    const normalizedCustomization = normalizeCustomization(customization);

    try {
      const response = await fetch(`/api/users/${userId}/site-theme`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        credentials: 'include',
        body: JSON.stringify({
          baseTheme,
          customization: normalizedCustomization,
          animations,
          animationsEnabled,
        }),
      });

      if (response.ok) {
        // Update the theme cache with the new theme before closing
        // This ensures the updated theme is used immediately on page load
        const colorSpace = normalizedCustomization.colorSpace || 'Off';
        const colorTransformOptions = {
          colorSpace,
          temperatureLimit: normalizedCustomization.temperatureLimit,
          magentaLimit: normalizedCustomization.magentaLimit,
          intensityLimit: normalizedCustomization.intensityLimit,
          redLimit: normalizedCustomization.redLimit,
          greenLimit: normalizedCustomization.greenLimit,
          blueLimit: normalizedCustomization.blueLimit,
          hueLimit: normalizedCustomization.hueLimit,
          saturationLimit: normalizedCustomization.saturationLimit,
          valueLimit: normalizedCustomization.valueLimit,
          backgroundSaturationLimit: normalizedCustomization.backgroundSaturationLimit,
          backgroundBrightnessLimit: normalizedCustomization.backgroundBrightnessLimit,
        };

        const transformedTextColor = transformColor(normalizedCustomization.textColor, colorTransformOptions);
        const transformedAccentColor = transformColor(normalizedCustomization.accentColor, colorTransformOptions);
        const transformedPrimaryColor = transformColor(normalizedCustomization.primaryColor, colorTransformOptions);
        const transformedSecondaryColor = transformColor(normalizedCustomization.secondaryColor, colorTransformOptions);

        const navbarBg = calculateNavbarBg(transformedPrimaryColor, normalizedCustomization.navbarOpacity ?? 92);
        const tabBarBg = calculateNavbarBg(transformedPrimaryColor, normalizedCustomization.tabBarOpacity ?? 88);
        const cardBg = calculateNavbarBg(transformedPrimaryColor, normalizedCustomization.cardOpacity ?? 100);

        // UNIFIED CACHE: Cache to siteCustomization (used by ThemeContext)
        const computedColors = {
          textColor: transformedTextColor,
          accentColor: transformedAccentColor,
          cardBg: cardBg,
          appBg: normalizedCustomization.appBg || 'rgba(0, 0, 0, 0)',
          navbarBg: navbarBg,
          tabBarBg: tabBarBg,
          primaryColor: transformedPrimaryColor,
          secondaryColor: transformedSecondaryColor,
          cardGradientColor1: normalizedCustomization.cardGradientColor1,
          cardGradientColor2: normalizedCustomization.cardGradientColor2,
        };

        cacheSiteCustomization({
          baseColors: normalizedCustomization,
          computedColors: computedColors,
          settings: {
            ...normalizedCustomization,
            colorSpace: colorSpace,
            animationsEnabled: animationsEnabled,
          },
          animations: animations,
        });

        // Also update currentPageTheme for LoadingScreen continuity
        try {
          sessionStorage.setItem('currentPageTheme', JSON.stringify({
            type: 'site',
            colors: {
              primaryColor: transformedPrimaryColor,
              secondaryColor: transformedSecondaryColor,
              textColor: transformedTextColor,
              accentColor: transformedAccentColor
            }
          }));
        } catch (e) {
          console.warn('Failed to update currentPageTheme:', e);
        }

        alert('Site theme saved successfully!');
        // Trigger a global theme update event
        window.dispatchEvent(new CustomEvent('siteThemeUpdated', {
          detail: { customization, animations, animationsEnabled, baseTheme }
        }));
        onClose();
      } else {
        const error = await response.json();
        alert(error.error || 'Failed to save site theme');
      }
    } catch (error) {
      alert('Error saving site theme');
    }
  };

  const handleReset = () => {
    setCustomization(DEFAULT_CUSTOMIZATION);
    setAnimations(getThemeAnimations('ocean-turtle'));
    setAnimationsEnabled(true);
  };

  // Only render the modal once theme data is fully loaded
  return (
    <ThemeCustomizerModal
      isOpen={isOpen && isReady}
      onClose={onClose}
      customization={customization}
      setCustomization={setCustomization}
      animations={animations}
      setAnimations={setAnimations}
      animationsEnabled={animationsEnabled}
      setAnimationsEnabled={setAnimationsEnabled}
      onSave={handleSave}
      onReset={handleReset}
      title="Site Theme Customizer"
      scopeName="site"
      showProfileOptions={false}
      showUploadOptions={false}
      baseTheme={baseTheme}
      setBaseTheme={setBaseTheme}
      userId={userId}
    />
  );
}
