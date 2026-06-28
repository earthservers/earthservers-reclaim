# Changelog

All notable changes to Earth Reclaim are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.3] - 2026-06-28

> Supersedes 1.0.2, whose release build failed before publishing (the NoScript
> build step couldn't be found from the CI working directory). Same changes,
> plus the build-pipeline fix below.

### Fixed
- **Private Bookmarks and Local AI / History passwords are now per-profile.** They
  were stored in a single shared row, so the same password applied to every
  profile and survived wiping one. Each profile now has its own, independent
  password, and wiping/deleting a profile clears it. The old shared password is
  migrated away on first launch (bookmarks/AI data are device-key encrypted, so
  nothing is lost — set a new per-profile password if you want the gate back).
- **NoScript now works in installed builds (.rpm/.deb/AppImage), not just dev.**
  The WebKit web-process extension (`libearth_noscript_ext.so`) is now built and
  bundled as an app resource, and located in the bundle at runtime. Previously it
  was only found in the dev source tree, so installed builds reported "No script
  sources detected yet."

### Changed
- The release build now compiles and bundles the NoScript extension automatically
  (`build:noscript` step), invoked via `pnpm --filter reclaim` so it resolves from
  the CI working directory; CI installs the WebKit 4.0 web-extension dev library.

## [1.0.1] - 2026-06-28

### Added
- **Dedicated Incognito profile** that is always private: it's auto-created
  alongside Default, forced into incognito mode, and its toolbar toggle is
  locked on (can't be turned off).
- **4-digit delete code** for profiles: required when creating a profile and
  needed to delete or wipe it. Profiles without a code (e.g. the existing
  Default) must set one before they can be wiped.
- **Wipe profile** action that erases all of a profile's data but keeps the
  profile — the only destructive option for the protected Default/Incognito
  profiles.
- Media tab **access lock**: when a media password is set, the tab is locked
  until the password is entered (per session), enforced in every privacy mode.

### Changed
- Deleting a profile now performs a **complete wipe** of every per-profile table
  (history, bookmarks, domains, media history/playlists, tabs, themes, saved
  passwords & 2FA codes, scraper data, …) in a single transaction.
- The Default and Incognito profiles are **protected**: they can be wiped but
  never deleted.

### Fixed
- **Incognito now persists across restarts.** Each profile's incognito flag is
  stored in the database and restored on startup (previously it was in-memory
  only and reset on every relaunch).
- **Media tab was effectively unprotected.** A media password no longer just
  shows a one-time setup prompt — it now actually gates access to the tab, and
  incognito no longer leaves it wide open.
- Profile deletion previously left **orphaned data** (bookmarks, media, saved
  passwords, themes, etc.) because it only cleared a handful of tables and
  relied on foreign-key cascades that SQLite wasn't enforcing.

## [1.0.0] - 2026

- First stable release.
</content>
</invoke>
