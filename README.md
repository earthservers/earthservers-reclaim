<div align="center">
  <h1>EarthServers Reclaim</h1>
  <p><strong>Reclaim Your Digital Sovereignty</strong></p>
  <p>A privacy-first desktop browser with on-device AI, curated search, manual media saving, and community trust ratings.</p>

  [![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
  [![Rust](https://img.shields.io/badge/rust-%23000000.svg?style=flat&logo=rust&logoColor=white)](https://www.rust-lang.org/)
  [![Tauri](https://img.shields.io/badge/tauri-%2324C8DB.svg?style=flat&logo=tauri&logoColor=%23FFFFFF)](https://tauri.app/)
</div>

---

## Mission

> **"We don't desire to rule the Earth. Only to serve it."**

Reclaim puts you back in control of your digital life. Everything runs **locally** — your browsing, your AI, your data. Nothing leaves your device.

## Highlights

- **Browser** — tabbed WebKitGTK browser with a per‑tab page cache (switching tabs doesn't reload), persistent cookies/sessions, and a curated/resolved address bar.
- **NoScript + privacy** — per‑site JavaScript trust, third‑party cookie/ITP controls, UA spoofing, incognito.
- **Local AI (Ollama)**
  - **Knowledge Curator** — quietly summarizes pages you visit into a personal knowledge graph (EarthMemory). Transparent, unbiased, skips incognito.
  - **AI Assistant** — a private streaming chat, grounded in your own saved pages, media notes, and past conversations. Model auto‑selected by your GPU tier (or pick any installed model).
- **Password manager + autofill** — Argon2id‑gated vault, login autofill/autosave prompts.
- **Media downloader** — manually save images/gifs/videos from a page (with descriptions for your AI), plus **yt‑dlp** for streaming sites like YouTube.
- **EarthSearch** — trusted‑domain management, community trust/bias ratings.

---

## Prerequisites

| Requirement | Version | Notes |
|---|---|---|
| **Rust** | stable (1.77+) | <https://rustup.rs> |
| **Node.js** | 18+ | <https://nodejs.org> |
| **pnpm** | 8+ | `npm i -g pnpm` |
| **Ollama** | latest | *Optional but required for any AI feature* — <https://ollama.com> |
| **yt-dlp** | latest | *Optional* — only for downloading streaming‑site videos |

### System libraries (Linux)

Reclaim is a Tauri 2 app using WebKitGTK 4.1 and GStreamer (for media).

**Fedora / Nobara / RHEL:**
```bash
sudo dnf install -y webkit2gtk4.1-devel gtk3-devel libappindicator-gtk3-devel \
  librsvg2-devel openssl-devel curl wget file \
  gstreamer1-devel gstreamer1-plugins-base-devel \
  gstreamer1-plugins-good gstreamer1-plugins-bad-free gstreamer1-libav
# optional: yt-dlp
sudo dnf install -y yt-dlp
```

**Debian / Ubuntu:**
```bash
sudo apt update && sudo apt install -y libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev \
  libgtk-3-dev gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
  gstreamer1.0-plugins-bad gstreamer1.0-libav
# optional: pipx install yt-dlp
```

> macOS / Windows: install Rust + Node + pnpm; Tauri handles the rest. (Primary target is Linux.)

---

## Setup

```bash
git clone https://github.com/earthservers/earthservers-reclaim.git
cd earthservers-reclaim
pnpm install
```

### Enable the AI (Ollama)

The Curator and Assistant talk to a local Ollama daemon. Start it and pull the models:

```bash
# 1. Run the Ollama server (or use the system service / desktop app)
ollama serve

# 2. Curator model (page summaries) — small + fast
ollama pull llama3.2:3b

# 3. Assistant model — pick the tier that fits your GPU VRAM:
ollama pull llama3.2:1b      #  < 3 GB / CPU‑only
ollama pull llama3.2:3b      #  3–6 GB
ollama pull llama3.1:8b      #  6–12 GB   (good default)
ollama pull qwen2.5:14b      #  12–24 GB
ollama pull qwen2.5:32b      #  24 GB+
```

The Assistant auto‑recommends a model from your detected VRAM, but you can pick **any installed model** from the dropdown in the **Local AI** tab. If Ollama isn't running, AI features simply stay idle.

---

## Run & Compile

### Development (hot reload)

```bash
pnpm reclaim:x11      # recommended on Linux (forces X11 backend — avoids Wayland/GL quirks)
# or
pnpm reclaim          # default backend
```

The first run compiles the Rust backend (a few minutes); subsequent runs are fast.

### Production build (installable bundle)

```bash
pnpm reclaim:build
```

Bundles are written to `apps/reclaim/src-tauri/target/release/bundle/` (`.deb`, `.rpm`, AppImage, etc.).

### Useful flags
- `EARTH_EMBED=x11` — opt into the legacy X11 page‑surface embed (default is the GTK overlay embed).

---

## Architecture

```
                                 ┌──────────────────────────────────────────────────┐
                                 │                REACT UI  (host webview)            │
                                 │   App.tsx · WebView · LocalAIHub · panels (R-dock) │
                                 └───────────────┬───────────────────────────────────┘
                                                 │ Tauri invoke() / emit() events
                                                 ▼
┌──────────────────────────────────────────  RUST BACKEND  ──────────────────────────────────────────┐
│                                                                                                     │
│  SEARCH / NAVIGATION                                                                                 │
│   WebView --invoke('navigate')--> router/mod.rs                                                      │
│        |- RESOLUTION axis: LocalCache -> P2P(.click) -> Federated -> Blockchain(.earth) -> ICANN      │
│        \- RENDER axis:  .earth -> Servo (separate window)                                            │
│                          else  -> browser_overlay  (GTK overlay, ONE webview PER TAB,                │
│                                   shared PERSISTENT WebContext => cookies/sessions on disk)          │
│                                                                                                     │
│  PAGE <-> RUST BRIDGE  (injected content script, 'reclaimVault' channel)                            │
│   page --postMessage--> browser_surface::configure_page_webview                                      │
│        |- autofill-request / autosave  -> vault (Argon2id + AES-GCM)  -> autofill / save prompts      │
│        |- media-list (img/gif/video)   -> MediaPanel                                                 │
│        \- noscript:seen (web-ext .so)  -> per-tab SEEN_ORIGINS -> NoScript shield                     │
│                                                                                                     │
│  MEDIA                                                                                               │
│   MediaPanel --+- download_media(url, description)        -> ~/Downloads/Reclaim + media_downloads   │
│                \- download_video_ytdlp(pageUrl, desc)     -> yt-dlp ------------> (table w/ notes)   │
│                                                                                                     │
│  LOCAL AI  (Ollama @ localhost:11434)                                                               │
│   CURATOR:    page load -> curate_page -> ai::curate (summarize) -> memory.journal_page -> indexed   │
│   ASSISTANT:  LocalAIHub -> assistant_chat_stream                                                    │
│                 |- retrieve_context()  <-- indexed_pages (summaries)                                 │
│                 |                       <-- media_downloads (your descriptions)                      │
│                 |                       <-- past conversations (journaled chats)                     │
│                 |- SYSTEM_PROMPT + context + history -> Ollama /api/chat (stream)                    │
│                 \- assistant-chunk events -> UI ;  on done -> journal_conversation -> indexed_pages  │
│                                                                                                     │
│  STORAGE:  SQLite (earthservers.db)  +  OS keyring (at-rest keys)  +  ~/Downloads/Reclaim            │
└─────────────────────────────────────────────────────────────────────────────────────────────────┘
```

Everything above runs **on your machine**. The only outbound calls are the page loads you initiate, optional GitHub update checks, and (optionally) Ollama on `localhost`.

---

## Updating

Reclaim checks GitHub Releases on launch and shows a banner if a newer version is published, linking you to the download. To update, grab the latest release from
<https://github.com/earthservers/earthservers-reclaim/releases> (or `git pull` + rebuild if running from source).

---

## Contributing

Issues and PRs welcome. See `CLAUDE.md` for the developer guide, commit conventions, and project layout.

## License

MIT — see [LICENSE](LICENSE).
