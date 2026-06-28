# Theme Customizer System Guide

## Overview

The EarthSocial theme customizer system has been refactored into a modular, reusable architecture with **4 separate theme scopes**:

1. **Site-wide Theme** - Controls the global app appearance (navigation, feed, community list)
2. **Profile Theme** - Controls individual user profile appearance
3. **Messages Theme** - Controls all messaging interfaces (DMs and group chats)
4. **Community Theme** - Controls individual community page appearance

## Architecture

### Components Structure

```
frontend/src/components/ThemeCustomizer/
├── ThemeCustomizerModal.tsx       # Base reusable modal component
├── SiteThemeCustomizer.tsx        # Site-wide theme customizer
├── ProfileThemeCustomizer.tsx     # Profile theme customizer
├── GroupChatThemeCustomizer.tsx   # Messages theme customizer
├── CommunityThemeCustomizer.tsx   # Community theme customizer
└── index.ts                        # Exports
```

### Base Component: ThemeCustomizerModal

The `ThemeCustomizerModal` is a fully reusable side-panel component that:
- Slides in from the right with smooth transitions
- Can be resized between 35% and 90% width
- Provides live preview of changes
- Includes all theme customization controls
- Supports scope-specific options via props

**Key Props:**
```typescript
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
  showProfileOptions?: boolean;  // Profile-specific controls
  showUploadOptions?: boolean;   // Upload buttons for images
};
```

## Theme Customization Options

### Available Customizations

All theme scopes support:

#### 1. **Theme Presets**
- Ocean Turtle (default)
- Mountain Eagle
- Sun Fire
- Lightning Bolt
- Air Clouds

#### 2. **Animations & Effects**
- Master toggle for all animations
- Individual character toggles (turtles, eagles, etc.)
- Decoration toggles (coral, mountains, clouds)
- Bubble effects
- Animation speed control (0.5x - 3x)

#### 3. **Colors**
- Primary Color
- Secondary Color
- Accent Color
- Text Color
- Profile Frame Color (profile scope only)

#### 4. **Background Styling**
- Gradient enable/disable
- Gradient angle (0-360°)
- Solid color fallback

#### 5. **Card Styling**
- Card gradient enable/disable
- Card gradient colors (2 colors)
- Card gradient angle (0-360°)
- Card transparency (when gradient disabled)

#### 6. **Profile-Specific Options** (Profile scope only)
- Profile frame shape (square/circle)
- Name alignment (left/center/right)
- Profile decoration design (coral/waves/bubbles)

## Usage Examples

### 1. Site Theme Customizer

```typescript
import { SiteThemeCustomizer } from '@/components/ThemeCustomizer';

function SettingsPage() {
  const [showCustomizer, setShowCustomizer] = useState(false);
  const currentUser = useUser(); // Your user hook

  return (
    <div>
      <button onClick={() => setShowCustomizer(true)}>
        Customize Site Theme
      </button>

      <SiteThemeCustomizer
        isOpen={showCustomizer}
        onClose={() => setShowCustomizer(false)}
        userId={currentUser.id}
      />
    </div>
  );
}
```

### 2. Profile Theme Customizer

```typescript
import { ProfileThemeCustomizer } from '@/components/ThemeCustomizer';

function ProfileClient({ user, currentUser }) {
  const [showCustomizer, setShowCustomizer] = useState(false);

  return (
    <div>
      {user.isCurrentUser && (
        <>
          <button onClick={() => setShowCustomizer(true)}>
            Customize Profile
          </button>

          <ProfileThemeCustomizer
            isOpen={showCustomizer}
            onClose={() => setShowCustomizer(false)}
            userId={user.id}
          />
        </>
      )}
    </div>
  );
}
```

### 3. Community Theme Customizer

```typescript
import { CommunityThemeCustomizer } from '@/components/ThemeCustomizer';

function CommunityClient({ community, currentUser }) {
  const [showCustomizer, setShowCustomizer] = useState(false);
  const isOwnerOrAdmin = checkPermissions(currentUser, community);

  return (
    <div>
      {isOwnerOrAdmin && (
        <>
          <button onClick={() => setShowCustomizer(true)}>
            Customize Community Theme
          </button>

          <CommunityThemeCustomizer
            isOpen={showCustomizer}
            onClose={() => setShowCustomizer(false)}
            communityId={community.id}
            isOwnerOrAdmin={isOwnerOrAdmin}
          />
        </>
      )}
    </div>
  );
}
```

### 4. Messages Theme Customizer

```typescript
import { GroupChatThemeCustomizer } from '@/components/ThemeCustomizer';

function MessagesPage({ currentUser }) {
  const [showCustomizer, setShowCustomizer] = useState(false);

  return (
    <div>
      <button onClick={() => setShowCustomizer(true)}>
        Customize Messages Theme
      </button>

      <GroupChatThemeCustomizer
        isOpen={showCustomizer}
        onClose={() => setShowCustomizer(false)}
        userId={currentUser.id}
      />
    </div>
  );
}
```

## API Endpoints (To Be Implemented)

### Site Theme
```
GET    /users/:userId/site-theme
PUT    /users/:userId/site-theme
```

### Profile Theme
```
GET    /profile/:userId
PUT    /profile/:userId
```
(Already exists, just needs to handle theme data)

### Messages Theme
```
GET    /users/:userId/messages-theme
PUT    /users/:userId/messages-theme
```

### Community Theme
```
GET    /communities/:communityId/theme
PUT    /communities/:communityId/theme
```

## Data Storage

### Database Schema Updates Needed

```prisma
model User {
  // Existing fields...

  // Site-wide theme
  siteThemeId       Int?    @map("site_theme_id")
  siteTheme         Theme?  @relation("UserSiteTheme", fields: [siteThemeId], references: [id])

  // Messages theme
  messagesThemeId   Int?    @map("messages_theme_id")
  messagesTheme     Theme?  @relation("UserMessagesTheme", fields: [messagesThemeId], references: [id])

  // Profile theme (existing)
  profileThemeId    Int?    @map("profile_theme_id")
  profileTheme      Theme?  @relation("UserProfileTheme", fields: [profileThemeId], references: [id])

  // Legacy field - contains customization JSON
  profileCustomization Json? @map("profile_customization")
}

model Community {
  // Existing fields...

  // Community theme
  themeId           Int?    @map("theme_id")
  theme             Theme?  @relation("CommunityTheme", fields: [themeId], references: [id])
}
```

### Theme Data Structure

```typescript
{
  "customization": {
    "primaryColor": "#0891b2",
    "secondaryColor": "#0e7490",
    "accentColor": "#f97316",
    "textColor": "#ffffff",
    "cardBg": "rgba(6, 182, 212, 0.9)",
    "cardGradientEnabled": false,
    "cardGradientAngle": 135,
    "cardGradientColor1": "#0891b2",
    "cardGradientColor2": "#0e7490",
    "gradientEnabled": true,
    "gradientAngle": 135,
    "bubbleColor": "#67e8f9",
    "turtleColor": "#10b981",
    "coralColors": ["#f97316", "#ec4899", "#8b5cf6", "#10b981", "#0ea5e9"],
    "profileFrameColor": "#fbbf24",
    "profileFrameShape": "square",
    "profileNameAlign": "left",
    "profileFrameDesign": "coral",
    "animationsEnabled": true,
    "selectedCharacter": "turtle",
    "selectedDecoration": "coral"
  },
  "animations": {
    "characters": [
      { "id": "turtle", "enabled": true, "speed": 1.0 }
    ],
    "decorations": [
      { "id": "coral", "enabled": true, "speed": 1.0 }
    ],
    "bubbles": { "enabled": true }
  },
  "animationsEnabled": true
}
```

## Event System

The customizers emit custom events when themes are updated:

```typescript
// Site theme updated
window.addEventListener('siteThemeUpdated', (event) => {
  const { customization, animations, animationsEnabled } = event.detail;
  // Update global theme state
});

// Messages theme updated
window.addEventListener('messagesThemeUpdated', (event) => {
  const { customization, animations, animationsEnabled } = event.detail;
  // Update messages theme state
});
```

## Theme Context Architecture (Future)

Each scope will have its own context provider:

```typescript
// Site theme - wraps entire app
<SiteThemeProvider>
  <App />
</SiteThemeProvider>

// Profile theme - wraps profile pages
<ProfileThemeProvider userId={userId}>
  <ProfileClient />
</ProfileThemeProvider>

// Community theme - wraps community pages
<CommunityThemeProvider communityId={communityId}>
  <CommunityClient />
</CommunityThemeProvider>

// Messages theme - wraps messages pages
<MessagesThemeProvider userId={userId}>
  <MessagesClient />
</MessagesThemeProvider>
```

## Theme Fallback Hierarchy

1. **Profile**: Profile theme → Site theme → Default
2. **Community**: Community theme → Site theme → Default
3. **Messages**: Messages theme → Site theme → Default
4. **Site**: Site theme → Default

## Permissions

- **Site Theme**: Any authenticated user can customize their own site theme
- **Profile Theme**: Users can only customize their own profile theme
- **Messages Theme**: Users can only customize their own messages theme
- **Community Theme**: Only community owners and admins can customize community theme

## Migration Strategy

1. Keep existing `profileCustomization` field for backward compatibility
2. Create new Theme records in database
3. Migrate existing profile customizations to new system
4. Update all components to use new theme contexts
5. Test theme switching across all scopes
6. Deploy with feature flag for gradual rollout

## Benefits of New Architecture

✅ **Reusability**: Single modal component used across all scopes
✅ **Consistency**: Same UX for all theme customization
✅ **Maintainability**: Changes to one component affect all scopes
✅ **Separation of Concerns**: Each scope manages its own theme independently
✅ **Live Preview**: Users see changes in real-time before saving
✅ **Flexibility**: Easy to add new theme scopes in the future

## Next Steps

1. ✅ Create all customizer components
2. ⏳ Integrate ProfileThemeCustomizer into ProfileClient
3. ⏳ Create theme context providers
4. ⏳ Implement API endpoints
5. ⏳ Update database schema
6. ⏳ Build settings pages for each scope
7. ⏳ Apply themes to respective components
8. ⏳ Test theme cascade and fallbacks
