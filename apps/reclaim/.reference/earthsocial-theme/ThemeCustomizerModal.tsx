// Base theme customizer modal component
// Redesigned with section navigation and improved layout

import React, { useState, useEffect, useMemo } from 'react';
import { X, ChevronLeft, ChevronRight, Palette, Upload, Square, Circle, AlignLeft, AlignCenter, AlignRight, Trash2 } from 'lucide-react';
import { PRESET_THEMES, getThemeByKey } from '@theme/tokens';
import type { PresetThemeKey } from '@theme/tokens';
import { getThemeAnimations } from '@theme/animation-config';
import type { ThemeAnimations } from '@theme/animation-config';
import { useThemeColors } from '../../../context/ThemeContext';
import { transformColor, type ColorTransformOptions } from '../../../utils/colorTransform';
import { MediaUploadButton } from '../MediaUploadButton';
import { MediaDisplay } from '../MediaDisplay';
import { toast } from '../../ui/toaster';

const PRESET_OPTIONS: PresetThemeKey[] = ["ocean-turtle", "mountain-eagle", "sun-fire", "lightning-bolt", "air-clouds"];

export type ThemeCustomization = {
  primaryColor: string;
  secondaryColor: string;
  accentColor: string;
  highlightColor?: string; // for input focus, dropdown highlights, etc.
  textColor: string;
  navbarBg: string; // Top navbar background
  navbarOpacity: number; // 0-100, default 95
  tabBarBg: string; // Tab bar background (All Posts/Following/etc)
  tabBarOpacity: number; // 0-100, default 88
  dropdownColor: string; // Dropdown background color
  dropdownOpacity: number; // 0-100, default 95 (for dropdown menu)
  dropdownButtonOpacity: number; // 0-100, default 100 (for dropdown button)
  cardBg: string;
  cardGradientEnabled: boolean;
  cardGradientAngle: number;
  cardGradientColor1: string;
  cardGradientColor2: string;
  cardGradientFavorability: number; // 0-100, default 50
  cardOpacity: number; // 0-100, default 100
  cardGradientStrength: number; // 0-100, default 100 (controls how distinct the gradient is)
  gradientEnabled: boolean;
  gradientAngle: number;
  gradientFrom?: string; // Gradient start color
  gradientTo?: string; // Gradient end color
  gradientFavorability: number; // 0-100, default 50
  gradientStrength: number; // 0-100, default 100
  bubbleColor: string;
  turtleColor: string;
  coralColors: string[];
  profileFrameColor: string;
  profileFrameShape: 'square' | 'circle';
  profileNameAlign: 'left' | 'center' | 'right';
  profileFrameDesign: 'coral' | 'waves' | 'bubbles';
  animationsEnabled: boolean;
  // Additional alignment and positioning options
  cardPosition?: 'left' | 'center' | 'right';
  tabAlignment?: 'left' | 'center' | 'right';
  profilePicturePosition?: 'left' | 'right';
  profilePictureSize?: 'small' | 'medium' | 'large';
  profileTitleAlign?: 'left' | 'center' | 'right';
  profileBioAlign?: 'left' | 'center' | 'right';
  postsAlignment?: 'left' | 'center' | 'right';
  selectedCharacter: string;
  selectedDecoration: string;
  customPresets?: any[]; // Array of custom theme presets
  focusAnimationEnabled?: boolean;
  presetTheme?: string; // Track which preset theme is being used for animations
  // New Midnight Lizard-inspired background options
  backgroundSaturationLimit?: number; // 0-100, default 70
  backgroundContrast?: number; // 0-100, default 50
  backgroundBrightnessLimit?: number; // 0-100, default 14
  backgroundGraySaturation?: number; // 0-100, default 5
  backgroundHueGravity?: number; // 0-100, default 0 (strength/gravity)
  backgroundDefaultHue?: number; // 0-360, default 165 (hue in degrees)
  // Color space selection and options
  colorSpace?: 'Off' | 'RGB' | 'HSV' | 'TMI'; // Which color space to use for transformations
  // TMI options
  temperatureLimit?: number; // 0-100, default 50
  magentaLimit?: number; // 0-100, default 50
  intensityLimit?: number; // 0-100, default 50
  // RGB options
  redLimit?: number; // 0-100, default 50
  greenLimit?: number; // 0-100, default 50
  blueLimit?: number; // 0-100, default 50
};

type Props = {
  isOpen: boolean;
  onClose: () => void;
  customization: ThemeCustomization;
  setCustomization: (customization: ThemeCustomization) => void;
  animations: ThemeAnimations;
  setAnimations: (animations: ThemeAnimations) => void;
  animationsEnabled: boolean;
  setAnimationsEnabled: (enabled: boolean) => void;
  onSave: () => Promise<void>;
  onReset: () => void;
  title: string;
  scopeName: 'profile' | 'site' | 'messages' | 'community';
  showProfileOptions?: boolean; // Only show profile-specific options for profile scope
  showUploadOptions?: boolean; // Only show upload options for profile/community
  baseTheme?: string; // Which preset theme is being used
  setBaseTheme?: (theme: string) => void;
  userId?: string | number; // User ID for saving custom presets

  // Profile-specific editing fields
  profileName?: string;
  setProfileName?: (name: string) => void;
  profileTitle?: string;
  setProfileTitle?: (title: string) => void;
  profileBio?: string;
  setProfileBio?: (bio: string) => void;
  cardPosition?: 'left' | 'center' | 'right';
  setCardPosition?: (position: 'left' | 'center' | 'right') => void;
  tabAlignment?: 'left' | 'center' | 'right';
  setTabAlignment?: (alignment: 'left' | 'center' | 'right') => void;
  profilePicturePosition?: 'left' | 'right';
  setProfilePicturePosition?: (position: 'left' | 'right') => void;
  profilePictureSize?: 'small' | 'medium' | 'large';
  setProfilePictureSize?: (size: 'small' | 'medium' | 'large') => void;
  profileTitleAlign?: 'left' | 'center' | 'right';
  setProfileTitleAlign?: (align: 'left' | 'center' | 'right') => void;
  profileBioAlign?: 'left' | 'center' | 'right';
  setProfileBioAlign?: (align: 'left' | 'center' | 'right') => void;
  postsAlignment?: 'left' | 'center' | 'right';
  setPostsAlignment?: (align: 'left' | 'center' | 'right') => void;
  additionalSettings?: React.ReactNode; // Optional additional settings to display
  profilePicFileId?: string | null;
  setProfilePicFileId?: (fileId: string | null) => void;
};

export default function ThemeCustomizerModal({
  isOpen,
  onClose,
  customization,
  setCustomization,
  animations,
  setAnimations,
  animationsEnabled,
  setAnimationsEnabled,
  onSave,
  onReset,
  title,
  scopeName,
  showProfileOptions = false,
  showUploadOptions = false,
  baseTheme,
  setBaseTheme,
  userId,
  profileName,
  setProfileName,
  profileTitle,
  setProfileTitle,
  profileBio,
  setProfileBio,
  cardPosition,
  setCardPosition,
  tabAlignment,
  setTabAlignment,
  profilePicturePosition,
  setProfilePicturePosition,
  profilePictureSize,
  setProfilePictureSize,
  profileTitleAlign,
  setProfileTitleAlign,
  profileBioAlign,
  setProfileBioAlign,
  postsAlignment,
  setPostsAlignment,
  additionalSettings,
  profilePicFileId,
  setProfilePicFileId,
}: Props) {
  const [customizerWidth, setCustomizerWidth] = useState<'35' | '90'>('35');
  const [activeSection, setActiveSection] = useState<number>(1);

  // Wrap setCustomization for future needs
  const wrappedSetCustomization = (newValue: ThemeCustomization) => {
    setCustomization(newValue);
  };

  // Drag state
  const [isDragging, setIsDragging] = useState(false);
  const [dragOffset, setDragOffset] = useState({ x: 0, y: 0 });
  const [position, setPosition] = useState({ x: 0, y: 0 });

  // Create localTheme with transformed colors for live preview in customizer
  const localTheme = useMemo(() => {
    const colorSpace = customization.colorSpace || 'Off';

    // If color space is "Off", return raw colors without transformation
    if (colorSpace === 'Off') {
      // Helper to convert hex to rgba with opacity
      const hexToRgbaWithOpacity = (hex: string, opacity: number) => {
        const r = parseInt(hex.slice(1, 3), 16);
        const g = parseInt(hex.slice(3, 5), 16);
        const b = parseInt(hex.slice(5, 7), 16);
        return `rgba(${r}, ${g}, ${b}, ${opacity / 100})`;
      };

      // Simple: colors are stored as 7-char hex, opacity stored separately
      const processedNavbarBg = customization.navbarBg
        ? hexToRgbaWithOpacity(customization.navbarBg, customization.navbarOpacity ?? 92)
        : hexToRgbaWithOpacity(customization.primaryColor, customization.navbarOpacity ?? 92);

      const processedTabBarBg = customization.tabBarBg
        ? hexToRgbaWithOpacity(customization.tabBarBg, customization.tabBarOpacity ?? 88)
        : processedNavbarBg;

      return {
        textColor: customization.textColor,
        accentColor: customization.accentColor,
        primaryColor: customization.primaryColor,
        secondaryColor: customization.secondaryColor,
        cardBg: customization.cardBg,
        cardGradientColor1: customization.cardGradientColor1 || customization.cardBg,
        cardGradientColor2: customization.cardGradientColor2 || customization.cardBg,
        navbarBg: processedNavbarBg,
        tabBarBg: processedTabBarBg,
      };
    }

    // Otherwise, apply color transformations
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

    // Transform primary color first
    const transformedPrimary = transformColor(customization.primaryColor, transformOptions);

    // Helper to convert hex to rgba with opacity
    const hexToRgbaWithOpacity = (hex: string, opacity: number) => {
      const r = parseInt(hex.slice(1, 3), 16);
      const g = parseInt(hex.slice(3, 5), 16);
      const b = parseInt(hex.slice(5, 7), 16);
      return `rgba(${r}, ${g}, ${b}, ${opacity / 100})`;
    };

    // Transform navbar and tab bar colors (stored as 7-char hex), then apply opacity
    const transformedNavbarBgColor = customization.navbarBg
      ? transformColor(customization.navbarBg, transformOptions)
      : transformedPrimary;
    const transformedNavbarBg = hexToRgbaWithOpacity(transformedNavbarBgColor, customization.navbarOpacity ?? 92);

    const transformedTabBarBgColor = customization.tabBarBg
      ? transformColor(customization.tabBarBg, transformOptions)
      : transformedPrimary;
    const transformedTabBarBg = customization.tabBarBg
      ? hexToRgbaWithOpacity(transformedTabBarBgColor, customization.tabBarOpacity ?? 88)
      : transformedNavbarBg;

    return {
      textColor: transformColor(customization.textColor, transformOptions),
      accentColor: transformColor(customization.accentColor, transformOptions),
      primaryColor: transformedPrimary,
      secondaryColor: transformColor(customization.secondaryColor, transformOptions),
      cardBg: transformColor(customization.cardBg, transformOptions),
      cardGradientColor1: transformColor(customization.cardGradientColor1 || customization.cardBg, transformOptions),
      cardGradientColor2: transformColor(customization.cardGradientColor2 || customization.cardBg, transformOptions),
      navbarBg: transformedNavbarBg,
      tabBarBg: transformedTabBarBg,
    };
  }, [customization]);

  // Legacy themeColors for compatibility
  const themeColors = localTheme;

  // Initialize new background options with defaults if undefined
  useEffect(() => {
    if (isOpen) {
      const needsUpdate =
        customization.backgroundSaturationLimit === undefined ||
        customization.backgroundContrast === undefined ||
        customization.backgroundBrightnessLimit === undefined ||
        customization.backgroundGraySaturation === undefined ||
        customization.backgroundHueGravity === undefined ||
        customization.backgroundDefaultHue === undefined;

      if (needsUpdate) {
        setCustomization({
          ...customization,
          backgroundSaturationLimit: customization.backgroundSaturationLimit ?? 70,
          backgroundContrast: customization.backgroundContrast ?? 50,
          backgroundBrightnessLimit: customization.backgroundBrightnessLimit ?? 14,
          backgroundGraySaturation: customization.backgroundGraySaturation ?? 5,
          backgroundHueGravity: customization.backgroundHueGravity ?? 0,
          backgroundDefaultHue: customization.backgroundDefaultHue ?? 165,
        });
      }
    }
  }, [isOpen]);

  // Drag handlers
  const handleMouseDown = (e: React.MouseEvent<HTMLDivElement>) => {
    // Only start dragging if clicking directly on the header (not on buttons)
    if ((e.target as HTMLElement).closest('button')) {
      return;
    }
    setIsDragging(true);
    setDragOffset({
      x: e.clientX - position.x,
      y: e.clientY - position.y,
    });
  };

  const handleMouseMove = (e: MouseEvent) => {
    if (isDragging) {
      setPosition({
        x: e.clientX - dragOffset.x,
        y: e.clientY - dragOffset.y,
      });
    }
  };

  const handleMouseUp = () => {
    setIsDragging(false);
  };

  // Add and remove mouse event listeners for dragging
  useEffect(() => {
    if (isDragging) {
      document.addEventListener('mousemove', handleMouseMove);
      document.addEventListener('mouseup', handleMouseUp);
      return () => {
        document.removeEventListener('mousemove', handleMouseMove);
        document.removeEventListener('mouseup', handleMouseUp);
      };
    }
  }, [isDragging, dragOffset]);

  // Reset position when modal opens
  useEffect(() => {
    if (isOpen) {
      setPosition({ x: 0, y: 0 });
    }
  }, [isOpen]);

  // Compute card background from current customization state
  const getCustomizerCardBg = () => {
    const hexToRgba = (hex: string, alpha: number) => {
      const r = parseInt(hex.slice(1, 3), 16);
      const g = parseInt(hex.slice(3, 5), 16);
      const b = parseInt(hex.slice(5, 7), 16);
      return `rgba(${r}, ${g}, ${b}, ${alpha})`;
    };

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

    const opacity = (customization.cardOpacity ?? 100) / 100;

    if (customization.cardGradientEnabled) {
      const fav = customization.cardGradientFavorability ?? 50;
      const strength = (customization.cardGradientStrength ?? 100) / 100;

      // Use transformed colors from localTheme
      const color1 = localTheme.cardGradientColor1 || localTheme.cardBg;
      const color2 = localTheme.cardGradientColor2 || localTheme.cardBg;

      const blendedColor2 = strength === 1 ? color2 : blendColors(color1, color2, strength);

      const rgba1 = hexToRgba(color1, opacity);
      const rgba2 = hexToRgba(blendedColor2, opacity);

      return `linear-gradient(${customization.cardGradientAngle || 135}deg, ${rgba1} 0%, ${rgba2} ${fav * 2}%)`;
    } else {
      // Use transformed cardBg from localTheme
      const baseColor = localTheme.cardBg;
      if (baseColor.startsWith('#')) {
        return hexToRgba(baseColor, opacity);
      }
      if (baseColor.startsWith('rgba')) {
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
  };

  // Custom presets are stored globally and shared across all customizers (site/profile/community)
  const [customPresets, setCustomPresets] = useState<Record<string, { name: string; settings: ThemeCustomization & { animations: ThemeAnimations; animationsEnabled: boolean } }>>({});
  const [showPresetModal, setShowPresetModal] = useState(false);
  const [presetName, setPresetName] = useState('');
  const [savingToSlot, setSavingToSlot] = useState<string | null>(null);
  const [globalPresetsLoaded, setGlobalPresetsLoaded] = useState(false);

  // Load global custom presets from site customization on mount
  useEffect(() => {
    if (isOpen && !globalPresetsLoaded && userId) {
      const loadGlobalPresets = async () => {
        try {
          // Load site customization which contains global customPresets
          const themeResponse = await fetch(`/api/users/${userId}/site-theme`, { credentials: 'include' });
          if (themeResponse.ok) {
            const themeData = await themeResponse.json();
            if (themeData.customization?.customPresets) {
              setCustomPresets(themeData.customization.customPresets);
            }
          }
        } catch (error) {
        }
        setGlobalPresetsLoaded(true);
      };
      loadGlobalPresets();
    }
  }, [isOpen, globalPresetsLoaded, userId]);

  if (!isOpen) return null;

  // Helper function to convert rgba/hex to hex for color picker
  const rgbaToHex = (color: string): string => {
    if (!color) return '#06b6d4';

    // If already hex, return it
    if (color.startsWith('#')) return color.substring(0, 7);

    // Parse rgba format
    const rgbaMatch = color.match(/rgba?\((\d+),\s*(\d+),\s*(\d+)/);
    if (rgbaMatch) {
      const r = parseInt(rgbaMatch[1]).toString(16).padStart(2, '0');
      const g = parseInt(rgbaMatch[2]).toString(16).padStart(2, '0');
      const b = parseInt(rgbaMatch[3]).toString(16).padStart(2, '0');
      return `#${r}${g}${b}`;
    }

    return '#06b6d4';
  };

  const getCustomizerStyle = () => {
    if (customizerWidth === '90') return { width: '90%' };
    return { width: '35%' };
  };

  const toggleAnimation = (category: 'characters' | 'decorations', id: string) => {
    setAnimations({
      ...animations,
      [category]: animations[category].map(anim =>
        anim.id === id
          ? { ...anim, enabled: !anim.enabled }
          : anim
      )
    });
  };

  const toggleBubbles = () => {
    setAnimations({
      ...animations,
      bubbles: animations.bubbles
        ? { ...animations.bubbles, enabled: !animations.bubbles.enabled }
        : {
            id: 'bubbles',
            type: 'rising',
            enabled: true,
            count: 20,
            speed: 1,
            size: { min: 5, max: 15 },
          }
    });
  };

  const updateBubblesCount = (count: number) => {
    if (animations.bubbles) {
      setAnimations({
        ...animations,
        bubbles: { ...animations.bubbles, count }
      });
    }
  };

  const updateBubblesSpeed = (speed: number) => {
    if (animations.bubbles) {
      setAnimations({
        ...animations,
        bubbles: { ...animations.bubbles, speed }
      });
    }
  };

  const updateBubblesSize = (min: number, max: number) => {
    if (animations.bubbles) {
      setAnimations({
        ...animations,
        bubbles: { ...animations.bubbles, size: { min, max } }
      });
    }
  };

  const updateAnimationSpeed = (
    category: 'characters' | 'decorations',
    id: string,
    speed: number
  ) => {
    setAnimations({
      ...animations,
      [category]: animations[category].map(anim =>
        anim.id === id
          ? { ...anim, speed }
          : anim
      )
    });
  };

  // Define sections based on scope
  const getSections = () => {
    const isProfileOrCommunity = scopeName === 'profile' || scopeName === 'community';
    const sections: { id: number; label: string }[] = [];

    if (isProfileOrCommunity && showProfileOptions) {
      sections.push({
        id: 1,
        label: scopeName === 'profile' ? 'Profile Information' : 'Community Information'
      });
    }

    const baseId = (isProfileOrCommunity && showProfileOptions) ? 2 : 1;

    sections.push(
      { id: baseId, label: 'Theme Presets' },
      { id: baseId + 1, label: 'Animations & Effects' }
    );

    if (isProfileOrCommunity && showProfileOptions) {
      sections.push({ id: baseId + 2, label: 'Profile Card Design' });
    }

    sections.push(
      { id: sections.length + 1, label: 'Card Styling' },
      { id: sections.length + 2, label: 'Background' },
      { id: sections.length + 3, label: 'Navigation & Text Colors' }
    );

    if (isProfileOrCommunity && showProfileOptions) {
      sections.push({ id: sections.length + 1, label: 'Alignments' });
    }

    sections.push(
      { id: sections.length + 1, label: 'Advanced Controls' }
    );

    return sections;
  };

  const sections = getSections();

  return (
    <>
      <style>{`
        @keyframes scale-in {
          from {
            transform: scale(0.8);
            opacity: 0;
          }
          to {
            transform: scale(1);
            opacity: 1;
          }
        }
        .animate-scale-in {
          animation: scale-in 0.2s ease-out;
        }

        /* Override color picker styling to completely remove burgundy background */
        .theme-customizer input[type="color"] {
          -webkit-appearance: none !important;
          -moz-appearance: none !important;
          appearance: none !important;
          width: 100% !important;
          height: 40px !important;
          border: 2px solid ${localTheme.accentColor} !important;
          border-radius: 8px !important;
          cursor: pointer !important;
          padding: 0 !important;
          margin: 0 !important;
          background-color: #ffffff !important;
          background-image: none !important;
          background: #ffffff !important;
          box-shadow: none !important;
        }
        .theme-customizer input[type="color"]::-webkit-color-swatch-wrapper {
          padding: 0 !important;
          border: none !important;
          border-radius: 6px !important;
          background: transparent !important;
        }
        .theme-customizer input[type="color"]::-webkit-color-swatch {
          border: none !important;
          border-radius: 6px !important;
          box-shadow: none !important;
        }
        .theme-customizer input[type="color"]::-moz-color-swatch {
          border: none !important;
          border-radius: 6px !important;
          box-shadow: none !important;
        }
        .theme-customizer input[type="color"]::-moz-focus-inner {
          border: 0 !important;
          padding: 0 !important;
        }
        .theme-customizer input[type="color"]:focus {
          outline: 2px solid ${localTheme.accentColor} !important;
          outline-offset: 2px !important;
          box-shadow: none !important;
        }
      `}</style>

      <div
        className="fixed shadow-2xl swipe-panel flex flex-col theme-customizer"
        style={{
          background: getCustomizerCardBg(),
          ...getCustomizerStyle(),
          top: '50%',
          right: '20px',
          transform: `translate(${position.x}px, calc(-50% + ${position.y}px))`,
          height: '75vh',
          transition: isDragging ? 'none' : 'all 0.3s ease-in-out',
          zIndex: 1000,
          overflow: 'hidden',
          borderRadius: '16px',
          border: `2px solid ${themeColors.accentColor}40`,
          backdropFilter: 'blur(10px)',
          cursor: isDragging ? 'grabbing' : 'default'
        }}
      >
        {/* Header */}
        <div
          key={`header-${customization.colorSpace}-${customization.redLimit}-${customization.greenLimit}-${customization.blueLimit}-${customization.temperatureLimit}-${customization.magentaLimit}-${customization.intensityLimit}-${customization.tabBarBg}-${customization.tabBarOpacity}`}
          className="flex-shrink-0 border-b relative"
          onMouseDown={handleMouseDown}
          style={{
            background: localTheme.tabBarBg,
            borderColor: `${themeColors.accentColor}40`,
            borderTopLeftRadius: '16px',
            borderTopRightRadius: '16px',
            cursor: isDragging ? 'grabbing' : 'grab'
          }}
        >
          <div className="p-6 pb-4">
            <div className="text-center">
              <h3 className="text-xl font-bold" style={{ color: localTheme.textColor }}>
                {title}
              </h3>
              <p className="text-sm opacity-75" style={{ color: localTheme.textColor }}>
                Customize your {scopeName} theme
              </p>
            </div>
            {/* Buttons positioned in top-right */}
            <div className="absolute top-4 right-4 flex gap-2" style={{ zIndex: 10 }}>
              <button
                onClick={() => setCustomizerWidth(customizerWidth === '35' ? '90' : '35')}
                className="p-2 rounded-lg transition-all"
                style={{ backgroundColor: themeColors.accentColor, color: localTheme.textColor }}
              >
                {customizerWidth === '35' ? <ChevronLeft size={20} /> : <ChevronRight size={20} />}
              </button>
              <button
                onClick={onClose}
                className="p-2 rounded-lg transition-all"
                style={{ backgroundColor: themeColors.accentColor, color: localTheme.textColor }}
              >
                <X size={20} />
              </button>
            </div>
          </div>
        </div>

        {/* Main Content Area with Sidebar */}
        <div className="flex-1 flex overflow-hidden relative">
          {/* Main Content (Left 82%) */}
          <div
            className="overflow-y-auto"
            style={{
              width: '82%',
              overscrollBehavior: 'contain',
              WebkitOverflowScrolling: 'touch',
              padding: '32px 40px',
              paddingBottom: '100px' // Space for fixed button
            }}
          >
            {/* SECTION 1: Profile/Community Information */}
            {activeSection === 1 && (scopeName === 'profile' || scopeName === 'community') && showProfileOptions && (
              <div className="space-y-8">
                <h4 className="font-semibold text-2xl mb-8" style={{ color: localTheme.textColor }}>
                  {scopeName === 'profile' ? 'Profile Information' : 'Community Information'}
                </h4>

                {/* Display Name */}
                {setProfileName && (
                  <div>
                    <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                      Display Name
                    </label>
                    <input
                      type="text"
                      value={profileName}
                      onChange={(e) => setProfileName(e.target.value)}
                      className="w-full px-4 py-2 rounded-lg border focus:outline-none focus:ring-2"
                      style={{
                        backgroundColor: 'rgba(255, 255, 255, 0.05)',
                        borderColor: localTheme.accentColor + '40',
                        color: localTheme.textColor,
                      }}
                      placeholder="Your name"
                    />
                  </div>
                )}

                {/* Profile Title */}
                {setProfileTitle && (
                  <div>
                    <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                      Profile Title
                    </label>
                    <input
                      type="text"
                      value={profileTitle}
                      onChange={(e) => setProfileTitle(e.target.value)}
                      className="w-full px-4 py-2 rounded-lg border focus:outline-none focus:ring-2"
                      style={{
                        backgroundColor: 'rgba(255, 255, 255, 0.05)',
                        borderColor: localTheme.accentColor + '40',
                        color: localTheme.textColor,
                      }}
                      placeholder="e.g. Earthling, Artist, Developer"
                    />
                  </div>
                )}

                {/* Bio */}
                {setProfileBio && (
                  <div>
                    <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                      Bio (one quote per line, max 10 lines, 100 chars per line)
                    </label>
                    <textarea
                      value={profileBio}
                      onChange={(e) => {
                        const lines = e.target.value.split('\n');
                        if (lines.length > 10) return;
                        const limitedLines = lines.map(line => line.slice(0, 100));
                        setProfileBio(limitedLines.join('\n'));
                      }}
                      className="w-full px-4 py-2 rounded-lg border focus:outline-none focus:ring-2 resize-none"
                      style={{
                        backgroundColor: 'rgba(255, 255, 255, 0.05)',
                        borderColor: localTheme.accentColor + '40',
                        color: localTheme.textColor,
                      }}
                      placeholder="Your bio quotes..."
                      rows={10}
                    />
                    <p className="text-xs mt-1 opacity-60" style={{ color: localTheme.textColor }}>
                      {profileBio?.split('\n').length || 0}/10 lines
                    </p>
                  </div>
                )}

                {/* Profile Picture Upload */}
                {showUploadOptions && setProfilePicFileId && (
                  <div>
                    <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                      Profile Picture
                    </label>

                    {profilePicFileId ? (
                      <div className="space-y-3">
                        <div className="relative w-32 h-32 mx-auto">
                          <MediaDisplay
                            fileId={profilePicFileId}
                            variant="small"
                            alt="Profile preview"
                            className="w-full h-full object-cover rounded-lg"
                            style={{
                              border: `2px solid ${localTheme.accentColor}`,
                            }}
                          />
                        </div>
                        <button
                          onClick={() => setProfilePicFileId?.(null)}
                          className="w-full flex items-center justify-center gap-2 px-4 py-2 rounded-lg transition-all"
                          style={{
                            backgroundColor: 'rgba(239, 68, 68, 0.1)',
                            color: '#ef4444',
                            border: '1px solid rgba(239, 68, 68, 0.3)',
                          }}
                        >
                          <Trash2 size={16} />
                          Remove Picture
                        </button>
                      </div>
                    ) : (
                      <MediaUploadButton
                        onUploadComplete={(fileId) => setProfilePicFileId?.(fileId)}
                        onUploadError={(error) => toast('Upload failed', { description: error, duration: 4000 })}
                        accept="image/*"
                        theme={{
                          accentColor: localTheme.accentColor,
                          textColor: localTheme.textColor,
                        }}
                        showProgress={true}
                        buttonText="Upload"
                      />
                    )}
                  </div>
                )}
              </div>
            )}

            {/* SECTION 2: Theme Presets (adjust ID based on scope) */}
            {activeSection === ((scopeName === 'profile' || scopeName === 'community') && showProfileOptions ? 2 : 1) && (
              <div className="space-y-8">
                <h4 className="font-semibold text-2xl mb-8" style={{ color: localTheme.textColor }}>
                  Theme Presets
                </h4>

                <div className="grid grid-cols-2 gap-3">
                  {PRESET_OPTIONS.map((preset) => (
                    <div
                      key={preset}
                      style={{
                        overflow: 'hidden',
                        borderRadius: '8px',
                        background: themeColors.accentColor,
                        border: `2px solid ${themeColors.accentColor}`
                      }}
                    >
                      <button
                        onClick={() => {
                          const presetTheme = getThemeByKey(preset);

                          setCustomization({
                            ...customization,
                            primaryColor: presetTheme.primaryColor || presetTheme.accentColor,
                            secondaryColor: presetTheme.secondaryColor || presetTheme.accentColor,
                            textColor: presetTheme.textColor,
                            accentColor: presetTheme.accentColor,
                            highlightColor: presetTheme.highlightColor || presetTheme.accentColor,
                            navbarBg: presetTheme.navbarBg || presetTheme.accentColor,
                            tabBarBg: presetTheme.tabBarBg || presetTheme.accentColor,
                            dropdownColor: presetTheme.dropdownColor || presetTheme.primaryColor || presetTheme.accentColor,
                            dropdownOpacity: presetTheme.dropdownOpacity ?? 95,
                            dropdownButtonOpacity: presetTheme.dropdownButtonOpacity ?? 90,
                            cardBg: presetTheme.cardBg,
                            cardGradientEnabled: presetTheme.cardGradientEnabled ?? false,
                            cardGradientAngle: presetTheme.cardGradientAngle ?? 135,
                            cardGradientColor1: presetTheme.cardGradientColor1 || presetTheme.primaryColor || presetTheme.accentColor,
                            cardGradientColor2: presetTheme.cardGradientColor2 || presetTheme.secondaryColor || presetTheme.accentColor,
                            bubbleColor: presetTheme.accentColor,
                            turtleColor: presetTheme.cardGradientColor2 || presetTheme.secondaryColor || presetTheme.accentColor,
                            profileFrameColor: presetTheme.accentColor,
                            coralColors: [
                              presetTheme.accentColor,
                              presetTheme.secondaryColor || presetTheme.accentColor,
                              presetTheme.cardGradientColor2 || presetTheme.accentColor,
                              presetTheme.cardGradientColor1 || presetTheme.accentColor,
                              presetTheme.primaryColor || presetTheme.accentColor
                            ],
                            gradientEnabled: presetTheme.appBg?.includes('gradient') || false,
                            presetTheme: preset,
                          });
                          setAnimations(getThemeAnimations(preset));
                          if (setBaseTheme) {
                            setBaseTheme(preset);
                          }
                        }}
                        className="w-full p-3 hover:opacity-80 transition-all text-sm"
                        style={{ color: localTheme.textColor, border: 'none', background: 'transparent' }}
                      >
                        {PRESET_THEMES[preset]?.name || preset}
                      </button>
                    </div>
                  ))}

                  {/* Custom Preset Slots */}
                  {['custom1', 'custom2', 'custom3'].map((slot, index) => (
                    <div
                      key={slot}
                      style={{
                        overflow: 'hidden',
                        borderRadius: '8px',
                        background: themeColors.accentColor,
                        border: `2px solid ${themeColors.accentColor}`
                      }}
                    >
                      <button
                        onClick={() => {
                          setSavingToSlot(slot);
                          if (customPresets[slot]) {
                            setPresetName(customPresets[slot].name);
                          } else {
                            setPresetName('');
                          }
                          setShowPresetModal(true);
                        }}
                        className="w-full p-3 hover:opacity-80 transition-all text-sm"
                        style={{ color: localTheme.textColor, border: 'none', background: 'transparent' }}
                      >
                        {customPresets[slot]?.name || `Custom #${index + 1}`}
                      </button>
                    </div>
                  ))}
                </div>
              </div>
            )}

            {/* SECTION 3: Animations & Effects (adjust ID based on scope) */}
            {activeSection === ((scopeName === 'profile' || scopeName === 'community') && showProfileOptions ? 3 : 2) && (
              <div className="space-y-8">
                <h4 className="font-semibold text-2xl mb-8" style={{ color: localTheme.textColor }}>
                  Animations & Effects
                </h4>

                {/* Master toggle */}
                <div className="mb-4">
                  <div className="flex items-center justify-between p-3 rounded-lg" style={{ background: 'rgba(255, 255, 255, 0.05)' }}>
                    <span className="text-sm font-semibold" style={{ color: localTheme.textColor }}>
                      Enable All Animations
                    </span>
                    <div style={{
                      overflow: 'hidden',
                      borderRadius: '8px',
                      background: animationsEnabled ? '#10b981' : '#6b7280',
                      marginRight: '0.5rem'
                    }}>
                      <button
                        onClick={() => setAnimationsEnabled(!animationsEnabled)}
                        className="px-4 py-2 transition-all font-semibold"
                        style={{ color: '#fff', border: 'none', background: 'transparent' }}
                      >
                        {animationsEnabled ? 'ON' : 'OFF'}
                      </button>
                    </div>
                  </div>
                </div>

                {/* Individual controls */}
                {animationsEnabled && (
                  <>
                    {/* Floating Characters & Decorations */}
                    <div className="mb-4">
                      <h5 className="text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                        Floating Characters & Decorations
                      </h5>
                      <div className="space-y-2">
                        {/* Characters */}
                        {animations.characters.map(anim => (
                          <div key={anim.id} className="p-3 rounded-lg" style={{ background: 'rgba(255, 255, 255, 0.05)' }}>
                            <div className="flex items-center justify-between mb-2">
                              <span className="text-sm capitalize" style={{ color: localTheme.textColor }}>
                                {anim.id}
                              </span>
                              <div style={{
                                overflow: 'hidden',
                                borderRadius: '4px',
                                background: anim.enabled ? '#10b981' : '#6b7280',
                                marginRight: '0.5rem'
                              }}>
                                <button
                                  onClick={() => toggleAnimation('characters', anim.id)}
                                  className="px-3 py-1 text-xs font-semibold"
                                  style={{ color: '#fff', border: 'none', background: 'transparent' }}
                                >
                                  {anim.enabled ? 'Visible' : 'Hidden'}
                                </button>
                              </div>
                            </div>
                            {anim.enabled && (
                              <div className="mt-2">
                                <label className="text-xs" style={{ color: localTheme.textColor }}>
                                  Speed: {anim.speed}x
                                </label>
                                <input
                                  type="range"
                                  min="0.5"
                                  max="3"
                                  step="0.1"
                                  value={anim.speed}
                                  onChange={(e) => updateAnimationSpeed('characters', anim.id, parseFloat(e.target.value))}
                                  className="w-full"
                                />
                              </div>
                            )}
                          </div>
                        ))}

                        {/* Decorations */}
                        {animations.decorations.map(anim => (
                          <div key={anim.id} className="p-3 rounded-lg" style={{ background: 'rgba(255, 255, 255, 0.05)' }}>
                            <div className="flex items-center justify-between mb-2">
                              <span className="text-sm capitalize" style={{ color: localTheme.textColor }}>
                                {anim.id}
                              </span>
                              <div style={{
                                overflow: 'hidden',
                                borderRadius: '4px',
                                background: anim.enabled ? '#10b981' : '#6b7280',
                                marginRight: '0.5rem'
                              }}>
                                <button
                                  onClick={() => toggleAnimation('decorations', anim.id)}
                                  className="px-3 py-1 text-xs font-semibold"
                                  style={{ color: '#fff', border: 'none', background: 'transparent' }}
                                >
                                  {anim.enabled ? 'Visible' : 'Hidden'}
                                </button>
                              </div>
                            </div>
                            {anim.enabled && (
                              <div className="mt-2">
                                <label className="text-xs" style={{ color: localTheme.textColor }}>
                                  Speed: {anim.speed}x
                                </label>
                                <input
                                  type="range"
                                  min="0.5"
                                  max="3"
                                  step="0.1"
                                  value={anim.speed}
                                  onChange={(e) => updateAnimationSpeed('decorations', anim.id, parseFloat(e.target.value))}
                                  className="w-full"
                                />
                              </div>
                            )}
                          </div>
                        ))}
                      </div>
                    </div>

                    {/* Bubbles toggle */}
                    {animations.bubbles && (
                      <div className="mb-4">
                        <div className="p-3 rounded-lg" style={{ background: 'rgba(255, 255, 255, 0.05)' }}>
                          <div className="flex items-center justify-between mb-3">
                            <span className="text-sm font-semibold" style={{ color: localTheme.textColor }}>
                              Bubbles Effect
                            </span>
                            <div style={{
                              overflow: 'hidden',
                              borderRadius: '4px',
                              background: animations.bubbles.enabled ? '#10b981' : '#6b7280',
                              marginRight: '0.5rem'
                            }}>
                              <button
                                onClick={toggleBubbles}
                                className="px-3 py-1 text-xs font-semibold"
                                style={{ color: '#fff', border: 'none', background: 'transparent' }}
                              >
                                {animations.bubbles.enabled ? 'Visible' : 'Hidden'}
                              </button>
                            </div>
                          </div>

                          {/* Bubbles customization controls */}
                          {animations.bubbles.enabled && (
                            <div className="space-y-3 mt-3 pt-3" style={{ borderTop: '1px solid rgba(255,255,255,0.1)' }}>
                              <div>
                                <div className="flex items-center justify-between mb-1">
                                  <span className="text-xs" style={{ color: localTheme.textColor + 'cc' }}>
                                    Count
                                  </span>
                                  <span className="text-xs" style={{ color: localTheme.textColor + 'cc' }}>
                                    {animations.bubbles.count}
                                  </span>
                                </div>
                                <input
                                  type="range"
                                  min="5"
                                  max="50"
                                  step="1"
                                  value={animations.bubbles.count}
                                  onChange={(e) => updateBubblesCount(parseInt(e.target.value))}
                                  className="w-full"
                                />
                              </div>

                              <div>
                                <div className="flex items-center justify-between mb-1">
                                  <span className="text-xs" style={{ color: localTheme.textColor + 'cc' }}>
                                    Speed
                                  </span>
                                  <span className="text-xs" style={{ color: localTheme.textColor + 'cc' }}>
                                    {animations.bubbles.speed.toFixed(1)}x
                                  </span>
                                </div>
                                <input
                                  type="range"
                                  min="0.3"
                                  max="3"
                                  step="0.1"
                                  value={animations.bubbles.speed}
                                  onChange={(e) => updateBubblesSpeed(parseFloat(e.target.value))}
                                  className="w-full"
                                />
                              </div>

                              <div>
                                <div className="flex items-center justify-between mb-1">
                                  <span className="text-xs" style={{ color: localTheme.textColor + 'cc' }}>
                                    Min Size
                                  </span>
                                  <span className="text-xs" style={{ color: localTheme.textColor + 'cc' }}>
                                    {animations.bubbles.size.min}px
                                  </span>
                                </div>
                                <input
                                  type="range"
                                  min="2"
                                  max="20"
                                  step="1"
                                  value={animations.bubbles.size.min}
                                  onChange={(e) => updateBubblesSize(parseInt(e.target.value), animations.bubbles!.size.max)}
                                  className="w-full"
                                />
                              </div>

                              <div>
                                <div className="flex items-center justify-between mb-1">
                                  <span className="text-xs" style={{ color: localTheme.textColor + 'cc' }}>
                                    Max Size
                                  </span>
                                  <span className="text-xs" style={{ color: localTheme.textColor + 'cc' }}>
                                    {animations.bubbles.size.max}px
                                  </span>
                                </div>
                                <input
                                  type="range"
                                  min="5"
                                  max="40"
                                  step="1"
                                  value={animations.bubbles.size.max}
                                  onChange={(e) => updateBubblesSize(animations.bubbles!.size.min, parseInt(e.target.value))}
                                  className="w-full"
                                />
                              </div>
                            </div>
                          )}
                        </div>
                      </div>
                    )}

                    {/* Focus Animation */}
                    <div className="mb-4">
                      <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                        Focus Animation
                      </label>
                      <button
                        onClick={() => setCustomization({...customization, focusAnimationEnabled: !customization.focusAnimationEnabled})}
                        className="w-full p-3 rounded-lg transition-all"
                        style={{
                          backgroundColor: customization.focusAnimationEnabled ? localTheme.accentColor : 'rgba(255, 255, 255, 0.1)',
                          color: localTheme.textColor
                        }}
                      >
                        {customization.focusAnimationEnabled ? 'Enabled' : 'Disabled'}
                      </button>
                      <p className="text-xs mt-2" style={{ color: localTheme.textColor, opacity: 0.7 }}>
                        Scale up focused post when scrolling
                      </p>
                    </div>
                  </>
                )}
              </div>
            )}

            {/* SECTION 4: Profile Card Design (only for profile/community with showProfileOptions) */}
            {activeSection === 4 && (scopeName === 'profile' || scopeName === 'community') && showProfileOptions && (
              <div className="space-y-8">
                <h4 className="font-semibold text-2xl mb-8" style={{ color: localTheme.textColor }}>
                  Profile Card Design
                </h4>

                {/* Profile Frame Shape */}
                <div>
                  <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                    Profile Frame Shape
                  </label>
                  <div className="flex gap-2">
                    <button
                      onClick={() => setCustomization({...customization, profileFrameShape: 'square'})}
                      className="flex-1 p-3 rounded-lg transition-all flex items-center justify-center"
                      style={{
                        backgroundColor: customization.profileFrameShape === 'square'
                          ? localTheme.accentColor
                          : 'rgba(255, 255, 255, 0.1)',
                        color: localTheme.textColor
                      }}
                    >
                      <Square size={20} className="mr-2" /> Square
                    </button>
                    <button
                      onClick={() => setCustomization({...customization, profileFrameShape: 'circle'})}
                      className="flex-1 p-3 rounded-lg transition-all flex items-center justify-center"
                      style={{
                        backgroundColor: customization.profileFrameShape === 'circle'
                          ? localTheme.accentColor
                          : 'rgba(255, 255, 255, 0.1)',
                        color: localTheme.textColor
                      }}
                    >
                      <Circle size={20} className="mr-2" /> Circle
                    </button>
                  </div>
                </div>

                {/* Profile Frame Color */}
                <div>
                  <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                    <Palette size={16} className="inline mr-2" />
                    Profile Frame Color
                  </label>
                  <input
                    type="color"
                    value={customization.profileFrameColor}
                    onChange={(e) => setCustomization({...customization, profileFrameColor: e.target.value})}
                    className="w-full h-10 rounded cursor-pointer"
                  />
                </div>

                {/* Profile Frame Decoration */}
                <div>
                  <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                    Profile Frame Decoration
                  </label>
                  <div className="flex gap-2">
                    {[
                      { value: 'coral' as const, label: 'Coral' },
                      { value: 'waves' as const, label: 'Waves' },
                      { value: 'bubbles' as const, label: 'Bubbles' }
                    ].map(({ value, label }) => (
                      <button
                        key={value}
                        onClick={() => setCustomization({...customization, profileFrameDesign: value})}
                        className="flex-1 p-3 rounded-lg transition-all flex items-center justify-center"
                        style={{
                          backgroundColor: customization.profileFrameDesign === value
                            ? localTheme.accentColor
                            : 'rgba(255, 255, 255, 0.1)',
                          color: localTheme.textColor
                        }}
                      >
                        <span className="text-sm font-semibold">{label}</span>
                      </button>
                    ))}
                  </div>
                </div>

                {/* Profile Picture Size */}
                <div>
                  <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                    Profile Picture Size
                  </label>
                  <div className="flex gap-2">
                    {[
                      { value: 'small' as const, label: 'Small' },
                      { value: 'medium' as const, label: 'Medium' },
                      { value: 'large' as const, label: 'Large' }
                    ].map(({ value, label }) => (
                      <button
                        key={value}
                        onClick={() => setCustomization({...customization, profilePictureSize: value})}
                        className="flex-1 p-3 rounded-lg transition-all flex items-center justify-center"
                        style={{
                          backgroundColor: customization.profilePictureSize === value
                            ? localTheme.accentColor
                            : 'rgba(255, 255, 255, 0.1)',
                          color: localTheme.textColor
                        }}
                      >
                        <span className="text-sm font-semibold">{label}</span>
                      </button>
                    ))}
                  </div>
                </div>
              </div>
            )}

            {/* SECTION 5: Card Styling (adjust ID based on scope) */}
            {activeSection === ((scopeName === 'profile' || scopeName === 'community') && showProfileOptions ? 5 : 3) && (
              <div className="space-y-8">
                <h4 className="font-semibold text-2xl mb-8" style={{ color: localTheme.textColor }}>
                  Card Styling
                </h4>

                {/* Card Background Toggle */}
                <div>
                  <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                    Enable Card Gradient
                  </label>
                  <button
                    onClick={() => setCustomization({...customization, cardGradientEnabled: !customization.cardGradientEnabled})}
                    className="w-full p-3 rounded-lg transition-all"
                    style={{
                      backgroundColor: customization.cardGradientEnabled ? localTheme.accentColor : 'rgba(255, 255, 255, 0.1)',
                      color: localTheme.textColor
                    }}
                  >
                    {customization.cardGradientEnabled ? 'Enabled' : 'Disabled'}
                  </button>
                </div>

                {/* Card Primary Color */}
                <div>
                  <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                    <Palette size={16} className="inline mr-2" />
                    Card Primary Color
                  </label>
                  <input
                    type="color"
                    value={customization.cardGradientColor1}
                    onChange={(e) => setCustomization({...customization, cardGradientColor1: e.target.value})}
                    className="w-full h-10 rounded cursor-pointer"
                  />
                </div>

                {/* Card Gradient Settings */}
                {customization.cardGradientEnabled && (
                  <>
                    <div>
                      <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                        <Palette size={16} className="inline mr-2" />
                        Card Gradient Color 2
                      </label>
                      <input
                        type="color"
                        value={customization.cardGradientColor2}
                        onChange={(e) => setCustomization({...customization, cardGradientColor2: e.target.value})}
                        className="w-full h-10 rounded cursor-pointer"
                      />
                    </div>

                    <div>
                      <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                        Color Balance: {customization.cardGradientFavorability}% (Primary ← → Secondary)
                      </label>
                      <input
                        type="range"
                        min="0"
                        max="100"
                        value={customization.cardGradientFavorability}
                        onChange={(e) => setCustomization({...customization, cardGradientFavorability: parseInt(e.target.value)})}
                        className="w-full"
                      />
                    </div>

                    <div>
                      <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                        Gradient Strength: {customization.cardGradientStrength}%
                      </label>
                      <input
                        type="range"
                        min="0"
                        max="100"
                        step="10"
                        value={customization.cardGradientStrength}
                        onChange={(e) => setCustomization({...customization, cardGradientStrength: parseInt(e.target.value)})}
                        className="w-full"
                      />
                      <p className="text-xs mt-1 opacity-75" style={{ color: localTheme.textColor }}>
                        Controls color blending (0% = solid color, 100% = full gradient)
                      </p>
                    </div>

                    <div>
                      <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                        Card Gradient Angle: {customization.cardGradientAngle}°
                      </label>
                      <input
                        type="range"
                        min="0"
                        max="360"
                        value={customization.cardGradientAngle}
                        onChange={(e) => setCustomization({...customization, cardGradientAngle: parseInt(e.target.value)})}
                        className="w-full"
                      />
                    </div>
                  </>
                )}

                {/* Card Opacity */}
                <div>
                  <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                    Opacity: {customization.cardOpacity}%
                  </label>
                  <input
                    type="range"
                    min="0"
                    max="100"
                    step="5"
                    value={customization.cardOpacity}
                    onChange={(e) => setCustomization({...customization, cardOpacity: parseInt(e.target.value)})}
                    className="w-full"
                  />
                </div>
              </div>
            )}

            {/* SECTION 6: Background (adjust ID based on scope) */}
            {activeSection === ((scopeName === 'profile' || scopeName === 'community') && showProfileOptions ? 6 : 4) && (
              <div className="space-y-8">
                <h4 className="font-semibold text-2xl mb-8" style={{ color: localTheme.textColor }}>
                  Background
                </h4>

                {/* Primary Color */}
                <div>
                  <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                    <Palette size={16} className="inline mr-2" />
                    Primary Color (Gradient Start)
                  </label>
                  <input
                    type="color"
                    value={customization.primaryColor}
                    onChange={(e) => setCustomization({...customization, primaryColor: e.target.value})}
                    className="w-full h-10 rounded cursor-pointer"
                  />
                </div>

                {/* Secondary Color */}
                <div>
                  <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                    <Palette size={16} className="inline mr-2" />
                    Secondary Color (Gradient End)
                  </label>
                  <input
                    type="color"
                    value={customization.secondaryColor}
                    onChange={(e) => setCustomization({...customization, secondaryColor: e.target.value})}
                    className="w-full h-10 rounded cursor-pointer"
                  />
                </div>

                {/* Enable Background Gradient */}
                <div>
                  <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                    Enable Background Gradient
                  </label>
                  <button
                    onClick={() => setCustomization({...customization, gradientEnabled: !customization.gradientEnabled})}
                    className="w-full p-3 rounded-lg transition-all"
                    style={{
                      backgroundColor: customization.gradientEnabled ? localTheme.accentColor : 'rgba(255, 255, 255, 0.1)',
                      color: localTheme.textColor
                    }}
                  >
                    {customization.gradientEnabled ? 'Enabled' : 'Disabled'}
                  </button>
                </div>

                {/* Gradient Settings */}
                {customization.gradientEnabled && (
                  <>
                    <div>
                      <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                        Color Balance: {customization.gradientFavorability}% (Primary ← → Secondary)
                      </label>
                      <input
                        type="range"
                        min="0"
                        max="100"
                        value={customization.gradientFavorability}
                        onChange={(e) => setCustomization({...customization, gradientFavorability: parseInt(e.target.value)})}
                        className="w-full"
                      />
                    </div>

                    <div>
                      <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                        Gradient Angle: {customization.gradientAngle}°
                      </label>
                      <input
                        type="range"
                        min="0"
                        max="360"
                        value={customization.gradientAngle}
                        onChange={(e) => setCustomization({...customization, gradientAngle: parseInt(e.target.value)})}
                        className="w-full"
                      />
                    </div>

                    <div>
                      <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                        Gradient Strength: {customization.gradientStrength}%
                      </label>
                      <input
                        type="range"
                        min="0"
                        max="100"
                        step="10"
                        value={customization.gradientStrength}
                        onChange={(e) => setCustomization({...customization, gradientStrength: parseInt(e.target.value)})}
                        className="w-full"
                      />
                      <p className="text-xs mt-1 opacity-75" style={{ color: localTheme.textColor }}>
                        Controls color blending (0% = solid color, 100% = full gradient)
                      </p>
                    </div>
                  </>
                )}

              </div>
            )}

            {/* SECTION 7: Navigation & Text Colors (adjust ID based on scope) */}
            {activeSection === ((scopeName === 'profile' || scopeName === 'community') && showProfileOptions ? 7 : 5) && (
              <div className="space-y-8">
                <h4 className="font-semibold text-2xl mb-8" style={{ color: localTheme.textColor }}>
                  Navigation & Text Colors
                </h4>

                {/* Text Color */}
                <div>
                  <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                    <Palette size={16} className="inline mr-2" />
                    Text Color {scopeName === 'site' ? '(Site-wide)' : ''}
                  </label>
                  <input
                    type="color"
                    value={customization.textColor}
                    onChange={(e) => setCustomization({...customization, textColor: e.target.value})}
                    className="w-full h-10 rounded cursor-pointer"
                  />
                </div>

                {/* Accent Color */}
                <div>
                  <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                    <Palette size={16} className="inline mr-2" />
                    Accent Color {scopeName === 'site' ? '(Site-wide)' : '(Buttons, Links, Borders)'}
                  </label>
                  <input
                    type="color"
                    value={customization.accentColor}
                    onChange={(e) => setCustomization({...customization, accentColor: e.target.value})}
                    className="w-full h-10 rounded cursor-pointer"
                  />
                </div>

                {/* Navbar Background */}
                <div>
                  <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                    <Palette size={16} className="inline mr-2" />
                    Top Navbar Background
                  </label>
                  <input
                    type="color"
                    value={rgbaToHex(customization.navbarBg)}
                    onChange={(e) => setCustomization({...customization, navbarBg: e.target.value})}
                    className="w-full h-10 rounded cursor-pointer"
                  />
                </div>

                {/* Navbar Opacity */}
                <div>
                  <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                    Navbar Opacity: {customization.navbarOpacity}%
                  </label>
                  <input
                    type="range"
                    min="0"
                    max="100"
                    step="5"
                    value={customization.navbarOpacity}
                    onChange={(e) => setCustomization({...customization, navbarOpacity: parseInt(e.target.value)})}
                    className="w-full"
                  />
                </div>

                {/* Tab Bar Background */}
                <div>
                  <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                    <Palette size={16} className="inline mr-2" />
                    Tab Bar Background
                  </label>
                  <input
                    type="color"
                    value={rgbaToHex(customization.tabBarBg)}
                    onChange={(e) => setCustomization({...customization, tabBarBg: e.target.value})}
                    className="w-full h-10 rounded cursor-pointer"
                  />
                </div>

                {/* Tab Bar Opacity */}
                <div>
                  <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                    Tab Bar Opacity: {customization.tabBarOpacity ?? 88}%
                  </label>
                  <input
                    type="range"
                    min="0"
                    max="100"
                    step="5"
                    value={customization.tabBarOpacity ?? 88}
                    onChange={(e) => setCustomization({...customization, tabBarOpacity: parseInt(e.target.value)})}
                    className="w-full"
                  />
                </div>

                {/* Dropdown Color */}
                <div>
                  <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                    <Palette size={16} className="inline mr-2" />
                    Dropdown Color
                  </label>
                  <input
                    type="color"
                    value={rgbaToHex(customization.dropdownColor || customization.primaryColor)}
                    onChange={(e) => setCustomization({...customization, dropdownColor: e.target.value})}
                    className="w-full h-10 rounded cursor-pointer"
                  />
                </div>

                {/* Dropdown Button Opacity */}
                <div>
                  <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                    Dropdown Button Opacity: {customization.dropdownButtonOpacity ?? 100}%
                  </label>
                  <input
                    type="range"
                    min="0"
                    max="100"
                    step="5"
                    value={customization.dropdownButtonOpacity ?? 100}
                    onChange={(e) => setCustomization({...customization, dropdownButtonOpacity: parseInt(e.target.value)})}
                    className="w-full"
                  />
                </div>

                {/* Dropdown Menu Opacity */}
                <div>
                  <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                    Dropdown Menu Opacity: {customization.dropdownOpacity ?? 95}%
                  </label>
                  <input
                    type="range"
                    min="70"
                    max="100"
                    step="5"
                    value={customization.dropdownOpacity ?? 95}
                    onChange={(e) => setCustomization({...customization, dropdownOpacity: parseInt(e.target.value)})}
                    className="w-full"
                  />
                </div>

                {/* Highlight Color */}
                <div>
                  <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                    <Palette size={16} className="inline mr-2" />
                    Highlight Color (Input focus, dropdowns)
                  </label>
                  <input
                    type="color"
                    value={customization.highlightColor || localTheme.accentColor}
                    onChange={(e) => setCustomization({...customization, highlightColor: e.target.value})}
                    className="w-full h-10 rounded cursor-pointer"
                  />
                </div>
              </div>
            )}

            {/* SECTION 8: Alignments (only for profile/community with showProfileOptions) */}
            {activeSection === 8 && (scopeName === 'profile' || scopeName === 'community') && showProfileOptions && (
              <div className="space-y-8">
                <h4 className="font-semibold text-2xl mb-8" style={{ color: localTheme.textColor }}>
                  Alignments
                </h4>

                <p className="text-sm opacity-75 mb-8" style={{ color: localTheme.textColor }}>
                  Control the alignment and positioning of profile elements, content, and navigation.
                </p>

                {/* Name Alignment */}
                <div>
                  <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                    Name Alignment
                  </label>
                  <div className="flex gap-2">
                    {[
                      { value: 'left' as const, icon: AlignLeft, label: 'Left' },
                      { value: 'center' as const, icon: AlignCenter, label: 'Center' },
                      { value: 'right' as const, icon: AlignRight, label: 'Right' }
                    ].map(({ value, icon: Icon, label }) => (
                      <button
                        key={value}
                        onClick={() => setCustomization({...customization, profileNameAlign: value})}
                        className="flex-1 p-3 rounded-lg transition-all flex flex-col items-center justify-center"
                        style={{
                          backgroundColor: customization.profileNameAlign === value
                            ? localTheme.accentColor
                            : 'rgba(255, 255, 255, 0.1)',
                          color: localTheme.textColor
                        }}
                      >
                        <Icon size={20} className="mb-1" />
                        <span className="text-xs">{label}</span>
                      </button>
                    ))}
                  </div>
                </div>

                {/* Title Alignment */}
                <div>
                  <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                    Title Alignment
                  </label>
                  <div className="flex gap-2">
                    {[
                      { value: 'left' as const, icon: AlignLeft, label: 'Left' },
                      { value: 'center' as const, icon: AlignCenter, label: 'Center' },
                      { value: 'right' as const, icon: AlignRight, label: 'Right' }
                    ].map(({ value, icon: Icon, label }) => (
                      <button
                        key={value}
                        onClick={() => setCustomization({...customization, profileTitleAlign: value})}
                        className="flex-1 p-3 rounded-lg transition-all flex flex-col items-center justify-center"
                        style={{
                          backgroundColor: customization.profileTitleAlign === value
                            ? localTheme.accentColor
                            : 'rgba(255, 255, 255, 0.1)',
                          color: localTheme.textColor
                        }}
                      >
                        <Icon size={20} className="mb-1" />
                        <span className="text-xs">{label}</span>
                      </button>
                    ))}
                  </div>
                </div>

                {/* Bio Alignment */}
                <div>
                  <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                    Bio Alignment
                  </label>
                  <div className="flex gap-2">
                    {[
                      { value: 'left' as const, icon: AlignLeft, label: 'Left' },
                      { value: 'center' as const, icon: AlignCenter, label: 'Center' },
                      { value: 'right' as const, icon: AlignRight, label: 'Right' }
                    ].map(({ value, icon: Icon, label }) => (
                      <button
                        key={value}
                        onClick={() => setCustomization({...customization, profileBioAlign: value})}
                        className="flex-1 p-3 rounded-lg transition-all flex flex-col items-center justify-center"
                        style={{
                          backgroundColor: customization.profileBioAlign === value
                            ? localTheme.accentColor
                            : 'rgba(255, 255, 255, 0.1)',
                          color: localTheme.textColor
                        }}
                      >
                        <Icon size={20} className="mb-1" />
                        <span className="text-xs">{label}</span>
                      </button>
                    ))}
                  </div>
                </div>

                {/* Card Position */}
                <div>
                  <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                    Card Position
                  </label>
                  <div className="flex gap-2">
                    {[
                      { value: 'left' as const, icon: AlignLeft, label: 'Left' },
                      { value: 'center' as const, icon: AlignCenter, label: 'Center' },
                      { value: 'right' as const, icon: AlignRight, label: 'Right' }
                    ].map(({ value, icon: Icon, label }) => (
                      <button
                        key={value}
                        onClick={() => setCustomization({...customization, cardPosition: value})}
                        className="flex-1 p-3 rounded-lg transition-all flex flex-col items-center justify-center"
                        style={{
                          backgroundColor: customization.cardPosition === value
                            ? localTheme.accentColor
                            : 'rgba(255, 255, 255, 0.1)',
                          color: localTheme.textColor
                        }}
                      >
                        <Icon size={20} className="mb-1" />
                        <span className="text-xs">{label}</span>
                      </button>
                    ))}
                  </div>
                </div>

                {/* Profile Picture Position */}
                <div>
                  <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                    Profile Picture Position
                  </label>
                  <div className="flex gap-2">
                    {[
                      { value: 'left' as const, icon: AlignLeft, label: 'Left' },
                      { value: 'right' as const, icon: AlignRight, label: 'Right' }
                    ].map(({ value, icon: Icon, label }) => (
                      <button
                        key={value}
                        onClick={() => setCustomization({...customization, profilePicturePosition: value})}
                        className="flex-1 p-3 rounded-lg transition-all flex flex-col items-center justify-center"
                        style={{
                          backgroundColor: customization.profilePicturePosition === value
                            ? localTheme.accentColor
                            : 'rgba(255, 255, 255, 0.1)',
                          color: localTheme.textColor
                        }}
                      >
                        <Icon size={20} className="mb-1" />
                        <span className="text-xs">{label}</span>
                      </button>
                    ))}
                  </div>
                </div>

                {/* Tab Alignment */}
                <div>
                  <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                    Tab Bar Alignment
                  </label>
                  <div className="flex gap-2">
                    {[
                      { value: 'left' as const, icon: AlignLeft, label: 'Left' },
                      { value: 'center' as const, icon: AlignCenter, label: 'Center' },
                      { value: 'right' as const, icon: AlignRight, label: 'Right' }
                    ].map(({ value, icon: Icon, label }) => (
                      <button
                        key={value}
                        onClick={() => setCustomization({...customization, tabAlignment: value})}
                        className="flex-1 p-3 rounded-lg transition-all flex flex-col items-center justify-center"
                        style={{
                          backgroundColor: customization.tabAlignment === value
                            ? localTheme.accentColor
                            : 'rgba(255, 255, 255, 0.1)',
                          color: localTheme.textColor
                        }}
                      >
                        <Icon size={20} className="mb-1" />
                        <span className="text-xs">{label}</span>
                      </button>
                    ))}
                  </div>
                </div>

                {/* Posts Alignment */}
                <div>
                  <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                    Posts Alignment
                  </label>
                  <div className="flex gap-2">
                    {[
                      { value: 'left' as const, icon: AlignLeft, label: 'Left' },
                      { value: 'center' as const, icon: AlignCenter, label: 'Center' },
                      { value: 'right' as const, icon: AlignRight, label: 'Right' }
                    ].map(({ value, icon: Icon, label }) => {
                      const isConflicting = (customization.cardPosition === 'right' && value === 'left') || (customization.cardPosition === 'left' && value === 'right');
                      const isSelected = customization.postsAlignment === value;

                      return (
                        <div
                          key={value}
                          onClick={(e) => {
                            e.preventDefault();
                            e.stopPropagation();
                            if (!isConflicting) {
                              setCustomization({...customization, postsAlignment: value});
                            }
                          }}
                            className="flex-1 p-3 rounded-lg flex flex-col items-center justify-center select-none"
                            style={{
                              backgroundColor: isSelected
                                ? localTheme.accentColor
                                : isConflicting
                                ? '#dc2626'
                                : 'rgba(255, 255, 255, 0.1)',
                              color: localTheme.textColor,
                              cursor: isConflicting ? 'not-allowed' : 'pointer',
                              opacity: isConflicting ? 0.7 : 1,
                              pointerEvents: isConflicting ? 'none' : 'auto'
                            }}
                          >
                            <Icon size={20} className="mb-1" />
                            <span className="text-xs">{label}</span>
                          </div>
                        );
                      })}
                    </div>
                    <p className="text-xs mt-2" style={{ color: localTheme.textColor, opacity: 0.7 }}>
                      Red buttons cannot be selected (card right = no left posts, card left = no right posts)
                    </p>
                  </div>
              </div>
            )}

            {/* SECTION 9: Advanced Controls (adjust ID based on scope) */}
            {activeSection === ((scopeName === 'profile' || scopeName === 'community') && showProfileOptions ? 9 : 6) && (
              <div className="space-y-8">
                <h4 className="font-semibold text-2xl mb-8" style={{ color: localTheme.textColor }}>
                  Advanced Controls
                </h4>

                <p className="text-sm opacity-75 mb-8" style={{ color: localTheme.textColor }}>
                  These controls affect the overall color processing across the entire theme including backgrounds, cards, and all UI elements.
                </p>

                {/* Color Space Selector */}
                <div>
                  <label className="block text-sm font-semibold mb-3" style={{ color: localTheme.textColor }}>
                    Color Space
                  </label>
                  <div className="flex gap-2">
                    {(['Off', 'RGB', 'HSV', 'TMI'] as const).map((space) => (
                      <button
                        key={space}
                        onClick={() => {
                          wrappedSetCustomization({...customization, colorSpace: space});
                        }}
                        className={`flex-1 py-2 px-4 rounded-lg transition-all ${
                          (customization.colorSpace || 'Off') === space
                            ? 'font-semibold'
                            : 'opacity-60 hover:opacity-80'
                        }`}
                        style={{
                          background: (customization.colorSpace || 'Off') === space
                            ? customization.accentColor
                            : 'rgba(255, 255, 255, 0.1)', // Semi-transparent for visibility
                          color: localTheme.textColor, // Always use textColor for readability
                          border: `1px solid ${(customization.colorSpace || 'Off') === space
                            ? customization.accentColor
                            : 'rgba(255, 255, 255, 0.2)'}`,
                        }}
                      >
                        {space}
                        {space === 'TMI' && (
                          <span className="block text-xs opacity-75 mt-0.5">Recommended</span>
                        )}
                        {space === 'Off' && (
                          <span className="block text-xs opacity-75 mt-0.5">No Transform</span>
                        )}
                      </button>
                    ))}
                  </div>
                  <p className="text-xs opacity-60 mt-2" style={{ color: localTheme.textColor }}>
                    {(customization.colorSpace || 'Off') === 'Off' && 'Use colors exactly as selected - no transformations applied'}
                    {(customization.colorSpace || 'Off') === 'RGB' && 'Direct control over Red, Green, and Blue channels'}
                    {(customization.colorSpace || 'Off') === 'HSV' && 'Hue, Saturation, and Value for intuitive color control'}
                    {(customization.colorSpace || 'Off') === 'TMI' && 'Temperature, Magenta, and Intensity for perceptual calibration'}
                  </p>
                </div>

                {/* TMI Sliders */}
                {(customization.colorSpace || 'Off') === 'TMI' && (
                  <>
                    {/* Temperature */}
                    <div>
                      <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                        Temperature: {customization.temperatureLimit ?? 50}
                      </label>
                      <input
                        type="range"
                        min="0"
                        max="100"
                        value={customization.temperatureLimit ?? 50}
                        onChange={(e) => {
                          const value = parseInt(e.target.value);
                          wrappedSetCustomization({...customization, temperatureLimit: value});
                        }}
                        className="w-full h-8 rounded-lg appearance-none cursor-pointer"
                        style={{
                          background: 'linear-gradient(to right, #4A90E2, #B0B0B0, #FF8C42)',
                        }}
                      />
                      <p className="text-xs opacity-60 mt-1" style={{ color: localTheme.textColor }}>
                        Cool (blue) ← → Warm (orange)
                      </p>
                    </div>

                    {/* Magenta */}
                    <div>
                      <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                        Magenta: {customization.magentaLimit ?? 50}
                      </label>
                      <input
                        type="range"
                        min="0"
                        max="100"
                        value={customization.magentaLimit ?? 50}
                        onChange={(e) => {
                          const value = parseInt(e.target.value);
                          wrappedSetCustomization({...customization, magentaLimit: value});
                        }}
                        className="w-full h-8 rounded-lg appearance-none cursor-pointer"
                        style={{
                          background: 'linear-gradient(to right, #00FF00, #B0B0B0, #FF00FF)',
                        }}
                      />
                      <p className="text-xs opacity-60 mt-1" style={{ color: localTheme.textColor }}>
                        Green ← → Magenta
                      </p>
                    </div>

                    {/* Intensity */}
                    <div>
                      <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                        Intensity: {customization.intensityLimit ?? 50}
                      </label>
                      <input
                        type="range"
                        min="0"
                        max="100"
                        value={customization.intensityLimit ?? 50}
                        onChange={(e) => {
                          const value = parseInt(e.target.value);
                          wrappedSetCustomization({...customization, intensityLimit: value});
                        }}
                        className="w-full h-8 rounded-lg appearance-none cursor-pointer"
                        style={{
                          background: 'linear-gradient(to right, #000000, #FFFFFF)',
                        }}
                      />
                      <p className="text-xs opacity-60 mt-1" style={{ color: localTheme.textColor }}>
                        Dark ← → Bright
                      </p>
                    </div>
                  </>
                )}

                {/* HSV Sliders */}
                {customization.colorSpace === 'HSV' && (
                  <>
                    {/* Hue */}
                    <div>
                      <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                        Hue Shift
                      </label>
                      <input
                        type="range"
                        min="0"
                        max="360"
                        value={customization.backgroundDefaultHue ?? 184}
                        onChange={(e) => {
                          const value = parseInt(e.target.value);
                          setCustomization({...customization, backgroundDefaultHue: value});
                        }}
                        className="w-full h-8 rounded-lg appearance-none cursor-pointer"
                        style={{
                          background: 'linear-gradient(to right, #FF0000, #FFFF00, #00FF00, #00FFFF, #0000FF, #FF00FF, #FF0000)',
                        }}
                      />
                      <p className="text-xs opacity-60 mt-1" style={{ color: localTheme.textColor }}>
                        Current: {customization.backgroundDefaultHue ?? 184}°
                      </p>
                    </div>

                    {/* Saturation */}
                    <div>
                      <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                        Saturation: {customization.backgroundSaturationLimit ?? 50}
                      </label>
                      <input
                        type="range"
                        min="0"
                        max="100"
                        value={customization.backgroundSaturationLimit ?? 50}
                        onChange={(e) => {
                          const value = parseInt(e.target.value);
                          setCustomization({...customization, backgroundSaturationLimit: value});
                        }}
                        className="w-full h-8 rounded-lg appearance-none cursor-pointer"
                        style={{
                          background: 'linear-gradient(to right, #808080, #00CED1)',
                        }}
                      />
                      <p className="text-xs opacity-60 mt-1" style={{ color: localTheme.textColor }}>
                        Grayscale ← → Vivid
                      </p>
                    </div>

                    {/* Value (Brightness) */}
                    <div>
                      <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                        Value: {customization.backgroundBrightnessLimit ?? 50}
                      </label>
                      <input
                        type="range"
                        min="0"
                        max="100"
                        value={customization.backgroundBrightnessLimit ?? 50}
                        onChange={(e) => {
                          const value = parseInt(e.target.value);
                          setCustomization({...customization, backgroundBrightnessLimit: value});
                        }}
                        className="w-full h-8 rounded-lg appearance-none cursor-pointer"
                        style={{
                          background: 'linear-gradient(to right, #000000, #FFFFFF)',
                        }}
                      />
                      <p className="text-xs opacity-60 mt-1" style={{ color: localTheme.textColor }}>
                        Dark ← → Bright
                      </p>
                    </div>

                    {/* Contrast */}
                    <div>
                      <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                        Contrast: {customization.backgroundContrast ?? 50}
                      </label>
                      <input
                        type="range"
                        min="0"
                        max="100"
                        value={customization.backgroundContrast ?? 50}
                        onChange={(e) => {
                          const value = parseInt(e.target.value);
                          setCustomization({...customization, backgroundContrast: value});
                        }}
                        className="w-full h-8 rounded-lg appearance-none cursor-pointer"
                        style={{
                          background: 'linear-gradient(to right, #505050, #000000, #FFFFFF)',
                        }}
                      />
                    </div>

                    {/* Hue Gravity */}
                    <div>
                      <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                        Hue Gravity: {customization.backgroundHueGravity ?? 0}
                      </label>
                      <input
                        type="range"
                        min="0"
                        max="100"
                        value={customization.backgroundHueGravity ?? 0}
                        onChange={(e) => {
                          const value = parseInt(e.target.value);
                          wrappedSetCustomization({...customization, backgroundHueGravity: value});
                        }}
                        className="w-full"
                      />
                      <p className="text-xs opacity-60 mt-1" style={{ color: localTheme.textColor }}>
                        Pull colors toward target hue
                      </p>
                    </div>

                    {/* Gray Saturation */}
                    <div>
                      <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                        Gray Saturation: {customization.backgroundGraySaturation ?? 0}
                      </label>
                      <input
                        type="range"
                        min="0"
                        max="100"
                        value={customization.backgroundGraySaturation ?? 0}
                        onChange={(e) => {
                          const value = parseInt(e.target.value);
                          wrappedSetCustomization({...customization, backgroundGraySaturation: value});
                        }}
                        className="w-full"
                      />
                      <p className="text-xs opacity-60 mt-1" style={{ color: localTheme.textColor }}>
                        Add color tint to gray tones
                      </p>
                    </div>
                  </>
                )}

                {/* RGB Sliders */}
                {customization.colorSpace === 'RGB' && (
                  <>
                    {/* Red */}
                    <div>
                      <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                        Red: {customization.redLimit ?? 50}
                      </label>
                      <input
                        type="range"
                        min="0"
                        max="100"
                        value={customization.redLimit ?? 50}
                        onChange={(e) => {
                          const value = parseInt(e.target.value);
                          setCustomization({...customization, redLimit: value});
                        }}
                        className="w-full h-8 rounded-lg appearance-none cursor-pointer"
                        style={{
                          background: 'linear-gradient(to right, #000000, #FF0000)',
                        }}
                      />
                    </div>

                    {/* Green */}
                    <div>
                      <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                        Green: {customization.greenLimit ?? 50}
                      </label>
                      <input
                        type="range"
                        min="0"
                        max="100"
                        value={customization.greenLimit ?? 50}
                        onChange={(e) => {
                          const value = parseInt(e.target.value);
                          setCustomization({...customization, greenLimit: value});
                        }}
                        className="w-full h-8 rounded-lg appearance-none cursor-pointer"
                        style={{
                          background: 'linear-gradient(to right, #000000, #00FF00)',
                        }}
                      />
                    </div>

                    {/* Blue */}
                    <div>
                      <label className="block text-sm font-semibold mb-2" style={{ color: localTheme.textColor }}>
                        Blue: {customization.blueLimit ?? 50}
                      </label>
                      <input
                        type="range"
                        min="0"
                        max="100"
                        value={customization.blueLimit ?? 50}
                        onChange={(e) => {
                          const value = parseInt(e.target.value);
                          setCustomization({...customization, blueLimit: value});
                        }}
                        className="w-full h-8 rounded-lg appearance-none cursor-pointer"
                        style={{
                          background: 'linear-gradient(to right, #000000, #0000FF)',
                        }}
                      />
                    </div>
                  </>
                )}
              </div>
            )}
          </div>

          {/* Right Sidebar Navigation (18%) */}
          <div
            className="border-l overflow-y-auto"
            style={{
              width: '18%',
              borderColor: `${themeColors.accentColor}40`,
              background: 'rgba(0, 0, 0, 0.1)',
              padding: '24px 16px'
            }}
          >
            <div className="flex flex-col gap-4">
              {sections.map((section) => (
                <button
                  key={section.id}
                  onClick={() => setActiveSection(section.id)}
                  className="px-4 py-4 rounded-lg text-sm font-semibold transition-all text-center"
                  style={{
                    backgroundColor: activeSection === section.id
                      ? localTheme.accentColor
                      : 'rgba(255, 255, 255, 0.05)',
                    color: localTheme.textColor,
                    border: `1px solid ${activeSection === section.id ? localTheme.accentColor : 'transparent'}`
                  }}
                >
                  {section.label}
                </button>
              ))}
            </div>
          </div>
        </div>

        {/* Fixed Save Button at Bottom */}
        <div
          className="flex-shrink-0 border-t p-4"
          style={{
            borderColor: `${themeColors.accentColor}40`
          }}
        >
          <button
            type="button"
            onClick={onSave}
            className="w-full p-3 rounded-lg font-semibold transition-all"
            style={{
              backgroundColor: localTheme.accentColor,
              color: localTheme.textColor
            }}
          >
            Save & Apply
          </button>
        </div>
      </div>

      {/* Preset Save Modal */}
      {showPresetModal && (
        <div
          className="fixed inset-0 flex items-center justify-center z-[2000]"
          style={{ backgroundColor: 'rgba(0, 0, 0, 0.7)' }}
          onClick={() => setShowPresetModal(false)}
        >
          <div
            className="rounded-2xl p-8 shadow-2xl animate-scale-in"
            style={{
              background: getCustomizerCardBg(),
              border: `2px solid ${localTheme.accentColor}`,
              maxWidth: '400px',
              width: '90%'
            }}
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-2xl font-bold" style={{ color: localTheme.textColor }}>
                Save Custom Preset
              </h3>
              <button
                onClick={async () => {
                  if (savingToSlot && customPresets[savingToSlot]) {
                    setCustomization(customPresets[savingToSlot].settings);
                    setAnimations(customPresets[savingToSlot].settings.animations);
                    setAnimationsEnabled(customPresets[savingToSlot].settings.animationsEnabled);
                    setShowPresetModal(false);
                    setSavingToSlot(null);
                  }
                }}
                disabled={!savingToSlot || !customPresets[savingToSlot]}
                className="px-4 py-2 rounded-lg font-semibold transition-all disabled:opacity-50 disabled:cursor-not-allowed"
                style={{
                  backgroundColor: localTheme.accentColor,
                  color: localTheme.textColor,
                  marginRight: '0.5rem'
                }}
              >
                Apply
              </button>
            </div>
            <p className="mb-4 opacity-75" style={{ color: localTheme.textColor }}>
              Give your custom theme a name:
            </p>
            <input
              type="text"
              value={presetName}
              onChange={(e) => setPresetName(e.target.value)}
              placeholder="My Custom Theme"
              className="w-full p-3 rounded-lg mb-4"
              style={{
                background: 'rgba(255, 255, 255, 0.1)',
                border: `2px solid ${localTheme.accentColor}`,
                color: localTheme.textColor
              }}
              maxLength={30}
            />
            <div className="flex gap-2">
              <button
                onClick={async () => {
                  if (presetName.trim()) {
                    const slot = savingToSlot || `custom${Object.keys(customPresets).length + 1}`;
                    const updatedPresets = {
                      ...customPresets,
                      [slot]: {
                        name: presetName.trim(),
                        settings: {
                          ...customization,
                          animations,
                          animationsEnabled
                        }
                      }
                    };
                    setCustomPresets(updatedPresets);

                    try {
                      if (!userId) {
                        toast('User ID not available', {
                          description: 'Please try logging in again.',
                          duration: 4000
                        });
                        return;
                      }

                      const getThemeResponse = await fetch(`/api/users/${userId}/site-theme`, { credentials: 'include' });
                      if (getThemeResponse.ok) {
                        const currentTheme = await getThemeResponse.json();
                        const updatedCustomization = {
                          ...currentTheme.customization,
                          customPresets: updatedPresets
                        };
                        const saveResponse = await fetch(`/api/users/${userId}/site-theme`, {
                          method: 'PUT',
                          headers: { 'Content-Type': 'application/json' },
                          credentials: 'include',
                          body: JSON.stringify({
                            ...currentTheme,
                            customization: updatedCustomization
                          })
                        });

                        if (!saveResponse.ok) {
                          throw new Error('Failed to save preset');
                        }

                        toast('Preset saved successfully!', {
                          description: `Your custom theme "${presetName.trim()}" has been saved.`,
                          duration: 3000
                        });
                      } else {
                        throw new Error('Failed to load current theme');
                      }
                    } catch (error) {
                      toast('Failed to save preset', {
                        description: 'Please try again.',
                        duration: 4000
                      });
                      return;
                    }

                    setPresetName('');
                    setSavingToSlot(null);
                  }
                }}
                className="flex-1 p-3 rounded-lg font-semibold transition-all"
                style={{
                  backgroundColor: localTheme.accentColor,
                  color: localTheme.textColor
                }}
              >
                Save Preset
              </button>
              <button
                onClick={() => {
                  setPresetName('');
                  setShowPresetModal(false);
                  setSavingToSlot(null);
                }}
                className="flex-1 p-3 rounded-lg font-semibold transition-all"
                style={{
                  backgroundColor: 'rgba(255, 255, 255, 0.1)',
                  color: localTheme.textColor
                }}
              >
                Close
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
