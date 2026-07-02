# Changelog

All notable changes to Earth Reclaim are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Per-tab search.** Every search tab now has its **own independent search** —
  query, streamed results, page number, kinds/sources/retention filters, and
  collapse state. Switching tabs swaps to that tab's search *preserved* (results
  are restored from a per-tab cache without re-running the search, and a search
  still streaming keeps filling in); a brand-new tab starts empty. Previously one
  shared search showed in every tab.
- **Saved searches & search history.** A new **Searches** panel in the right dock
  (button on the Search navbar) lists your saved searches — stored **with their
  retention/kinds/sources config** so re-running restores the exact same search —
  and your recent search history (grouped, with run / save / remove / clear-all).
  The same lists also appear as a third **Searches** tab on the Local AI / History
  page. Clicking an entry runs it in the current search tab (or a fresh tab if
  you're on a web page). All local, per profile.
- **Media "Enhance" — super-resolution for videos *and* photos.** A new toolbar
  button in the Media player cycles **Off → FSR → AI**:
  - **FSR** — AMD FidelityFX Super Resolution 1.0 (edge-adaptive 2x upscale +
    contrast-adaptive sharpening) running as GL shaders inside the playback
    pipeline; vendor-agnostic (any GPU with GStreamer GL). Photos get the same
    shaders via WebGL in the image viewer.
  - **AI (Real-ESRGAN)** — real neural super-resolution (Real-ESRGAN compact x2,
    fp16) on the GPU via onnxruntime, as a **TensorRT engine** (compiled on the
    first engage — one-time, minutes — then cached in
    `~/.earthreclaim/aisr/trt-cache`; falls back to plain CUDA when TensorRT
    isn't installed, `EARTH_AISR_TRT=off` forces that). Per-frame IO uses
    reusable CUDA-pinned buffers (onnxruntime IOBinding) — no per-frame
    allocations. Optional install: `scripts/install-ai-upscaler.sh` (NVIDIA GPU
    required; ~4 GB of freely-redistributable runtime libraries into
    `~/.earthreclaim/aisr`). The mode appears automatically once installed. AI
    runs on ≤720p sources (that's what super-resolution is for) and
    transparently degrades to FSR above; over-budget frames drop via QoS
    instead of stalling playback. Measured on an RTX 4060 Ti (TensorRT fp16):
    ~13 ms/frame for 640x360→1280x720 and ~26 ms for 854x480→1708x960 —
    solidly real-time through 480p.
  - Mode switches happen **live** — no pipeline restart, no black flash; the
    upscale caps at 4K; everything stays on-device (model is inert weights,
    runtime telemetry disabled, zero network at playback time).
  - Escape hatches: `EARTH_VIDEO_SR=off` disables the feature entirely;
    `EARTH_AISR_DIR` relocates the AI runtime dir.
- **Clearer toggle states in the Media player.** Active toggles (shuffle, repeat,
  slideshow, layout, pane selector, playlists, privacy, fullscreen) highlight
  **green**; the Enhance button highlights **yellow** so on/off is obvious at a
  glance.

### Fixed

- **Secondary windows were broken in packaged builds** — they couldn't be closed,
  showed the Search tab (which is main-window-only), and failed with *"command …
  not allowed by ACL"* (e.g. loading profiles). Cause: packaged secondary windows
  load from the localhost asset server, and newer Tauri rejects app commands from
  remote origins unless explicitly allowed — every custom command is now declared
  and granted to both window classes. A window served from the asset server also
  treats itself as secondary *by origin*, so it boots straight to Media and never
  falls back to "main" even if IPC fails.
- **Local AI chat: you couldn't scroll up while an answer was streaming** — the
  transcript re-pinned to the bottom on every token. It now pins only while you're
  already at the bottom; scrolling back down (or sending a message) re-engages it.

### Developer notes

- New Tauri commands must now be registered in **three** places: `lib.rs`
  `generate_handler![]`, the command list in `src-tauri/build.rs`, and
  `src-tauri/permissions/all-app-commands.toml` — otherwise the command is
  rejected by the ACL in every window. (See the comment in `build.rs`.)
- `earth-media` and `ort` compile at `opt-level = 3` even in dev profiles —
  unoptimized per-frame pixel loops made enhancement crawl under `pnpm desktop`.

## [1.1.1] - 2026-06-30

### Fixed

- **New windows rendered as a blank "The URL can't be shown" box in packaged
  builds.** A secondary WebKitGTK webview can't load `tauri://` app routes in a
  packaged build (the asset protocol isn't on its web context) — the v1.1.0
  incognito-matching attempt didn't address this. New windows (single-instance /
  tray "New Window" / detached tabs / media-controls) now load the embedded
  frontend over the app's localhost asset server (`http://127.0.0.1:9877`), the
  same mechanism the media controls already use; dev is unchanged (Vite). A new
  capability grants those loopback-served windows the same IPC as the main window.

## [1.1.0] - 2026-06-30

### Added

- **Local search index — "Google, but completely local."** Type a query and get
  fast results that are scraped, indexed, and grep-able on your device. It fuses the
  local SearXNG meta-search, the web scraper, and the AI curator behind one
  `local_search` command plus a new unified **FTS5 + vector** index with a fusion
  ranker. The first search for a topic is slow (live discover → scrape → index);
  every search after is instant off a warm local index — and the index only ever
  contains things you actually searched (strong privacy property).
  - **Two-speed streaming:** SearXNG-speed snippets paint immediately, then scraped
    + indexed results stream in, then the list re-orders to a fused ranking.
  - **Hybrid ranking:** FTS5/BM25 ⊕ vector cosine ⊕ SearXNG position ⊕ a private
    click-log, fused with Reciprocal Rank Fusion.
  - **Lifecycle ladder & retention tiers:** browse → auto-cache (TTL'd, no curation
    cost) → favorite/pin (permanent, curated) → archived (summary kept, body/FTS/
    embeddings dropped) → forgotten. Auto-GC only ever touches ephemeral/cache;
    pinned/archived are protected. Login/credential pages are never cached or indexed.
  - **Favorites = pins, one source of truth:** a pin control (distinct from the
    bookmark star) in the address bar, History rows, and search results, all reading
    the same retention tier. Bookmarks stay separate (URL-only, no indexing).
  - **Review-pinned panel:** the curator *proposes* prune candidates (disuse, age,
    semantic redundancy; a dead upstream is protected, not pruned) and *you dispose* —
    nothing pinned is ever silently removed; the default destructive action is archive.
- **Crawler results in unified search.** Pages from the web scraper's crawl jobs now
  appear in search results, fused via the same RRF — read-only, capped per domain,
  with a "from crawl: \<job\>" badge. Crawler storage is never modified.
- **Comments & discussions search.** A typed-content filter (All / Comments &
  discussions / Comments only) with per-platform adapters that pull posts and comments
  from Reddit and forums (Discourse / Stack Exchange / generic) by default, plus
  YouTube and TikTok via yt-dlp. Instagram/Facebook are best-effort and **off by
  default** (public, logged-out only). An optional **opt-in** "use my own session"
  toggle (default off, with a blunt Terms-of-Service warning) exists for the social
  adapters — no credential automation, ever.
- **Pagination** — "More results / next page" fetches a genuinely different page of
  results (SearXNG `pageno`).
- **Per-profile Local-AI settings** — the knowledge curator and assistant toggles now
  persist per profile in the database.
- README screenshots (hero + gallery).

### Changed

- **Search page UX:** the search controls (retention, kinds, sources, ranking-signals
  debug, DuckDuckGo) are now an always-visible bar — selectable *before* a search. The
  domain manager collapses while searching to keep results in focus, and the address
  bar placeholder is now "Search or enter a URL to visit".
- **Feature naming:** EarthMemory → **Journal**, EarthMultiMedia → **Media**, and the
  local AI is now **Sage** (summarizes what you read into your Journal and answers from
  it & the web, fully offline). The About panel is now a right-side drawer.

### Fixed

- **Secondary windows rendered as a blank "The URL can't be shown" box in packaged
  builds.** Programmatic windows now match the main window's incognito web context, so
  the app's asset protocol resolves (regression from the v1.0.8 single-instance change).
- **The knowledge curator switched itself back on after every restart.** Its on/off
  state was kept in the incognito WebView's localStorage (wiped each launch); it's now
  persisted per profile in the database.
- **Crawled pages polluted unrelated searches** (e.g. "best grapheneos phone" surfaced a
  crawled site with none of those words). The crawler fan-in now requires *all* query
  terms, is capped per domain, and live results are kept after ranking completes.
- A GC scheduler panic on startup (background task spawned outside the Tokio runtime).

## [1.0.8] - 2026-06-29

### Security
Defense-in-depth hardening against memory-corruption exploits and in-process theft of
secrets (the password/OTP vault, cookies, tokens). Each item is tagged by how strong a
boundary it really is — we don't oversell tripwires.

- **Vault isolation [BOUNDARY].** Saved logins are bound to the *real* page origin (read
  from the live webview, never a page- or caller-supplied string), so a malicious page can
  no longer trick an autofill into leaking another site's credential. Autofill injects the
  password directly into the page in the backend and never returns it to JS. A page cannot
  read or enumerate the vault by construction.
- **Redact-by-default [HARDENING].** The password/OTP lists now return *metadata only* — no
  plaintext. A single secret is fetched on demand through one gated, **rate-limited and
  audited** path (`vault_reveal`), and OTP codes are generated in the backend so the TOTP
  seed never reaches the UI. This contains the blast radius of a compromised UI: no silent
  mass-dump.
- **Append-only vault audit log + live security feed.** Every secret access (allowed or
  denied), origin mismatch, and rate-limit hit is recorded and surfaced.
- **In-process secret hygiene [HYGIENE].** Decrypted secrets are zeroized on drop and the
  cached master is mlock'd + kept out of core dumps; constant-time comparison for secrets;
  the backend disables core dumps so a crash can't spill plaintext to disk.
- **Sandboxing [BOUNDARY].** The WebKitGTK renderer sandbox (bubblewrap + seccomp) is now
  enabled, and the `yt-dlp` helper runs confined (no-new-privs + Landlock + seccomp): it can
  only write to your downloads folder and can't inspect other processes.
- **Allocator hardening [HARDENING].** Optional GrapheneOS hardened_malloc preload for the
  process tree (build it with `scripts/build-hardened-malloc.sh`; disable with
  `RECLAIM_HARDENED_MALLOC=0`).
- **Compile-time hardening [HARDENING].** Full RELRO + immediate binding, PIE/ASLR, non-exec
  stack, and FORTIFY/stack-protector/stack-clash/CET for bundled C code; a CI job verifies
  the flags actually land in the binary, plus `cargo-audit`/`cargo-deny` supply-chain checks.
- **Security panel.** A new right-dock **Security** panel shows a live posture header (engine
  isolation — Servo = safe Rust vs WebKit = C/C++, sandbox/allocator/integrity status) and an
  event feed, each item honestly tagged. A startup integrity self-check
  [DEFENSE-IN-DEPTH] flags corruption/tampering (not anti-tamper against root — we say so).
- **AI security assistant (advisory only).** An optional, clearly-labeled "AI · advisory"
  overlay summarizes/translates/triages events. It can never authorize, unblock, or suppress
  anything; security log text is treated as untrusted (prompt-injection-guarded) and stored
  separately from your browsing data. The panel works fully with it disabled.

### Added
- **Multiple windows.** Opening a new window (desktop tray / launcher) used to start a
  whole second copy of the app — two processes fighting over the same controls server
  and GPU video path, which caused crashes, duplicate control bars, and one window's
  controls driving the other. New windows now open **inside the running app**
  (single-instance), so they share one backend and behave predictably.
- **One shared media-controls bar across all windows** that follows the **last-clicked
  video pane** in any window. Each window's videos are independent players.
- **Per-window drag-and-drop** of media files, and working **window controls** (move,
  minimize, maximize, close) on every window; new windows open maximized.
- **Model reasoning in the AI Research/chat tool.** Thinking-capable models (e.g.
  deepseek-r1, qwen3, gpt-oss) now stream their reasoning under a 💭 *Thinking* header
  above the answer (with a graceful fallback for models that don't support it).
- **Idle auto-hide** for the floating media controls and the mouse cursor — they hide
  after a few seconds of inactivity and return on movement.

### Fixed
- **Escape now exits video fullscreen.** The native video surface renders above the
  page, hiding the on-screen exit control, so there was no way out; Escape is now wired
  up for both the native and CSS fullscreen paths.
- **The floating controls now show the active video's title** and an **exit-fullscreen
  button** (the in-page title/exit are occluded by the video in fullscreen).
- **Clearing/removing the queue no longer leaves images stuck** in unselected panes, and
  panes no longer show duplicate images.
- **Duplicate image title** stacked in the fullscreen overlay is gone.
- **Typing a URL navigates the current tab** (classic address-bar behavior). The
  "When opening links" toggle now only affects link/domain **clicks** — clicking a
  domain opens a new tab (New Tab) or reuses the tab (Overwrite) per the setting.
- Code-created windows no longer fall into browser/mock mode (they were missing window
  controls and real data). Secondary windows are **media-only** (the browser engine is
  single-window), so the Search button is hidden there and they open on Media.
- **Drag-and-drop now works on a second window.** Dropping video files onto a second
  window's Media panes did nothing — the drop was being routed to the main window (or
  nowhere) because the webview's reported identity was wrong for code-created windows.
  Drops are now delivered straight to the window they land on. A dropped file is also no
  longer added two or three times.
- **The floating controls now appear and work for a second window's video.** They used
  to stay hidden unless you first played something in the main window, and you couldn't
  drive a second window's video from them. The controls now show for whichever window
  starts playing.
- **The controls follow the last-clicked video across windows — in both directions.**
  Clicking back to the first window's video now returns the controls (and its shuffle/
  repeat/skip/playlist actions) to it; previously they got stuck on the other window.
- **Closing a second window while videos are playing no longer crashes the app.** The
  window's video pipelines are now stopped and their native surfaces torn down before the
  window closes, instead of being destroyed out from under the still-running video (which
  triggered an X11 `RenderBadPicture` crash).
- **Second windows reliably open on the Media tab.** Restoring the shared profile's last
  tab could flip a freshly-opened second window to Search; that restore now only runs in
  the main window.

### Changed
- **Fullscreen header:** Shuffle/Repeat moved to the left of the slideshow controls.
- Quieted noisy debug logging in the terminal and DevTools console.

## [1.0.7] - 2026-06-28

### Fixed
- **Videos now auto-advance through the queue.** When a clip finished, the pane
  used to freeze on the last frame instead of playing the next item. GStreamer's
  `playbin` stays in the *Playing* state at end-of-stream with the position parked
  at the duration, so the old "position is 0 and paused" end-check never fired. The
  player now detects end-of-stream via the GStreamer EOS bus message (exposed on
  the player status) with a position-frozen-at-end fallback, and advances the pane.
- **Shuffle and Repeat now actually work for video playback.** Queue advancement
  could stall after one or two clips — and the Next button would stop responding —
  because the next-item picker required a clip that was both unplayed *and* not
  already on another pane, a set that empties quickly when the queue is near the
  pane count, and the repeat-all reset read a stale played-set. Selection was
  rewritten to read live state and never stall: it prefers an unplayed, off-screen
  clip, advances anyway when the queue is smaller than the pane count, and loops
  the whole queue under Repeat-all. Repeat-one replays the current clip on auto-end
  but still advances on a manual Next.
- **Floating media controls now load in packaged builds** (and the stuck "The URL
  can't be shown" window is gone). The raw WebKitGTK controls webview couldn't load
  the embedded frontend in a packaged build; a small localhost asset server now
  serves the controls page so the X11 controls window loads in both dev and
  packaged builds.

### Changed
- **Floating controls: replaced the Shuffle button with Previous-/Next-video
  buttons.** Shuffle and Repeat are now single toggles in the media toolbar (they
  drive both photos and videos); the in-window seek buttons (±10s) use chevron
  icons to distinguish them from the new track-skip buttons, which respect the
  active Shuffle/Repeat cycle.

## [1.0.6] - 2026-06-28

### Fixed
- **Media playback and floating controls now work in packaged builds on Wayland.**
  The app now forces the X11 (GTK) backend, like the dev build does. The media
  player embeds video via X11 window reparenting, which only works on X11 — on a
  Wayland session, dropped videos opened in separate top-level windows and the
  floating controls never appeared. (Set `GDK_BACKEND` yourself to override.)

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
