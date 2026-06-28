# Moderation System & 4-Scope Theme Architecture

## ✅ Implemented: Site Moderator System

### Schema Changes

#### User Model - New Fields
```prisma
model User {
  isSiteAdmin     Boolean @default(false)  // Existing
  isSiteModerator Boolean @default(false)  // ✅ NEW
}
```

#### Post Model - Moderation Tracking
```prisma
model Post {
  // Moderation tracking
  editedByModeratorId Int?      @map("edited_by_moderator_id")  // ✅ NEW
  editedByModeratorAt DateTime? @map("edited_by_moderator_at")  // ✅ NEW
  moderatorEditReason String?   @map("moderator_edit_reason")   // ✅ NEW
}
```

#### CommunityPost Model - Moderation Tracking
```prisma
model CommunityPost {
  // Moderation tracking
  editedByModeratorId Int?      @map("edited_by_moderator_id")  // ✅ NEW
  editedByModeratorAt DateTime? @map("edited_by_moderator_at")  // ✅ NEW
  moderatorEditReason String?   @map("moderator_edit_reason")   // ✅ NEW
}
```

### Migration Required
```bash
cd backend
npx prisma migrate dev --name add_site_moderator_and_moderation_tracking
npx prisma generate
```

### Moderation Hierarchy

```
Site Admin (isSiteAdmin = true)
  ├─ Full system access
  ├─ Can promote/demote site moderators
  ├─ Can edit/delete any content site-wide
  └─ Can create/delete communities

Site Moderator (isSiteModerator = true)
  ├─ Can edit/delete posts site-wide
  ├─ Can create communities
  ├─ Edits are tracked and displayed
  └─ Cannot promote/demote other moderators

Community Owner (CommunityRole = owner)
  ├─ Full control over their community
  ├─ Can promote/demote community admins/moderators
  ├─ Can edit community settings
  └─ Can delete the community

Community Admin (CommunityRole = admin)
  ├─ Can edit/delete posts in the community
  ├─ Can promote/demote community moderators
  └─ Can edit community settings

Community Moderator (CommunityRole = moderator)
  ├─ Can edit/delete posts in the community
  ├─ Edits are tracked and displayed
  └─ Cannot change community settings

Community Member (CommunityRole = member)
  ├─ Can create posts
  └─ Can delete own posts
```

### CommunityList Features

#### Create Community Button
- ✅ Only visible to Site Admins and Site Moderators
- ✅ Located in page header next to "Communities" title
- ✅ Opens modal for community creation

#### Create Community Modal
- ✅ Community name input (required, 50 char max)
- ✅ Description textarea (optional, 500 char max)
- ✅ Character counters
- ✅ Info box explaining moderator privileges
- ✅ Creates public community by default
- ✅ Creator becomes community owner

### Post Moderation Display

When a post has been edited by a moderator, display:

```tsx
{post.editedByModeratorId && (
  <div className="mb-3 px-3 py-2 rounded border border-yellow-500/50 bg-yellow-500/10">
    <div className="flex items-center gap-2">
      <AlertCircle size={14} className="text-yellow-400" />
      <p className="text-xs text-yellow-100">
        This post was edited by a moderator
        {post.editedByModeratorAt && ` on ${formatDate(post.editedByModeratorAt)}`}
      </p>
    </div>
    {post.moderatorEditReason && (
      <p className="text-xs text-yellow-100/80 mt-1">
        Reason: {post.moderatorEditReason}
      </p>
    )}
  </div>
)}
```

### API Endpoints Needed

#### Create Community
```typescript
POST /communities
Headers: { Authorization: Bearer <token> }
Body: {
  name: string (required)
  description?: string
  accessType?: 'public' | 'request' | 'invite_only' | 'private'
}
Response: {
  community: Community
}
```

#### Edit Post (Site Moderator)
```typescript
PUT /posts/:id/moderate
Headers: { Authorization: Bearer <token> }
Body: {
  content: string
  title?: string
  tags?: string[]
  reason: string (required for moderation log)
}
Response: {
  post: Post
}
```

#### Edit Community Post (Community Moderator)
```typescript
PUT /communities/:communityId/posts/:postId/moderate
Headers: { Authorization: Bearer <token> }
Body: {
  content: string
  title?: string
  tags?: string[]
  reason: string (required for moderation log)
}
Response: {
  post: CommunityPost
}
```

#### Delete Post (Site Moderator)
```typescript
DELETE /posts/:id/moderate
Headers: { Authorization: Bearer <token> }
Body: {
  reason: string (required for audit log)
}
```

#### Delete Community Post (Community Moderator)
```typescript
DELETE /communities/:communityId/posts/:postId/moderate
Headers: { Authorization: Bearer <token> }
Body: {
  reason: string (required for audit log)
}
```

---

## 🎨 4-Scope Theme System Architecture

### Theme Scopes

#### 1. Site-Wide Theme
- **Scope**: `site`
- **Applies to**: All pages by default
- **Controls**: Navigation, feed, community list, profile pages (unless overridden)
- **User Setting**: Settings > Appearance > Site Theme
- **Database**: `User.siteThemeId` (to be added)

#### 2. Community Theme
- **Scope**: `community`
- **Applies to**: Individual community pages
- **Controls**: Community posts, threads, chat within that community
- **Admin Setting**: Community Settings > Appearance (owner/admin only)
- **Database**: `Community.themeId` (already exists)
- **Fallback**: Site-wide theme if no community theme set

#### 3. Profile Theme
- **Scope**: `profile`
- **Applies to**: User's own profile page + existing customizations
- **Controls**: Profile header, posts on profile, decorations
- **User Setting**: Settings > Profile > Appearance
- **Database**: `User.profileThemeId` (already exists)
- **Fallback**: Site-wide theme if no profile theme set

#### 4. Messages/GroupChats Theme
- **Scope**: `messages`
- **Applies to**: DM and group chat interfaces
- **Controls**: Message bubbles, chat list, conversation view
- **User Setting**: Settings > Messages > Appearance
- **Database**: `Conversation.themeId` (already exists), `User.messagesThemeId` (to be added)
- **Fallback**: Site-wide theme if no messages theme set

### Theme Cascade & Priority

```
User visits page → Check context:

┌─ Feed/CommunityList/General pages
│  └─ Use Site-Wide Theme
│
├─ Community /community/:id
│  └─ Check Community.themeId
│     ├─ If set → Use Community Theme
│     └─ If null → Use Site-Wide Theme
│
├─ Profile /profile/:username
│  └─ Check User.profileThemeId (of profile owner)
│     ├─ If set → Use Profile Theme
│     └─ If null → Use Site-Wide Theme
│
└─ Messages /messages/:conversationId
   └─ Check Conversation.themeId or User.messagesThemeId
      ├─ If set → Use Messages Theme
      └─ If null → Use Site-Wide Theme
```

### Implementation Plan

#### Phase 1: Schema Updates
```prisma
model User {
  // Add new theme fields
  siteThemeId     String?  // ✅ NEW: User's preferred site-wide theme
  siteTheme       Theme?   @relation("SiteTheme", fields: [siteThemeId], references: [id])

  messagesThemeId String?  // ✅ NEW: User's preferred messages theme
  messagesTheme   Theme?   @relation("MessagesTheme", fields: [messagesThemeId], references: [id])

  // Existing
  profileThemeId  String?
  profileTheme    Theme?   @relation("ProfileTheme", fields: [profileThemeId], references: [id])
}

model Theme {
  // Add new relations
  siteUsers    User[]         @relation("SiteTheme")          // ✅ NEW
  messagesUsers User[]        @relation("MessagesTheme")       // ✅ NEW

  // Existing
  profileUsers User[]         @relation("ProfileTheme")
  communities  Community[]
  conversations Conversation[]
}
```

#### Phase 2: Context Architecture

Create separate theme contexts:

```typescript
// context/SiteThemeContext.tsx
export function SiteThemeProvider({ children }) {
  const [themePreset, setThemePreset] = useState<PresetThemeKey>('ocean-turtle');
  // Load from user preferences or localStorage
  return <SiteThemeContext.Provider>{children}</SiteThemeContext.Provider>;
}

// context/CommunityThemeContext.tsx
export function CommunityThemeProvider({ communityId, children }) {
  const [themePreset, setThemePreset] = useState<PresetThemeKey>();
  // Load from Community.themeId or fall back to site theme
  return <CommunityThemeContext.Provider>{children}</CommunityThemeContext.Provider>;
}

// context/ProfileThemeContext.tsx
export function ProfileThemeProvider({ userId, children }) {
  const [themePreset, setThemePreset] = useState<PresetThemeKey>();
  // Load from User.profileThemeId or fall back to site theme
  return <ProfileThemeContext.Provider>{children}</ProfileThemeContext.Provider>;
}

// context/MessagesThemeContext.tsx
export function MessagesThemeProvider({ conversationId, children }) {
  const [themePreset, setThemePreset] = useState<PresetThemeKey>();
  // Load from Conversation.themeId or User.messagesThemeId or fall back to site theme
  return <MessagesThemeContext.Provider>{children}</MessagesThemeContext.Provider>;
}
```

#### Phase 3: App-Level Provider Structure

```tsx
// App.tsx or main provider wrapper
<SiteThemeProvider>
  <Router>
    <Routes>
      {/* General pages use site theme */}
      <Route path="/feed" element={<Feed />} />
      <Route path="/community" element={<CommunityList />} />

      {/* Community pages override with community theme */}
      <Route path="/community/:id" element={
        <CommunityThemeProvider communityId={params.id}>
          <CommunityClient />
        </CommunityThemeProvider>
      } />

      {/* Profile pages override with profile theme */}
      <Route path="/profile/:username" element={
        <ProfileThemeProvider userId={params.userId}>
          <ProfileClient />
        </ProfileThemeProvider>
      } />

      {/* Messages pages override with messages theme */}
      <Route path="/messages/*" element={
        <MessagesThemeProvider>
          <MessagesRoutes />
        </MessagesThemeProvider>
      } />
    </Routes>
  </Router>
</SiteThemeProvider>
```

#### Phase 4: Settings Pages

Create dedicated settings pages for each scope:

```
/settings/appearance
  ├─ Site Theme Selector (presets + custom)
  └─ Preview panel

/settings/profile
  ├─ Profile Theme Selector
  ├─ Existing customizations (frame, decorations, etc.)
  └─ Preview panel

/settings/messages
  ├─ Messages Theme Selector
  └─ Preview panel

/community/:id/settings (owner/admin only)
  ├─ Community Theme Selector
  └─ Preview panel
```

#### Phase 5: Theme Customizer Refactor

Extract from ProfileClient and make reusable:

```tsx
// components/theme/ThemeCustomizer.tsx
type ThemeCustomizerProps = {
  scope: 'site' | 'community' | 'profile' | 'messages';
  currentTheme: ThemeTokens;
  onSave: (theme: ThemeTokens) => Promise<void>;
  onSelectPreset: (preset: PresetThemeKey) => void;
};

export function ThemeCustomizer({ scope, currentTheme, onSave, onSelectPreset }: ThemeCustomizerProps) {
  // Color pickers, preset selector, preview
  // Reusable across all scopes
}
```

### API Endpoints

#### Get User Themes
```typescript
GET /api/user/themes
Response: {
  siteTheme: ThemeTokens | null,
  profileTheme: ThemeTokens | null,
  messagesTheme: ThemeTokens | null
}
```

#### Update Site Theme
```typescript
PUT /api/user/themes/site
Body: { themePreset: PresetThemeKey | null, customTokens?: ThemeTokens }
```

#### Update Profile Theme
```typescript
PUT /api/user/themes/profile
Body: { themePreset: PresetThemeKey | null, customTokens?: ThemeTokens }
```

#### Update Messages Theme
```typescript
PUT /api/user/themes/messages
Body: { themePreset: PresetThemeKey | null, customTokens?: ThemeTokens }
```

#### Update Community Theme (admin only)
```typescript
PUT /api/communities/:id/theme
Body: { themePreset: PresetThemeKey | null, customTokens?: ThemeTokens }
```

### Benefits of 4-Scope System

1. **User Control**: Users can customize their personal experience
2. **Community Identity**: Communities can have distinct visual identities
3. **Context-Appropriate**: Different themes for different contexts (casual chat vs formal community)
4. **Backwards Compatible**: Existing profile customization works with new system
5. **Performance**: Only load theme for current scope
6. **Modular**: Easy to add new scopes in future

### Migration Strategy

1. Add new schema fields (siteThemeId, messagesThemeId)
2. Migrate existing users: set siteThemeId to current saved theme or default
3. Keep existing profileThemeId and community themeId
4. Create new context providers
5. Refactor components to use appropriate context
6. Build settings UI
7. Implement API endpoints
8. Test theme switching across scopes

---

## 📋 Implementation Checklist

### Moderation System
- [x] Add `isSiteModerator` to User model
- [x] Add moderation tracking fields to Post model
- [x] Add moderation tracking fields to CommunityPost model
- [x] Add Create Community button to CommunityList
- [x] Create Community creation modal
- [x] Add moderation disclaimer UI to FeedClient
- [x] Add moderation disclaimer UI to CommunityClient
- [ ] Implement edit post API endpoints
- [ ] Add moderation UI (edit button for moderators)
- [ ] Add delete post API endpoints
- [ ] Add audit log for moderation actions

### Theme System - Core Components
- [x] Extract ThemeCustomizer from ProfileClient into reusable components
- [x] Create ThemeCustomizerModal (base component with side panel, live preview)
- [x] Create SiteThemeCustomizer component
- [x] Create ProfileThemeCustomizer component
- [x] Create ChatThemeCustomizer component (renamed from GroupChatThemeCustomizer)
- [x] Create CommunityThemeCustomizer component
- [x] Create THEME_CUSTOMIZER_GUIDE.md documentation
- [x] Create IMPLEMENTATION_CHECKLIST.md
- [x] ProfileClient customizer already implemented (inline version working)

### Theme System - Integration (Pending)
- [ ] Add site theme button to FeedClient (pencil icon in nav)
- [ ] Add community theme button to CommunityClient (pencil icon in header)
- [ ] Add chat theme button to Messages page (waiting on mockup)
- [ ] ~~Build /settings/appearance page~~ (Using modal approach instead)
- [ ] ~~Build /settings/profile page~~ (Using modal approach instead)
- [ ] ~~Build /settings/messages page~~ (Using modal approach instead)

### Theme System - Backend (Optional/Future)
- [ ] Add `siteThemeId` and `chatThemeId` to User schema
- [ ] Update Theme model relations
- [ ] Implement site theme API endpoints
- [ ] Implement chat theme API endpoints
- [ ] Implement community theme API endpoints
- [ ] Create SiteThemeContext (optional advanced feature)
- [ ] Create CommunityThemeContext (optional advanced feature)
- [ ] Create ProfileThemeContext (optional advanced feature)
- [ ] Create ChatThemeContext (optional advanced feature)
- [ ] Test theme cascade and fallbacks

---

## 🚀 Next Steps

1. **Run Migration**: Apply schema changes for moderator system
2. **Test Community Creation**: Verify site moderators can create communities
3. **Add Moderation UI**: Display disclaimer on edited posts
4. **Plan Theme Migration**: Design schema changes for 4-scope system
5. **Refactor Theme Contexts**: Extract and modularize theme system
6. **Build Settings Pages**: Create UI for theme management

---

## 📝 Notes

- Site moderators and community moderators are completely separate roles
- A user can be a site moderator AND a community moderator in different communities
- Moderation edits are always tracked and displayed to maintain transparency
- Each theme scope has its own context to avoid prop drilling
- Theme cascade ensures every page has a theme (fallback to site theme)
- Community themes allow for community branding and identity
