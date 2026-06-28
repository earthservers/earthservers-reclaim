# 🌍 Earth Reclaim - Claude Developer Guide

> **Note:** This file contains instructions optimized for working with Claude Code and AI assistants.

---

## 📋 Table of Contents

- [Quick Reference](#quick-reference)
- [Release Process](#release-process)
- [Commit Conventions](#commit-conventions)
- [Development Workflow](#development-workflow)
- [Project Structure](#project-structure)
- [Common Tasks](#common-tasks)
- [Working with Claude Code](#working-with-claude-code)
- [Troubleshooting](#troubleshooting)
- [Key Files](#key-files)

---

## ⚡ Quick Reference

### Development Commands

```bash
# Install dependencies
pnpm install

# Run desktop app (development mode with mocks)
pnpm desktop

# Run desktop app (production mode without mocks)
pnpm desktop:prod

# Build for production
pnpm build:desktop

# Run tests
pnpm test

# Lint code
pnpm lint

# Format code
pnpm format
```

### Git Workflow

```bash
# Create feature branch
git checkout -b feature/your-feature-name

# Stage changes
git add .

# Commit with conventional commits
git commit -m "feat: Add new feature"

# Push to remote
git push origin feature/your-feature-name

# Merge to main
git checkout main
git merge feature/your-feature-name
git push origin main
```

---

## 🚀 Release Process

### Step 1: Update Version Numbers

Update version in **both** files:

1. `apps/desktop/src-tauri/Cargo.toml`:
```toml
[package]
version = "1.0.0"
```

2. `apps/desktop/package.json`:
```json
{
  "version": "1.0.0"
}
```

### Step 2: Update CHANGELOG.md

Add release notes under a new version header:

```markdown
## [1.0.0] - 2025-01-22

### Added
- New feature X
- New feature Y

### Changed
- Updated feature Z

### Fixed
- Bug fix A
- Bug fix B
```

### Step 3: Commit and Tag

```bash
# Stage all changes
git add .

# Commit version bump
git commit -m "chore: Release v1.0.0"

# Create annotated tag
git tag -a v1.0.0 -m "Release v1.0.0 - First stable release"

# Push commits and tags
git push origin main --tags
```

### Step 4: GitHub Actions Builds

GitHub Actions will automatically:
1. ✅ Build for Windows, macOS (Intel + Apple Silicon), Linux
2. ✅ Sign binaries with your private key
3. ✅ Create a draft release
4. ✅ Upload installers

### Step 5: Publish Release

1. Go to: https://github.com/earthiverse42/earthservers-reclaim/releases
2. Find the draft release
3. Review installers
4. Click **"Publish release"**

### Step 6: Test Auto-Update

Install the previous version, then launch it. It should detect and offer the new update.

---

## 📝 Commit Conventions

Follow [Conventional Commits](https://www.conventionalcommits.org/):

### Format

```
<type>(<scope>): <subject>

<body>

<footer>
```

### Types

- `feat:` - New feature
- `fix:` - Bug fix
- `docs:` - Documentation changes
- `style:` - Code style changes (formatting, etc.)
- `refactor:` - Code refactoring
- `perf:` - Performance improvements
- `test:` - Adding or updating tests
- `build:` - Build system changes
- `ci:` - CI/CD changes
- `chore:` - Other changes (dependencies, etc.)

### Examples

```bash
# New feature
git commit -m "feat: Add semantic search with embeddings"

# Bug fix
git commit -m "fix: Resolve tab duplication on Ctrl+T"

# With scope
git commit -m "feat(ui): Add dark mode theme preset"

# Breaking change
git commit -m "feat!: Change API response format

BREAKING CHANGE: The /api/ratings endpoint now returns..."

# Multiple changes
git commit -m "feat: Add AI summarization

- Integrate Ollama API
- Add summarize button to context menu
- Store summaries in knowledge graph"
```

---

## 🔄 Development Workflow

### Branch Strategy

```
main (production)
  ├── dev (active development)
  │   ├── feature/ai-integration
  │   ├── feature/web-scraper
  │   └── fix/tab-crash
  └── hotfix/critical-bug
```

### Creating a Feature

```bash
# Start from dev branch
git checkout dev
git pull origin dev

# Create feature branch
git checkout -b feature/ai-summarization

# Work on feature...
git add .
git commit -m "feat: Add AI text summarization"

# Push to remote
git push origin feature/ai-summarization

# Create Pull Request on GitHub
# After review, merge to dev

# When ready for release, merge dev to main
git checkout main
git merge dev
git push origin main
```

### Hotfix Process

```bash
# Critical bug in production
git checkout main
git checkout -b hotfix/security-patch

# Fix the bug
git add .
git commit -m "fix: Patch security vulnerability CVE-2025-1234"

# Merge to main
git checkout main
git merge hotfix/security-patch

# Tag and release immediately
git tag -a v1.0.1 -m "Hotfix v1.0.1"
git push origin main --tags

# Merge back to dev
git checkout dev
git merge hotfix/security-patch
git push origin dev
```

---

## 📁 Project Structure

```
earth-reclaim/
├── apps/
│   ├── desktop/                    # Tauri desktop application
│   │   ├── src/                    # React frontend
│   │   │   ├── components/         # UI components
│   │   │   ├── contexts/           # React contexts
│   │   │   ├── hooks/              # Custom hooks
│   │   │   ├── lib/                # Utilities
│   │   │   ├── mocks/              # Mock data for development
│   │   │   └── App.tsx             # Main app component
│   │   ├── src-tauri/              # Rust backend
│   │   │   ├── src/
│   │   │   │   ├── main.rs         # Entry point
│   │   │   │   ├── ai.rs           # AI/Ollama integration
│   │   │   │   ├── search.rs       # Search functionality
│   │   │   │   ├── tabs.rs         # Tab management
│   │   │   │   ├── bookmarks.rs    # Bookmark system
│   │   │   │   ├── webview.rs      # WebView integration
│   │   │   │   └── scraper.rs      # Web scraper
│   │   │   ├── Cargo.toml          # Rust dependencies
│   │   │   └── tauri.conf.json     # Tauri configuration
│   │   └── package.json
│   │
│   └── ratings-server/             # Community ratings API
│       ├── src/
│       │   ├── main.rs
│       │   ├── api.rs
│       │   └── ratings.rs
│       └── migrations/
│
├── packages/                       # Shared packages
│   ├── ui/                         # Shared UI components
│   ├── database/                   # SQLite schemas
│   └── api-client/                 # API client library
│
├── deployment/                     # Deployment configs
│   ├── docker-compose.yml
│   ├── nginx.conf
│   └── deploy.sh
│
├── .github/
│   └── workflows/
│       └── release.yml             # Automated release builds
│
├── CHANGELOG.md                    # Release notes
├── README.md                       # Project overview
├── CONTRIBUTING.md                 # Contribution guidelines
├── CLAUDE.md                       # This file
└── LICENSE
```

---

## 🔧 Common Tasks

### Adding a New Feature

```bash
# 1. Create branch
git checkout -b feature/new-feature

# 2. Make changes
# ... edit files ...

# 3. Test locally
pnpm desktop

# 4. Commit
git add .
git commit -m "feat: Add new feature"

# 5. Push
git push origin feature/new-feature

# 6. Create Pull Request
```

### Fixing a Bug

```bash
# 1. Reproduce the bug
pnpm desktop

# 2. Create branch
git checkout -b fix/bug-description

# 3. Fix the bug
# ... edit files ...

# 4. Test the fix
pnpm desktop

# 5. Commit
git add .
git commit -m "fix: Resolve bug description"

# 6. Push
git push origin fix/bug-description
```

### Updating Dependencies

```bash
# Check for outdated packages
pnpm outdated

# Update all dependencies
pnpm update

# Update specific package
pnpm update package-name

# Update Rust dependencies
cd apps/desktop/src-tauri
cargo update

# Test after updates
pnpm desktop

# Commit
git add .
git commit -m "chore: Update dependencies"
```

### Adding a New Tauri Command

1. **Add Rust function** in `apps/desktop/src-tauri/src/*.rs`:

```rust
#[tauri::command]
pub async fn my_new_command(arg: String) -> Result<String, String> {
    // Implementation
    Ok("result".to_string())
}
```

2. **Register in main.rs**:

```rust
.invoke_handler(tauri::generate_handler![
    // ... existing commands
    my_module::my_new_command,
])
```

3. **Call from frontend**:

```typescript
import { invoke } from '@tauri-apps/api/tauri';

const result = await invoke('my_new_command', { arg: 'value' });
```

### Adding a New React Component

1. **Create component** in `apps/desktop/src/components/`:

```typescript
// MyComponent.tsx
import React from 'react';

interface MyComponentProps {
  title: string;
}

export const MyComponent: React.FC<MyComponentProps> = ({ title }) => {
  return (
    <div className="p-4">
      <h2>{title}</h2>
    </div>
  );
};
```

2. **Export from index**:

```typescript
// components/index.ts
export { MyComponent } from './MyComponent';
```

3. **Use in app**:

```typescript
import { MyComponent } from './components';

<MyComponent title="Hello" />
```

---

## 🤖 Working with Claude Code

### Effective Prompts

**Good prompts are:**
- ✅ Specific about what to change
- ✅ Include file paths
- ✅ Mention existing patterns to follow
- ✅ Provide context about the feature

**Example good prompt:**

```
Add a "duplicate tab" feature:

1. In apps/desktop/src-tauri/src/tabs.rs:
   - Add a duplicate_tab() command that copies a tab with a new ID
   
2. In apps/desktop/src/components/TabBar.tsx:
   - Add "Duplicate" to the tab context menu
   - Call invoke('duplicate_tab', { tabId })
   
3. Follow the existing pattern from close_tab()
```

**Example bad prompt:**

```
Add duplicate tabs
```

### Context to Provide

When asking Claude Code for help, provide:

1. **Current file structure**
```bash
ls -la apps/desktop/src/components/
```

2. **Relevant code snippets**
```typescript
// Current implementation of similar feature
```

3. **Error messages** (if applicable)
```
error[E0425]: cannot find value `foo` in this scope
```

4. **What you've already tried**

### Reviewing Changes

Always review what Claude Code changes:

```bash
# See all changes
git diff

# See changes in specific file
git diff apps/desktop/src/App.tsx

# Review before committing
git add -p  # Interactive staging
```

---

## 🐛 Troubleshooting

### Build Errors

**Rust compilation fails:**

```bash
cd apps/desktop/src-tauri
cargo clean
cargo build
```

**Node modules issues:**

```bash
rm -rf node_modules
pnpm install
```

**Tauri build fails:**

```bash
# Clear Tauri cache
rm -rf apps/desktop/src-tauri/target
pnpm build:desktop
```

### Development Issues

**"Command not found: tauri"**

```bash
pnpm add -D @tauri-apps/cli -w
```

**"Ollama not running"**

```bash
ollama serve
```

**Mock data not loading:**

```bash
# Check env variable
echo $VITE_USE_MOCK_DATA

# Should be 'true' for dev
export VITE_USE_MOCK_DATA=true
pnpm desktop
```

### Database Issues

**SQLite errors:**

```bash
# Delete database and restart
rm ~/.earthreclaim/*.db
pnpm desktop
```

**Migration fails:**

```bash
cd apps/desktop/src-tauri
sqlx migrate run
```

### Git Issues

**Merge conflicts:**

```bash
# See conflicted files
git status

# Edit files to resolve conflicts
# Then:
git add .
git commit -m "chore: Resolve merge conflicts"
```

**Accidentally committed secrets:**

```bash
# Remove from git history
git filter-branch --force --index-filter \
  "git rm --cached --ignore-unmatch path/to/secret.key" \
  --prune-empty --tag-name-filter cat -- --all

# Force push (DANGEROUS - only if necessary)
git push origin --force --all
```

---

## 🔑 Key Files

### Configuration Files

| File | Purpose |
|------|---------|
| `tauri.conf.json` | Tauri app configuration, updater settings |
| `Cargo.toml` | Rust dependencies and metadata |
| `package.json` | Node dependencies and scripts |
| `tsconfig.json` | TypeScript configuration |
| `tailwind.config.js` | Tailwind CSS configuration |
| `.env.development` | Development environment variables |
| `.env.production` | Production environment variables |

### Important Directories

| Directory | Contents |
|-----------|----------|
| `apps/desktop/src/components/` | React UI components |
| `apps/desktop/src-tauri/src/` | Rust backend code |
| `apps/desktop/src/mocks/` | Mock data for development |
| `packages/` | Shared code across apps |
| `.github/workflows/` | CI/CD automation |
| `deployment/` | Server deployment configs |

### Database Files

| File | Purpose |
|------|---------|
| `~/.earthreclaim/earthreclaim.db` | Main SQLite database |
| `~/.earthreclaim/*.db-shm` | SQLite shared memory |
| `~/.earthreclaim/*.db-wal` | SQLite write-ahead log |

**Note:** Database files are NOT committed to Git (in `.gitignore`).

---

## 🔒 Security Notes

### Never Commit These

- ❌ `*.key` files (signing keys)
- ❌ `.env` with real credentials
- ❌ Database files (`*.db`)
- ❌ API keys or passwords
- ❌ Private configuration

### Always Check Before Pushing

```bash
# Search for potential secrets
git diff | grep -i "password\|secret\|key\|token"

# Check what will be committed
git status
git diff --cached
```

### If You Accidentally Commit Secrets

1. **Immediately revoke/rotate** the compromised secret
2. Remove from git history (see Troubleshooting)
3. Force push (coordinate with team)
4. Update GitHub secrets if needed

---

## 📚 Additional Resources

### Documentation

- [Tauri Docs](https://tauri.app/v1/guides/)
- [React Docs](https://react.dev/)
- [Rust Book](https://doc.rust-lang.org/book/)
- [SQLite Docs](https://www.sqlite.org/docs.html)
- [Ollama API](https://github.com/ollama/ollama/blob/main/docs/api.md)

### Internal Docs

- `README.md` - Project overview
- `CONTRIBUTING.md` - Contribution guidelines
- `apps/desktop/README.md` - Desktop app specifics
- `apps/ratings-server/README.md` - Ratings API docs

### Tools

- [Claude Code](https://claude.ai/code) - AI-powered coding assistant
- [GitHub Actions](https://github.com/earthiverse42/earthservers-reclaim/actions) - CI/CD
- [Tauri CLI](https://tauri.app/v1/api/cli/) - Build tools

---

## 📞 Getting Help

### From Claude Code

Provide:
1. What you're trying to do
2. What you've tried
3. Error messages (full output)
4. Relevant file paths
5. Code snippets

### From the Community

- **GitHub Issues**: Bug reports, feature requests
- **Discussions**: Questions, ideas
- **Discord**: Real-time chat (coming soon)

---

## 🎯 Quick Checklist

### Before Every Commit

- [ ] Code builds without errors
- [ ] Features work as expected
- [ ] No console errors
- [ ] Follows project code style
- [ ] Commit message follows conventions
- [ ] No secrets in code

### Before Every Release

- [ ] Version updated in Cargo.toml
- [ ] Version updated in package.json
- [ ] CHANGELOG.md updated
- [ ] All tests pass
- [ ] Build succeeds on all platforms
- [ ] Git tag created
- [ ] Release notes written

### After Every Release

- [ ] GitHub release published
- [ ] Installers tested
- [ ] Auto-update works
- [ ] Announcement made (if applicable)

---

## 🌍 Philosophy

> **"We don't desire to rule the Earth. Only to serve it."**

When working on Earth Reclaim, remember:

- 🔒 **Privacy First** - User data stays local
- 🌱 **Open Source** - Transparent and auditable
- 🤝 **Community Driven** - Built for users, by users
- 🧠 **Local AI** - No cloud dependence
- 🌐 **User Freedom** - No lock-in, full control

---

**Last Updated:** 2025-01-22  
**Version:** 1.0.0  

---

*For questions about this guide, open an issue or ask Claude Code!* 🤖
