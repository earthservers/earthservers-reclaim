// Profile-specific theme customizer
// Controls individual user profile appearance

import React, { useState, useEffect } from 'react';
import { profileEventBus } from '../../../lib/profileEventBus';
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
  userId: string;
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

export default function ProfileThemeCustomizer({ isOpen, onClose, userId, initialCustomization, initialAnimations, initialAnimationsEnabled }: Props) {
  const [customization, setCustomization] = useState<ThemeCustomization>(initialCustomization || DEFAULT_CUSTOMIZATION);
  const [animations, setAnimations] = useState<ThemeAnimations>(initialAnimations || getThemeAnimations('ocean-turtle'));
  const [animationsEnabled, setAnimationsEnabled] = useState(initialAnimationsEnabled ?? true);
  const [isReady, setIsReady] = useState(true);
  const [profilePicFileId, setProfilePicFileId] = useState<string | null>(null);

  // Profile information fields
  const [profileName, setProfileName] = useState<string>('');
  const [profileTitle, setProfileTitle] = useState<string>('');
  const [profileBio, setProfileBio] = useState<string>('');

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


  // Pre-calculate and load profile theme data BEFORE showing the modal
  useEffect(() => {
    const loadProfileTheme = async () => {
      if (!isOpen) {
        return;
      }

      // If we have initial customization from parent, use it and skip API fetch
      if (initialCustomization) {
        setCustomization(initialCustomization);
        if (initialAnimations) {
          setAnimations(initialAnimations);
        }
        if (typeof initialAnimationsEnabled === 'boolean') {
          setAnimationsEnabled(initialAnimationsEnabled);
        }
        setIsReady(true);
        return;
      }

      // Otherwise fetch from API
      try {
        const response = await fetch(`/api/profile/${userId}`);
        if (response.ok) {
          const data = await response.json();
          if (data.profileCustomization) {
            setCustomization(data.profileCustomization);
          } else {
            setCustomization(DEFAULT_CUSTOMIZATION);
          }
          if (data.profileCustomization?.animations) {
            setAnimations(validateAnimations(data.profileCustomization.animations));
          }
          if (typeof data.profileCustomization?.animationsEnabled === 'boolean') {
            setAnimationsEnabled(data.profileCustomization.animationsEnabled);
          }

          // Load profile information fields
          if (data.profileName) setProfileName(data.profileName);
          if (data.profileTitle) setProfileTitle(data.profileTitle);
          if (data.profileBio) setProfileBio(data.profileBio);
          if (data.profilePicture) setProfilePicFileId(data.profilePicture);
        } else {
          setCustomization(DEFAULT_CUSTOMIZATION);
          setAnimations(getThemeAnimations('ocean-turtle'));
          setAnimationsEnabled(true);
        }
      } catch (error) {
        setCustomization(DEFAULT_CUSTOMIZATION);
        setAnimations(getThemeAnimations('ocean-turtle'));
        setAnimationsEnabled(true);
      }

      setIsReady(true);
    };

    loadProfileTheme();
  }, [isOpen, userId, initialCustomization, initialAnimations, initialAnimationsEnabled]);

  // Live preview: dispatch profile theme update event whenever customization changes
  // Debounce to prevent excessive re-renders while dragging sliders
  useEffect(() => {

    if (isOpen && isReady) {
      const timeoutId = setTimeout(() => {
        // Use refs to get the latest values, not the stale closure values
        const latestCustomization = customizationRef.current;
        const latestAnimations = animationsRef.current;
        const latestAnimationsEnabled = animationsEnabledRef.current;

        window.dispatchEvent(new CustomEvent('profileThemeUpdated', {
          detail: {
            customization: latestCustomization,
            animations: latestAnimations,
            animationsEnabled: latestAnimationsEnabled
          }
        }));
      }, 50); // 50ms debounce - fast enough to feel instant, slow enough to reduce re-renders

      return () => {
        clearTimeout(timeoutId);
      };
    } else {
    }
  }, [customization, animations, animationsEnabled, isOpen, isReady]);

  const handleSave = async () => {
    try {
      // animations and animationsEnabled should be stored inside customization
      const payload: any = {
        customization: {
          ...customization,
          animations,
          animationsEnabled,
        },
      };

      // Include profile information fields
      if (profileName) payload.profileName = profileName;
      if (profileTitle) payload.profileTitle = profileTitle;
      if (profileBio) payload.profileBio = profileBio;

      // Include avatar if a new one was uploaded
      if (profilePicFileId) {
        payload.avatar = profilePicFileId;
      }


      const response = await fetch(`/api/profile/${userId}`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        credentials: 'include',
        body: JSON.stringify(payload),
      });

      if (response.ok) {
        await response.json();

        // Update the theme cache with the new theme before reloading
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
        }, 'profile', userId.toString());

        // Also update currentPageTheme for LoadingScreen continuity
        try {
          sessionStorage.setItem('currentPageTheme', JSON.stringify({
            type: 'profile',
            userId: userId.toString(),
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

        alert('Profile theme saved successfully!');

        // Emit profile updated event to refresh navbar and other components
        profileEventBus.emitProfileUpdated(userId!.toString());

        // Emit profile theme updated event to update ProfileClient without reload
        window.dispatchEvent(new CustomEvent('profileThemeUpdated', {
          detail: {
            customization: {
              ...customization,
              primaryColor: transformedPrimaryColor,
              secondaryColor: transformedSecondaryColor,
              textColor: transformedTextColor,
              accentColor: transformedAccentColor,
              navbarBg: navbarBg,
              tabBarBg: tabBarBg,
              cardBg: cardBg,
              appBg: 'rgba(0, 0, 0, 0)',
            },
            animations: animations,
            animationsEnabled: animationsEnabled
          }
        }));
      } else {
        const error = await response.json();
        alert(error.error || 'Failed to save profile theme');
      }
    } catch (error) {
      console.error('[ProfileThemeCustomizer] ❌ Error in handleSave:', error);
      alert('Error saving profile theme');
    }
  };

  const handleReset = () => {
    setCustomization(DEFAULT_CUSTOMIZATION);
    setAnimations(getThemeAnimations('ocean-turtle'));
    setAnimationsEnabled(true);
  };

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
      title="Profile Theme Customizer"
      scopeName="profile"
      showProfileOptions={true}
      showUploadOptions={true}
      userId={userId}
      profilePicFileId={profilePicFileId}
      setProfilePicFileId={setProfilePicFileId}
      profileName={profileName}
      setProfileName={setProfileName}
      profileTitle={profileTitle}
      setProfileTitle={setProfileTitle}
      profileBio={profileBio}
      setProfileBio={setProfileBio}
    />
  );
}
