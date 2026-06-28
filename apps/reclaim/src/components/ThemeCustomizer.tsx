import { useState, useEffect, useCallback, useRef } from 'react';
import { useTheme, Theme, PresetTheme } from '../contexts/ThemeContext';
import { RightDockPanel, RIGHT_DOCK_WIDTH_WIDE } from '../lib/rightDock';

// Draggable hook for the modal
function useDraggable(defaultPosition = { x: 100, y: 100 }) {
  const [position, setPosition] = useState(defaultPosition);
  const [isDragging, setIsDragging] = useState(false);
  const dragOffset = useRef({ x: 0, y: 0 });

  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    if ((e.target as HTMLElement).closest('button, input, select')) return;
    e.preventDefault();
    setIsDragging(true);
    dragOffset.current = {
      x: e.clientX - position.x,
      y: e.clientY - position.y,
    };
  }, [position]);

  useEffect(() => {
    if (!isDragging) return;

    const handleMouseMove = (e: MouseEvent) => {
      setPosition({
        x: Math.max(0, e.clientX - dragOffset.current.x),
        y: Math.max(0, e.clientY - dragOffset.current.y),
      });
    };

    const handleMouseUp = () => setIsDragging(false);

    window.addEventListener('mousemove', handleMouseMove);
    window.addEventListener('mouseup', handleMouseUp);
    return () => {
      window.removeEventListener('mousemove', handleMouseMove);
      window.removeEventListener('mouseup', handleMouseUp);
    };
  }, [isDragging]);

  return { position, handleMouseDown, isDragging };
}

interface ThemeCustomizerProps {
  profileId: number | null;
  isOpen: boolean;
  onClose: () => void;
}

// Color picker input with label
function ColorInput({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
}) {
  return (
    <div className="flex items-center justify-between">
      <label className="text-sm text-gray-300">{label}</label>
      <div className="flex items-center gap-2">
        <input
          type="color"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          className="w-8 h-8 rounded cursor-pointer border border-white/20"
        />
        <input
          type="text"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          className="w-20 px-2 py-1 text-xs bg-white/5 border border-white/10 rounded text-white font-mono"
        />
      </div>
    </div>
  );
}

// Slider input with label
function SliderInput({
  label,
  value,
  onChange,
  min = 0,
  max = 100,
  suffix = '%',
}: {
  label: string;
  value: number;
  onChange: (value: number) => void;
  min?: number;
  max?: number;
  suffix?: string;
}) {
  return (
    <div className="space-y-1">
      <div className="flex justify-between">
        <label className="text-sm text-gray-300">{label}</label>
        <span className="text-sm text-gray-400">
          {value}{suffix}
        </span>
      </div>
      <input
        type="range"
        min={min}
        max={max}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        className="w-full accent-earth-teal"
      />
    </div>
  );
}

export function ThemeCustomizer({ profileId, isOpen, onClose }: ThemeCustomizerProps) {
  const { theme, themes, presets, isLoading, saveTheme, applyPreset, updateTheme, setActiveTheme, deleteTheme } = useTheme();
  const [activeTab, setActiveTab] = useState<'presets' | 'colors' | 'background' | 'cards' | 'advanced'>('presets');
  const [themeName, setThemeName] = useState('');
  const [isSaving, setIsSaving] = useState(false);
  const [hasChanges, setHasChanges] = useState(false);
  // Docked as a right panel now (not draggable); keep handleMouseDown as a no-op
  // sink for the header's mousedown so it doesn't start a text selection/drag.
  const { handleMouseDown } = useDraggable({ x: 50, y: 50 });

  // Track original theme for detecting changes
  const [originalTheme, setOriginalTheme] = useState<Theme | null>(null);

  useEffect(() => {
    if (theme) {
      setThemeName(theme.name);
      setOriginalTheme({ ...theme });
    }
  }, [isOpen]);

  // Check for changes
  useEffect(() => {
    if (theme && originalTheme) {
      const changed = JSON.stringify(theme) !== JSON.stringify(originalTheme);
      setHasChanges(changed);
    }
  }, [theme, originalTheme]);

  const handleSave = useCallback(async () => {
    if (!theme || !profileId) return;

    setIsSaving(true);
    try {
      const themeToSave = { ...theme, name: themeName || theme.name };
      await saveTheme(themeToSave);
      setOriginalTheme({ ...themeToSave });
      setHasChanges(false);
    } catch (err) {
      console.error('Failed to save theme:', err);
    } finally {
      setIsSaving(false);
    }
  }, [theme, themeName, profileId, saveTheme]);

  const handleApplyPreset = useCallback(async (preset: PresetTheme) => {
    if (!profileId) return;
    try {
      await applyPreset(profileId, preset.id);
    } catch (err) {
      console.error('Failed to apply preset:', err);
    }
  }, [profileId, applyPreset]);

  const handleSwitchTheme = useCallback(async (themeId: number) => {
    if (!profileId) return;
    try {
      await setActiveTheme(profileId, themeId);
    } catch (err) {
      console.error('Failed to switch theme:', err);
    }
  }, [profileId, setActiveTheme]);

  const handleDeleteTheme = useCallback(async (themeId: number) => {
    if (!profileId) return;
    if (themes.length <= 1) {
      alert('Cannot delete the only theme');
      return;
    }
    const confirmed = window.confirm('Are you sure you want to delete this theme?');
    if (!confirmed) return;

    try {
      await deleteTheme(themeId, profileId);
    } catch (err) {
      console.error('Failed to delete theme:', err);
    }
  }, [profileId, themes.length, deleteTheme]);

  const handleClose = useCallback(() => {
    if (hasChanges) {
      const confirmed = window.confirm('You have unsaved changes. Discard them?');
      if (!confirmed) return;
      // Revert to original theme
      if (originalTheme) {
        updateTheme(originalTheme);
      }
    }
    onClose();
  }, [hasChanges, originalTheme, updateTheme, onClose]);

  if (!isOpen) return null;

  // Show loading state while theme is loading
  if (isLoading || !theme) {
    return (
      <RightDockPanel id="theme" open={isOpen} width={RIGHT_DOCK_WIDTH_WIDE} title="Theme Customizer" onClose={onClose}>
        <div className="flex flex-col items-center gap-4 p-8">
          <div className="w-8 h-8 border-2 border-earth-teal border-t-transparent rounded-full animate-spin" />
          <p className="text-gray-400">Loading theme...</p>
        </div>
      </RightDockPanel>
    );
  }

  return (
    <RightDockPanel id="theme" open={isOpen} width={RIGHT_DOCK_WIDTH_WIDE} onClose={onClose}>
      {/* Header - Draggable */}
      <div
        className="flex items-center justify-between px-6 py-4 border-b border-white/10 cursor-grab active:cursor-grabbing select-none rounded-t-2xl"
        onMouseDown={handleMouseDown}
      >
          <div className="flex items-center gap-4">
            <h2 className="text-xl font-bold text-white">Theme Customizer</h2>
            {hasChanges && (
              <span className="px-2 py-0.5 text-xs bg-amber-500/20 text-amber-400 rounded">
                Unsaved changes
              </span>
            )}
          </div>
          <div className="flex items-center gap-3">
            <button
              onClick={handleSave}
              disabled={isSaving || !hasChanges}
              className={`px-4 py-2 rounded-lg font-medium transition-all ${
                hasChanges
                  ? 'bg-earth-teal text-white hover:opacity-90'
                  : 'bg-white/5 text-gray-500 cursor-not-allowed'
              }`}
            >
              {isSaving ? 'Saving...' : 'Save Theme'}
            </button>
            <button
              onClick={handleClose}
              className="p-2 text-gray-400 hover:text-white transition-colors"
            >
              <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>
        </div>

        {/* Tab Navigation */}
        <div className="px-6 py-3 border-b border-white/10 flex gap-2 overflow-x-auto">
          {(['presets', 'colors', 'background', 'cards', 'advanced'] as const).map((tab) => (
            <button
              key={tab}
              onClick={() => setActiveTab(tab)}
              className={`px-4 py-2 rounded-lg text-sm font-medium transition-colors whitespace-nowrap ${
                activeTab === tab
                  ? 'bg-earth-teal text-white'
                  : 'text-gray-400 hover:text-white hover:bg-white/5'
              }`}
            >
              {tab.charAt(0).toUpperCase() + tab.slice(1)}
            </button>
          ))}
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto p-6">
          {/* Presets Tab */}
          {activeTab === 'presets' && (
            <div className="space-y-6">
              {/* Theme Presets */}
              <div>
                <h3 className="text-lg font-semibold text-white mb-4">Theme Presets</h3>
                <div className="grid grid-cols-2 md:grid-cols-3 gap-4">
                  {presets.map((preset) => (
                    <button
                      key={preset.id}
                      onClick={() => handleApplyPreset(preset)}
                      className={`p-4 rounded-xl border transition-all text-left ${
                        theme.base_preset === preset.id
                          ? 'border-earth-teal bg-earth-teal/10'
                          : 'border-white/10 bg-white/5 hover:border-white/30'
                      }`}
                    >
                      {/* Preview Colors */}
                      <div className="flex gap-1 mb-3">
                        <div
                          className="w-6 h-6 rounded-full"
                          style={{ backgroundColor: preset.primary_color }}
                        />
                        <div
                          className="w-6 h-6 rounded-full"
                          style={{ backgroundColor: preset.secondary_color }}
                        />
                        <div
                          className="w-6 h-6 rounded-full"
                          style={{ backgroundColor: preset.accent_color }}
                        />
                      </div>
                      <div className="text-white font-medium">{preset.name}</div>
                      {theme.base_preset === preset.id && (
                        <div className="text-xs text-earth-teal mt-1">Active</div>
                      )}
                    </button>
                  ))}
                </div>
              </div>

              {/* Saved Themes */}
              <div>
                <h3 className="text-lg font-semibold text-white mb-4">Saved Themes</h3>
                <div className="space-y-2">
                  {themes.map((t) => (
                    <div
                      key={t.id}
                      className={`flex items-center justify-between p-4 rounded-xl border transition-all ${
                        t.is_active
                          ? 'border-earth-teal bg-earth-teal/10'
                          : 'border-white/10 bg-white/5'
                      }`}
                    >
                      <div className="flex items-center gap-3">
                        <div className="flex gap-1">
                          <div
                            className="w-4 h-4 rounded-full"
                            style={{ backgroundColor: t.primary_color }}
                          />
                          <div
                            className="w-4 h-4 rounded-full"
                            style={{ backgroundColor: t.secondary_color }}
                          />
                        </div>
                        <span className="text-white">{t.name}</span>
                        {t.is_active && (
                          <span className="px-2 py-0.5 text-xs bg-earth-teal/20 text-earth-teal rounded">
                            Active
                          </span>
                        )}
                      </div>
                      <div className="flex items-center gap-2">
                        {!t.is_active && t.id && (
                          <>
                            <button
                              onClick={() => handleSwitchTheme(t.id!)}
                              className="px-3 py-1 text-sm text-earth-teal hover:bg-earth-teal/10 rounded transition-colors"
                            >
                              Activate
                            </button>
                            <button
                              onClick={() => handleDeleteTheme(t.id!)}
                              className="p-1 text-gray-500 hover:text-red-400 transition-colors"
                            >
                              <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                              </svg>
                            </button>
                          </>
                        )}
                      </div>
                    </div>
                  ))}
                </div>
              </div>

              {/* Theme Name */}
              <div>
                <h3 className="text-lg font-semibold text-white mb-4">Theme Name</h3>
                <input
                  type="text"
                  value={themeName}
                  onChange={(e) => setThemeName(e.target.value)}
                  placeholder="My Custom Theme"
                  className="w-full px-4 py-2 bg-white/5 border border-white/10 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-earth-teal"
                />
              </div>
            </div>
          )}

          {/* Colors Tab */}
          {activeTab === 'colors' && (
            <div className="space-y-6">
              <h3 className="text-lg font-semibold text-white mb-4">Core Colors</h3>
              <div className="grid gap-4 max-w-md">
                <ColorInput
                  label="Primary Color"
                  value={theme.primary_color}
                  onChange={(v) => updateTheme({ primary_color: v })}
                />
                <ColorInput
                  label="Secondary Color"
                  value={theme.secondary_color}
                  onChange={(v) => updateTheme({ secondary_color: v })}
                />
                <ColorInput
                  label="Accent Color"
                  value={theme.accent_color}
                  onChange={(v) => updateTheme({ accent_color: v })}
                />
                <ColorInput
                  label="Text Color"
                  value={theme.text_color}
                  onChange={(v) => updateTheme({ text_color: v })}
                />
              </div>
            </div>
          )}

          {/* Background Tab */}
          {activeTab === 'background' && (
            <div className="space-y-6">
              <h3 className="text-lg font-semibold text-white mb-4">Background Settings</h3>
              <div className="grid gap-4 max-w-md">
                <ColorInput
                  label="Background Color"
                  value={theme.background_color}
                  onChange={(v) => updateTheme({ background_color: v })}
                />

                {/* Gradient Toggle */}
                <div className="flex items-center justify-between">
                  <label className="text-sm text-gray-300">Enable Gradient</label>
                  <button
                    onClick={() => updateTheme({ background_gradient_enabled: !theme.background_gradient_enabled })}
                    className={`w-12 h-6 rounded-full transition-colors ${
                      theme.background_gradient_enabled ? 'bg-earth-teal' : 'bg-white/20'
                    }`}
                  >
                    <div
                      className={`w-5 h-5 bg-white rounded-full transition-transform ${
                        theme.background_gradient_enabled ? 'translate-x-6' : 'translate-x-0.5'
                      }`}
                    />
                  </button>
                </div>

                {theme.background_gradient_enabled && (
                  <>
                    <ColorInput
                      label="Gradient From"
                      value={theme.background_gradient_from || theme.background_color}
                      onChange={(v) => updateTheme({ background_gradient_from: v })}
                    />
                    <ColorInput
                      label="Gradient To"
                      value={theme.background_gradient_to || theme.background_color}
                      onChange={(v) => updateTheme({ background_gradient_to: v })}
                    />
                    <SliderInput
                      label="Gradient Angle"
                      value={theme.background_gradient_angle}
                      onChange={(v) => updateTheme({ background_gradient_angle: v })}
                      min={0}
                      max={360}
                      suffix="°"
                    />
                  </>
                )}
              </div>

              {/* Preview */}
              <div className="mt-6">
                <h4 className="text-sm font-medium text-gray-400 mb-2">Preview</h4>
                <div
                  className="w-full h-32 rounded-xl border border-white/10"
                  style={{
                    background: theme.background_gradient_enabled && theme.background_gradient_from && theme.background_gradient_to
                      ? `linear-gradient(${theme.background_gradient_angle}deg, ${theme.background_gradient_from}, ${theme.background_gradient_to})`
                      : theme.background_color,
                  }}
                />
              </div>
            </div>
          )}

          {/* Cards Tab */}
          {activeTab === 'cards' && (
            <div className="space-y-6">
              <h3 className="text-lg font-semibold text-white mb-4">Card & Navbar Settings</h3>
              <div className="grid gap-4 max-w-md">
                <ColorInput
                  label="Card Background"
                  value={theme.card_bg_color}
                  onChange={(v) => updateTheme({ card_bg_color: v })}
                />
                <SliderInput
                  label="Card Opacity"
                  value={theme.card_opacity}
                  onChange={(v) => updateTheme({ card_opacity: v })}
                />

                {/* Card Gradient Toggle */}
                <div className="flex items-center justify-between">
                  <label className="text-sm text-gray-300">Card Gradient</label>
                  <button
                    onClick={() => updateTheme({ card_gradient_enabled: !theme.card_gradient_enabled })}
                    className={`w-12 h-6 rounded-full transition-colors ${
                      theme.card_gradient_enabled ? 'bg-earth-teal' : 'bg-white/20'
                    }`}
                  >
                    <div
                      className={`w-5 h-5 bg-white rounded-full transition-transform ${
                        theme.card_gradient_enabled ? 'translate-x-6' : 'translate-x-0.5'
                      }`}
                    />
                  </button>
                </div>

                {theme.card_gradient_enabled && (
                  <>
                    <ColorInput
                      label="Card Gradient Color 1"
                      value={theme.card_gradient_color1 || theme.card_bg_color}
                      onChange={(v) => updateTheme({ card_gradient_color1: v })}
                    />
                    <ColorInput
                      label="Card Gradient Color 2"
                      value={theme.card_gradient_color2 || theme.card_bg_color}
                      onChange={(v) => updateTheme({ card_gradient_color2: v })}
                    />
                  </>
                )}

                <div className="border-t border-white/10 pt-4 mt-4">
                  <h4 className="text-sm font-medium text-gray-400 mb-4">Navbar</h4>
                  <ColorInput
                    label="Navbar Color"
                    value={theme.navbar_color || theme.background_color}
                    onChange={(v) => updateTheme({ navbar_color: v })}
                  />
                  <div className="mt-4">
                    <SliderInput
                      label="Navbar Opacity"
                      value={theme.navbar_opacity}
                      onChange={(v) => updateTheme({ navbar_opacity: v })}
                    />
                  </div>
                </div>
              </div>

              {/* Card Preview */}
              <div className="mt-6">
                <h4 className="text-sm font-medium text-gray-400 mb-2">Card Preview</h4>
                <div
                  className="p-6 rounded-xl border border-white/10"
                  style={{
                    backgroundColor: theme.card_bg_color,
                    opacity: theme.card_opacity / 100,
                  }}
                >
                  <div className="text-white font-medium">Sample Card</div>
                  <div className="text-sm text-gray-400 mt-1">This is how your cards will look</div>
                </div>
              </div>
            </div>
          )}

          {/* Advanced Tab */}
          {activeTab === 'advanced' && (
            <div className="space-y-6">
              <h3 className="text-lg font-semibold text-white mb-4">Custom CSS</h3>
              <p className="text-sm text-gray-400 mb-4">
                Add custom CSS to further customize your theme. This CSS will be injected into the page.
              </p>
              <textarea
                value={theme.custom_css || ''}
                onChange={(e) => updateTheme({ custom_css: e.target.value || null })}
                placeholder={`/* Example: */\n.my-class {\n  color: var(--color-primary);\n}`}
                className="w-full h-64 px-4 py-3 bg-white/5 border border-white/10 rounded-lg text-white font-mono text-sm placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-earth-teal resize-none"
              />

              <div className="border-t border-white/10 pt-6">
                <h4 className="text-sm font-medium text-gray-400 mb-2">CSS Variables Available</h4>
                <div className="bg-white/5 rounded-lg p-4 font-mono text-xs text-gray-300 space-y-1">
                  <div>--color-primary: {theme.primary_color}</div>
                  <div>--color-secondary: {theme.secondary_color}</div>
                  <div>--color-accent: {theme.accent_color}</div>
                  <div>--color-text: {theme.text_color}</div>
                  <div>--color-background: {theme.background_color}</div>
                  <div>--color-card: {theme.card_bg_color}</div>
                  <div>--card-opacity: {theme.card_opacity / 100}</div>
                  <div>--navbar-opacity: {theme.navbar_opacity / 100}</div>
                </div>
              </div>
            </div>
          )}
        </div>
    </RightDockPanel>
  );
}

export default ThemeCustomizer;
