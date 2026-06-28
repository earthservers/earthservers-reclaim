//! GTK-native embedded browser surface (Linux/X11).
//!
//! Tauri's `add_child` webview does not overlay at absolute coordinates on
//! WebKitGTK — it tiles vertically, which is why the embedded site ended up
//! below the app chrome. This module instead creates a standalone WebKitGTK
//! `WebView` inside a GTK window and **reparents it into the Tauri window via
//! X11** (`XReparentWindow`), positioned in the same coordinate space as the
//! DOM. This is the exact mechanism `video_surface.rs` uses to embed video; we
//! reuse it for a navigable browser.
//!
//! Threading: GTK/WebKit objects are not `Send`/`Sync`, so the live `WebView`
//! and `gtk::Window` are kept in a `thread_local!` on the GTK main thread.
//! Every operation that touches them is dispatched there via
//! `glib::MainContext::invoke`. X11 positioning (move/resize/map/unmap) uses the
//! stored XID and works from any thread, mirroring `video_surface.rs`.
//!
//! Single-surface model: there is one embedded browser surface, reused across
//! tabs (navigate / show / hide). Per-tab webviews remain out of scope.

use crate::webview::WebviewBounds;

/// User-toggleable privacy protections. All default ON (privacy-first).
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyConfig {
    pub block_webrtc: bool,
    pub block_third_party_cookies: bool,
    pub tracking_prevention: bool,
    pub block_dns_prefetch: bool,
    pub spoof_user_agent: bool,
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            block_webrtc: true,
            block_third_party_cookies: true,
            tracking_prevention: true,
            block_dns_prefetch: true,
            spoof_user_agent: true,
        }
    }
}

#[cfg(target_os = "linux")]
pub(crate) mod imp {
    use super::{PrivacyConfig, WebviewBounds};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::Mutex;
    use tauri::Manager;

    /// Monotonic id assigned to each download so progress events can be correlated.
    static DOWNLOAD_ID: AtomicU64 = AtomicU64::new(1);

    #[derive(Clone, serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct DownloadStarted {
        id: u64,
        url: String,
        filename: String,
    }
    #[derive(Clone, serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct DownloadProgress {
        id: u64,
        progress: f64,
    }
    #[derive(Clone, serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct DownloadDone {
        id: u64,
        path: String,
        ok: bool,
    }

    /// Hook WebKitGTK downloads on this context and forward them to the frontend
    /// as `download-started` / `download-progress` / `download-finished` events.
    /// Downloads are saved to ~/Downloads and tracked in-memory by the frontend
    /// (so the list clears on exit).
    fn wire_downloads(app: &tauri::AppHandle, web_context: &webkit2gtk::WebContext) {
        use tauri::Emitter;
        use webkit2gtk::{DownloadExt, URIRequestExt, WebContextExt};

        let app = app.clone();
        web_context.connect_download_started(move |_ctx, download| {
            let id = DOWNLOAD_ID.fetch_add(1, Ordering::SeqCst);
            let url = download
                .request()
                .and_then(|r| r.uri())
                .map(|s| s.to_string())
                .unwrap_or_default();

            // Pick a destination under ~/Downloads and announce the download.
            {
                let app = app.clone();
                let url = url.clone();
                download.connect_decide_destination(move |dl, suggested| {
                    let home = std::env::var("HOME").unwrap_or_default();
                    let dir = format!("{}/Downloads", home);
                    let _ = std::fs::create_dir_all(&dir);
                    dl.set_destination(&format!("file://{}/{}", dir, suggested));
                    let _ = app.emit(
                        "download-started",
                        DownloadStarted { id, url: url.clone(), filename: suggested.to_string() },
                    );
                    true
                });
            }
            {
                let app = app.clone();
                download.connect_received_data(move |dl, _len| {
                    let _ = app.emit("download-progress", DownloadProgress { id, progress: dl.estimated_progress() });
                });
            }
            {
                let app = app.clone();
                download.connect_finished(move |dl| {
                    let path = dl.destination().map(|s| s.to_string()).unwrap_or_default();
                    let _ = app.emit("download-finished", DownloadDone { id, path, ok: true });
                });
            }
            {
                let app = app.clone();
                download.connect_failed(move |_dl, _err| {
                    let _ = app.emit("download-finished", DownloadDone { id, path: String::new(), ok: false });
                });
            }
        });
    }

    lazy_static::lazy_static! {
        /// XID of the reparented GTK window (for X11 move/resize/map/unmap/destroy).
        static ref SURFACE_XID: Mutex<Option<u64>> = Mutex::new(None);
    }
    /// Guards against two concurrent create calls racing.
    static CREATING: AtomicBool = AtomicBool::new(false);

    lazy_static::lazy_static! {
        /// NoScript (true allowlist): JavaScript is BLOCKED by default; a host
        /// runs JS only if it is in this set. Hot-path lookups happen on the GTK
        /// thread, so this in-memory set is the source of truth; the SQLite table
        /// `js_allowlist` is the persistent backing, loaded once at startup.
        static ref JS_ALLOW: Mutex<std::collections::HashSet<String>> =
            Mutex::new(std::collections::HashSet::new());
        /// Session-only "temp trust": origins trusted until the app exits. Not
        /// persisted (unlike JS_ALLOW). The union of the two is the trust set.
        static ref TEMP_TRUST: Mutex<std::collections::HashSet<String>> =
            Mutex::new(std::collections::HashSet::new());
        /// Path to the app DB, set once via `init_js_policy`.
        static ref JS_DB_PATH: Mutex<Option<String>> = Mutex::new(None);
    }

    /// Registrable host for a URL, lowercased (reuses the router's parser).
    fn host_of(url: &str) -> String {
        crate::router::url::parse(url).host.trim_end_matches('.').to_ascii_lowercase()
    }

    /// Whether `host` is trusted (default: blocked). Trusted = persistent
    /// allowlist OR session temp-trust. Governs first-party JS and counts toward
    /// the set pushed to the extension for third-party request blocking.
    fn js_allowed(host: &str) -> bool {
        JS_ALLOW.lock().map(|s| s.contains(host)).unwrap_or(false)
            || TEMP_TRUST.lock().map(|s| s.contains(host)).unwrap_or(false)
    }

    /// The full trusted-origin set (persistent ∪ temp) pushed to the extension.
    fn trusted_origins() -> Vec<String> {
        let mut set: std::collections::HashSet<String> = JS_ALLOW
            .lock()
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default();
        if let Ok(t) = TEMP_TRUST.lock() {
            set.extend(t.iter().cloned());
        }
        set.into_iter().collect()
    }

    /// Broadcast the current trusted set to the NoScript web extension so it can
    /// block third-party requests from untrusted origins. Runs on the GTK thread.
    pub(crate) fn push_trust_to_extensions() {
        let origins = trusted_origins();
        // Overlay embed (default) owns its own WebContext, not the X11 SURFACE —
        // route the trust message there so its NoScript extension is initialized
        // and reports/blocks. (Without this the extension never gets set-trust and
        // detection stays empty.)
        if crate::browser_overlay::enabled() {
            crate::browser_overlay::push_trust(origins);
            return;
        }
        glib::MainContext::default().invoke(move || {
            use glib::ToVariant;
            use webkit2gtk::{UserMessage, WebContextExt};
            SURFACE.with(|s| {
                if let Some(obj) = s.borrow().as_ref() {
                    let msg = UserMessage::new("noscript:set-trust", Some(&origins.to_variant()));
                    obj.web_context.send_message_to_all_extensions(&msg);
                }
            });
        });
    }

    /// Device-bound key for at-rest encryption of the allowlist — same model as
    /// multimedia playlists: tied to this machine (machine-uid), never stored, so
    /// the persisted allowlist is ciphertext to anyone reading the DB file, yet
    /// needs no password to read on THIS device.
    fn browser_storage_key() -> String {
        crate::multimedia::local_data_secret()
    }

    /// Legacy machine-id key — retained ONLY to decrypt + migrate allowlists
    /// written before the keyring secret. Never used for new writes.
    fn legacy_browser_storage_key() -> String {
        let machine =
            machine_uid::get().unwrap_or_else(|_| "earthservers-default-device".to_string());
        format!("EarthBrowser::jsallow::{}", machine)
    }

    /// Encrypt the whole allowlist as ONE blob and write it to the single-row
    /// `js_allowlist` table. (A per-host row can't be both encrypted — random
    /// nonce — and a stable lookup key, so we store the set as one ciphertext.)
    fn persist_js_allow() -> Result<(), String> {
        let path = match JS_DB_PATH.lock().ok().and_then(|p| p.clone()) {
            Some(p) => p,
            None => return Ok(()), // not initialized (e.g. tests) — in-memory only
        };
        let hosts: Vec<String> = JS_ALLOW.lock().map_err(|e| e.to_string())?.iter().cloned().collect();
        let json = serde_json::to_string(&hosts).map_err(|e| e.to_string())?;
        let enc = crate::multimedia::encrypt_data(&json, &browser_storage_key())?;
        let conn = rusqlite::Connection::open(&path).map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT OR REPLACE INTO js_allowlist (id, data) VALUES (1, ?1)",
            [&enc],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Load the persisted (encrypted) allowlist into memory and remember the DB
    /// path. Safe to call once at startup; creates the table if missing.
    pub fn init_js_policy(db_path: String) {
        if let Ok(mut p) = JS_DB_PATH.lock() {
            *p = Some(db_path.clone());
        }
        match rusqlite::Connection::open(&db_path) {
            Ok(conn) => {
                let _ = conn.execute(
                    "CREATE TABLE IF NOT EXISTS js_allowlist (id INTEGER PRIMARY KEY CHECK (id = 1), data TEXT NOT NULL)",
                    [],
                );
                let enc: Option<String> = conn
                    .query_row("SELECT data FROM js_allowlist WHERE id = 1", [], |r| r.get(0))
                    .ok();
                if let Some(enc) = enc {
                    // Decrypt with the new keyring secret; fall back to the legacy
                    // machine-id key for allowlists written before the migration.
                    let decrypted = crate::multimedia::decrypt_data(&enc, &browser_storage_key())
                        .map(|j| (j, false))
                        .or_else(|_| {
                            crate::multimedia::decrypt_data(&enc, &legacy_browser_storage_key())
                                .map(|j| (j, true))
                        });
                    if let Ok((json, was_legacy)) = decrypted {
                        if let Ok(hosts) = serde_json::from_str::<Vec<String>>(&json) {
                            if let Ok(mut set) = JS_ALLOW.lock() {
                                set.extend(hosts);
                            }
                        }
                        // Re-encrypt under the keyring secret so the machine-id
                        // copy is replaced.
                        if was_legacy {
                            let _ = persist_js_allow();
                        }
                    }
                }

                // Privacy config (plain JSON — just toggles, nothing sensitive).
                let _ = conn.execute(
                    "CREATE TABLE IF NOT EXISTS privacy_config (id INTEGER PRIMARY KEY CHECK (id = 1), data TEXT NOT NULL)",
                    [],
                );
                let pc: Option<String> = conn
                    .query_row("SELECT data FROM privacy_config WHERE id = 1", [], |r| r.get(0))
                    .ok();
                if let Some(json) = pc {
                    if let Ok(cfg) = serde_json::from_str::<PrivacyConfig>(&json) {
                        if let Ok(mut c) = PRIVACY_CONFIG.lock() {
                            *c = cfg;
                        }
                    }
                }
            }
            Err(e) => log::error!("[browser_surface] js_allowlist init failed: {}", e),
        }
    }

    /// Read the current privacy config.
    pub async fn get_privacy_config() -> Result<PrivacyConfig, String> {
        Ok(PRIVACY_CONFIG.lock().map_err(|e| e.to_string())?.clone())
    }

    /// Update the privacy config: persist it, apply it to the live surface, and
    /// reload so it takes full effect (UA / cookie policy change at load time).
    pub async fn set_privacy_config(cfg: PrivacyConfig) -> Result<(), String> {
        {
            let mut c = PRIVACY_CONFIG.lock().map_err(|e| e.to_string())?;
            *c = cfg.clone();
        }
        if let Some(path) = JS_DB_PATH.lock().ok().and_then(|p| p.clone()) {
            let json = serde_json::to_string(&cfg).map_err(|e| e.to_string())?;
            let conn = rusqlite::Connection::open(&path).map_err(|e| e.to_string())?;
            conn.execute(
                "INSERT OR REPLACE INTO privacy_config (id, data) VALUES (1, ?1)",
                [&json],
            )
            .map_err(|e| e.to_string())?;
        }
        apply_privacy_live();
        Ok(())
    }

    /// Apply the current privacy config to the live surface (settings + context)
    /// and reload. Runs on the GTK thread.
    fn apply_privacy_live() {
        let cfg = match PRIVACY_CONFIG.lock() {
            Ok(c) => c.clone(),
            Err(_) => return,
        };
        // Overlay embed (default) owns its own webview + context, not the X11
        // SURFACE — apply the live toggle there so flipping a Privacy switch takes
        // effect immediately on the current page.
        if crate::browser_overlay::enabled() {
            crate::browser_overlay::apply_privacy(cfg);
            return;
        }
        glib::MainContext::default().invoke(move || {
            use webkit2gtk::{CookieAcceptPolicy, CookieManagerExt, WebContextExt, WebViewExt, WebsiteDataManagerExt};
            SURFACE.with(|s| {
                if let Some(obj) = s.borrow().as_ref() {
                    if let Some(settings) = WebViewExt::settings(&obj.webview) {
                        apply_privacy_settings(&settings, &cfg);
                    }
                    if let Some(cm) = obj.web_context.cookie_manager() {
                        cm.set_accept_policy(if cfg.block_third_party_cookies {
                            CookieAcceptPolicy::NoThirdParty
                        } else {
                            CookieAcceptPolicy::Always
                        });
                    }
                    if let Some(wdm) = obj.web_context.website_data_manager() {
                        wdm.set_itp_enabled(cfg.tracking_prevention);
                    }
                    WebViewExt::reload(&obj.webview);
                }
            });
        });
    }

    /// Apply the privacy toggles to a WebView's settings (shared by create + live,
    /// X11 surface + GTK overlay).
    pub(crate) fn apply_privacy_settings(settings: &webkit2gtk::Settings, cfg: &PrivacyConfig) {
        use webkit2gtk::SettingsExt;
        SettingsExt::set_enable_webrtc(settings, !cfg.block_webrtc);
        SettingsExt::set_enable_dns_prefetching(settings, !cfg.block_dns_prefetch);
        SettingsExt::set_user_agent(settings, if cfg.spoof_user_agent { Some(SPOOF_UA) } else { None });
        // Note: `<a ping>` tracking beacons go to third-party origins, which the
        // NoScript extension already blocks by default — and WebKit 2.52 made the
        // hyperlink-auditing setting a no-op, so we don't call it here.
    }

    /// Record where the NoScript web-process extension (.so) lives, so the
    /// surface's WebContext can load it. Called once at startup.
    pub fn set_noscript_ext_dir(dir: String) {
        if let Ok(mut g) = NOSCRIPT_EXT_DIR.lock() {
            *g = Some(dir);
        }
    }

    /// Per-host NoScript toggle. Persists (encrypted), updates the in-memory set,
    /// and — when the toggled host is the page currently showing — applies it
    /// live + reloads.
    pub async fn set_js_allowed(host: String, allowed: bool) -> Result<(), String> {
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        {
            let mut set = JS_ALLOW.lock().map_err(|e| e.to_string())?;
            if allowed { set.insert(host.clone()); } else { set.remove(&host); }
        }
        persist_js_allow()?;
        push_trust_to_extensions();
        // Apply to the live surface only if it is currently on this host, then reload.
        on_webview(move |wv| {
            use webkit2gtk::{SettingsExt, WebViewExt};
            let current = WebViewExt::uri(wv).map(|s| s.to_string()).unwrap_or_default();
            if host_of(&current) == host {
                if let Some(settings) = WebViewExt::settings(wv) {
                    SettingsExt::set_enable_javascript(&settings, allowed);
                }
                WebViewExt::reload(wv);
            }
        });
        Ok(())
    }

    /// Whether `host` currently runs JS (for the shield's per-page state).
    pub async fn get_js_allowed(host: String) -> Result<bool, String> {
        Ok(js_allowed(&host.trim_end_matches('.').to_ascii_lowercase()))
    }

    /// Origins seen on the current page (origin, first_party), with each one's
    /// current trust state — so the panel populates on open regardless of timing.
    /// Record an observed origin for a specific tab (deduped).
    pub(crate) fn record_seen_origin(tab_id: i64, origin: String, first_party: bool) {
        if let Ok(mut map) = SEEN_ORIGINS.lock() {
            let v = map.entry(tab_id).or_default();
            if !v.iter().any(|(o, _)| o == &origin) {
                v.push((origin, first_party));
            }
        }
    }

    pub async fn list_origins() -> Result<Vec<(String, bool, String)>, String> {
        // The ACTIVE tab's origins (CURRENT_TAB is updated on every tab switch).
        let tab = CURRENT_TAB.lock().ok().and_then(|g| *g).unwrap_or(-1);
        let seen = SEEN_ORIGINS
            .lock()
            .map_err(|e| e.to_string())?
            .get(&tab)
            .cloned()
            .unwrap_or_default();
        let mut out = Vec::with_capacity(seen.len());
        for (origin, first_party) in seen {
            let state = if JS_ALLOW.lock().map(|s| s.contains(&origin)).unwrap_or(false) {
                "trusted"
            } else if TEMP_TRUST.lock().map(|s| s.contains(&origin)).unwrap_or(false) {
                "temp"
            } else {
                "untrusted"
            };
            out.push((origin, first_party, state.to_string()));
        }
        Ok(out)
    }

    /// Per-origin trust state for the NoScript modal: "trusted" (persistent),
    /// "temp" (this session), or "untrusted" (default/blocked).
    pub async fn get_trust(origin: String) -> Result<String, String> {
        let o = origin.trim_end_matches('.').to_ascii_lowercase();
        if JS_ALLOW.lock().map(|s| s.contains(&o)).unwrap_or(false) {
            Ok("trusted".into())
        } else if TEMP_TRUST.lock().map(|s| s.contains(&o)).unwrap_or(false) {
            Ok("temp".into())
        } else {
            Ok("untrusted".into())
        }
    }

    /// Set a per-origin trust state, persist (for "trusted"), push the new trust
    /// set to the extension, and reload the live page so the change takes effect.
    pub async fn set_trust(origin: String, state: String) -> Result<(), String> {
        let o = origin.trim_end_matches('.').to_ascii_lowercase();
        match state.as_str() {
            "trusted" => {
                if let Ok(mut s) = TEMP_TRUST.lock() { s.remove(&o); }
                if let Ok(mut s) = JS_ALLOW.lock() { s.insert(o.clone()); }
                persist_js_allow()?;
            }
            "temp" => {
                if let Ok(mut s) = JS_ALLOW.lock() { s.remove(&o); }
                persist_js_allow()?;
                if let Ok(mut s) = TEMP_TRUST.lock() { s.insert(o.clone()); }
            }
            "untrusted" => {
                if let Ok(mut s) = JS_ALLOW.lock() { s.remove(&o); }
                persist_js_allow()?;
                if let Ok(mut s) = TEMP_TRUST.lock() { s.remove(&o); }
            }
            other => return Err(format!("invalid trust state: {other}")),
        }
        push_trust_to_extensions();
        // Apply first-party JS policy if this is the current page, then reload so
        // third-party blocking changes take effect for all origins on the page.
        on_webview(move |wv| {
            use webkit2gtk::{SettingsExt, WebViewExt};
            let current = WebViewExt::uri(wv).map(|s| s.to_string()).unwrap_or_default();
            if host_of(&current) == o {
                if let Some(settings) = WebViewExt::settings(wv) {
                    SettingsExt::set_enable_javascript(&settings, js_allowed(&o));
                }
            }
            WebViewExt::reload(wv);
        });
        Ok(())
    }

    /// All allowlisted hosts (for a management view).
    pub async fn list_js_allowed() -> Result<Vec<String>, String> {
        let mut v: Vec<String> = JS_ALLOW
            .lock()
            .map_err(|e| e.to_string())?
            .iter()
            .cloned()
            .collect();
        v.sort();
        Ok(v)
    }

    // Live GTK objects live ONLY on the GTK main thread.
    thread_local! {
        static SURFACE: std::cell::RefCell<Option<SurfaceObjects>> = std::cell::RefCell::new(None);
    }

    struct SurfaceObjects {
        window: gtk::Window,
        webview: webkit2gtk::WebView,
        /// The surface's context — used to broadcast trust updates to the
        /// NoScript web extension via `send_message_to_all_extensions`.
        web_context: webkit2gtk::WebContext,
    }

    lazy_static::lazy_static! {
        /// Tab that last drove the surface, so emitted events carry the right tab id.
        static ref CURRENT_TAB: Mutex<Option<i64>> = Mutex::new(None);
        /// Latest observed page title / URL (served by the get_title command).
        static ref LATEST_TITLE: Mutex<Option<String>> = Mutex::new(None);
        static ref LATEST_URL: Mutex<Option<String>> = Mutex::new(None);
        /// Last URL we were asked to load. Lets a remount/restore (same URL,
        /// with bounds) skip reloading so the live page + scroll position survive.
        static ref LAST_NAV_URL: Mutex<Option<String>> = Mutex::new(None);
        /// Directory holding the NoScript web-process extension (.so), set at
        /// startup. When present, the surface's WebContext loads it so the
        /// extension can observe (Phase 1) / block (Phase 2) per-origin requests.
        static ref NOSCRIPT_EXT_DIR: Mutex<Option<String>> = Mutex::new(None);
        /// Origins the extension reported for the CURRENT page (origin, first_party),
        /// cleared on navigation. The panel queries this on open so it works even
        /// when it mounts after the page already loaded (events aren't buffered).
        /// Observed request origins PER TAB (tab_id -> Vec<(origin, first_party)>).
        /// Per-tab because the overlay now keeps one live webview per tab, so the
        /// NoScript panel must show the ACTIVE tab's origins, not a global mix.
        static ref SEEN_ORIGINS: Mutex<std::collections::HashMap<i64, Vec<(String, bool)>>> =
            Mutex::new(std::collections::HashMap::new());
        /// User-toggleable privacy protections, loaded from the DB at startup.
        static ref PRIVACY_CONFIG: Mutex<PrivacyConfig> = Mutex::new(PrivacyConfig::default());
    }

    /// True while a page is loading. The page webview is kept HIDDEN for its whole
    /// load and only revealed once it has painted — so the user never sees the
    /// half-rendered "white page with raw text" first paint. Set from the WebKit
    /// `load-changed`/`load-failed` signals.
    static SURFACE_LOADING: AtomicBool = AtomicBool::new(false);
    /// Whether the UI wants the surface visible at all (set by show()/hide()). The
    /// post-load reveal only shows the page if it's still wanted — so finishing a
    /// load that the user already navigated away from won't flash the old page.
    static SURFACE_WANTED: AtomicBool = AtomicBool::new(false);

    pub(crate) fn surface_set_loading(v: bool) {
        SURFACE_LOADING.store(v, Ordering::SeqCst);
    }
    pub(crate) fn surface_loading() -> bool {
        SURFACE_LOADING.load(Ordering::SeqCst)
    }
    pub(crate) fn surface_set_wanted(v: bool) {
        SURFACE_WANTED.store(v, Ordering::SeqCst);
    }
    pub(crate) fn surface_wanted() -> bool {
        SURFACE_WANTED.load(Ordering::SeqCst)
    }

    /// Delay before revealing a freshly-finished page, giving WebKit a beat to
    /// paint so the reveal shows real content, not the initial blank document.
    const REVEAL_SETTLE_MS: u64 = 220;

    /// A common, current Firefox-on-Linux UA. Presenting a widely-shared UA (and
    /// not advertising "embedded WebKitGTK") shrinks the user-agent fingerprint by
    /// blending in with the large Firefox population.
    const SPOOF_UA: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:128.0) Gecko/20100101 Firefox/128.0";

    #[derive(Clone, serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct TitlePayload {
        tab_id: i64,
        title: String,
        url: String,
    }

    /// A request origin the NoScript extension observed on the current page.
    #[derive(Clone, serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct NoscriptOrigin {
        origin: String,
        first_party: bool,
    }

    /// Page load-state change pushed to the UI so it can drive a loading spinner
    /// from REAL WebKit load events (not a timer). `phase` is `started` /
    /// `finished` / `failed`.
    #[derive(Clone, serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct LoadPayload {
        tab_id: i64,
        phase: &'static str,
    }

    #[derive(Clone, serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct AutofillEvent {
        origin: String,
    }

    #[derive(Clone, serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct AutosaveEvent {
        origin: String,
        username: String,
    }

    /// A downloadable media element found on the page (received from the page,
    /// re-emitted to the Media panel).
    #[derive(Clone, serde::Serialize, serde::Deserialize)]
    struct MediaItem {
        kind: String,
        url: String,
        #[serde(default)]
        thumb: String,
    }

    #[derive(Clone, serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct MediaListEvent {
        origin: String,
        items: Vec<MediaItem>,
    }

    /// Injected into every page. Detects a login form, asks Rust for saved
    /// credentials (`autofill-request`), exposes `__reclaimFill` for Rust to fill
    /// them, and reports login submits (`autosave`) so we can offer to save. All
    /// traffic goes over the `reclaimVault` script-message channel. No-ops on pages
    /// with no password field; never reads anything until the user submits.
    const AUTOFILL_USER_SCRIPT: &str = r#"
(function(){
  if (window.__reclaimVaultInit) return; window.__reclaimVaultInit = true;
  var ch = window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.reclaimVault;
  if (!ch) return;
  // Liveness ping: if you see "[autofill] hello" in the terminal, the bridge ran
  // (JS is enabled for this site). If you DON'T, JS is blocked by NoScript here.
  ch.postMessage(JSON.stringify({type:'hello', origin: location.origin}));
  function fields(){
    var pw = document.querySelector('input[type="password"]');
    if (!pw) return null;
    var scope = pw.form || document;
    var ins = Array.prototype.slice.call(scope.querySelectorAll('input'));
    var user = null;
    for (var i=0;i<ins.length;i++){
      var t = (ins[i].type||'text').toLowerCase();
      if (ins[i]===pw) break;
      if (t==='text'||t==='email'||t==='tel') user = ins[i];
    }
    return { pw: pw, user: user };
  }
  window.__reclaimFill = function(username, password){
    var f = fields(); if (!f) return;
    function set(el,val){ if(!el||val==null) return;
      var d = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype,'value');
      d && d.set ? d.set.call(el,val) : (el.value=val);
      el.dispatchEvent(new Event('input',{bubbles:true}));
      el.dispatchEvent(new Event('change',{bubbles:true}));
    }
    set(f.user, username); set(f.pw, password);
  };
  // Enumerate downloadable media on the page (called on demand from Rust when the
  // Media panel opens). Posts the list back over the same channel.
  window.__reclaimCollectMedia = function(){
    var seen = {}, out = [];
    function add(kind, url, thumb){
      if (!url || url.lastIndexOf('data:',0)===0) return;
      if (seen[url]) return; seen[url] = 1;
      if (out.length < 300) out.push({ kind: kind, url: url, thumb: thumb || url });
    }
    try {
      var imgs = document.querySelectorAll('img');
      for (var i=0;i<imgs.length;i++){ var s = imgs[i].currentSrc || imgs[i].src; add(/\.gif(\?|$)/i.test(s||'') ? 'gif' : 'image', s); }
      var vids = document.querySelectorAll('video');
      for (var j=0;j<vids.length;j++){ var v = vids[j]; var vs = v.currentSrc || v.src; if(!vs){ var so = v.querySelector('source'); if(so) vs = so.src; } add('video', vs, v.poster || ''); }
    } catch(e){}
    ch.postMessage(JSON.stringify({ type:'media-list', origin: location.origin, items: out }));
  };
  var requested = false;
  var mo = null;
  function requestFill(){
    if (requested) return;
    if (fields()){
      requested = true;
      if (mo){ try{ mo.disconnect(); }catch(e){} }
      ch.postMessage(JSON.stringify({type:'autofill-request', origin: location.origin}));
    }
  }
  // Login forms are often rendered late or revealed on interaction (SPA / Turbo /
  // "sign in with email" toggles), so detect aggressively: watch the DOM, retry on
  // load, and re-check whenever the user focuses or clicks into the page.
  try {
    mo = new MutationObserver(requestFill);
    mo.observe(document.documentElement, {childList:true, subtree:true, attributes:true, attributeFilter:['type']});
    setTimeout(function(){ if(mo){ try{ mo.disconnect(); }catch(e){} } }, 60000);
  } catch(e){}
  window.addEventListener('load', requestFill);
  document.addEventListener('focusin', requestFill, true);
  document.addEventListener('click', requestFill, true);

  // Autosave capture. Real <form> submits are only one path — many sites log in
  // via a button + JS with no submit event — so we remember the latest typed
  // credentials and report on submit, login-button click, or page unload.
  var pending = null, sent = false;
  document.addEventListener('input', function(e){
    var t = e.target;
    if (t && t.tagName === 'INPUT' && (t.type||'').toLowerCase() === 'password' && t.value){
      var f = fields();
      pending = { origin: location.origin, username: (f && f.user && f.user.value) || '', password: t.value };
      sent = false;
    }
  }, true);
  function report(){
    if (pending && pending.password && !sent){
      sent = true;
      ch.postMessage(JSON.stringify({ type:'autosave', origin: pending.origin, username: pending.username, password: pending.password }));
    }
  }
  document.addEventListener('submit', report, true);
  document.addEventListener('click', function(e){
    var el = e.target; if (!el || !el.closest) return;
    if (el.closest('button, input[type=submit], input[type=button], [role=button], a')) report();
  }, true);
  document.addEventListener('keydown', function(e){
    if (e.key === 'Enter' && pending && pending.password) report();
  }, true);
  window.addEventListener('pagehide', report, true);

  if (document.readyState==='loading') document.addEventListener('DOMContentLoaded', requestFill);
  else requestFill();
})();
"#;

    /// Pending captured login awaiting the user's "save?" confirmation. Holds
    /// (origin, username, password) in RAM only; cleared on confirm/dismiss. The
    /// password never travels to the JS frontend — only origin+username do.
    static PENDING_AUTOSAVE: Mutex<Option<(String, String, String)>> = Mutex::new(None);

    pub(crate) fn set_pending_autosave(origin: String, username: String, password: String) {
        if let Ok(mut g) = PENDING_AUTOSAVE.lock() {
            *g = Some((origin, username, password));
        }
    }
    pub(crate) fn take_pending_autosave() -> Option<(String, String, String)> {
        PENDING_AUTOSAVE.lock().ok().and_then(|mut g| g.take())
    }
    /// Read the pending capture WITHOUT consuming it (so the prompt-or-skip
    /// decision can inspect it before `take` on confirm).
    pub(crate) fn peek_pending_autosave() -> Option<(String, String, String)> {
        PENDING_AUTOSAVE.lock().ok().and_then(|g| g.clone())
    }

    /// Handle a `reclaimVault` message from a page: relay autofill requests and
    /// login captures to the frontend (which owns profile + unlock context).
    fn handle_vault_message(app: &tauri::AppHandle, jr: &webkit2gtk::JavascriptResult) {
        use tauri::Emitter;
        let json = jr.js_value().map(|v| v.to_string()).unwrap_or_default();
        #[derive(serde::Deserialize)]
        struct Msg {
            #[serde(rename = "type")]
            kind: String,
            origin: String,
            #[serde(default)]
            username: String,
            #[serde(default)]
            password: String,
            #[serde(default)]
            items: Vec<MediaItem>,
        }
        let msg: Msg = match serde_json::from_str(&json) {
            Ok(m) => m,
            Err(_) => return,
        };
        match msg.kind.as_str() {
            "autofill-request" => {
                let _ = app.emit("autofill-request", AutofillEvent { origin: msg.origin });
            }
            "autosave" => {
                set_pending_autosave(msg.origin.clone(), msg.username.clone(), msg.password);
                let _ = app.emit(
                    "autofill-save-request",
                    AutosaveEvent { origin: msg.origin, username: msg.username },
                );
            }
            "media-list" => {
                let _ = app.emit("media-list", MediaListEvent { origin: msg.origin, items: msg.items });
            }
            _ => {}
        }
    }

    pub(crate) fn set_current_tab(tab_id: i64) {
        if let Ok(mut g) = CURRENT_TAB.lock() {
            *g = Some(tab_id);
        }
    }

    /// Clear a tab's observed-origins list (call on that tab's navigation start).
    pub(crate) fn clear_seen_origins(tab_id: i64) {
        if let Ok(mut map) = SEEN_ORIGINS.lock() {
            map.remove(&tab_id);
        }
    }

    /// Per-host JS policy (NoScript): whether scripts run for `url`'s host.
    pub(crate) fn js_allowed_for(url: &str) -> bool {
        js_allowed(&host_of(url))
    }

    /// Build a fully-configured page `WebView` — NoScript web extension, privacy
    /// (cookies/ITP/UA/WebRTC), downloads, and title/origin events — WITHOUT
    /// attaching it to a container or loading a URL. Shared by the X11 surface and
    /// the GTK-overlay embed so page security/behavior is identical no matter how
    /// it's mounted. Must run on the GTK main thread; `url` seeds the initial
    /// per-host JS policy. Returns the webview AND its `WebContext` (the caller
    /// must keep the context to send NoScript trust messages to its extension).
    pub(crate) fn configure_page_webview(
        app: &tauri::AppHandle,
        tab_id: i64,
        url: &str,
        shared_context: Option<&webkit2gtk::WebContext>,
    ) -> (webkit2gtk::WebView, webkit2gtk::WebContext) {
        use webkit2gtk::{
            CookieAcceptPolicy, CookieManagerExt, HardwareAccelerationPolicy, LoadEvent,
            SettingsExt, UserContentManagerExt, UserMessageExt, WebContext, WebContextExt,
            WebViewExt, WebsiteDataManagerExt,
        };
        let _ = gtk::init();

        // Reuse a shared context when given (so cached per-tab webviews share one
        // cookie jar / session); otherwise create + configure a fresh one. The
        // context-level setup (extensions dir, cookie policy, ITP, downloads) is
        // done ONCE on the context — skipped when reusing a shared one.
        let web_context = match shared_context {
            Some(ctx) => ctx.clone(),
            None => {
                use tauri::Manager;
                // Persist cookies + site storage across restarts for NORMAL
                // browsing (so logins survive a restart). Use an ephemeral
                // (in-memory, nothing on disk) context only when incognito is
                // active. NOTE: the overlay shares one context across tabs, so this
                // is decided once at creation — full per-profile incognito isolation
                // is a follow-up.
                let incognito = !crate::privacy::PrivacyManager::get_incognito_profiles().is_empty();
                let data_dir = if incognito {
                    None
                } else {
                    app.path().app_data_dir().ok().map(|d| d.join("browser-data"))
                };
                let web_context = match &data_dir {
                    Some(dir) => {
                        let _ = std::fs::create_dir_all(dir);
                        let wdm = webkit2gtk::WebsiteDataManager::builder()
                            .base_data_directory(dir.to_string_lossy().to_string())
                            .base_cache_directory(dir.join("cache").to_string_lossy().to_string())
                            .build();
                        let ctx = WebContext::with_website_data_manager(&wdm);
                        if let Some(cm) = ctx.cookie_manager() {
                            cm.set_persistent_storage(
                                &dir.join("cookies.sqlite").to_string_lossy(),
                                webkit2gtk::CookiePersistentStorage::Sqlite,
                            );
                        }
                        ctx
                    }
                    None => WebContext::new_ephemeral(),
                };
                if let Some(dir) = NOSCRIPT_EXT_DIR.lock().ok().and_then(|g| g.clone()) {
                    web_context.set_web_extensions_directory(&dir);
                }
                {
                    let cfg = PRIVACY_CONFIG.lock().map(|c| c.clone()).unwrap_or_default();
                    if let Some(cm) = web_context.cookie_manager() {
                        cm.set_accept_policy(if cfg.block_third_party_cookies {
                            CookieAcceptPolicy::NoThirdParty
                        } else {
                            CookieAcceptPolicy::Always
                        });
                    }
                    if let Some(wdm) = web_context.website_data_manager() {
                        wdm.set_itp_enabled(cfg.tracking_prevention);
                    }
                }
                wire_downloads(app, &web_context);
                web_context
            }
        };

        // Autofill/autosave bridge: a user script (injected into every frame)
        // detects login forms and captures submits, talking to Rust over the
        // `reclaimVault` script-message channel. Filling is driven back from the
        // command layer (which knows the profile + unlock state).
        let ucm = webkit2gtk::UserContentManager::new();
        ucm.register_script_message_handler("reclaimVault");
        {
            use webkit2gtk::{UserContentInjectedFrames, UserScript, UserScriptInjectionTime};
            let script = UserScript::new(
                AUTOFILL_USER_SCRIPT,
                UserContentInjectedFrames::AllFrames,
                UserScriptInjectionTime::End,
                &[],
                &[],
            );
            ucm.add_script(&script);
        }
        {
            let a = app.clone();
            ucm.connect_script_message_received(Some("reclaimVault"), move |_ucm, jr| {
                handle_vault_message(&a, jr);
            });
        }

        let webview = webkit2gtk::WebView::builder()
            .web_context(&web_context)
            .user_content_manager(&ucm)
            .build();
        if let Some(settings) = WebViewExt::settings(&webview) {
            SettingsExt::set_enable_developer_extras(&settings, true);
            SettingsExt::set_javascript_can_access_clipboard(&settings, true);
            // GL deadlocks on the reparented setup; the overlay is software too.
            SettingsExt::set_hardware_acceleration_policy(&settings, HardwareAccelerationPolicy::Never);
            SettingsExt::set_enable_smooth_scrolling(&settings, true);
            SettingsExt::set_enable_javascript(&settings, js_allowed(&host_of(url)));
            if let Ok(cfg) = PRIVACY_CONFIG.lock() {
                apply_privacy_settings(&settings, &cfg);
            }
        }

        // NoScript: relay each observed request origin to the frontend.
        {
            let a = app.clone();
            webview.connect_user_message_received(move |_v, msg| {
                if msg.name().as_deref() == Some("noscript:seen") {
                    let parsed = msg.parameters().and_then(|p| {
                        p.get::<(String, bool)>()
                            .or_else(|| p.get::<String>().map(|o| (o, false)))
                    });
                    if let Some((origin, first_party)) = parsed {
                        record_seen_origin(tab_id, origin.clone(), first_party);
                        // Only push live to the panel when THIS tab is the active
                        // one — a background tab's late reports shouldn't appear.
                        let cur = CURRENT_TAB.lock().ok().and_then(|g| *g).unwrap_or(-1);
                        if tab_id == cur {
                            use tauri::Emitter;
                            let _ = a.emit("noscript-origin", NoscriptOrigin { origin, first_party });
                        }
                    }
                }
                false
            });
        }
        // Push title / URL to the frontend as the page loads.
        {
            let a = app.clone();
            webview.connect_load_changed(move |wv, ev| {
                // Drive the loading spinner from real load state: show on Started,
                // hide on Finished (load-failed below covers the error path).
                match ev {
                    LoadEvent::Started => {
                        emit_load(&a, "started");
                        // Hide the page for the whole load so the half-painted first
                        // render never shows; the spinner covers this gap.
                        hide_during_load(wv);
                    }
                    LoadEvent::Finished => {
                        emit_load(&a, "finished");
                        reveal_after_load(wv);
                    }
                    _ => {}
                }
                if matches!(ev, LoadEvent::Committed | LoadEvent::Finished) {
                    let title = WebViewExt::title(wv).map(|s| s.to_string()).unwrap_or_default();
                    let uri = WebViewExt::uri(wv).map(|s| s.to_string()).unwrap_or_default();
                    emit_title(&a, title, uri);
                }
            });
        }
        {
            // A failed load never reaches `Finished`, so hide the spinner and
            // reveal the (error) page here too.
            let a = app.clone();
            webview.connect_load_failed(move |wv, _ev, _uri, _err| {
                emit_load(&a, "failed");
                reveal_after_load(wv);
                false // let WebKit render its default error page
            });
        }
        {
            let a = app.clone();
            webview.connect_title_notify(move |wv| {
                let title = WebViewExt::title(wv).map(|s| s.to_string()).unwrap_or_default();
                let uri = WebViewExt::uri(wv).map(|s| s.to_string()).unwrap_or_default();
                if !title.is_empty() {
                    emit_title(&a, title, uri);
                }
            });
        }
        (webview, web_context)
    }

    /// Record the latest title/url and emit `browser-title-changed` to the
    /// frontend (the WebView listener installed in Phase 1 consumes this).
    fn emit_title(app: &tauri::AppHandle, title: String, url: String) {
        use tauri::Emitter;
        if let Ok(mut g) = LATEST_TITLE.lock() {
            *g = Some(title.clone());
        }
        if let Ok(mut g) = LATEST_URL.lock() {
            *g = Some(url.clone());
        }
        let tab_id = CURRENT_TAB.lock().ok().and_then(|g| *g).unwrap_or(-1);
        let _ = app.emit("browser-title-changed", TitlePayload { tab_id, title, url });
    }

    /// Emit a page load-state change so the UI's loading spinner is driven by real
    /// WebKit load events. Fires uniformly for reloads, URL navigations,
    /// back/forward, and in-page link clicks — all of which go through WebKit.
    fn emit_load(app: &tauri::AppHandle, phase: &'static str) {
        use tauri::Emitter;
        let tab_id = CURRENT_TAB.lock().ok().and_then(|g| *g).unwrap_or(-1);
        let _ = app.emit("browser-load-changed", LoadPayload { tab_id, phase });
    }

    /// Hide the page webview for the duration of a load so its half-painted first
    /// render is never shown (the React spinner covers the gap). Overlay embed
    /// only — the X11 fallback maps/unmaps its surface separately.
    fn hide_during_load(wv: &webkit2gtk::WebView) {
        if !crate::browser_overlay::enabled() {
            return;
        }
        surface_set_loading(true);
        use gtk::prelude::WidgetExt;
        wv.hide();
    }

    /// Reveal the page after a load finishes/fails — once it has had a beat to
    /// paint, and only if the UI still wants the surface shown (the user may have
    /// switched away or opened a panel mid-load).
    fn reveal_after_load(wv: &webkit2gtk::WebView) {
        if !crate::browser_overlay::enabled() {
            return;
        }
        surface_set_loading(false);
        let wv = wv.clone();
        glib::timeout_add_local_once(
            std::time::Duration::from_millis(REVEAL_SETTLE_MS),
            move || {
                // Only reveal if still wanted, not loading, AND this is the active
                // tab — a backgrounded tab finishing its load must not pop over the
                // page the user switched to.
                if surface_wanted()
                    && !surface_loading()
                    && crate::browser_overlay::is_active_webview(&wv)
                {
                    use gtk::prelude::WidgetExt;
                    wv.show();
                }
            },
        );
    }

    fn is_x11() -> bool {
        std::env::var("XDG_SESSION_TYPE").map(|v| v == "x11").unwrap_or(false)
            || std::env::var("GDK_BACKEND").map(|v| v == "x11").unwrap_or(false)
            || std::env::var("DISPLAY").is_ok()
    }

    fn parent_xid(window: &tauri::Window) -> Result<u64, String> {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        match window.window_handle() {
            Ok(handle) => match handle.as_raw() {
                RawWindowHandle::Xlib(h) => Ok(h.window as u64),
                RawWindowHandle::Xcb(h) => Ok(h.window.get() as u64),
                _ => Err("Unsupported window handle type".to_string()),
            },
            Err(e) => Err(format!("Failed to get window handle: {}", e)),
        }
    }

    /// Scale logical (CSS-px) bounds to physical pixels for X11.
    fn scaled(window: &tauri::Window, b: &WebviewBounds) -> Result<(i32, i32, u32, u32), String> {
        let sf = window.scale_factor().map_err(|e| e.to_string())?;
        let x = (b.x * sf) as i32;
        let y = (b.y * sf) as i32;
        let w = ((b.width * sf).max(1.0)) as u32;
        let h = ((b.height * sf).max(1.0)) as u32;
        Ok((x, y, w, h))
    }

    /// Run `f` against the live WebView on the GTK main thread (no-op if no surface).
    fn on_webview<F>(f: F)
    where
        F: FnOnce(&webkit2gtk::WebView) + Send + 'static,
    {
        glib::MainContext::default().invoke(move || {
            SURFACE.with(|s| {
                if let Some(obj) = s.borrow().as_ref() {
                    f(&obj.webview);
                }
            });
        });
    }

    /// Run JS in the legacy X11 surface page (autofill fallback path).
    pub(crate) fn eval_js_surface(script: String) {
        on_webview(move |wv| {
            use webkit2gtk::WebViewExt;
            wv.run_javascript(&script, None::<&gtk::gio::Cancellable>, |_| {});
        });
    }

    /// XMoveResizeWindow on the surface's XID.
    fn x11_move_resize(xid: u64, x: i32, y: i32, w: u32, h: u32) {
        use std::ptr;
        use x11::xlib;
        unsafe {
            let display = xlib::XOpenDisplay(ptr::null());
            if display.is_null() {
                return;
            }
            xlib::XMoveResizeWindow(display, xid as xlib::Window, x, y, w, h);
            xlib::XRaiseWindow(display, xid as xlib::Window);
            xlib::XSync(display, xlib::False);
            xlib::XCloseDisplay(display);
        }
    }

    fn x11_set_mapped(xid: u64, mapped: bool) {
        use std::ptr;
        use x11::xlib;
        unsafe {
            let display = xlib::XOpenDisplay(ptr::null());
            if display.is_null() {
                return;
            }
            if mapped {
                xlib::XMapWindow(display, xid as xlib::Window);
                xlib::XRaiseWindow(display, xid as xlib::Window);
            } else {
                xlib::XUnmapWindow(display, xid as xlib::Window);
            }
            xlib::XSync(display, xlib::False);
            xlib::XCloseDisplay(display);
        }
    }


    /// Replicate "move the mouse off the app and back" — the only thing that
    /// un-freezes scroll after a click — by warping the pointer just outside the
    /// nearest surface edge and immediately back. Each warp makes the X server
    /// emit the leave/enter crossings that reset WebKit's stuck pointer state; the
    /// round trip is synchronous so the cursor's net position is unchanged. We hop
    /// only ~2px past the closest edge (instead of to the screen corner) so the
    /// brief default-cursor flash is minimal. This is the faithful reproduction of
    /// the manual fix, after repaint/resize/focus/grab/mask attempts all failed.
    fn pulse_crossing() {
        use std::ptr;
        use x11::xlib;
        let xid = match SURFACE_XID.lock().ok().and_then(|g| *g) {
            Some(x) => x,
            None => return,
        };
        unsafe {
            let dpy = xlib::XOpenDisplay(ptr::null());
            if dpy.is_null() {
                return;
            }
            let root = xlib::XDefaultRootWindow(dpy);
            let mut rr: xlib::Window = 0;
            let mut cr: xlib::Window = 0;
            let (mut rx, mut ry, mut wx, mut wy) = (0i32, 0i32, 0i32, 0i32);
            let mut mask: u32 = 0;
            // Surface rectangle in root coordinates.
            let mut sx = 0i32;
            let mut sy = 0i32;
            let mut child: xlib::Window = 0;
            let mut attrs: xlib::XWindowAttributes = std::mem::zeroed();
            let have_pointer = xlib::XQueryPointer(
                dpy, root, &mut rr, &mut cr, &mut rx, &mut ry, &mut wx, &mut wy, &mut mask,
            ) != 0;
            xlib::XTranslateCoordinates(dpy, xid, root, 0, 0, &mut sx, &mut sy, &mut child);
            let have_attrs = xlib::XGetWindowAttributes(dpy, xid, &mut attrs) != 0;
            if have_pointer && have_attrs {
                let (w, h) = (attrs.width, attrs.height);
                // Pick the closest edge and hop 2px past it, then warp back.
                let d_top = ry - sy;
                let d_bottom = (sy + h) - ry;
                let d_left = rx - sx;
                let d_right = (sx + w) - rx;
                let m = d_top.min(d_bottom).min(d_left).min(d_right);
                let (ox, oy) = if m == d_top {
                    (rx, sy - 2)
                } else if m == d_bottom {
                    (rx, sy + h + 2)
                } else if m == d_left {
                    (sx - 2, ry)
                } else {
                    (sx + w + 2, ry)
                };
                xlib::XWarpPointer(dpy, 0, root, 0, 0, 0, 0, ox, oy);
                xlib::XWarpPointer(dpy, 0, root, 0, 0, 0, 0, rx, ry);
                xlib::XFlush(dpy);
            }
            xlib::XCloseDisplay(dpy);
        }
    }

    /// Create the embedded surface: a GTK window holding a WebKitGTK WebView,
    /// reparented under the Tauri window at the given bounds, loading `url`.
    /// Returns the GTK window XID. Runs the GTK work on the GTK main thread.
    fn create_surface(
        window: &tauri::Window,
        app: tauri::AppHandle,
        url: String,
        b: &WebviewBounds,
    ) -> Result<u64, String> {
        use std::sync::mpsc;

        if !is_x11() {
            return Err("Embedded browser requires X11".to_string());
        }

        let parent = parent_xid(window)?;
        let (x, y, w, h) = scaled(window, b)?;

        log::info!(
            "[browser_surface] creating at ({},{}) {}x{} under parent 0x{:x}, url: {}",
            x, y, w, h, parent, url
        );

        let (tx, rx) = mpsc::channel();
        let url_for_gtk = url.clone();
        // NoScript: resolve JS policy for the target host before the page loads.
        let js_allow = js_allowed(&host_of(&url));

        glib::MainContext::default().invoke(move || {
            use gtk::prelude::*;
            use webkit2gtk::{CookieAcceptPolicy, CookieManagerExt, HardwareAccelerationPolicy, LoadEvent, SettingsExt, UserMessageExt, WebContext, WebContextExt, WebsiteDataManagerExt, WebView, WebViewExt};
            use std::ptr;
            use x11::xlib;

            let result: Result<u64, String> = (|| {
                let _ = gtk::init();

                // Toplevel (not Popup/override-redirect): override-redirect popups
                // get flaky pointer/scroll/focus delivery and can be preempted in the
                // X stack by the main webview, causing intermittent "scroll/click
                // sometimes works" behavior. A Toplevel reparented into the Tauri
                // window is the proven `video_surface` pattern and gets standard input
                // handling; once reparented the WM no longer manages it, so the
                // decoration/centering issues that drove the Popup choice don't apply
                // here. Undecorated + skip hints keep any pre-reparent frame invisible.
                let gtk_window = gtk::Window::new(gtk::WindowType::Toplevel);
                gtk_window.set_title("reclaim-browser");
                gtk_window.set_default_size(w as i32, h as i32);
                gtk_window.set_decorated(false);
                gtk_window.set_skip_taskbar_hint(true);
                gtk_window.set_skip_pager_hint(true);
                gtk_window.set_type_hint(gdk::WindowTypeHint::Normal);

                // The page content webview. Use a dedicated ephemeral context so
                // the NoScript web extension is loaded BEFORE this context's web
                // process spawns (the default context's process may already exist).
                let web_context = WebContext::new_ephemeral();
                if let Some(dir) = NOSCRIPT_EXT_DIR.lock().ok().and_then(|g| g.clone()) {
                    web_context.set_web_extensions_directory(&dir);
                }
                // Privacy: block third-party cookies (so they can't track you across
                // sites) and enable Intelligent Tracking Prevention (WebKit's built-in
                // cross-site tracker/cookie blocker — the same mechanism Safari uses).
                // Both driven by the user's PrivacyConfig.
                {
                    let cfg = PRIVACY_CONFIG.lock().map(|c| c.clone()).unwrap_or_default();
                    if let Some(cm) = web_context.cookie_manager() {
                        cm.set_accept_policy(if cfg.block_third_party_cookies {
                            CookieAcceptPolicy::NoThirdParty
                        } else {
                            CookieAcceptPolicy::Always
                        });
                    }
                    if let Some(wdm) = web_context.website_data_manager() {
                        wdm.set_itp_enabled(cfg.tracking_prevention);
                    }
                }
                wire_downloads(&app, &web_context);
                let webview = WebView::with_context(&web_context);
                if let Some(settings) = WebViewExt::settings(&webview) {
                    // Developer extras ON: lets you right-click the page -> Inspect
                    // to open the PAGE's own inspector (F12 inspects Reclaim's UI,
                    // not the embedded page). Disabling it did not fix the media
                    // freeze, so the earlier NeedDebuggerBreak theory was wrong.
                    SettingsExt::set_enable_developer_extras(&settings, true);
                    SettingsExt::set_javascript_can_access_clipboard(&settings, true);
                    // GPU-ACCEL SPIKE (step 1): the GL+reparent deadlock was seen on
                    // an older WebKitGTK; we're on 2.52 now. Toggle the policy via
                    // EARTH_GL to test whether accelerated rendering (and thus media
                    // hardware decode/replay) works on the CURRENT architecture before
                    // we rewrite the embedding:
                    //   EARTH_GL unset / "0"  -> Never    (software; current safe default)
                    //   EARTH_GL=1 / "ondemand" -> OnDemand (GL when a page needs it)
                    //   EARTH_GL=always        -> Always   (force GL)
                    let accel = match std::env::var("EARTH_GL").ok().as_deref() {
                        Some("1") | Some("ondemand") => HardwareAccelerationPolicy::OnDemand,
                        Some("always") => HardwareAccelerationPolicy::Always,
                        _ => HardwareAccelerationPolicy::Never,
                    };
                    let accel_label = match accel {
                        HardwareAccelerationPolicy::OnDemand => "on-demand",
                        HardwareAccelerationPolicy::Always => "always",
                        _ => "never (software)",
                    };
                    eprintln!("[browser_surface] hardware acceleration: {accel_label}");
                    SettingsExt::set_hardware_acceleration_policy(&settings, accel);
                    // Smooth (kinetic) scrolling — discrete wheel steps alone deliver
                    // erratically through the reparented surface; smooth scrolling
                    // makes the wheel consistent.
                    SettingsExt::set_enable_smooth_scrolling(&settings, true);
                    // NoScript: JS is allowed only for hosts on the allowlist.
                    SettingsExt::set_enable_javascript(&settings, js_allow);

                    // Privacy hardening, driven by the user's PrivacyConfig:
                    //  * WebRTC — its peer connections can leak your real local/public
                    //    IP even behind a VPN (a common de-anonymizer).
                    //  * Hyperlink auditing — `<a ping>` tracking beacons.
                    //  * DNS prefetching — pre-resolving link domains leaks intent.
                    //  * User-agent — spoof a common UA to shrink fingerprint.
                    if let Ok(cfg) = PRIVACY_CONFIG.lock() {
                        apply_privacy_settings(&settings, &cfg);
                    }
                }
                webview.set_hexpand(true);
                webview.set_vexpand(true);
                // CRITICAL for wheel scrolling. Diagnostics proved GTK stops
                // delivering scroll events to this webview the moment it becomes the
                // focused widget (which WebKit does on click). Clearing focus after
                // the click only half-works because WebKit re-focuses asynchronously
                // from its web process, re-breaking scroll (a race). Making the widget
                // non-focusable removes the broken state entirely: the click can no
                // longer focus it, so scroll keeps flowing. Trade-off: the page won't
                // hold GTK keyboard focus (page text fields), which we accept until
                // wheel routing is solid.
                webview.set_can_focus(false);
                // Make sure scroll + motion events reach the webview. Without these
                // the reparented (override-redirect) surface can drop wheel/motion
                // events, so scrolling freezes and CSS cursor changes lag.
                //
                // BUTTON_RELEASE_MASK is essential alongside BUTTON_PRESS_MASK: a
                // button press starts a GTK implicit pointer grab that is only
                // released when GTK receives the matching release event. Without
                // the release selected, that grab stays STUCK after a click and
                // captures/misroutes subsequent wheel events — the page "freezes"
                // until you click off the app (which drops the grab on focus-out).
                // ENTER/LEAVE are as important as the rest: when the automatic
                // pointer grab from a button-press ends at release, X emits crossing
                // events (mode=NotifyUngrab) to restore which window the pointer is
                // "in". Without these masks selected, GTK never sees them, its
                // pointer tracking stays stale, and scroll stops routing to the
                // surface after a click — only physically moving the mouse off/back
                // (real crossings) fixes it. Selecting them lets the ungrab crossings
                // through so scroll keeps working after every click.
                webview.add_events(
                    gdk::EventMask::SCROLL_MASK
                        | gdk::EventMask::SMOOTH_SCROLL_MASK
                        | gdk::EventMask::POINTER_MOTION_MASK
                        | gdk::EventMask::BUTTON_PRESS_MASK
                        | gdk::EventMask::BUTTON_RELEASE_MASK
                        | gdk::EventMask::ENTER_NOTIFY_MASK
                        | gdk::EventMask::LEAVE_NOTIFY_MASK,
                );

                // RENDER-STALL FIX (software rendering on a reparented surface).
                //
                // Confirmed by diagnostics: after a click WebKit still RECEIVES
                // scroll events (the page scrolls internally) but stops pushing new
                // frames to this window, so the screen freezes until a focus-out
                // forces a repaint. `set_can_focus(false)` (above) keeps input
                // flowing; here we replace the missing repaint by pumping redraws
                // ourselves — on every wheel tick (so scrolling shows) and a short
                // burst after every click (so the page/cursor refreshes like it does
                // when you click off the app).
                webview.connect_scroll_event(|wv, _event| {
                    use gtk::prelude::WidgetExt;
                    wv.queue_draw();
                    let wv2 = wv.clone();
                    glib::idle_add_local_once(move || {
                        // After WebKit applies the scroll, draw the new frame.
                        wv2.queue_draw();
                    });
                    glib::Propagation::Proceed
                });
                webview.connect_button_release_event(|_wv, _event| {
                    // Reset WebKit's pointer state after the click (idle = after
                    // WebKit handles the release) by replaying a real crossing —
                    // the only thing proven to un-freeze scroll.
                    glib::idle_add_local_once(pulse_crossing);
                    glib::Propagation::Proceed
                });

                // NoScript: the web extension reports each distinct request origin
                // it sees on a page via a `noscript:seen` user message. Relay the
                // origin to the frontend (Phase 1: the UI just lists them).
                {
                    let a = app.clone();
                    webview.connect_user_message_received(move |_v, msg| {
                        if msg.name().as_deref() == Some("noscript:seen") {
                            // Tolerate both the (origin, first_party) tuple and a
                            // bare origin string (older extension build).
                            let parsed = msg.parameters().and_then(|p| {
                                p.get::<(String, bool)>()
                                    .or_else(|| p.get::<String>().map(|o| (o, false)))
                            });
                            if let Some((origin, first_party)) = parsed {
                                // Legacy X11 single-surface path: attribute to the
                                // current tab.
                                let cur = CURRENT_TAB.lock().ok().and_then(|g| *g).unwrap_or(-1);
                                record_seen_origin(cur, origin.clone(), first_party);
                                use tauri::Emitter;
                                let _ = a.emit("noscript-origin", NoscriptOrigin { origin, first_party });
                            }
                        }
                        false
                    });
                }

                // Push page title / URL / nav-state to the frontend as it loads.
                {
                    let a = app.clone();
                    webview.connect_load_changed(move |wv, ev| {
                        if matches!(ev, LoadEvent::Committed | LoadEvent::Finished) {
                            let title = WebViewExt::title(wv).map(|s| s.to_string()).unwrap_or_default();
                            let uri = WebViewExt::uri(wv).map(|s| s.to_string()).unwrap_or_default();
                            emit_title(&a, title, uri);
                        }
                        // A freshly loaded page can come up render-stale on this
                        // software-rendered reparented surface (hover/custom cursors
                        // dead until you refresh). Kick it once it finishes loading.
                        if matches!(ev, LoadEvent::Finished) {
                            pulse_crossing();
                        }
                    });
                }
                {
                    // Catches title changes after load (e.g. SPA route changes).
                    let a = app.clone();
                    webview.connect_title_notify(move |wv| {
                        let title = WebViewExt::title(wv).map(|s| s.to_string()).unwrap_or_default();
                        let uri = WebViewExt::uri(wv).map(|s| s.to_string()).unwrap_or_default();
                        if !title.is_empty() {
                            emit_title(&a, title, uri);
                        }
                    });
                }

                WebViewExt::load_uri(&webview, &url_for_gtk);

                gtk_window.add(&webview);
                // Park off-screen before mapping so the WM can't briefly show the
                // Toplevel as a floating window before we reparent it.
                gtk_window.move_(-32000, -32000);
                gtk_window.show_all();

                // GTK window XID for reparenting/positioning.
                let gdk_win = gtk_window.window().ok_or("GTK window has no GDK window")?;
                // Deliver every motion event instead of coalescing — keeps the
                // cursor and CSS-cursor updates fluid over the embedded surface.
                gdk_win.set_event_compression(false);
                let win_xid: u64 = unsafe {
                    use glib::translate::ToGlibPtr;
                    extern "C" {
                        fn gdk_x11_window_get_xid(window: *mut std::ffi::c_void) -> u64;
                    }
                    let p: *mut std::ffi::c_void =
                        ToGlibPtr::<*mut gdk::ffi::GdkWindow>::to_glib_none(&gdk_win).0 as *mut _;
                    gdk_x11_window_get_xid(p)
                };
                if win_xid == 0 {
                    return Err("Failed to get XID for browser surface".to_string());
                }

                // Reparent under the Tauri window at (x, y) — same mechanism as video_surface.
                unsafe {
                    let display = xlib::XOpenDisplay(ptr::null());
                    if display.is_null() {
                        return Err("Failed to open X11 display".to_string());
                    }
                    xlib::XReparentWindow(display, win_xid as xlib::Window, parent as xlib::Window, x, y);
                    xlib::XMoveResizeWindow(display, win_xid as xlib::Window, x, y, w, h);
                    xlib::XMapWindow(display, win_xid as xlib::Window);
                    xlib::XRaiseWindow(display, win_xid as xlib::Window);
                    xlib::XSync(display, xlib::False);
                    xlib::XCloseDisplay(display);
                }

                // Keep the GTK objects alive on this (GTK) thread for later navigation.
                SURFACE.with(|s| {
                    *s.borrow_mut() = Some(SurfaceObjects { window: gtk_window, webview, web_context });
                });
                // Push the current trust set now that the extension's context exists.
                push_trust_to_extensions();

                Ok(win_xid)
            })();

            let _ = tx.send(result);
        });

        rx.recv()
            .map_err(|e| format!("GTK thread channel error: {}", e))?
    }

    pub async fn navigate(
        window: tauri::Window,
        tab_id: i64,
        url: String,
        bounds: Option<WebviewBounds>,
    ) -> Result<(), String> {
        set_current_tab(tab_id);
        let existing = *SURFACE_XID.lock().map_err(|e| e.to_string())?;

        if let Some(xid) = existing {
            // Reposition if the caller owns layout, then navigate in place.
            if let Some(b) = &bounds {
                let (x, y, w, h) = scaled(&window, b)?;
                x11_move_resize(xid, x, y, w, h);
            }
            // Ensure the surface is visible: it may have been unmapped when the
            // previous tab was closed/hidden. Navigating means "show me this page".
            x11_set_mapped(xid, true);

            // Restore-without-reload: a remount/restore (bounds present) for the
            // SAME url should NOT reload — that would discard the live page and
            // scroll position. Just remapping (above) brings the page back as-is.
            // A bar navigation (bounds = None) or a different url still loads.
            let same_url = LAST_NAV_URL
                .lock()
                .ok()
                .and_then(|g| g.clone())
                .as_deref()
                == Some(url.as_str());
            if bounds.is_some() && same_url {
                return Ok(());
            }

            if let Ok(mut g) = LAST_NAV_URL.lock() {
                *g = Some(url.clone());
            }
            {
                let cur = CURRENT_TAB.lock().ok().and_then(|g| *g).unwrap_or(-1);
                clear_seen_origins(cur);
            }
            let u = url.clone();
            // NoScript: apply the target host's JS policy before loading it.
            let allow = js_allowed(&host_of(&u));
            on_webview(move |wv| {
                use webkit2gtk::{SettingsExt, WebViewExt};
                if let Some(settings) = WebViewExt::settings(wv) {
                    SettingsExt::set_enable_javascript(&settings, allow);
                }
                WebViewExt::load_uri(wv, &u);
            });
            return Ok(());
        }

        // Needs creation — requires bounds (position + size).
        let b = bounds.ok_or("Browser surface not created yet and no bounds provided")?;
        if CREATING.swap(true, Ordering::SeqCst) {
            // Another create is in flight; let it win.
            return Ok(());
        }
        let app = window.app_handle().clone();
        if let Ok(mut g) = LAST_NAV_URL.lock() {
            *g = Some(url.clone());
        }
        let res = create_surface(&window, app, url, &b);
        match res {
            Ok(xid) => {
                *SURFACE_XID.lock().map_err(|e| e.to_string())? = Some(xid);
                CREATING.store(false, Ordering::SeqCst);
                Ok(())
            }
            Err(e) => {
                CREATING.store(false, Ordering::SeqCst);
                Err(e)
            }
        }
    }

    pub async fn set_bounds(window: tauri::Window, bounds: WebviewBounds) -> Result<(), String> {
        if let Some(xid) = *SURFACE_XID.lock().map_err(|e| e.to_string())? {
            let (x, y, w, h) = scaled(&window, &bounds)?;
            x11_move_resize(xid, x, y, w, h);
        }
        Ok(())
    }

    pub async fn show() -> Result<(), String> {
        if let Some(xid) = *SURFACE_XID.lock().map_err(|e| e.to_string())? {
            x11_set_mapped(xid, true);
        }
        Ok(())
    }

    pub async fn hide() -> Result<(), String> {
        if let Some(xid) = *SURFACE_XID.lock().map_err(|e| e.to_string())? {
            x11_set_mapped(xid, false);
        }
        Ok(())
    }

    /// Re-map the surface if it exists (it may have been unmapped when another
    /// tab was closed). Keeps back/forward/reload from acting on a hidden window.
    fn ensure_mapped() -> Result<(), String> {
        if let Some(xid) = *SURFACE_XID.lock().map_err(|e| e.to_string())? {
            x11_set_mapped(xid, true);
        }
        Ok(())
    }

    pub async fn back() -> Result<(), String> {
        ensure_mapped()?;
        on_webview(|wv| webkit2gtk::WebViewExt::go_back(wv));
        Ok(())
    }

    pub async fn forward() -> Result<(), String> {
        ensure_mapped()?;
        on_webview(|wv| webkit2gtk::WebViewExt::go_forward(wv));
        Ok(())
    }

    pub async fn reload() -> Result<(), String> {
        ensure_mapped()?;
        on_webview(|wv| webkit2gtk::WebViewExt::reload(wv));
        Ok(())
    }

    pub async fn destroy() -> Result<(), String> {
        *SURFACE_XID.lock().map_err(|e| e.to_string())? = None;
        if let Ok(mut g) = LATEST_TITLE.lock() { *g = None; }
        if let Ok(mut g) = LATEST_URL.lock() { *g = None; }
        glib::MainContext::default().invoke(|| {
            use gtk::prelude::*;
            SURFACE.with(|s| {
                if let Some(obj) = s.borrow_mut().take() {
                    obj.window.close();
                }
            });
        });
        Ok(())
    }

    /// Whether an embedded surface currently exists (real state, not hardcoded).
    pub async fn is_embedded() -> Result<bool, String> {
        Ok(SURFACE_XID.lock().ok().and_then(|g| *g).is_some())
    }

    /// Latest observed page title (updated by the load/title signals).
    pub async fn get_title() -> Result<String, String> {
        Ok(LATEST_TITLE.lock().ok().and_then(|g| g.clone()).unwrap_or_default())
    }

    /// Live page HTML, via `document.documentElement.outerHTML` in the surface.
    pub async fn get_html() -> Result<String, String> {
        use std::sync::mpsc;
        let (tx, rx) = mpsc::channel();
        glib::MainContext::default().invoke(move || {
            use webkit2gtk::WebViewExt;
            let dispatched = SURFACE.with(|s| {
                if let Some(obj) = s.borrow().as_ref() {
                    let tx2 = tx.clone();
                    obj.webview.run_javascript(
                        "document.documentElement.outerHTML",
                        None::<&gtk::gio::Cancellable>,
                        move |res| {
                            let html = res
                                .ok()
                                .and_then(|jr| jr.js_value())
                                .map(|v| v.to_string())
                                .unwrap_or_default();
                            let _ = tx2.send(html);
                        },
                    );
                    true
                } else {
                    false
                }
            });
            if !dispatched {
                let _ = tx.send(String::new());
            }
        });
        rx.recv().map_err(|e| e.to_string())
    }
}

// ============================================================================
// Non-Linux stubs (keep the command surface compiling cross-platform)
// ============================================================================
#[cfg(not(target_os = "linux"))]
mod imp {
    use super::WebviewBounds;

    pub async fn navigate(_w: tauri::Window, _tab: i64, _u: String, _b: Option<WebviewBounds>) -> Result<(), String> {
        Err("Embedded browser only supported on Linux/X11".to_string())
    }
    pub async fn set_bounds(_w: tauri::Window, _b: WebviewBounds) -> Result<(), String> { Ok(()) }
    pub async fn show() -> Result<(), String> { Ok(()) }
    pub async fn hide() -> Result<(), String> { Ok(()) }
    pub async fn back() -> Result<(), String> { Ok(()) }
    pub async fn forward() -> Result<(), String> { Ok(()) }
    pub async fn reload() -> Result<(), String> { Ok(()) }
    pub async fn destroy() -> Result<(), String> { Ok(()) }
    pub async fn is_embedded() -> Result<bool, String> { Ok(false) }
    pub async fn get_title() -> Result<String, String> { Ok(String::new()) }
    pub async fn get_html() -> Result<String, String> { Ok(String::new()) }
    pub fn init_js_policy(_db_path: String) {}
    pub fn set_noscript_ext_dir(_dir: String) {}
    pub async fn set_js_allowed(_host: String, _allowed: bool) -> Result<(), String> { Ok(()) }
    pub async fn get_js_allowed(_host: String) -> Result<bool, String> { Ok(false) }
    pub async fn list_js_allowed() -> Result<Vec<String>, String> { Ok(Vec::new()) }
    pub async fn get_trust(_origin: String) -> Result<String, String> { Ok("untrusted".into()) }
    pub async fn set_trust(_origin: String, _state: String) -> Result<(), String> { Ok(()) }
    pub async fn list_origins() -> Result<Vec<(String, bool, String)>, String> { Ok(Vec::new()) }
    pub async fn get_privacy_config() -> Result<super::PrivacyConfig, String> { Ok(super::PrivacyConfig::default()) }
    pub async fn set_privacy_config(_cfg: super::PrivacyConfig) -> Result<(), String> { Ok(()) }
}

/// Engine entry point: create-or-reuse the surface and navigate it.
/// `bounds` Some => create/reposition (caller owns layout); None => navigate in place.
/// `tab_id` tags title/nav events emitted while this navigation is current.
pub async fn navigate_surface(
    window: tauri::Window,
    tab_id: i64,
    url: String,
    bounds: Option<WebviewBounds>,
) -> Result<(), String> {
    imp::navigate(window, tab_id, url, bounds).await
}

/// Real embedded-state / title / HTML accessors (used by the `webview_*` commands).
pub async fn is_embedded() -> Result<bool, String> {
    imp::is_embedded().await
}
pub async fn get_title() -> Result<String, String> {
    imp::get_title().await
}
pub async fn get_html() -> Result<String, String> {
    imp::get_html().await
}

/// Open a finished download with the system default handler.
#[tauri::command(rename_all = "camelCase")]
pub async fn open_download(path: String) -> Result<(), String> {
    let p = path.strip_prefix("file://").unwrap_or(&path);
    open::that(p).map_err(|e| e.to_string())
}

// ---- Frontend control commands ----

#[tauri::command(rename_all = "camelCase")]
pub async fn browser_surface_set_bounds(window: tauri::Window, bounds: WebviewBounds) -> Result<(), String> {
    if crate::browser_overlay::enabled() {
        crate::browser_overlay::set_bounds(bounds);
        return Ok(());
    }
    imp::set_bounds(window, bounds).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn browser_surface_show() -> Result<(), String> {
    if crate::browser_overlay::enabled() {
        crate::browser_overlay::show();
        return Ok(());
    }
    imp::show().await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn browser_surface_hide() -> Result<(), String> {
    if crate::browser_overlay::enabled() {
        crate::browser_overlay::hide();
        return Ok(());
    }
    imp::hide().await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn browser_surface_back() -> Result<(), String> {
    if crate::browser_overlay::enabled() {
        crate::browser_overlay::back();
        return Ok(());
    }
    imp::back().await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn browser_surface_forward() -> Result<(), String> {
    if crate::browser_overlay::enabled() {
        crate::browser_overlay::forward();
        return Ok(());
    }
    imp::forward().await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn browser_surface_reload() -> Result<(), String> {
    if crate::browser_overlay::enabled() {
        crate::browser_overlay::reload();
        return Ok(());
    }
    imp::reload().await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn browser_surface_destroy() -> Result<(), String> {
    imp::destroy().await
}

/// Fill the active page's login form via the injected `__reclaimFill`. Values are
/// JSON-encoded for safe embedding into the eval'd script. Routed to whichever
/// embed (overlay default / X11 fallback) is active.
pub fn fill_login(username: &str, password: &str) {
    let u = serde_json::to_string(username).unwrap_or_else(|_| "\"\"".into());
    let p = serde_json::to_string(password).unwrap_or_else(|_| "\"\"".into());
    let script = format!("window.__reclaimFill && window.__reclaimFill({u},{p});");
    if crate::browser_overlay::enabled() {
        crate::browser_overlay::eval_js(script);
    } else {
        imp::eval_js_surface(script);
    }
}

/// Ask the active page to enumerate its downloadable media. Results come back
/// asynchronously as a `media-list` event (see the injected script).
#[tauri::command(rename_all = "camelCase")]
pub async fn browser_collect_media() -> Result<(), String> {
    let script = "window.__reclaimCollectMedia && window.__reclaimCollectMedia();".to_string();
    if crate::browser_overlay::enabled() {
        crate::browser_overlay::eval_js(script);
    } else {
        imp::eval_js_surface(script);
    }
    Ok(())
}

/// Initialize the NoScript allowlist store (load persisted hosts). Called once
/// at startup with the app DB path.
pub fn init_js_policy(db_path: String) {
    imp::init_js_policy(db_path)
}

/// Tell the browser surface where the NoScript web-process extension (.so) lives.
pub fn set_noscript_ext_dir(dir: String) {
    imp::set_noscript_ext_dir(dir)
}

/// NoScript (per-site allowlist): allow/deny JavaScript for `host`. Persisted and
/// applied live (with a reload) when `host` is the page currently showing.
#[tauri::command(rename_all = "camelCase")]
pub async fn browser_surface_set_js(host: String, allowed: bool) -> Result<(), String> {
    imp::set_js_allowed(host, allowed).await
}

/// Whether `host` is currently allowed to run JavaScript (for the shield state).
#[tauri::command(rename_all = "camelCase")]
pub async fn browser_surface_get_js(host: String) -> Result<bool, String> {
    imp::get_js_allowed(host).await
}

/// All allowlisted hosts (management view).
#[tauri::command(rename_all = "camelCase")]
pub async fn browser_surface_list_js() -> Result<Vec<String>, String> {
    imp::list_js_allowed().await
}

/// Origins seen on the current page, each as (origin, firstParty, state). The
/// panel queries this on open so it shows scripts even if it mounted late.
#[tauri::command(rename_all = "camelCase")]
pub async fn noscript_list_origins() -> Result<Vec<(String, bool, String)>, String> {
    imp::list_origins().await
}

/// Read the current privacy protections config.
#[tauri::command(rename_all = "camelCase")]
pub async fn privacy_get_config() -> Result<PrivacyConfig, String> {
    imp::get_privacy_config().await
}

/// Update the privacy protections config (persisted; applied live + reload).
///
/// Incognito forces maximum protection: while any profile is in incognito mode
/// every protection is forced ON, regardless of what the caller requested. The
/// UI also locks the toggles, but we enforce it here so no path can weaken
/// privacy while incognito is active.
#[tauri::command(rename_all = "camelCase")]
pub async fn privacy_set_config(config: PrivacyConfig) -> Result<(), String> {
    let config = if crate::privacy::PrivacyManager::get_incognito_profiles().is_empty() {
        config
    } else {
        PrivacyConfig::default() // all protections on
    };
    imp::set_privacy_config(config).await
}

/// NoScript per-origin trust state for the modal: "trusted" | "temp" | "untrusted".
#[tauri::command(rename_all = "camelCase")]
pub async fn noscript_get_trust(origin: String) -> Result<String, String> {
    imp::get_trust(origin).await
}

/// Set a per-origin trust state (persists for "trusted", session for "temp"),
/// pushes the new trust set to the web extension, and reloads the page.
#[tauri::command(rename_all = "camelCase")]
pub async fn noscript_set_trust(origin: String, state: String) -> Result<(), String> {
    imp::set_trust(origin, state).await
}
