# Changelog

All notable changes to Earth Reclaim are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.5] - 2026-06-28

### Fixed
- **Clicking a folder in the bookmarks bar** now opens the Bookmark Manager with
  that folder selected, instead of a dropdown that was cut off behind the web page.
- **Restarting now restores the page you ended on.** Navigating from a search
  (e.g. DuckDuckGo) to a result then restarting brought back the search page,
  because only the tab's title — not its live URL — was being persisted. The tab's
  URL is now saved as you navigate, so the last page is restored.

## [1.0.4] - 2026-06-28

### Added
- **Local AI / History lock now actually works.** A "Lock now" toggle on the tab
  locks it immediately (hiding the AI tools and history behind the password);
  setting a password also locks right away so the gate engages. Previously it only
  showed "Locked" but never gated access.
- **Local AI / History is shown as disabled** on the Incognito profile / in
  incognito mode (nothing is recorded there).
- **Switching profiles re-locks every feature password** (password manager,
  authenticator, media, private bookmarks, Local AI/History) so the new profile
  starts fully gated.

### Fixed
- **Private bookmarks and toolbar bookmarks now save correctly.** Bookmark
  "location" (toolbar / list / private) is persisted again — private bookmarks
  show in the private list, and toolbar bookmarks appear on the bar.
- **Dragging bookmarks works.** Fixed drag never starting (a pointer handler was
  cancelling native drag) and the move failing (wrong argument names); you can
  drop a bookmark on a folder to move it, or on another bookmark to make a folder.
- **Bookmark toolbar and tab strip scroll** horizontally when they overflow,
  instead of pushing into other controls.
- **Modals that were hidden behind the web page are now right-side panels** (the
  create-folder prompt, Edit Bookmark, the profile dropdown + delete/wipe confirm),
  so they're visible and clickable while browsing.
- **Private Bookmarks panel no longer closes when you click inside it** (you can
  type the password and use autofill).
- **Password Manager entries** show their full title and URL instead of being
  truncated to one letter / wrapped onto extra lines.
- Removed the broken "fake rounded" top corners on the header and incognito banner.
- The "Create New Profile" button sits at the top of the profile panel; new
  profiles are listed below Default and Incognito.

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
