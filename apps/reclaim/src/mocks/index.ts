// Centralized mock data for development
// This file consolidates all mock data used when running outside Tauri

// Check if we should use mocks (browser dev mode)
export const USE_MOCKS = import.meta.env.VITE_USE_MOCK_DATA === 'true' || !('__TAURI__' in window);

// ============================================
// PROFILES
// ============================================
export const mockProfile = {
  id: 1,
  name: 'Default Profile',
  icon: null,
  created_at: new Date().toISOString(),
  is_active: true,
};

// ============================================
// THEMES
// ============================================
export const mockPresets = [
  {
    id: 'earthservers-default',
    name: 'EarthServers Default',
    primary_color: '#0fab89',
    secondary_color: '#e91e63',
    accent_color: '#0178C6',
    text_color: '#f0f0f0',
    background_color: '#0a0a0f',
    background_gradient_from: '#0a0a0f',
    background_gradient_to: '#1a1a2e',
    card_bg_color: '#1a1a2e',
  },
  {
    id: 'ocean-turtle',
    name: 'Ocean Turtle',
    primary_color: '#26c6da',
    secondary_color: '#00838f',
    accent_color: '#00acc1',
    text_color: '#e0f7fa',
    background_color: '#0d1b2a',
    background_gradient_from: '#0d1b2a',
    background_gradient_to: '#1b3a4b',
    card_bg_color: '#1b3a4b',
  },
  {
    id: 'mountain-eagle',
    name: 'Mountain Eagle',
    primary_color: '#78909c',
    secondary_color: '#37474f',
    accent_color: '#546e7a',
    text_color: '#eceff1',
    background_color: '#1c1c1c',
    background_gradient_from: '#1c1c1c',
    background_gradient_to: '#2d2d2d',
    card_bg_color: '#2d2d2d',
  },
  {
    id: 'sun-fire',
    name: 'Sun Fire',
    primary_color: '#ff9800',
    secondary_color: '#f44336',
    accent_color: '#ffb300',
    text_color: '#fff8e1',
    background_color: '#1a0a00',
    background_gradient_from: '#1a0a00',
    background_gradient_to: '#2d1500',
    card_bg_color: '#2d1500',
  },
  {
    id: 'lightning-bolt',
    name: 'Lightning Bolt',
    primary_color: '#7c4dff',
    secondary_color: '#448aff',
    accent_color: '#536dfe',
    text_color: '#ede7f6',
    background_color: '#0d0d1a',
    background_gradient_from: '#0d0d1a',
    background_gradient_to: '#1a1a3e',
    card_bg_color: '#1a1a3e',
  },
  {
    id: 'air-clouds',
    name: 'Air Clouds',
    primary_color: '#64b5f6',
    secondary_color: '#90caf9',
    accent_color: '#42a5f5',
    text_color: '#e3f2fd',
    background_color: '#0a1929',
    background_gradient_from: '#0a1929',
    background_gradient_to: '#1a3a52',
    card_bg_color: '#1a3a52',
  },
];

// Helper to create a full theme from a preset
export const createThemeFromPreset = (presetId: string, profileId: number = 1) => {
  const preset = mockPresets.find(p => p.id === presetId) || mockPresets[0];
  return {
    id: 1,
    profile_id: profileId,
    name: preset.name,
    is_active: true,
    base_preset: preset.id,
    primary_color: preset.primary_color,
    secondary_color: preset.secondary_color,
    accent_color: preset.accent_color,
    text_color: preset.text_color,
    background_color: preset.background_color,
    background_gradient_enabled: true,
    background_gradient_angle: 135,
    background_gradient_from: preset.background_gradient_from,
    background_gradient_to: preset.background_gradient_to,
    card_bg_color: preset.card_bg_color,
    card_opacity: 80,
    card_gradient_enabled: false,
    card_gradient_color1: preset.card_bg_color,
    card_gradient_color2: preset.background_color,
    navbar_color: preset.background_color,
    navbar_opacity: 90,
    custom_css: null,
    extra_settings: null,
    created_at: new Date().toISOString(),
    updated_at: null,
  };
};

// Default mock theme
export let mockTheme = createThemeFromPreset('earthservers-default');

// ============================================
// DOMAINS (EarthSearch)
// ============================================
export let mockDomains = [
  { id: 1, url: 'reuters.com', category: 'news', trust_score: 0.85, added_date: new Date().toISOString(), metadata: null, profileId: 1 },
  { id: 2, url: 'apnews.com', category: 'news', trust_score: 0.85, added_date: new Date().toISOString(), metadata: null, profileId: 1 },
  { id: 3, url: 'scholar.google.com', category: 'academic', trust_score: 0.90, added_date: new Date().toISOString(), metadata: null, profileId: 1 },
  { id: 4, url: 'nature.com', category: 'academic', trust_score: 0.95, added_date: new Date().toISOString(), metadata: null, profileId: 1 },
  { id: 5, url: 'snopes.com', category: 'fact-check', trust_score: 0.85, added_date: new Date().toISOString(), metadata: null, profileId: 1 },
  { id: 6, url: 'github.com', category: 'technology', trust_score: 0.85, added_date: new Date().toISOString(), metadata: null, profileId: 1 },
  { id: 7, url: 'stackoverflow.com', category: 'technology', trust_score: 0.80, added_date: new Date().toISOString(), metadata: null, profileId: 1 },
  { id: 8, url: 'mayoclinic.org', category: 'health', trust_score: 0.95, added_date: new Date().toISOString(), metadata: null, profileId: 1 },
  { id: 9, url: 'wikipedia.org', category: 'reference', trust_score: 0.75, added_date: new Date().toISOString(), metadata: null, profileId: 1 },
  { id: 10, url: 'propublica.org', category: 'journalism', trust_score: 0.90, added_date: new Date().toISOString(), metadata: null, profileId: 1 },
];

export let mockDomainLists = [
  { id: 1, name: 'Mainstream News', description: 'Major news outlets', author: 'EarthServers', version: '1.0', created_at: new Date().toISOString(), profileId: 1, domain_count: 2 },
  { id: 2, name: 'Academic & Research', description: 'Academic journals and research institutions', author: 'EarthServers', version: '1.0', created_at: new Date().toISOString(), profileId: 1, domain_count: 2 },
  { id: 3, name: 'Technology', description: 'Tech news and programming resources', author: 'EarthServers', version: '1.0', created_at: new Date().toISOString(), profileId: 1, domain_count: 2 },
];

// ============================================
// TABS
// ============================================
export let mockTabs: any[] = [
  { id: 1, profileId: 1, title: 'Search', url: 'earth://search', favicon: null, position: 0, is_pinned: false, is_active: true, scroll_position: 0, created_at: new Date().toISOString(), last_accessed: new Date().toISOString() },
];

// ============================================
// BOOKMARKS
// ============================================
export let mockBookmarks: any[] = [
  { id: 1, profileId: 1, title: 'Search', url: 'earth://search', favicon: null, folderId: null, folder_name: null, tags: ['home'], notes: 'Return to Search home', position: 0, location: 'toolbar', created_at: new Date().toISOString(), updated_at: new Date().toISOString(), is_system: true },
  { id: 2, profileId: 1, title: 'EarthServers', url: 'https://earthservers.net', favicon: null, folderId: null, folder_name: null, tags: ['default'], notes: 'EarthServers homepage', position: 1, location: 'toolbar', created_at: new Date().toISOString(), updated_at: new Date().toISOString(), is_system: false },
  { id: 3, profileId: 1, title: 'EarthSocial', url: 'https://social.earthservers.net', favicon: null, folderId: null, folder_name: null, tags: ['default', 'social'], notes: 'EarthServers social platform', position: 2, location: 'toolbar', created_at: new Date().toISOString(), updated_at: new Date().toISOString(), is_system: false },
  { id: 4, profileId: 1, title: 'Private Site', url: 'https://private.example.com', favicon: null, folderId: null, folder_name: null, tags: ['private'], notes: 'A private bookmark', position: 3, location: 'private', created_at: new Date().toISOString(), updated_at: new Date().toISOString(), is_system: false },
  { id: 5, profileId: 1, title: 'Secret Docs', url: 'https://docs.secret.io', favicon: null, folderId: null, folder_name: null, tags: ['private', 'docs'], notes: 'Secret documentation', position: 4, location: 'private', created_at: new Date().toISOString(), updated_at: new Date().toISOString(), is_system: false },
];

export let mockBookmarkFolders: any[] = [
  { id: 1, profileId: 1, name: 'News', parentId: null, position: 0, created_at: new Date().toISOString(), bookmark_count: 0 },
  { id: 2, profileId: 1, name: 'Tech', parentId: null, position: 1, created_at: new Date().toISOString(), bookmark_count: 0 },
];

// ============================================
// RATINGS
// ============================================
export const mockRatings: Record<string, { avg_trust: number; avg_bias: number; total_ratings: number }> = {
  'reuters.com': { avg_trust: 4.5, avg_bias: 2.8, total_ratings: 1247 },
  'nature.com': { avg_trust: 4.9, avg_bias: 2.0, total_ratings: 543 },
  'apnews.com': { avg_trust: 4.3, avg_bias: 2.5, total_ratings: 892 },
  'github.com': { avg_trust: 4.7, avg_bias: 2.2, total_ratings: 2103 },
  'wikipedia.org': { avg_trust: 3.8, avg_bias: 2.8, total_ratings: 5621 },
};

// ============================================
// SPLIT VIEW CONFIG
// ============================================
export let mockSplitConfig: any = {
  profile_id: 1,
  layout: 'single',
  pane_1_tab_id: 1,
  pane_2_tab_id: null,
  pane_3_tab_id: null,
  pane_4_tab_id: null,
  active_pane: 1,
  pane_sizes: null,
};

// ============================================
// PRIVACY STATE
// ============================================
export let mockIncognitoStatus = false;
export let privateBookmarksPassword: string | null = null;

// ============================================
// PASSWORD MANAGER
// ============================================
export let passwordManagerMaster: Record<number, string> = {};
export let passwordEntries: any[] = [];

// ============================================
// OTP AUTHENTICATOR
// ============================================
export let otpMaster: Record<number, string> = {};
export let otpEntries: any[] = [];

// ============================================
// SETTERS (for mock state updates)
// ============================================
export const setMockIncognitoStatus = (status: boolean) => {
  mockIncognitoStatus = status;
};

export const setMockTheme = (theme: any) => {
  mockTheme = theme;
};

export const setMockDomains = (domains: any[]) => {
  mockDomains = domains;
};

export const setMockTabs = (tabs: any[]) => {
  mockTabs = tabs;
};

export const setMockBookmarks = (bookmarks: any[]) => {
  mockBookmarks = bookmarks;
};

export const setMockSplitConfig = (config: any) => {
  mockSplitConfig = config;
};
