// Community-specific theme customizer
// Controls the appearance of individual community pages

import React, { useState, useEffect } from 'react';
import ThemeCustomizerModal, { ThemeCustomization } from './ThemeCustomizerModal';
import { getThemeAnimations } from '@theme/animation-config';
import type { ThemeAnimations } from '@theme/animation-config';
import { PRESET_THEMES } from '@theme/tokens';
import { cacheTheme } from '../../../utils/themeCache';
import { transformColor } from '../../../utils/colorTransform';
import { calculateNavbarBg } from '../../../utils/backgroundHelpers';

type Props = {
  isOpen: boolean;
  onClose: () => void;
  communityId: string;
  isOwnerOrAdmin: boolean; // Only owners/admins can customize community theme
  initialCustomization?: ThemeCustomization;
  initialAnimations?: ThemeAnimations;
  initialAnimationsEnabled?: boolean;
};

// Helper function to validate and fix incomplete animation structures
const validateAnimations = (animations: any): ThemeAnimations => {
  const validated = { ...animations };

  // Fix incomplete bubbles structure
  if (validated.bubbles && typeof validated.bubbles === 'object') {
    // If bubbles exists but is missing required fields, add them
    if (!validated.bubbles.id || !validated.bubbles.type || !validated.bubbles.count) {
      validated.bubbles = validated.bubbles.enabled
        ? {
            id: 'bubbles',
            type: 'rising',
            enabled: true,
            count: 20,
            speed: 1,
            size: { min: 5, max: 15 },
          }
        : null;
    }
  }

  return validated as ThemeAnimations;
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

export default function CommunityThemeCustomizer({ isOpen, onClose, communityId, isOwnerOrAdmin, initialCustomization, initialAnimations, initialAnimationsEnabled }: Props) {
  const [customization, setCustomization] = useState<ThemeCustomization>(initialCustomization || DEFAULT_CUSTOMIZATION);
  const [animations, setAnimations] = useState<ThemeAnimations>(initialAnimations || getThemeAnimations('ocean-turtle'));
  const [animationsEnabled, setAnimationsEnabled] = useState(initialAnimationsEnabled ?? true);
  const [isPublic, setIsPublic] = useState(true);
  const [isReady, setIsReady] = useState(true);

  // Community information fields (using same prop names as profile for consistency with modal)
  const [profileName, setProfileName] = useState<string>(''); // Community name
  const [profileTitle, setProfileTitle] = useState<string>(''); // Community tagline
  const [profileBio, setProfileBio] = useState<string>(''); // Community description

  // Use refs to track the latest values for debounced dispatch
  const customizationRef = React.useRef(customization);
  const animationsRef = React.useRef(animations);
  const animationsEnabledRef = React.useRef(animationsEnabled);

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

  // Pre-calculate and load community theme data BEFORE showing the modal
  useEffect(() => {
    const loadCommunityData = async () => {
      if (!isOpen) {
        return;
      }

      // If we have initial customization from parent, use it for theme and only fetch community settings
      if (initialCustomization) {
        setCustomization(initialCustomization);
        if (initialAnimations) {
          setAnimations(initialAnimations);
        }
        if (typeof initialAnimationsEnabled === 'boolean') {
          setAnimationsEnabled(initialAnimationsEnabled);
        }

        // Still need to load community settings (name, tagline, etc.)
        try {
          const settingsResponse = await fetch(`/api/communities/${communityId}`);
          if (settingsResponse.ok) {
            const communityData = await settingsResponse.json();
            if (typeof communityData.isPublic === 'boolean') setIsPublic(communityData.isPublic);
            if (communityData.name) setProfileName(communityData.name);
            if (communityData.tagline) setProfileTitle(communityData.tagline);
            if (communityData.description) setProfileBio(communityData.description);
          }
        } catch (error) {
          console.error('Error loading community settings:', error);
        }

        setIsReady(true);
        return;
      }

      // Otherwise fetch everything from API
      try {
        // Load theme
        const themeResponse = await fetch(`/communities/${communityId}/theme`);
        let themeData = null;
        if (themeResponse.ok) {
          themeData = await themeResponse.json();
          if (themeData.customization) {
            setCustomization(themeData.customization);
          } else {
            setCustomization(DEFAULT_CUSTOMIZATION);
          }
          if (themeData.animations) {
            setAnimations(validateAnimations(themeData.animations));
          }
          if (typeof themeData.animationsEnabled === 'boolean') setAnimationsEnabled(themeData.animationsEnabled);
        } else {
          setCustomization(DEFAULT_CUSTOMIZATION);
          setAnimations(getThemeAnimations('ocean-turtle'));
          setAnimationsEnabled(true);
        }

        // Load community settings
        const settingsResponse = await fetch(`/api/communities/${communityId}`);
        if (settingsResponse.ok) {
          const communityData = await settingsResponse.json();
          if (typeof communityData.isPublic === 'boolean') setIsPublic(communityData.isPublic);

          // Load community information fields
          if (communityData.name) setProfileName(communityData.name);
          if (communityData.tagline) setProfileTitle(communityData.tagline);
          if (communityData.description) setProfileBio(communityData.description);
        }
      } catch (error) {
        setCustomization(DEFAULT_CUSTOMIZATION);
        setAnimations(getThemeAnimations('ocean-turtle'));
        setAnimationsEnabled(true);
      }

      setIsReady(true);
    };

    loadCommunityData();
  }, [isOpen, communityId, initialCustomization, initialAnimations, initialAnimationsEnabled]);

  // Live preview: dispatch community theme update event whenever customization changes
  // Debounce to prevent excessive re-renders while dragging sliders
  useEffect(() => {
    if (isOpen && isReady) {
      const timeoutId = setTimeout(() => {
        // Use refs to get the latest values, not the stale closure values
        const latestCustomization = customizationRef.current;
        const latestAnimations = animationsRef.current;
        const latestAnimationsEnabled = animationsEnabledRef.current;

        window.dispatchEvent(new CustomEvent('communityThemeUpdated', {
          detail: {
            customization: latestCustomization,
            animations: latestAnimations,
            animationsEnabled: latestAnimationsEnabled,
            communityId
          }
        }));
      }, 50); // 50ms debounce - fast enough to feel instant, slow enough to reduce re-renders

      return () => clearTimeout(timeoutId);
    }
  }, [customization, animations, animationsEnabled, isOpen, isReady, communityId]);

  const handleSave = async () => {
    if (!isOwnerOrAdmin) {
      alert('Only community owners and admins can customize the theme');
      return;
    }

    try {
      // Save theme
      const themeResponse = await fetch(`/communities/${communityId}/theme`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          customization,
          animations,
          animationsEnabled,
        }),
      });

      // Save community info and isPublic setting
      const settingsPayload: any = { isPublic };
      if (profileName) settingsPayload.name = profileName;
      if (profileTitle) settingsPayload.tagline = profileTitle;
      if (profileBio) settingsPayload.description = profileBio;

      const settingsResponse = await fetch(`/api/communities/${communityId}`, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        credentials: 'include',
        body: JSON.stringify(settingsPayload),
      });

      if (themeResponse.ok && settingsResponse.ok) {
        // Update the theme cache with the new theme before closing
        // This ensures the updated theme is used immediately on page load
        const colorSpace = customization.colorSpace || 'Off';
        const colorTransformOptions = {
          colorSpace,
          temperatureLimit: customization.temperatureLimit,
          magentaLimit: customization.magentaLimit,
          intensityLimit: customization.intensityLimit,
          redLimit: customization.redLimit,
          greenLimit: customization.greenLimit,
          blueLimit: customization.blueLimit,
          backgroundSaturationLimit: customization.backgroundSaturationLimit,
          backgroundBrightnessLimit: customization.backgroundBrightnessLimit,
        };

        const transformedTextColor = transformColor(customization.textColor, colorTransformOptions);
        const transformedAccentColor = transformColor(customization.accentColor, colorTransformOptions);
        const transformedPrimaryColor = transformColor(customization.primaryColor, colorTransformOptions);
        const transformedSecondaryColor = transformColor(customization.secondaryColor, colorTransformOptions);

        const navbarBg = calculateNavbarBg(transformedPrimaryColor, customization.navbarOpacity ?? 92);
        const tabBarBg = calculateNavbarBg(transformedPrimaryColor, customization.tabBarOpacity ?? 88);
        const cardBg = calculateNavbarBg(transformedPrimaryColor, customization.cardOpacity ?? 100);

        // Cache the transformed theme
        cacheTheme({
          textColor: transformedTextColor,
          accentColor: transformedAccentColor,
          cardBg: cardBg,
          appBg: 'rgba(0, 0, 0, 0)',
          navbarBg: navbarBg,
          tabBarBg: tabBarBg,
          primaryColor: transformedPrimaryColor,
          secondaryColor: transformedSecondaryColor,
          colorSpace: colorSpace,
          navbarOpacity: customization.navbarOpacity,
          tabBarOpacity: customization.tabBarOpacity,
          cardOpacity: customization.cardOpacity,
          gradientEnabled: customization.gradientEnabled,
          gradientAngle: customization.gradientAngle,
          gradientFavorability: customization.gradientFavorability,
          gradientStrength: customization.gradientStrength,
          cardGradientColor1: customization.cardGradientColor1,
          cardGradientColor2: customization.cardGradientColor2,
          cardGradientEnabled: customization.cardGradientEnabled,
          cardGradientFavorability: customization.cardGradientFavorability,
          cardGradientStrength: customization.cardGradientStrength,
          animations: animations,
          animationsEnabled: animationsEnabled,
        }, 'community', communityId);

        // Also update currentPageTheme for LoadingScreen continuity
        try {
          sessionStorage.setItem('currentPageTheme', JSON.stringify({
            type: 'community',
            communityId: communityId,
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

        alert('Community settings saved successfully!');
        // Dispatch theme update event to update the page without reload
        window.dispatchEvent(new CustomEvent('communityThemeUpdated', {
          detail: { customization, animations, animationsEnabled, communityId }
        }));
        onClose();
      } else {
        const error = themeResponse.ok ? await settingsResponse.json() : await themeResponse.json();
        alert(error.error || 'Failed to save community settings');
      }
    } catch (error) {
      alert('Error saving community settings');
    }
  };

  const handleReset = () => {
    setCustomization(DEFAULT_CUSTOMIZATION);
    setAnimations(getThemeAnimations('ocean-turtle'));
    setAnimationsEnabled(true);
  };

  if (!isOwnerOrAdmin && isOpen) {
    return (
      <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
        <div className="bg-red-900 text-white p-6 rounded-lg max-w-md">
          <h3 className="text-xl font-bold mb-2">Access Denied</h3>
          <p className="mb-4">Only community owners and admins can customize the community theme.</p>
          <button
            onClick={onClose}
            className="px-4 py-2 bg-white text-red-900 rounded font-semibold"
          >
            Close
          </button>
        </div>
      </div>
    );
  }

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
      title="Community Theme Customizer"
      scopeName="community"
      showProfileOptions={true}
      showUploadOptions={true}
      profileName={profileName}
      setProfileName={setProfileName}
      profileTitle={profileTitle}
      setProfileTitle={setProfileTitle}
      profileBio={profileBio}
      setProfileBio={setProfileBio}
      additionalSettings={
        <div className="mb-6 p-4 rounded-lg" style={{ backgroundColor: 'rgba(255, 255, 255, 0.05)' }}>
          <h3 className="text-lg font-semibold mb-3" style={{ color: customization.textColor }}>
            Community Settings
          </h3>
          <label className="flex items-center gap-3 cursor-pointer">
            <input
              type="checkbox"
              checked={isPublic}
              onChange={(e) => setIsPublic(e.target.checked)}
              className="w-5 h-5 rounded cursor-pointer"
              style={{ accentColor: customization.accentColor }}
            />
            <div>
              <span className="text-sm font-medium" style={{ color: customization.textColor }}>
                Public Community
              </span>
              <p className="text-xs mt-1" style={{ color: customization.textColor + '99' }}>
                Anyone can view and join this community
              </p>
            </div>
          </label>
        </div>
      }
    />
  );
}
