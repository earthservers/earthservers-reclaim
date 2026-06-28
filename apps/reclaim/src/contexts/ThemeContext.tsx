// Theme Context for EarthServers Local
// Manages theme state and synchronizes with Rust backend via Tauri

import { createContext, useContext, useState, useEffect, ReactNode, useCallback } from 'react';
import { invoke } from '../lib/tauri';

// Theme type matching Rust struct
export interface Theme {
  id: number | null;
  profile_id: number;
  name: string;
  is_active: boolean;
  base_preset: string;
  // Core colors
  primary_color: string;
  secondary_color: string;
  accent_color: string;
  text_color: string;
  // Background
  background_color: string;
  background_gradient_enabled: boolean;
  background_gradient_angle: number;
  background_gradient_from: string | null;
  background_gradient_to: string | null;
  // Card
  card_bg_color: string;
  card_opacity: number;
  card_gradient_enabled: boolean;
  card_gradient_color1: string | null;
  card_gradient_color2: string | null;
  // Navbar
  navbar_color: string | null;
  navbar_opacity: number;
  // Extra
  custom_css: string | null;
  extra_settings: string | null;
  created_at: string;
  updated_at: string | null;
}

export interface PresetTheme {
  id: string;
  name: string;
  primary_color: string;
  secondary_color: string;
  accent_color: string;
  text_color: string;
  background_color: string;
  background_gradient_from: string;
  background_gradient_to: string;
  card_bg_color: string;
}

interface ThemeContextType {
  theme: Theme | null;
  themes: Theme[];
  presets: PresetTheme[];
  isLoading: boolean;
  // Actions
  loadTheme: (profileId: number) => Promise<void>;
  saveTheme: (theme: Theme) => Promise<Theme>;
  applyPreset: (profileId: number, presetId: string) => Promise<void>;
  updateTheme: (updates: Partial<Theme>) => void;
  setActiveTheme: (profileId: number, themeId: number) => Promise<void>;
  deleteTheme: (themeId: number, profileId: number) => Promise<boolean>;
}

const ThemeContext = createContext<ThemeContextType | undefined>(undefined);

// Apply theme to CSS variables
function applyThemeToDOM(theme: Theme) {
  const root = document.documentElement;

  // Core colors
  root.style.setProperty('--color-primary', theme.primary_color);
  root.style.setProperty('--color-secondary', theme.secondary_color);
  root.style.setProperty('--color-accent', theme.accent_color);
  root.style.setProperty('--color-text', theme.text_color);

  // Background
  root.style.setProperty('--color-background', theme.background_color);
  if (theme.background_gradient_enabled && theme.background_gradient_from && theme.background_gradient_to) {
    root.style.setProperty(
      '--background-gradient',
      `linear-gradient(${theme.background_gradient_angle}deg, ${theme.background_gradient_from}, ${theme.background_gradient_to})`
    );
  } else {
    root.style.setProperty('--background-gradient', theme.background_color);
  }

  // Card
  root.style.setProperty('--color-card', theme.card_bg_color);
  root.style.setProperty('--card-opacity', (theme.card_opacity / 100).toString());

  // Navbar
  if (theme.navbar_color) {
    root.style.setProperty('--color-navbar', theme.navbar_color);
  }
  root.style.setProperty('--navbar-opacity', (theme.navbar_opacity / 100).toString());

  // Apply custom CSS if present
  let customStyleEl = document.getElementById('theme-custom-css');
  if (theme.custom_css) {
    if (!customStyleEl) {
      customStyleEl = document.createElement('style');
      customStyleEl.id = 'theme-custom-css';
      document.head.appendChild(customStyleEl);
    }
    customStyleEl.textContent = theme.custom_css;
  } else if (customStyleEl) {
    customStyleEl.remove();
  }
}

// Default theme matching Rust default
const DEFAULT_THEME: Theme = {
  id: null,
  profile_id: 1,
  name: 'Default',
  is_active: true,
  base_preset: 'earthservers-default',
  primary_color: '#0fab89',
  secondary_color: '#e91e63',
  accent_color: '#0178C6',
  text_color: '#f0f0f0',
  background_color: '#0a0a0f',
  background_gradient_enabled: true,
  background_gradient_angle: 135,
  background_gradient_from: '#0a0a0f',
  background_gradient_to: '#1a1a2e',
  card_bg_color: '#1a1a2e',
  card_opacity: 80,
  card_gradient_enabled: false,
  card_gradient_color1: '#1a1a2e',
  card_gradient_color2: '#2a2a3e',
  navbar_color: '#0a0a0f',
  navbar_opacity: 90,
  custom_css: null,
  extra_settings: null,
  created_at: '',
  updated_at: null,
};

export function ThemeProvider({ children, profileId }: { children: ReactNode; profileId: number | null }) {
  const [theme, setTheme] = useState<Theme | null>(null);
  const [themes, setThemes] = useState<Theme[]>([]);
  const [presets, setPresets] = useState<PresetTheme[]>([]);
  const [isLoading, setIsLoading] = useState(true);

  // Apply default theme immediately on mount to prevent flash
  useEffect(() => {
    applyThemeToDOM(DEFAULT_THEME);
  }, []);

  // Load presets on mount
  useEffect(() => {
    invoke<PresetTheme[]>('get_theme_presets')
      .then(setPresets)
      .catch(console.error);
  }, []);

  // Load theme when profileId changes
  const loadTheme = useCallback(async (pid: number) => {
    setIsLoading(true);
    try {
      const [activeTheme, allThemes] = await Promise.all([
        invoke<Theme | null>('get_active_theme', { profileId: pid }),
        invoke<Theme[]>('get_themes', { profileId: pid }),
      ]);

      if (activeTheme) {
        setTheme(activeTheme);
        applyThemeToDOM(activeTheme);
      } else {
        // Use default theme
        setTheme({ ...DEFAULT_THEME, profile_id: pid });
        applyThemeToDOM({ ...DEFAULT_THEME, profile_id: pid });
      }
      setThemes(allThemes);
    } catch (err) {
      console.error('Failed to load theme:', err);
      setTheme({ ...DEFAULT_THEME, profile_id: pid });
      applyThemeToDOM({ ...DEFAULT_THEME, profile_id: pid });
    } finally {
      setIsLoading(false);
    }
  }, []);

  // Load theme when profileId changes
  useEffect(() => {
    if (profileId) {
      loadTheme(profileId);
    }
  }, [profileId, loadTheme]);

  // Save theme to backend
  const saveTheme = useCallback(async (themeToSave: Theme): Promise<Theme> => {
    const savedTheme = await invoke<Theme>('save_theme', { theme: themeToSave });
    setTheme(savedTheme);
    applyThemeToDOM(savedTheme);
    // Refresh themes list
    if (savedTheme.profile_id) {
      const allThemes = await invoke<Theme[]>('get_themes', { profileId: savedTheme.profile_id });
      setThemes(allThemes);
    }
    return savedTheme;
  }, []);

  // Apply a preset theme
  const applyPreset = useCallback(async (pid: number, presetId: string) => {
    const updatedTheme = await invoke<Theme>('apply_preset_theme', {
      profileId: pid,
      presetId,
    });
    setTheme(updatedTheme);
    applyThemeToDOM(updatedTheme);
  }, []);

  // Update theme locally (for live preview)
  const updateTheme = useCallback((updates: Partial<Theme>) => {
    setTheme(prev => {
      if (!prev) return prev;
      const updated = { ...prev, ...updates };
      applyThemeToDOM(updated);
      return updated;
    });
  }, []);

  // Set active theme
  const setActiveTheme = useCallback(async (pid: number, themeId: number) => {
    const activeTheme = await invoke<Theme>('set_active_theme', {
      profileId: pid,
      themeId,
    });
    setTheme(activeTheme);
    applyThemeToDOM(activeTheme);
    // Refresh themes list
    const allThemes = await invoke<Theme[]>('get_themes', { profileId: pid });
    setThemes(allThemes);
  }, []);

  // Delete theme
  const deleteTheme = useCallback(async (themeId: number, pid: number): Promise<boolean> => {
    const success = await invoke<boolean>('delete_theme', { themeId, profileId: pid });
    if (success) {
      // Refresh themes list
      const allThemes = await invoke<Theme[]>('get_themes', { profileId: pid });
      setThemes(allThemes);
      // If we deleted the active theme, load the new active one
      const activeTheme = await invoke<Theme | null>('get_active_theme', { profileId: pid });
      if (activeTheme) {
        setTheme(activeTheme);
        applyThemeToDOM(activeTheme);
      }
    }
    return success;
  }, []);

  return (
    <ThemeContext.Provider
      value={{
        theme,
        themes,
        presets,
        isLoading,
        loadTheme,
        saveTheme,
        applyPreset,
        updateTheme,
        setActiveTheme,
        deleteTheme,
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

export default ThemeContext;
