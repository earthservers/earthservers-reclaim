# Theme Cache & Animation System Documentation

## Overview

EarthSocial implements a unified theme caching system that ensures consistent theme colors and animations across all pages while maintaining proper context awareness for profile, community, and site-wide themes.

---

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [Cache Structure](#cache-structure)
3. [Theme Context (Single Source of Truth)](#theme-context-single-source-of-truth)
4. [Animation System](#animation-system)
5. [Loading Screen Continuity](#loading-screen-continuity)
6. [Page-Specific Implementation](#page-specific-implementation)
7. [Common Patterns](#common-patterns)
8. [Troubleshooting Guide](#troubleshooting-guide)

---

## Architecture Overview

### Three Types of Themes

1. **Site Theme** - Global theme for all site-wide pages (Feed, Friends, Messages, etc.)
2. **Profile Theme** - Individual user's profile customization
3. **Community Theme** - Community-specific theme customization

### Key Principles

- **Single Source of Truth**: `ThemeContext` is the authoritative source for site-wide theme
- **Versioned Caching**: SessionStorage cache with version numbers to prevent stale data
- **Lazy Registration**: Pages register their theme AFTER loading completes to prevent race conditions
- **Animation Merging**: Partial backend animations are merged with full preset defaults

---

## Cache Structure

### SessionStorage Keys

```typescript
// Site theme cache
sessionStorage.setItem('siteThemeCache', JSON.stringify({
  version: 3,
  theme: { /* full theme object */ },
  customization: { /* customization settings */ },
  animations: { /* merged animation data */ },
  timestamp: Date.now()
}));

// Profile theme cache
sessionStorage.setItem('profileThemeCache_{userId}', JSON.stringify({
  version: 1,
  theme: { /* profile theme */ },
  customization: { /* profile customization */ },
  animations: { /* profile animations */ },
  timestamp: Date.now()
}));

// Community theme cache
sessionStorage.setItem('communityThemeCache_{communityId}', JSON.stringify({
  version: 1,
  theme: { /* community theme */ },
  customization: { /* community customization */ },
  animations: { /* community animations */ },
  timestamp: Date.now()
}));

// Loading screen theme (for continuity)
sessionStorage.setItem('currentPageTheme', JSON.stringify({
  type: 'site' | 'profile' | 'community',
  userId?: string,
  communityId?: string,
  colors: {
    primaryColor: '#...',
    secondaryColor: '#...',
    textColor: '#...',
    accentColor: '#...'
  }
}));
```

### Cache Version Management

The cache version is incremented when the data structure changes. ThemeContext checks the version and invalidates outdated caches:

```typescript
const SITE_THEME_CACHE_VERSION = 3;

if (cachedData.version !== SITE_THEME_CACHE_VERSION) {
  console.warn('[ThemeContext] Cache version mismatch, invalidating...');
  sessionStorage.removeItem('siteThemeCache');
}
```

---

## Theme Context (Single Source of Truth)

### Location
`frontend/src/context/ThemeContext.tsx`

### Responsibilities

1. **Load and cache site theme** from backend API
2. **Merge partial animations** with full preset defaults
3. **Provide unified interface** for all pages via `useTheme()` hook
4. **Emit theme update events** when customization changes
5. **Manage animation state** for site-wide pages

### Key States

```typescript
const ThemeContext = {
  theme: Theme,                    // Current displayed theme colors
  currentPreset: PresetThemeKey,   // Base preset (ocean, forest, sunset, etc.)
  siteCustomization: any,          // Customization settings (color space, limits, etc.)
  siteAnimations: any,             // Merged animations (characters, bubbles, decorations)
  updateTheme: (updates) => void,  // Update theme colors
  setCurrentPreset: (preset) => void,
  // ... other methods
};
```

### Animation Merging Logic

Backend may return partial animation data (only customized fields). ThemeContext merges this with full preset defaults:

```typescript
const fullAnimations = getThemeAnimations(preset);
const mergedAnimations = {
  characters: fullAnimations.characters.map((defaultChar) => {
    const customChar = backendAnimations.characters?.find((c) => c.id === defaultChar.id);
    return customChar ? { ...defaultChar, ...customChar } : defaultChar;
  }),
  bubbles: backendAnimations.bubbles && fullAnimations.bubbles
    ? { ...fullAnimations.bubbles, ...backendAnimations.bubbles }
    : fullAnimations.bubbles,
  decorations: fullAnimations.decorations.map((defaultDeco) => {
    const customDeco = backendAnimations.decorations?.find((d) => d.id === defaultDeco.id);
    return customDeco ? { ...defaultDeco, ...customDeco } : defaultDeco;
  }),
};
```

**Why this is critical**: Backend animations may only contain `{ id, speed, enabled }` but animations need `{ id, type, count, size, positions, speed, enabled }` to render properly.

---

## Animation System

### Animation Data Structure

```typescript
interface Animations {
  characters: Array<{
    id: string;           // e.g., "fish", "turtle"
    type: string;         // e.g., "swimming", "floating"
    enabled: boolean;
    count: number;        // How many instances
    speed: number;        // Animation speed multiplier
    size: { min: number; max: number };
    positions?: Array<{ x: number; y: number }>;
  }>;
  bubbles: {
    id: string;
    type: string;
    enabled: boolean;
    count: number;
    speed: number;
    size: { min: number; max: number };
  } | null;
  decorations: Array<{
    id: string;
    type: string;
    enabled: boolean;
    count: number;
    positions?: Array<{ x: number; y: number }>;
  }>;
}
```

### Animation Initialization Pattern

Every page that shows animations should:

1. **Initialize from ThemeContext**:
```typescript
const { siteAnimations, currentPreset } = useTheme();

const [themeAnimations, setThemeAnimations] = useState(() => {
  if (siteAnimations) {
    return siteAnimations;
  }
  return getThemeAnimations(currentPreset);
});
```

2. **Sync with ThemeContext changes**:
```typescript
useEffect(() => {
  if (siteAnimations) {
    setThemeAnimations(siteAnimations);
  } else {
    setThemeAnimations(getThemeAnimations(currentPreset));
  }
}, [siteAnimations, currentPreset]);
```

3. **Listen for customizer updates**:
```typescript
useEffect(() => {
  const handleThemeUpdate = (event: any) => {
    const { animations, animationsEnabled } = event.detail;

    if (typeof animationsEnabled === 'boolean') {
      setAnimationsEnabled(animationsEnabled);
    }

    if (animations) {
      const fixedAnimations = { ...animations };

      // Fix incomplete bubbles structure
      if (fixedAnimations.bubbles && typeof fixedAnimations.bubbles === 'object') {
        if (!fixedAnimations.bubbles.id || !fixedAnimations.bubbles.type) {
          fixedAnimations.bubbles = fixedAnimations.bubbles.enabled
            ? {
                id: 'bubbles',
                type: 'rising',
                enabled: true,
                count: 25,
                speed: 0.8,
                size: { min: 15, max: 35 },
              }
            : null;
        }
      }

      // Validate structure
      const hasValidStructure =
        fixedAnimations.characters && Array.isArray(fixedAnimations.characters) &&
        fixedAnimations.decorations && Array.isArray(fixedAnimations.decorations) &&
        fixedAnimations.characters.length > 0 &&
        fixedAnimations.characters[0]?.id;

      if (hasValidStructure) {
        setThemeAnimations(fixedAnimations);
      }
    }
  };

  window.addEventListener('siteThemeUpdated', handleThemeUpdate);
  return () => window.removeEventListener('siteThemeUpdated', handleThemeUpdate);
}, []);
```

4. **Pass to AnimationLayer**:
```typescript
<AnimationLayer
  animations={themeAnimations}
  isMobile={false}
  bubbleColor={theme.accentColor}
  enabled={animationsEnabled}
/>
```

---

## Loading Screen Continuity

### Problem
When navigating between pages, the loading screen should show the colors of the page you're coming FROM, not the page you're going TO.

### Solution: NavigationContext

#### Location
`frontend/src/context/NavigationContext.tsx`

#### How It Works

1. **Each page registers its theme** via `setCurrentTheme()`:
```typescript
const { setCurrentTheme } = useNavigation();

useEffect(() => {
  if (!loading && theme?.primaryColor && siteCustomization) {
    setCurrentTheme({
      type: 'site',
      colors: {
        primaryColor: theme.primaryColor,
        secondaryColor: theme.secondaryColor,
        textColor: theme.textColor,
        accentColor: theme.accentColor
      }
    });
  }
}, [loading, theme, siteCustomization, setCurrentTheme]);
```

2. **LoadingScreen reads from sessionStorage**:
```typescript
const currentPageTheme = sessionStorage.getItem('currentPageTheme');
if (currentPageTheme) {
  const { colors } = JSON.parse(currentPageTheme);
  // Use these colors for loading screen
}
```

### Critical Pattern: Delayed Registration

**DO NOT** register theme on mount. Register AFTER loading completes:

```typescript
// ❌ WRONG - Registers immediately on mount
useEffect(() => {
  if (theme?.primaryColor) {
    setCurrentTheme({ ... });
  }
}, [theme]);

// ✅ CORRECT - Registers after loading completes
useEffect(() => {
  if (!loading && theme?.primaryColor) {
    setCurrentTheme({ ... });
  }
}, [loading, theme]);
```

**Why**: If you register on mount, you overwrite the previous page's theme BEFORE the loading screen can read it, causing the loading screen to show the wrong colors.

### Pages With Loading State

Pages with a `loading` state variable (Messages, Friends) should use the `!loading` pattern:

- **Messages.tsx**: Lines 123-136
- **Friends.tsx**: Lines 79-92

### Pages Without Loading State

Pages without loading (FeedClient, CommunityList, etc.) can use a small timeout delay:

```typescript
useEffect(() => {
  if (theme?.primaryColor && siteCustomization) {
    const timer = setTimeout(() => {
      setCurrentTheme({ ... });
    }, 100);
    return () => clearTimeout(timer);
  }
}, [theme, siteCustomization, setCurrentTheme]);
```

---

## Page-Specific Implementation

### Site-Wide Pages

These pages use site theme from ThemeContext:

#### FeedClient.tsx
- Location: `frontend/src/components/desktop/FeedClient.tsx`
- Uses: `siteAnimations`, `siteCustomization` from `useTheme()`
- Registers: Site theme with 100ms delay (lines 302-321)
- Has: Animation sync + `siteThemeUpdated` listener

#### CommunityList.tsx
- Location: `frontend/src/components/desktop/CommunityList.tsx`
- Uses: `siteAnimations` from `useTheme()`
- Registers: Site theme with 100ms delay (lines 517-535)
- Has: Animation sync (lines 507-514) + `siteThemeUpdated` listener (lines 537-579)

#### Friends.tsx
- Location: `frontend/src/pages/Friends.tsx`
- Uses: `siteAnimations`, `siteCustomization` from `useTheme()`
- Registers: Site theme AFTER loading completes (lines 79-92)
- Has: Animation sync (lines 210-217) + `siteThemeUpdated` listener (lines 221-261)
- Has: Minimum 200ms loading time to ensure continuity (lines 324-334)

#### Messages.tsx
- Location: `frontend/src/pages/Messages.tsx`
- Uses: `siteAnimations`, `siteCustomization` from `useTheme()`
- Registers: Site theme AFTER loading completes (lines 123-136)
- Has: Animation sync (lines 138-145) + `siteThemeUpdated` listener (lines 147-188)
- Has: Minimum 200ms loading time (lines 390-400)

#### TagsPage.tsx
- Location: `frontend/src/pages/TagsPage.tsx`
- Uses: `siteAnimations` from `useTheme()`
- Registers: Site theme with 100ms delay (lines 80-98)
- Has: Animation sync (lines 95-102)

#### Moderation.tsx
- Location: `frontend/src/pages/Moderation.tsx`
- Uses: `siteAnimations` from `useTheme()`
- Registers: Site theme with 100ms delay (lines 63-81)
- Has: Animation sync (lines 78-85)

#### Ads.tsx
- Location: `frontend/src/pages/Ads.tsx`
- Uses: `siteAnimations`, `siteCustomization` from `useTheme()`
- Registers: Site theme with 100ms delay (lines 56-74)
- Has: Animation sync (lines 44-51) + `siteThemeUpdated` listener (lines 82-129)
- Has: `animationsEnabled` state for toggle support

#### Marketplace.tsx
- Location: `frontend/src/pages/Marketplace.tsx`
- Uses: `siteAnimations`, `siteCustomization` from `useTheme()`
- Registers: Site theme with 100ms delay (lines 104-122)
- Has: Animation sync (lines 92-99) + `siteThemeUpdated` listener (lines 132-182)
- Has: `animationsEnabled` state for toggle support

### Context-Aware Pages

These pages show different themes based on navigation context:

#### PostDetail.tsx
- Location: `frontend/src/pages/PostDetail.tsx`
- Special: Shows profile/community/site theme based on context
- Logic:
  - If from profile → Use author's profile theme
  - If from community → Use community theme
  - If from site/feed → Use site theme from ThemeContext
- Uses: `siteAnimations` only when NOT showing profile/community (lines 239-249)
- Has: `siteThemeUpdated` listener that respects context (lines 251-291)

#### ProfileClient.tsx
- Location: `frontend/src/components/desktop/ProfileClient.tsx`
- Uses: Own profile theme cache, NOT ThemeContext
- Registers: Profile theme to NavigationContext for loading screen continuity
- Does NOT sync with `siteAnimations` (profile has its own animations)

#### CommunityClient.tsx
- Location: `frontend/src/components/desktop/CommunityClient.tsx`
- Uses: Own community theme cache, NOT ThemeContext
- Registers: Community theme to NavigationContext for loading screen continuity
- Does NOT sync with `siteAnimations` (community has its own animations)

---

## Common Patterns

### Pattern 1: Standard Site Page

```typescript
export default function MyPage() {
  const { theme, currentPreset, siteAnimations, siteCustomization } = useTheme();
  const { setCurrentTheme } = useNavigation();

  // Initialize animations from ThemeContext
  const [themeAnimations, setThemeAnimations] = useState(() => {
    if (siteAnimations) return siteAnimations;
    return getThemeAnimations(currentPreset);
  });

  const [animationsEnabled, setAnimationsEnabled] = useState(true);

  // Register theme AFTER loading completes
  useEffect(() => {
    if (!loading && theme?.primaryColor && siteCustomization) {
      setCurrentTheme({
        type: 'site',
        colors: {
          primaryColor: theme.primaryColor,
          secondaryColor: theme.secondaryColor || theme.primaryColor,
          textColor: theme.textColor || '#ffffff',
          accentColor: theme.accentColor || theme.primaryColor
        }
      });
    }
  }, [loading, theme?.primaryColor, theme?.secondaryColor, theme?.textColor, theme?.accentColor, siteCustomization, setCurrentTheme]);

  // Sync animations with ThemeContext
  useEffect(() => {
    if (siteAnimations) {
      setThemeAnimations(siteAnimations);
    } else {
      setThemeAnimations(getThemeAnimations(currentPreset));
    }
  }, [siteAnimations, currentPreset]);

  // Listen for theme customizer updates
  useEffect(() => {
    const handleThemeUpdate = (event: any) => {
      const { animations, animationsEnabled: enabled } = event.detail;

      if (typeof enabled === 'boolean') {
        setAnimationsEnabled(enabled);
      }

      if (animations) {
        // Validate and fix animations structure
        const fixedAnimations = { ...animations };

        if (fixedAnimations.bubbles && typeof fixedAnimations.bubbles === 'object') {
          if (!fixedAnimations.bubbles.id || !fixedAnimations.bubbles.type) {
            fixedAnimations.bubbles = fixedAnimations.bubbles.enabled
              ? {
                  id: 'bubbles',
                  type: 'rising',
                  enabled: true,
                  count: 25,
                  speed: 0.8,
                  size: { min: 15, max: 35 },
                }
              : null;
          }
        }

        const hasValidStructure =
          fixedAnimations.characters && Array.isArray(fixedAnimations.characters) &&
          fixedAnimations.decorations && Array.isArray(fixedAnimations.decorations) &&
          fixedAnimations.characters.length > 0 &&
          fixedAnimations.characters[0]?.id;

        if (hasValidStructure) {
          setThemeAnimations(fixedAnimations);
        }
      }
    };

    window.addEventListener('siteThemeUpdated', handleThemeUpdate);
    return () => window.removeEventListener('siteThemeUpdated', handleThemeUpdate);
  }, []);

  return (
    <div>
      <AnimationLayer
        animations={themeAnimations}
        isMobile={false}
        bubbleColor={theme.accentColor}
        enabled={animationsEnabled}
      />
      {/* Page content */}
    </div>
  );
}
```

### Pattern 2: Profile/Community Page

```typescript
export default function ProfilePage() {
  const { user } = useAuth();
  const { setCurrentTheme } = useNavigation();

  // Load profile theme from cache or API
  useEffect(() => {
    async function loadProfileTheme() {
      const cached = getCachedTheme('profile', user.id);
      if (cached) {
        setProfileTheme(cached.theme);
        setProfileAnimations(cached.animations);
      } else {
        const data = await fetchProfileTheme(user.id);
        // Merge animations with preset defaults
        const mergedAnimations = mergeAnimations(data.animations, basePreset);
        setProfileTheme(data.theme);
        setProfileAnimations(mergedAnimations);
        // Cache it
        sessionStorage.setItem(`profileThemeCache_${user.id}`, JSON.stringify({
          version: 1,
          theme: data.theme,
          animations: mergedAnimations,
          timestamp: Date.now()
        }));
      }
    }
    loadProfileTheme();
  }, [user.id]);

  // Register profile theme for loading screen continuity
  useEffect(() => {
    if (profileTheme?.primaryColor) {
      setCurrentTheme({
        type: 'profile',
        userId: user.id,
        colors: {
          primaryColor: profileTheme.primaryColor,
          secondaryColor: profileTheme.secondaryColor,
          textColor: profileTheme.textColor,
          accentColor: profileTheme.accentColor
        }
      });
    }
  }, [profileTheme, user.id, setCurrentTheme]);

  return (
    <div>
      <AnimationLayer
        animations={profileAnimations}
        isMobile={false}
        bubbleColor={profileTheme.accentColor}
      />
      {/* Page content */}
    </div>
  );
}
```

---

## Troubleshooting Guide

### Problem: Animations not showing on page

**Check:**
1. Is `siteAnimations` imported from `useTheme()`?
2. Is there an animation sync `useEffect` listening to `siteAnimations` changes?
3. Is AnimationLayer receiving `enabled` prop?
4. Is there a `siteThemeUpdated` event listener?

**Solution:**
Add all three pieces from Pattern 1 above.

---

### Problem: Animations disappear after showing briefly

**Cause:** Missing `siteThemeUpdated` event listener or `animationsEnabled` state not being updated.

**Solution:**
Add the event listener that updates both `themeAnimations` and `animationsEnabled`:

```typescript
useEffect(() => {
  const handleThemeUpdate = (event: any) => {
    const { animations, animationsEnabled: enabled } = event.detail;

    if (typeof enabled === 'boolean') {
      setAnimationsEnabled(enabled);
    }

    if (animations) {
      // ... validation and update
    }
  };

  window.addEventListener('siteThemeUpdated', handleThemeUpdate);
  return () => window.removeEventListener('siteThemeUpdated', handleThemeUpdate);
}, []);
```

---

### Problem: Wrong animations showing on page

**Cause:** Page is not syncing with `siteAnimations` from ThemeContext.

**Solution:**
Add animation sync useEffect:

```typescript
useEffect(() => {
  if (siteAnimations) {
    setThemeAnimations(siteAnimations);
  } else {
    setThemeAnimations(getThemeAnimations(currentPreset));
  }
}, [siteAnimations, currentPreset]);
```

---

### Problem: Loading screen shows wrong colors

**Cause:** Page is calling `setCurrentTheme()` too early, before loading screen can read previous page's theme.

**Solution:**
Use the delayed registration pattern:

**For pages WITH loading state:**
```typescript
useEffect(() => {
  if (!loading && theme?.primaryColor && siteCustomization) {
    setCurrentTheme({ ... });
  }
}, [loading, theme, siteCustomization, setCurrentTheme]);
```

**For pages WITHOUT loading state:**
```typescript
useEffect(() => {
  if (theme?.primaryColor && siteCustomization) {
    const timer = setTimeout(() => {
      setCurrentTheme({ ... });
    }, 100);
    return () => clearTimeout(timer);
  }
}, [theme, siteCustomization, setCurrentTheme]);
```

---

### Problem: Loading screen flashes briefly then switches colors

**Cause:** Page data loads too quickly (< 100ms) so loading screen disappears before theme registration delay completes.

**Solution:**
Add minimum loading time:

```typescript
useEffect(() => {
  async function loadData() {
    const loadStartTime = Date.now();

    // ... fetch data ...

    finally {
      const loadTime = Date.now() - loadStartTime;
      const minLoadTime = 200;
      if (loadTime < minLoadTime) {
        setTimeout(() => {
          setLoading(false);
        }, minLoadTime - loadTime);
      } else {
        setLoading(false);
      }
    }
  }
  loadData();
}, []);
```

---

### Problem: Animations have incomplete data (missing count, size, positions)

**Cause:** Backend returns partial animation data with only customized fields.

**Solution:**
Use animation merging in ThemeContext (already implemented). Pages should always get animations from ThemeContext, not directly from API.

```typescript
// ❌ WRONG - Direct from API
const animations = await fetchSiteTheme();
setThemeAnimations(animations); // May be partial

// ✅ CORRECT - From ThemeContext
const { siteAnimations } = useTheme();
setThemeAnimations(siteAnimations); // Always complete
```

---

### Problem: Theme changes in customizer not reflected on page

**Cause:** Page is not listening to `siteThemeUpdated` event.

**Solution:**
Add event listener (see Pattern 1 above).

---

### Problem: PostDetail showing wrong theme for profile posts

**Cause:** PostDetail's animation sync is not checking navigation context.

**Current Implementation (Correct):**
```typescript
useEffect(() => {
  const navState = location.state as any;
  const isFromProfile = navState?.themeContext?.type === 'profile';
  const isFromCommunity = navState?.themeContext?.type === 'community';

  // Only use siteAnimations if NOT coming from profile or community
  if (!isFromProfile && !isFromCommunity && !profileCustomization && siteAnimations) {
    setThemeAnimations(siteAnimations);
  }
}, [siteAnimations, location.state, profileCustomization]);
```

---

## Testing Checklist

When implementing theme caching on a new page, verify:

- [ ] Page uses `siteAnimations` from `useTheme()`
- [ ] Animation sync `useEffect` listens to `siteAnimations` changes
- [ ] `siteThemeUpdated` event listener is implemented
- [ ] `setCurrentTheme()` is called AFTER loading completes (not on mount)
- [ ] Loading screen shows previous page's colors when navigating TO this page
- [ ] Loading screen shows this page's colors when navigating FROM this page
- [ ] Animations persist after page load
- [ ] Animations update when theme customizer changes them
- [ ] Animation enabled/disabled toggle works
- [ ] Page respects minimum loading time if needed (for fast-loading pages)
- [ ] Profile/community context is respected (for PostDetail and similar pages)

---

## Key Files Reference

### Core System
- `frontend/src/context/ThemeContext.tsx` - Site theme management and caching
- `frontend/src/context/NavigationContext.tsx` - Loading screen continuity
- `frontend/src/utils/themeCache.ts` - Cache utility functions
- `frontend/src/theme/animation-config.ts` - Preset animation definitions
- `frontend/src/components/desktop/LoadingScreen.tsx` - Loading screen component
- `frontend/src/components/desktop/AnimationLayer.tsx` - Animation rendering

### Site-Wide Pages
- `frontend/src/components/desktop/FeedClient.tsx`
- `frontend/src/components/desktop/CommunityList.tsx`
- `frontend/src/pages/Friends.tsx`
- `frontend/src/pages/Messages.tsx`
- `frontend/src/pages/TagsPage.tsx`
- `frontend/src/pages/Moderation.tsx`
- `frontend/src/pages/Ads.tsx`
- `frontend/src/pages/Marketplace.tsx`

### Context-Aware Pages
- `frontend/src/pages/PostDetail.tsx`
- `frontend/src/components/desktop/ProfileClient.tsx`
- `frontend/src/components/desktop/CommunityClient.tsx`

---

## Future Improvements

### Potential Enhancements

1. **IndexedDB Migration**: Move from sessionStorage to IndexedDB for larger cache capacity
2. **Cache Expiration**: Add TTL (time-to-live) for theme cache entries
3. **Preload Adjacent Themes**: Prefetch themes for likely next pages
4. **Animation Diff Updates**: Only update changed animation fields instead of replacing entire object
5. **Loading Screen Predictions**: Use ML to predict loading times and adjust minimum loading duration
6. **Theme Versioning API**: Backend API to notify clients of theme version changes

---

## Conclusion

The unified theme cache system provides:
- ✅ Consistent theme colors across all pages
- ✅ Proper animation persistence and updates
- ✅ Smooth loading screen transitions
- ✅ Context-aware theming (profile/community/site)
- ✅ Efficient caching with version management
- ✅ Real-time theme customizer updates

By following the patterns documented here, any new page can be integrated into the system with confidence.
