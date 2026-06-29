// Tauri utilities with browser fallback for development
// Detects if running in Tauri or browser and provides mock data for browser dev

// Check if we're running in Tauri
export const isTauri = () => {
  return typeof window !== 'undefined' && '__TAURI__' in window;
};

// Mock file save dialog
export async function saveDialog(options?: { defaultPath?: string; filters?: { name: string; extensions: string[] }[] }): Promise<string | null> {
  if (isTauri()) {
    const { save } = await import('@tauri-apps/plugin-dialog');
    return save(options);
  }
  // Browser fallback - just return a fake path
  console.log('[Mock] saveDialog', options);
  return options?.defaultPath || 'export.json';
}

// Mock file write
export async function writeFile(path: string, contents: string): Promise<void> {
  if (isTauri()) {
    const { writeTextFile } = await import('@tauri-apps/plugin-fs');
    return writeTextFile(path, contents);
  }
  // Browser fallback - trigger download
  console.log('[Mock] writeFile', path);
  const blob = new Blob([contents], { type: 'application/json' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = path.split('/').pop() || 'export.json';
  a.click();
  URL.revokeObjectURL(url);
}

// Window control functions for custom titlebar
export async function minimizeWindow(): Promise<void> {
  if (isTauri()) {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    await getCurrentWindow().minimize();
  }
}

export async function maximizeWindow(): Promise<void> {
  if (isTauri()) {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    const win = getCurrentWindow();
    const isMaximized = await win.isMaximized();
    if (isMaximized) {
      await win.unmaximize();
    } else {
      await win.maximize();
    }
  }
}

export async function closeWindow(): Promise<void> {
  if (isTauri()) {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    await getCurrentWindow().close();
  }
}

// Start dragging the window (for custom titlebar on Linux)
export async function startDragging(): Promise<void> {
  if (isTauri()) {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    await getCurrentWindow().startDragging();
  }
}

// Toggle fullscreen mode
export async function toggleFullscreen(): Promise<void> {
  if (isTauri()) {
    try {
      // Use custom command which handles the main window specifically
      const { invoke: tauriInvoke } = await import('@tauri-apps/api/core');
      await tauriInvoke('toggle_fullscreen');
    } catch (err) {
      // Fallback to direct window API
      console.error('toggle_fullscreen command failed, using fallback:', err);
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      const win = getCurrentWindow();
      const isFullscreen = await win.isFullscreen();
      await win.setFullscreen(!isFullscreen);
    }
  }
}

// Check if window is maximized
export async function isWindowMaximized(): Promise<boolean> {
  if (isTauri()) {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    return getCurrentWindow().isMaximized();
  }
  return true; // Browser dev mode - treat as maximized
}

// Toggle developer tools
export async function toggleDevTools(): Promise<void> {
  if (isTauri()) {
    const { invoke: tauriInvoke } = await import('@tauri-apps/api/core');
    await tauriInvoke('toggle_devtools');
  } else {
    console.log('[Mock] Would toggle devtools');
  }
}

// Listen for window resize events (includes maximize/unmaximize)
export async function onWindowResize(callback: (isMaximized: boolean) => void): Promise<() => void> {
  if (isTauri()) {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    const win = getCurrentWindow();

    // Listen for resize events and check maximized state
    const unlisten = await win.onResized(async () => {
      const maximized = await win.isMaximized();
      callback(maximized);
    });

    return unlisten;
  }
  return () => {}; // No-op for browser
}

// Mock data for browser development
const mockProfile = {
  id: 1,
  name: 'Default Profile',
  icon: null,
  created_at: Date.now().toString(),
  is_active: true,
};

// Full preset definitions with all theme colors
const mockPresets = [
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
const createThemeFromPreset = (presetId: string, profileId: number = 1) => {
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

// Default mock theme using the helper
let mockTheme = createThemeFromPreset('earthservers-default');

// Mock domains for browser development
let mockDomains = [
  { id: 1, url: 'reuters.com', category: 'news', trust_score: 0.85, added_date: Date.now().toString(), metadata: null, profileId: 1 },
  { id: 2, url: 'apnews.com', category: 'news', trust_score: 0.85, added_date: Date.now().toString(), metadata: null, profileId: 1 },
  { id: 3, url: 'scholar.google.com', category: 'academic', trust_score: 0.90, added_date: Date.now().toString(), metadata: null, profileId: 1 },
  { id: 4, url: 'nature.com', category: 'academic', trust_score: 0.95, added_date: Date.now().toString(), metadata: null, profileId: 1 },
  { id: 5, url: 'snopes.com', category: 'fact-check', trust_score: 0.85, added_date: Date.now().toString(), metadata: null, profileId: 1 },
  { id: 6, url: 'github.com', category: 'technology', trust_score: 0.85, added_date: Date.now().toString(), metadata: null, profileId: 1 },
  { id: 7, url: 'stackoverflow.com', category: 'technology', trust_score: 0.80, added_date: Date.now().toString(), metadata: null, profileId: 1 },
  { id: 8, url: 'mayoclinic.org', category: 'health', trust_score: 0.95, added_date: Date.now().toString(), metadata: null, profileId: 1 },
  { id: 9, url: 'wikipedia.org', category: 'reference', trust_score: 0.75, added_date: Date.now().toString(), metadata: null, profileId: 1 },
  { id: 10, url: 'propublica.org', category: 'journalism', trust_score: 0.90, added_date: Date.now().toString(), metadata: null, profileId: 1 },
];

let mockDomainLists = [
  { id: 1, name: 'Mainstream News', description: 'Major news outlets', author: 'EarthServers', version: '1.0', created_at: Date.now().toString(), profileId: 1, domain_count: 2 },
  { id: 2, name: 'Academic & Research', description: 'Academic journals and research institutions', author: 'EarthServers', version: '1.0', created_at: Date.now().toString(), profileId: 1, domain_count: 2 },
  { id: 3, name: 'Technology', description: 'Tech news and programming resources', author: 'EarthServers', version: '1.0', created_at: Date.now().toString(), profileId: 1, domain_count: 2 },
];

// Mock tabs for browser development
let mockTabs: any[] = [
  { id: 1, profileId: 1, title: 'Search', url: 'earth://search', favicon: null, position: 0, is_pinned: false, is_active: true, scroll_position: 0, created_at: new Date().toISOString(), last_accessed: new Date().toISOString() },
];

// Mock bookmarks for browser development - EarthSearch is mandatory first bookmark
let mockBookmarks: any[] = [
  { id: 1, profileId: 1, title: 'Search', url: 'earth://search', favicon: null, folderId: null, folder_name: null, tags: ['home'], notes: 'Return to Search home', position: 0, location: 'toolbar', created_at: new Date().toISOString(), updated_at: new Date().toISOString(), is_system: true },
  { id: 2, profileId: 1, title: 'EarthServers', url: 'https://earthservers.net', favicon: null, folderId: null, folder_name: null, tags: ['default'], notes: 'EarthServers homepage', position: 1, location: 'toolbar', created_at: new Date().toISOString(), updated_at: new Date().toISOString(), is_system: false },
  { id: 3, profileId: 1, title: 'EarthSocial', url: 'https://social.earthservers.net', favicon: null, folderId: null, folder_name: null, tags: ['default', 'social'], notes: 'EarthServers social platform', position: 2, location: 'toolbar', created_at: new Date().toISOString(), updated_at: new Date().toISOString(), is_system: false },
  { id: 4, profileId: 1, title: 'Private Site', url: 'https://private.example.com', favicon: null, folderId: null, folder_name: null, tags: ['private'], notes: 'A private bookmark', position: 3, location: 'private', created_at: new Date().toISOString(), updated_at: new Date().toISOString(), is_system: false },
  { id: 5, profileId: 1, title: 'Secret Docs', url: 'https://docs.secret.io', favicon: null, folderId: null, folder_name: null, tags: ['private', 'docs'], notes: 'Secret documentation', position: 4, location: 'private', created_at: new Date().toISOString(), updated_at: new Date().toISOString(), is_system: false },
];

// Per-profile feature-gate passwords (hashed in production). Keyed by profileId.
const privateBookmarksPasswords: Record<number, string> = {};
const aiLockPasswords: Record<number, string> = {};

// Password manager data
let passwordManagerMaster: Record<number, string> = {}; // profile_id -> master password
let passwordEntries: any[] = [];

// OTP Authenticator data
let otpMaster: Record<number, string> = {}; // profile_id -> master password
let otpEntries: any[] = [];

// Mock bookmark folders for browser development
let mockBookmarkFolders: any[] = [
  { id: 1, profileId: 1, name: 'News', parentId: null, position: 0, created_at: new Date().toISOString(), bookmark_count: 0 },
  { id: 2, profileId: 1, name: 'Tech', parentId: null, position: 1, created_at: new Date().toISOString(), bookmark_count: 0 },
];

// Mock split view config for browser development
let mockSplitConfig: any = {
  profile_id: 1,
  layout: 'single',
  pane_1_tab_id: 1,
  pane_2_tab_id: null,
  pane_3_tab_id: null,
  pane_4_tab_id: null,
  active_pane: 1,
  pane_sizes: null,
};

// Per-profile incognito state for browser development
let mockIncognitoProfiles: Record<number, boolean> = {};

// Mock command responses for browser development
const mockCommands: Record<string, (args?: any) => any> = {
  // Profile commands
  get_profiles: () => [mockProfile],
  get_active_profile: () => mockProfile,
  create_profile: (args: any) => ({ ...mockProfile, id: Date.now(), name: args.name }),
  switch_profile: () => mockProfile,
  update_profile: (args: any) => ({ ...mockProfile, ...args }),
  delete_profile: () => undefined,
  wipe_profile: () => undefined,
  // "Default" is protected (wipe-only) in the mock too.
  profile_is_protected: (args: any) => args?.profileId === mockProfile.id,
  profile_has_delete_pin: () => false,
  set_profile_delete_pin: () => undefined,

  // Per-profile incognito commands
  get_incognito_status: (args: any) => {
    return mockIncognitoProfiles[args.profileId] || false;
  },
  toggle_incognito: (args: any) => {
    const profileId = args.profileId;
    const newState = !mockIncognitoProfiles[profileId];
    mockIncognitoProfiles[profileId] = newState;
    console.log(`[Mock] Profile ${profileId} incognito toggled to:`, newState);
    return newState;
  },
  set_incognito: (args: any) => {
    mockIncognitoProfiles[args.profileId] = args.enabled;
    return undefined;
  },
  incognito_is_forced: () => false,
  get_incognito_profiles: () => {
    return Object.entries(mockIncognitoProfiles)
      .filter(([_, isIncognito]) => isIncognito)
      .map(([profileId]) => parseInt(profileId));
  },

  // Theme commands
  get_themes: () => [mockTheme],
  get_active_theme: () => mockTheme,
  save_theme: (args: any) => {
    mockTheme = { ...mockTheme, ...args.theme };
    return mockTheme;
  },
  set_active_theme: () => mockTheme,
  delete_theme: () => true,
  apply_preset_theme: (args: any) => {
    // Create a complete theme from the preset
    mockTheme = createThemeFromPreset(args.presetId, args.profileId || 1);
    return mockTheme;
  },
  get_theme_presets: () => mockPresets,
  export_theme: () => JSON.stringify(mockTheme),

  // History commands
  get_history: () => [],
  delete_history_entry: () => true,
  delete_history_by_date_range: () => 0,
  clear_all_history: () => 0,
  get_history_stats: () => ({ total_entries: 0, unique_domains: 0, date_range: { start: null, end: null } }),
  export_history: () => '{"history":[]}',

  // Domain commands (EarthSearch)
  get_domains: () => mockDomains,
  add_domain_entry: (args: any) => {
    const domain = { id: Date.now(), url: args.url, category: args.category, trust_score: args.trustScore, added_date: Date.now().toString(), metadata: null, profileId: args.profileId };
    mockDomains.push(domain);
    return domain;
  },
  update_domain: (args: any) => args.domain,
  delete_domain_entry: (args: any) => {
    mockDomains = mockDomains.filter(d => d.id !== args.domainId);
    return true;
  },
  search_domain_list: (args: any) => mockDomains.filter(d => d.url.includes(args.query) || d.category.includes(args.query)),
  get_domain_lists: () => mockDomainLists,
  create_domain_list: (args: any) => {
    const list = { id: Date.now(), name: args.name, description: args.description, author: 'User', version: '1.0', created_at: Date.now().toString(), profileId: args.profileId, domain_count: 0 };
    mockDomainLists.push(list);
    return list;
  },
  delete_domain_list: () => true,
  get_domain_stats: () => ({
    total_domains: mockDomains.length,
    total_lists: mockDomainLists.length,
    categories: [...new Set(mockDomains.map(d => d.category))].map(c => ({ category: c, count: mockDomains.filter(d => d.category === c).length })),
    avg_trust_score: mockDomains.length > 0 ? mockDomains.reduce((sum, d) => sum + d.trust_score, 0) / mockDomains.length : 0.5
  }),
  get_domain_categories: () => [...new Set(mockDomains.map(d => d.category))],
  export_domains: () => JSON.stringify({ domains: mockDomains }),
  import_domains: () => 0,
  seed_default_domains: () => mockDomains.length,
  force_reseed_domains: () => {
    // In mock mode, just return the count of existing mock domains
    return mockDomains.length;
  },

  // Router — single navigation front door (resolution + render axes)
  navigate: (args: any) => {
    const input: string = args?.url ?? '';
    const isInternal = input.startsWith('earth://') || input.startsWith('tauri://');
    let finalUrl = input;
    let host = '';
    if (!isInternal) {
      if (!/^[a-z][a-z0-9+.-]*:\/\//i.test(input)) {
        finalUrl = input.includes('.') && !/\s/.test(input)
          ? `https://${input}`
          : `https://duckduckgo.com/?q=${encodeURIComponent(input)}`;
      }
      try { host = new URL(finalUrl).hostname; } catch { host = ''; }
    }
    const cls = /\.earth$/.test(host) ? 'earth' : /\.click$/.test(host) ? 'click' : 'legacy';
    console.log('[Mock] navigate', { input, finalUrl, host, class: cls });
    return {
      finalUrl,
      host,
      class: cls,
      resolverSource: 'identity',
      engine: isInternal ? 'internal' : 'webkitgtk',
      isInternal,
    };
  },
  browser_surface_show: () => undefined,
  browser_surface_hide: () => undefined,
  browser_surface_set_bounds: () => undefined,
  browser_surface_back: () => undefined,
  browser_surface_forward: () => undefined,
  browser_surface_reload: () => undefined,
  browser_surface_destroy: () => undefined,
  // NoScript per-site allowlist (JS blocked by default; mock allows nothing)
  browser_surface_get_js: () => false,
  browser_surface_set_js: () => undefined,
  browser_surface_list_js: () => [],
  noscript_list_origins: () => [],
  noscript_get_trust: () => 'untrusted',
  noscript_set_trust: () => undefined,
  privacy_get_config: () => ({
    blockWebrtc: true, blockThirdPartyCookies: true, trackingPrevention: true,
    blockDnsPrefetch: true, spoofUserAgent: true,
  }),
  privacy_set_config: () => undefined,

  // Webview commands for multi-webview browsing
  create_tab_webview: () => {
    console.log('[Mock] create_tab_webview - would create embedded webview');
    return undefined;
  },
  webview_navigate: () => {
    console.log('[Mock] webview_navigate');
    return undefined;
  },
  show_tab_webview: () => undefined,
  hide_tab_webview: () => undefined,
  close_tab_webview: () => undefined,
  update_webview_bounds: () => undefined,
  webview_go_back: () => undefined,
  webview_go_forward: () => undefined,
  webview_reload: () => undefined,
  webview_get_html: () => 'Mock HTML',
  webview_get_url: () => 'http://localhost/',
  webview_get_title: () => 'Mock Page',
  webview_execute_js: () => undefined,
  has_tab_webview: () => false,
  is_webview_embedded: () => null,
  detach_tab_webview: () => 'mock-detached-window',
  open_in_system_browser: (args: any) => {
    window.open(args.url, '_blank');
    return undefined;
  },

  // Memory commands (EarthMemory)
  get_indexed_pages: () => [],
  index_page: (args: any) => ({ id: Date.now(), ...args.page }),
  search_memory: () => [],
  get_favorite_pages: () => [],
  toggle_page_favorite: () => true,
  update_page_tags: () => undefined,
  delete_indexed_page: () => true,
  add_page_note: (args: any) => ({ id: Date.now(), page_id: args.pageId, content: args.content, created_at: Date.now().toString() }),
  get_page_notes: () => [],
  update_page_note: () => undefined,
  delete_page_note: () => true,
  get_memory_stats: () => ({ total_pages: 0, total_notes: 0, favorites_count: 0, total_visits: 0, tags: [] }),
  get_memory_tags: () => [],
  export_memory: () => '{"pages":[]}',
  import_memory: () => 0,

  // Rating commands - mock data uses domain trust_score from mockDomains
  submit_rating: (args: any) => ({
    id: Date.now(),
    domain_id: args.rating.domain_id,
    user_id: args.rating.user_id,
    trust_rating: args.rating.trust_rating,
    bias_rating: args.rating.bias_rating,
    review_text: args.rating.review_text,
    created_at: new Date().toISOString(),
    updated_at: null,
    helpful_count: 0,
    reported: false,
  }),
  get_user_rating: () => null,
  get_domain_ratings: (args: any) => {
    // Return sample ratings based on domain
    const domain = mockDomains.find(d => d.id === args.domainId);
    if (!domain) return [];
    const baseRating = Math.round(domain.trust_score * 5);
    return [
      { id: 1, domain_id: args.domainId, user_id: 'user1', trust_rating: baseRating, bias_rating: 2, review_text: 'Reliable source with good coverage.', created_at: (Date.now() - 86400000).toString(), updated_at: null, helpful_count: 5, reported: false },
      { id: 2, domain_id: args.domainId, user_id: 'user2', trust_rating: Math.max(1, baseRating - 1), bias_rating: 3, review_text: 'Generally accurate, some bias.', created_at: (Date.now() - 172800000).toString(), updated_at: null, helpful_count: 3, reported: false },
    ];
  },
  delete_rating: () => true,
  get_rating_aggregate: (args: any) => {
    // Calculate real aggregate from domain trust_score
    const domain = mockDomains.find(d => d.id === args.domainId);
    if (!domain) {
      return { domain_id: args.domainId, avg_trust: 0, avg_bias: 0, total_ratings: 0, trust_distribution: [0, 0, 0, 0, 0], bias_distribution: [0, 0, 0, 0], last_updated: null };
    }
    const avgTrust = domain.trust_score * 5; // Convert 0-1 to 1-5
    const trustIdx = Math.min(4, Math.max(0, Math.round(avgTrust) - 1));
    const dist = [0, 0, 0, 0, 0];
    dist[trustIdx] = 8;
    dist[Math.max(0, trustIdx - 1)] = 4;
    dist[Math.min(4, trustIdx + 1)] = 3;
    return {
      domain_id: args.domainId,
      avg_trust: avgTrust,
      avg_bias: 2.5,
      total_ratings: 15,
      trust_distribution: dist,
      bias_distribution: [3, 5, 4, 3],
      last_updated: new Date().toISOString(),
    };
  },
  get_rating_summary: (args: any) => {
    // Calculate real summary from domain trust_score
    const domain = mockDomains.find(d => d.id === args.domainId);
    if (!domain) {
      return { domain_url: args.domainUrl || '', avg_trust: 0, avg_bias: 0, total_ratings: 0, trust_label: 'Unknown', bias_label: 'Unknown', category_scores: {} };
    }
    const avgTrust = domain.trust_score * 5;
    const getTrustLabel = (score: number) => {
      if (score < 1.5) return 'Very Low';
      if (score < 2.5) return 'Low';
      if (score < 3.5) return 'Moderate';
      if (score < 4.5) return 'High';
      return 'Very High';
    };
    return {
      domain_url: domain.url,
      avg_trust: avgTrust,
      avg_bias: 2.5,
      total_ratings: 15,
      trust_label: getTrustLabel(avgTrust),
      bias_label: 'Center-Left',
      category_scores: { 'Accuracy': avgTrust, 'Transparency': avgTrust - 0.3, 'Sourcing': avgTrust - 0.1 },
    };
  },
  submit_subdomain_rating: (args: any) => ({
    id: Date.now(),
    parent_domain_id: args.parentDomainId,
    subdomain: args.subdomain,
    avg_trust: args.trust,
    avg_bias: args.bias,
    total_ratings: 1,
  }),
  get_subdomain_ratings: () => [],
  mark_rating_helpful: () => 1,
  report_rating: () => true,
  get_user_rating_history: () => ({
    ratings: [],
    total_ratings: 0,
    avg_trust_given: 3.0,
    avg_bias_given: 2.5,
  }),
  add_rating_category_scores: () => undefined,

  // Tab commands
  create_tab: (args: any) => {
    const newTab = {
      id: Date.now(),
      profile_id: args.profile_id,
      title: args.title || 'New Tab',
      url: args.url,
      favicon: null,
      position: mockTabs.length,
      is_pinned: false,
      is_active: false,
      scroll_position: 0,
      created_at: new Date().toISOString(),
      last_accessed: new Date().toISOString(),
    };
    mockTabs.push(newTab);
    return newTab;
  },
  close_tab: (args: any) => {
    mockTabs = mockTabs.filter(t => t.id !== args.tab_id);
    return undefined;
  },
  get_all_tabs: () => mockTabs,
  update_tab: (args: any) => {
    const tab = mockTabs.find(t => t.id === args.tab_id);
    if (tab) {
      if (args.title) tab.title = args.title;
      if (args.url) tab.url = args.url;
      if (args.favicon) tab.favicon = args.favicon;
    }
    return tab;
  },
  reorder_tabs: (args: any) => {
    const newOrder = args.tab_ids.map((id: number, idx: number) => {
      const tab = mockTabs.find(t => t.id === id);
      if (tab) tab.position = idx;
      return tab;
    });
    mockTabs = newOrder.filter(Boolean);
    return undefined;
  },
  pin_tab: (args: any) => {
    const tab = mockTabs.find(t => t.id === args.tab_id);
    if (tab) tab.is_pinned = args.pinned;
    return tab;
  },
  set_active_tab: (args: any) => {
    mockTabs.forEach(t => t.is_active = t.id === args.tab_id);
    return mockTabs.find(t => t.id === args.tab_id);
  },
  get_tab_history: () => [],
  navigate_tab_back: () => null,
  navigate_tab_forward: () => null,
  duplicate_tab: (args: any) => {
    const original = mockTabs.find(t => t.id === args.tab_id);
    if (original) {
      const newTab = { ...original, id: Date.now(), position: mockTabs.length, is_active: false };
      mockTabs.push(newTab);
      return newTab;
    }
    return null;
  },
  close_tabs_to_right: () => undefined,
  close_unpinned_tabs: () => { mockTabs = mockTabs.filter(t => t.is_pinned); return undefined; },
  close_all_tabs: () => { mockTabs = []; return undefined; },

  // Bookmark commands
  add_bookmark: (args: any) => {
    const bookmark = {
      id: Date.now(),
      profile_id: args.profile_id,
      title: args.title,
      url: args.url,
      favicon: null,
      folder_id: args.folder_id,
      folder_name: null,
      tags: args.tags || [],
      notes: args.notes,
      position: mockBookmarks.length,
      location: args.location || 'toolbar',
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
    };
    mockBookmarks.push(bookmark);
    return bookmark;
  },
  delete_bookmark: (args: any) => {
    mockBookmarks = mockBookmarks.filter(b => b.id !== args.bookmark_id);
    return undefined;
  },
  get_all_bookmarks: () => mockBookmarks,
  get_bookmarks_by_folder: (args: any) => mockBookmarks.filter(b => b.folder_id === args.folder_id),
  search_bookmarks: (args: any) => mockBookmarks.filter(b =>
    b.title.toLowerCase().includes(args.query.toLowerCase()) ||
    b.url.toLowerCase().includes(args.query.toLowerCase())
  ),
  update_bookmark: (args: any) => {
    const bookmark = mockBookmarks.find(b => b.id === args.bookmark_id);
    if (bookmark) {
      if (args.title) bookmark.title = args.title;
      if (args.url) bookmark.url = args.url;
      if (args.folder_id !== undefined) bookmark.folder_id = args.folder_id;
      if (args.tags) bookmark.tags = args.tags;
      if (args.notes !== undefined) bookmark.notes = args.notes;
      if (args.location) bookmark.location = args.location;
      bookmark.updated_at = new Date().toISOString();
    }
    return bookmark;
  },
  is_url_bookmarked: (args: any) => {
    const bookmark = mockBookmarks.find(b => b.url === args.url);
    return bookmark ? bookmark.id : null;
  },
  create_bookmark_folder: (args: any) => {
    const folder = {
      id: Date.now(),
      profile_id: args.profile_id,
      name: args.name,
      parent_id: args.parent_id,
      position: mockBookmarkFolders.length,
      created_at: new Date().toISOString(),
      bookmark_count: 0,
    };
    mockBookmarkFolders.push(folder);
    return folder;
  },
  get_bookmark_folders: () => mockBookmarkFolders,
  delete_bookmark_folder: (args: any) => {
    mockBookmarkFolders = mockBookmarkFolders.filter(f => f.id !== args.folder_id);
    return undefined;
  },
  rename_bookmark_folder: (args: any) => {
    const folder = mockBookmarkFolders.find(f => f.id === args.folder_id);
    if (folder) folder.name = args.name;
    return folder;
  },
  export_bookmarks: () => JSON.stringify({ bookmarks: mockBookmarks, folders: mockBookmarkFolders }),
  import_bookmarks: () => 0,

  // Private bookmarks commands
  get_private_bookmarks: (args: any) => mockBookmarks.filter(b => b.location === 'private' && b.profile_id === args.profile_id),
  set_private_bookmarks_password: (args: any) => {
    privateBookmarksPasswords[args.profileId] = args.password;
    return true;
  },
  verify_private_bookmarks_password: (args: any) => {
    const pw = privateBookmarksPasswords[args.profileId];
    if (!pw) return true; // No password set
    return args.password === pw;
  },
  has_private_bookmarks_password: (args: any) => privateBookmarksPasswords[args.profileId] != null,
  get_bookmarks_by_location: (args: any) => mockBookmarks.filter(b => b.location === args.location && b.profile_id === args.profile_id && !b.url.startsWith('earth://')),

  // Local AI / History password gate (per profile)
  ai_lock_has_password: (args: any) => aiLockPasswords[args.profileId] != null,
  ai_lock_verify_password: (args: any) => {
    const pw = aiLockPasswords[args.profileId];
    if (!pw) return true;
    return args.password === pw;
  },
  ai_lock_set_password: (args: any) => { aiLockPasswords[args.profileId] = args.password; return undefined; },
  ai_lock_remove_password: (args: any) => {
    const pw = aiLockPasswords[args.profileId];
    if (pw && args.password !== pw) throw new Error('Incorrect password');
    delete aiLockPasswords[args.profileId];
    return undefined;
  },

  // Password Manager commands
  lock_all_vaults: () => undefined,
  has_password_manager_master: (args: any) => passwordManagerMaster[args.profile_id] !== undefined,
  verify_password_manager_master: (args: any) => {
    const stored = passwordManagerMaster[args.profile_id];
    if (!stored) return true;
    return args.password === stored;
  },
  set_password_manager_master: (args: any) => {
    passwordManagerMaster[args.profile_id] = args.password;
    return true;
  },
  get_password_entries: (args: any) => passwordEntries.filter(e => e.profile_id === args.profile_id),
  add_password_entry: (args: any) => {
    const entry = {
      id: Date.now(),
      profile_id: args.profile_id,
      title: args.title,
      username: args.username,
      password: args.password,
      url: args.url,
      notes: args.notes,
      category: args.category || 'General',
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
    };
    passwordEntries.push(entry);
    return entry;
  },
  update_password_entry: (args: any) => {
    const entry = passwordEntries.find(e => e.id === args.entry_id);
    if (entry) {
      entry.title = args.title;
      entry.username = args.username;
      entry.password = args.password;
      entry.url = args.url;
      entry.notes = args.notes;
      entry.category = args.category;
      entry.updated_at = new Date().toISOString();
    }
    return entry;
  },
  delete_password_entry: (args: any) => {
    passwordEntries = passwordEntries.filter(e => e.id !== args.entry_id);
    return true;
  },

  // OTP Authenticator commands
  has_otp_master: (args: any) => otpMaster[args.profile_id] !== undefined,
  verify_otp_master: (args: any) => {
    const stored = otpMaster[args.profile_id];
    if (!stored) return true;
    return args.password === stored;
  },
  set_otp_master: (args: any) => {
    otpMaster[args.profile_id] = args.password;
    return true;
  },
  get_otp_entries: (args: any) => otpEntries.filter(e => e.profile_id === args.profile_id),
  add_otp_entry: (args: any) => {
    const entry = {
      id: Date.now(),
      profile_id: args.profile_id,
      name: args.name,
      issuer: args.issuer,
      secret: args.secret,
      algorithm: args.algorithm || 'SHA1',
      digits: args.digits || 6,
      period: args.period || 30,
      created_at: new Date().toISOString(),
    };
    otpEntries.push(entry);
    return entry;
  },
  update_otp_entry: (args: any) => {
    const entry = otpEntries.find(e => e.id === args.entry_id);
    if (entry) {
      entry.name = args.name;
      entry.issuer = args.issuer;
      entry.secret = args.secret;
      entry.algorithm = args.algorithm;
      entry.digits = args.digits;
      entry.period = args.period;
    }
    return entry;
  },
  delete_otp_entry: (args: any) => {
    otpEntries = otpEntries.filter(e => e.id !== args.entry_id);
    return true;
  },

  // Split view commands
  get_split_config: () => mockSplitConfig,
  set_split_layout: (args: any) => {
    mockSplitConfig.layout = args.layout;
    return mockSplitConfig;
  },
  set_pane_tab: (args: any) => {
    const key = `pane_${args.pane_number}_tab_id` as keyof typeof mockSplitConfig;
    (mockSplitConfig as any)[key] = args.tab_id;
    return mockSplitConfig;
  },
  set_active_pane: (args: any) => {
    mockSplitConfig.active_pane = args.pane_number;
    return mockSplitConfig;
  },
  cycle_pane: (args: any) => {
    const maxPanes = mockSplitConfig.layout === 'single' ? 1 : mockSplitConfig.layout === 'quad' ? 4 : 2;
    if (args.direction > 0) {
      mockSplitConfig.active_pane = mockSplitConfig.active_pane >= maxPanes ? 1 : mockSplitConfig.active_pane + 1;
    } else {
      mockSplitConfig.active_pane = mockSplitConfig.active_pane <= 1 ? maxPanes : mockSplitConfig.active_pane - 1;
    }
    return mockSplitConfig;
  },
  update_pane_sizes: (args: any) => {
    mockSplitConfig.pane_sizes = args.sizes;
    return mockSplitConfig;
  },
  swap_panes: () => mockSplitConfig,
  reset_split_view: () => {
    mockSplitConfig.layout = 'single';
    mockSplitConfig.active_pane = 1;
    return mockSplitConfig;
  },

  // EarthMultiMedia commands - privacy-first (default: no history)
  get_media_privacy_settings: (args: any) => ({
    profile_id: args.profile_id,
    history_enabled: false,
    playlist_history_enabled: false,
    require_password: false,
    require_otp: false,
    password_hash: null,
    otp_secret: null,
    auto_clear_history_days: null,
  }),
  update_media_privacy_settings: (args: any) => args.settings,
  set_media_password: () => undefined,
  verify_media_password: () => true,
  generate_media_otp_secret: () => 'JBSWY3DPEHPK3PXP',
  verify_media_otp: () => true,
  add_media_history_entry: () => null, // Returns null when history disabled
  get_media_history: () => [],
  clear_media_history: () => 0,
  delete_media_history_entry: () => undefined,
  create_media_playlist: (args: any) => ({
    id: Date.now(),
    profile_id: args.profile_id,
    name: args.name,
    description: args.description,
    thumbnail: null,
    is_encrypted: args.encrypted,
    created_at: new Date().toISOString(),
    updated_at: null,
    item_count: 0,
  }),
  get_media_playlists: () => [],
  delete_media_playlist: () => undefined,
  add_to_media_playlist: (args: any) => ({
    id: Date.now(),
    playlist_id: args.playlist_id,
    source: args.source,
    media_type: args.media_type,
    title: args.title,
    thumbnail: args.thumbnail,
    position: 0,
    added_at: new Date().toISOString(),
  }),
  get_media_playlist_items: () => [],
  remove_from_media_playlist: () => undefined,
  reorder_media_playlist_items: () => undefined,
  get_media_stats: () => ({
    total_played: 0,
    total_time_watched: 0,
    videos_watched: 0,
    images_viewed: 0,
    audio_played: 0,
    playlists_count: 0,
  }),

  // Web Scraper commands
  create_scraping_job: (_args: any) => Date.now(),
  get_scraping_jobs: () => [],
  get_scraping_job: () => ({
    id: 1,
    profile_id: 1,
    name: 'Mock Job',
    base_url: 'https://example.com',
    url_pattern: null,
    max_depth: 2,
    max_pages: 100,
    content_selectors: [],
    schedule_cron: null,
    status: 'pending',
    last_run_at: null,
    pages_scraped: 0,
    created_at: new Date().toISOString(),
  }),
  delete_scraping_job: () => undefined,
  get_scraped_pages: () => [],
  search_scraped_content: () => [],

  // Window management commands (for tab drag-out)
  create_detached_window: (args: any) => {
    console.log('[Mock] Would create detached window for tab:', args.tabId, 'at position:', args.x, args.y);
    // In browser mode, we can't actually create a new window, but we can open a popup
    const windowId = `reclaim-${args.tabId}`;
    return windowId;
  },
  close_window_by_label: (args: any) => {
    console.log('[Mock] Would close window:', args.label);
    return undefined;
  },
  get_all_windows: () => ['main'],

  // ============================================================================
  // Servo Browser Commands (earth-servo crate)
  // ============================================================================
  create_servo_browser: (args: any) => {
    console.log('[Mock] create_servo_browser - would launch Servo for:', args.webviewId, args.url);
    // In browser mode, open in new tab/window
    window.open(args.url, '_blank');
    return undefined;
  },
  servo_navigate: (args: any) => {
    console.log('[Mock] servo_navigate:', args.webviewId, args.url);
    return undefined;
  },
  servo_close: (args: any) => {
    console.log('[Mock] servo_close:', args.webviewId);
    return undefined;
  },
  servo_close_all: () => {
    console.log('[Mock] servo_close_all');
    return undefined;
  },
  servo_get_url: (args: any) => {
    console.log('[Mock] servo_get_url:', args.webviewId);
    return 'http://localhost/';
  },
  servo_has_webview: (args: any) => {
    console.log('[Mock] servo_has_webview:', args.webviewId);
    return false;
  },
  servo_list_webviews: () => {
    console.log('[Mock] servo_list_webviews');
    return [];
  },

  // ============================================================================
  // GStreamer Media Player Commands (earth-media crate)
  // ============================================================================
  media_load: (args: any) => {
    console.log('[Mock] media_load:', args.uri);
    return undefined;
  },
  media_play: () => {
    console.log('[Mock] media_play');
    return undefined;
  },
  media_pause: () => {
    console.log('[Mock] media_pause');
    return undefined;
  },
  media_stop: () => {
    console.log('[Mock] media_stop');
    return undefined;
  },
  media_seek: (args: any) => {
    console.log('[Mock] media_seek:', args.positionMs, 'ms');
    return undefined;
  },
  media_set_volume: (args: any) => {
    console.log('[Mock] media_set_volume:', args.volume);
    return undefined;
  },
  media_get_status: () => {
    console.log('[Mock] media_get_status');
    return {
      state: 'Stopped',
      position_ms: 0,
      duration_ms: 0,
      volume: 1.0,
      muted: false,
      info: {
        uri: null,
        title: null,
        artist: null,
        album: null,
        duration_ms: null,
        width: null,
        height: null,
        is_video: false,
        is_live: false,
      },
    };
  },
  media_skip_forward: (args: any) => {
    console.log('[Mock] media_skip_forward:', args.seconds, 'seconds');
    return undefined;
  },
  media_skip_backward: (args: any) => {
    console.log('[Mock] media_skip_backward:', args.seconds, 'seconds');
    return undefined;
  },
  media_check_gstreamer: () => {
    console.log('[Mock] media_check_gstreamer');
    return 'GStreamer 1.x (mock)';
  },
  media_set_muted: (args: any) => {
    console.log('[Mock] media_set_muted:', args.muted);
    return undefined;
  },

  // YouTube Commands
  play_youtube: (args: any) => {
    console.log('[Mock] play_youtube:', args.url);
    // Simulate YouTube info extraction
    return {
      title: 'Mock YouTube Video',
      duration: 300.0,
      thumbnail: 'https://i.ytimg.com/vi/dQw4w9WgXcQ/maxresdefault.jpg',
      uploader: 'Mock Channel',
      description: 'This is a mock YouTube video for testing.',
      view_count: 1000000,
    };
  },
  get_youtube_info: (args: any) => {
    console.log('[Mock] get_youtube_info:', args.url);
    return {
      title: 'Mock YouTube Video Info',
      duration: 300.0,
      thumbnail: 'https://i.ytimg.com/vi/dQw4w9WgXcQ/maxresdefault.jpg',
      uploader: 'Mock Channel',
      description: 'This is a mock YouTube video for testing.',
      view_count: 1000000,
    };
  },
  check_youtube_available: () => {
    console.log('[Mock] check_youtube_available');
    return false; // In browser mode, yt-dlp is not available
  },
  is_youtube_url: (args: any) => {
    const url = args.url || '';
    return url.includes('youtube.com') || url.includes('youtu.be');
  },

  // ============================================================================
  // Multi-Player Commands (for multi-pane GStreamer support)
  // ============================================================================
  player_load: (args: any) => {
    console.log('[Mock] player_load:', args.playerId, args.uri);
    return undefined;
  },
  player_play: (args: any) => {
    console.log('[Mock] player_play:', args.playerId);
    return undefined;
  },
  player_pause: (args: any) => {
    console.log('[Mock] player_pause:', args.playerId);
    return undefined;
  },
  player_stop: (args: any) => {
    console.log('[Mock] player_stop:', args.playerId);
    return undefined;
  },
  player_seek: (args: any) => {
    console.log('[Mock] player_seek:', args.playerId, args.positionMs, 'ms');
    return undefined;
  },
  player_set_volume: (args: any) => {
    console.log('[Mock] player_set_volume:', args.playerId, args.volume);
    return undefined;
  },
  player_set_muted: (args: any) => {
    console.log('[Mock] player_set_muted:', args.playerId, args.muted);
    return undefined;
  },
  player_get_status: (args: any) => {
    console.log('[Mock] player_get_status:', args.playerId);
    return {
      state: 'Stopped',
      position_ms: 0,
      duration_ms: 0,
      volume: 1.0,
      muted: false,
      info: {
        uri: null,
        title: null,
        artist: null,
        album: null,
        duration_ms: null,
        width: null,
        height: null,
        is_video: false,
        is_live: false,
      },
    };
  },
  player_skip_forward: (args: any) => {
    console.log('[Mock] player_skip_forward:', args.playerId, args.seconds, 'seconds');
    return undefined;
  },
  player_skip_backward: (args: any) => {
    console.log('[Mock] player_skip_backward:', args.playerId, args.seconds, 'seconds');
    return undefined;
  },
  player_play_youtube: (args: any) => {
    console.log('[Mock] player_play_youtube:', args.playerId, args.url);
    return {
      title: 'Mock YouTube Video',
      duration: 300.0,
      thumbnail: 'https://i.ytimg.com/vi/dQw4w9WgXcQ/maxresdefault.jpg',
      uploader: 'Mock Channel',
      description: 'This is a mock YouTube video for testing.',
      view_count: 1000000,
    };
  },
  player_remove: (args: any) => {
    console.log('[Mock] player_remove:', args.playerId);
    return undefined;
  },
  player_get_all_statuses: () => {
    console.log('[Mock] player_get_all_statuses');
    return {}; // Empty map of player statuses
  },
  player_stop_all: () => {
    console.log('[Mock] player_stop_all');
    return undefined;
  },
  player_list: () => {
    console.log('[Mock] player_list');
    return []; // Empty list of player IDs
  },
};

// Wrapper for Tauri invoke that falls back to mock data in browser
export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri()) {
    // Use actual Tauri invoke (v2 uses @tauri-apps/api/core)
    const { invoke: tauriInvoke } = await import('@tauri-apps/api/core');
    return tauriInvoke<T>(cmd, args);
  }

  // Browser fallback with mock data
  console.log(`[Mock] ${cmd}`, args);

  const mockFn = mockCommands[cmd];
  if (mockFn) {
    // Simulate async delay
    await new Promise(resolve => setTimeout(resolve, 100));
    return mockFn(args) as T;
  }

  console.warn(`[Mock] Unknown command: ${cmd}`);
  throw new Error(`Unknown command: ${cmd}`);
}

// Wrapper for Tauri event listener with browser fallback
export async function listen<T>(
  event: string,
  handler: (event: { payload: T }) => void
): Promise<() => void> {
  if (isTauri()) {
    const { listen: tauriListen } = await import('@tauri-apps/api/event');
    return tauriListen<T>(event, handler);
  }

  // Browser fallback - no-op, return empty unlisten function
  console.log(`[Mock] listen for event: ${event}`);
  return () => {
    console.log(`[Mock] unlisten for event: ${event}`);
  };
}

// Wrapper for Tauri event emit with browser fallback
export async function emit<T>(event: string, payload?: T): Promise<void> {
  if (isTauri()) {
    const { emit: tauriEmit } = await import('@tauri-apps/api/event');
    return tauriEmit(event, payload);
  }

  // Browser fallback - log and dispatch custom event
  console.log(`[Mock] emit event: ${event}`, payload);
}

// Emit to a specific window (for cross-window communication)
export async function emitTo<T>(target: string, event: string, payload?: T): Promise<void> {
  if (isTauri()) {
    const { emitTo: tauriEmitTo } = await import('@tauri-apps/api/event');
    return tauriEmitTo(target, event, payload);
  }

  // Browser fallback - log
  console.log(`[Mock] emitTo ${target}: ${event}`, payload);
}
