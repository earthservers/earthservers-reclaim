//! GTK-overlay page embed — the DEFAULT page embedding (`EARTH_EMBED=x11` opts
//! back into the legacy X11 surface).
//!
//! The page `WebView`s are added as children of the Tauri toplevel via a single
//! `GtkOverlay`. There is ONE webview PER TAB (a multi-page cache): switching
//! tabs shows the target tab's already-loaded webview and hides the rest, so
//! visited pages are NOT reloaded. All tab webviews share one `WebContext`, so
//! cookies/sessions are shared across tabs like a normal browser. A small LRU cap
//! bounds memory — the least-recently-used tab's webview is dropped past the cap
//! (revisiting it reloads).
//!
//! Page-webview *configuration* is shared with the X11 path via
//! `browser_surface::imp::configure_page_webview`, so security/behavior is
//! identical regardless of how the webview is mounted.

use crate::webview::WebviewBounds;

/// Whether the GTK-overlay embed is used. It is now the DEFAULT — the legacy
/// X11-reparented surface (`browser_surface`) is opt-in via `EARTH_EMBED=x11`
/// and kept only as a temporary fallback. Set `EARTH_EMBED=x11` to use it.
pub fn enabled() -> bool {
    std::env::var("EARTH_EMBED").as_deref() != Ok("x11")
}

#[cfg(target_os = "linux")]
mod imp {
    use super::WebviewBounds;
    use gtk::prelude::*;
    use gtk::OverlaySignals;
    use std::cell::RefCell;
    use std::collections::HashMap;

    /// One cached tab: its live webview and the URL we last asked it to load.
    struct TabView {
        webview: webkit2gtk::WebView,
        last_url: Option<String>,
    }

    struct OverlayState {
        overlay: gtk::Overlay,
        /// Shared cookie/session context for ALL tab webviews (created with the
        /// first tab). `None` until the first tab is built.
        web_context: Option<webkit2gtk::WebContext>,
        /// Current target bounds (logical px) — `get_child_position` reads these.
        bounds: WebviewBounds,
        /// Live webview per tab id: the multi-page cache.
        tabs: HashMap<i64, TabView>,
        /// LRU order (oldest first, most-recent last) for eviction.
        order: Vec<i64>,
        /// The currently shown tab.
        active: Option<i64>,
    }

    thread_local! {
        static STATE: RefCell<Option<OverlayState>> = RefCell::new(None);
    }

    const DEFAULT_BOUNDS: WebviewBounds = WebviewBounds {
        x: 0.0,
        y: 130.0,
        width: 1400.0,
        height: 900.0,
    };

    /// Max live (cached) tab webviews. Beyond this the least-recently-used tab's
    /// webview is destroyed; revisiting that tab reloads it.
    const MAX_CACHED_TABS: usize = 6;

    fn reposition() {
        STATE.with(|s| {
            if let Some(st) = s.borrow().as_ref() {
                st.overlay.queue_resize();
            }
        });
    }

    /// The single authority for which cached page is on screen: show the active
    /// tab's webview iff the surface is wanted and the active page isn't mid-load;
    /// hide every other tab. Must run on the GTK main thread.
    fn update_visibility() {
        let wanted = crate::browser_surface::imp::surface_wanted();
        let loading = crate::browser_surface::imp::surface_loading();
        STATE.with(|s| {
            if let Some(st) = s.borrow().as_ref() {
                for (id, tv) in st.tabs.iter() {
                    if wanted && !loading && st.active == Some(*id) {
                        tv.webview.show();
                    } else {
                        tv.webview.hide();
                    }
                }
            }
        });
    }

    /// Run `f` against the ACTIVE tab's webview on the GTK main thread (no-op if
    /// there's no active tab yet).
    fn with_active_webview(f: impl FnOnce(&webkit2gtk::WebView) + Send + 'static) {
        glib::MainContext::default().invoke(move || {
            STATE.with(|s| {
                if let Some(st) = s.borrow().as_ref() {
                    if let Some(active) = st.active {
                        if let Some(tv) = st.tabs.get(&active) {
                            f(&tv.webview);
                        }
                    }
                }
            });
        });
    }

    /// The active tab's live, committed URL — read from the real webview on the
    /// GTK thread. This is the page a fill would actually inject into, so binding
    /// credential lookup to it (instead of any page- or frontend-supplied origin)
    /// makes cross-origin autofill structurally impossible. None if no active tab.
    pub fn active_page_url() -> Option<String> {
        use std::sync::mpsc;
        let (tx, rx) = mpsc::channel();
        glib::MainContext::default().invoke(move || {
            let url = STATE.with(|s| {
                s.borrow().as_ref().and_then(|st| {
                    let active = st.active?;
                    let tv = st.tabs.get(&active)?;
                    webkit2gtk::WebViewExt::uri(&tv.webview).map(|u| u.to_string())
                })
            });
            let _ = tx.send(url);
        });
        rx.recv().ok().flatten()
    }

    /// True if `wv` is the active tab's webview. The load-reveal logic checks this
    /// so a BACKGROUND tab finishing its load doesn't pop over the active page.
    pub fn is_active_webview(wv: &webkit2gtk::WebView) -> bool {
        STATE.with(|s| {
            s.borrow()
                .as_ref()
                .and_then(|st| st.active.and_then(|a| st.tabs.get(&a)).map(|tv| &tv.webview == wv))
                .unwrap_or(false)
        })
    }

    fn touch_lru(st: &mut OverlayState, tab_id: i64) {
        st.order.retain(|&id| id != tab_id);
        st.order.push(tab_id);
    }

    /// Drop the least-recently-used tab webviews beyond the cache cap (never the
    /// active tab). Dropping removes the overlay child and frees the page.
    fn evict_lru() {
        // Remove entries from the map under the borrow, but defer the GTK
        // `overlay.remove` (which re-enters STATE via get_child_position) until
        // after the borrow is released.
        let (overlay, views): (Option<gtk::Overlay>, Vec<webkit2gtk::WebView>) = STATE.with(|s| {
            let mut views = Vec::new();
            let mut overlay = None;
            if let Some(st) = s.borrow_mut().as_mut() {
                overlay = Some(st.overlay.clone());
                let order = st.order.clone();
                let mut count = st.tabs.len();
                for id in order {
                    if count <= MAX_CACHED_TABS {
                        break;
                    }
                    if st.active == Some(id) {
                        continue;
                    }
                    if let Some(tv) = st.tabs.remove(&id) {
                        views.push(tv.webview);
                        count -= 1;
                    }
                    st.order.retain(|&o| o != id);
                }
            }
            (overlay, views)
        });
        if let Some(overlay) = overlay {
            for wv in views {
                overlay.remove(&wv);
            }
        }
    }

    pub fn navigate(
        window: tauri::Window,
        tab_id: i64,
        url: String,
        bounds: Option<WebviewBounds>,
    ) -> Result<(), String> {
        use std::sync::mpsc;
        let (tx, rx) = mpsc::channel();
        glib::MainContext::default().invoke(move || {
            let _ = tx.send(build(window, tab_id, url, bounds));
        });
        rx.recv().map_err(|e| format!("GTK thread channel error: {}", e))?
    }

    fn build(
        window: tauri::Window,
        tab_id: i64,
        url: String,
        bounds: Option<WebviewBounds>,
    ) -> Result<(), String> {
        use tauri::Manager;
        use webkit2gtk::{SettingsExt, WebViewExt};
        let app = window.app_handle().clone();
        crate::browser_surface::imp::set_current_tab(tab_id);

        // First navigation ever: wrap the window's content in a GtkOverlay.
        let need_overlay = STATE.with(|s| s.borrow().is_none());
        if need_overlay {
            let gtk_win = window.gtk_window().map_err(|e| e.to_string())?;
            let child = gtk_win.child().ok_or("Tauri window has no child widget to wrap")?;
            gtk_win.remove(&child);
            let overlay = gtk::Overlay::new();
            overlay.add(&child); // React app underneath, fills the window
            overlay.connect_get_child_position(|_overlay, _widget| {
                STATE.with(|s| {
                    s.borrow().as_ref().map(|st| {
                        gdk::Rectangle::new(
                            st.bounds.x as i32,
                            st.bounds.y as i32,
                            st.bounds.width as i32,
                            st.bounds.height as i32,
                        )
                    })
                })
            });
            gtk_win.add(&overlay);
            STATE.with(|s| {
                *s.borrow_mut() = Some(OverlayState {
                    overlay: overlay.clone(),
                    web_context: None,
                    bounds: bounds.clone().unwrap_or(DEFAULT_BOUNDS),
                    tabs: HashMap::new(),
                    order: Vec::new(),
                    active: None,
                })
            });
            overlay.show_all();
        }

        // Keep bounds current (window resize / panel inset).
        if let Some(b) = bounds.clone() {
            STATE.with(|s| {
                if let Some(st) = s.borrow_mut().as_mut() {
                    st.bounds = b;
                }
            });
            reposition();
        }

        // Already have a live webview for this tab? Switch to it (the cache hit).
        let existing = STATE.with(|s| {
            s.borrow()
                .as_ref()
                .and_then(|st| st.tabs.get(&tab_id).map(|tv| (tv.webview.clone(), tv.last_url.clone())))
        });
        if let Some((wv, last)) = existing {
            STATE.with(|s| {
                if let Some(st) = s.borrow_mut().as_mut() {
                    st.active = Some(tab_id);
                    touch_lru(st, tab_id);
                }
            });

            // Restore-without-reload: switching back to a page we already hold must
            // NOT reload. Match the URL we last loaded OR the page's CURRENT live
            // URL (in-page nav/redirects drift it after load).
            let current_uri = WebViewExt::uri(&wv).map(|s| s.to_string());
            let same = bounds.is_some()
                && (last.as_deref() == Some(url.as_str())
                    || current_uri.as_deref() == Some(url.as_str()));
            if same {
                crate::browser_surface::imp::surface_set_loading(false);
                update_visibility();
                return Ok(());
            }

            // Same tab, genuinely new URL: load it (hidden until painted).
            crate::browser_surface::imp::clear_seen_origins(tab_id);
            if let Some(s) = WebViewExt::settings(&wv) {
                SettingsExt::set_enable_javascript(&s, crate::browser_surface::imp::js_allowed_for(&url));
            }
            crate::browser_surface::imp::surface_set_loading(true);
            update_visibility();
            WebViewExt::load_uri(&wv, &url);
            STATE.with(|s| {
                if let Some(st) = s.borrow_mut().as_mut() {
                    if let Some(tv) = st.tabs.get_mut(&tab_id) {
                        tv.last_url = Some(url.clone());
                    }
                }
            });
            return Ok(());
        }

        // New tab: build a webview on the SHARED context (or create the context).
        let shared = STATE.with(|s| s.borrow().as_ref().and_then(|st| st.web_context.clone()));
        let first_ctx = shared.is_none();
        let (webview, ctx) =
            crate::browser_surface::imp::configure_page_webview(&app, tab_id, &url, shared.as_ref());
        let overlay = match STATE.with(|s| s.borrow().as_ref().map(|st| st.overlay.clone())) {
            Some(o) => o,
            None => return Err("overlay not initialized".to_string()),
        };
        // Record the tab under a SHORT borrow with no GTK calls inside.
        STATE.with(|s| {
            if let Some(st) = s.borrow_mut().as_mut() {
                if st.web_context.is_none() {
                    st.web_context = Some(ctx.clone());
                }
                st.tabs.insert(tab_id, TabView { webview: webview.clone(), last_url: Some(url.clone()) });
                st.active = Some(tab_id);
                touch_lru(st, tab_id);
            }
        });
        // Attach + realize the child OUTSIDE the borrow — `add_overlay`/`show_all`
        // synchronously emit `get_child_position`, which borrows STATE.
        overlay.add_overlay(&webview);
        overlay.show_all();
        evict_lru();

        crate::browser_surface::imp::clear_seen_origins(tab_id);
        crate::browser_surface::imp::surface_set_loading(true);
        update_visibility(); // hide others; the new tab stays hidden until painted
        WebViewExt::load_uri(&webview, &url);
        if first_ctx {
            crate::browser_surface::imp::push_trust_to_extensions();
        }
        Ok(())
    }

    /// Send the NoScript trusted-origins set to the shared context's web extension.
    pub fn push_trust(origins: Vec<String>) {
        glib::MainContext::default().invoke(move || {
            use glib::ToVariant;
            use webkit2gtk::{UserMessage, WebContextExt};
            STATE.with(|s| {
                if let Some(st) = s.borrow().as_ref() {
                    if let Some(ctx) = st.web_context.as_ref() {
                        let msg = UserMessage::new("noscript:set-trust", Some(&origins.to_variant()));
                        ctx.send_message_to_all_extensions(&msg);
                    }
                }
            });
        });
    }

    /// Apply the live privacy config (settings on every tab webview + cookie/ITP on
    /// the shared context) and reload the active page.
    pub fn apply_privacy(cfg: crate::browser_surface::PrivacyConfig) {
        glib::MainContext::default().invoke(move || {
            use webkit2gtk::{
                CookieAcceptPolicy, CookieManagerExt, WebContextExt, WebViewExt,
                WebsiteDataManagerExt,
            };
            STATE.with(|s| {
                if let Some(st) = s.borrow().as_ref() {
                    if let Some(ctx) = st.web_context.as_ref() {
                        if let Some(cm) = ctx.cookie_manager() {
                            cm.set_accept_policy(if cfg.block_third_party_cookies {
                                CookieAcceptPolicy::NoThirdParty
                            } else {
                                CookieAcceptPolicy::Always
                            });
                        }
                        if let Some(wdm) = ctx.website_data_manager() {
                            wdm.set_itp_enabled(cfg.tracking_prevention);
                        }
                    }
                    for tv in st.tabs.values() {
                        if let Some(settings) = WebViewExt::settings(&tv.webview) {
                            crate::browser_surface::imp::apply_privacy_settings(&settings, &cfg);
                        }
                    }
                    if let Some(a) = st.active {
                        if let Some(tv) = st.tabs.get(&a) {
                            WebViewExt::reload(&tv.webview);
                        }
                    }
                }
            });
        });
    }

    pub fn set_bounds(bounds: WebviewBounds) {
        glib::MainContext::default().invoke(move || {
            STATE.with(|s| {
                if let Some(st) = s.borrow_mut().as_mut() {
                    st.bounds = bounds;
                    st.overlay.queue_resize();
                }
            });
        });
    }

    pub fn show() {
        crate::browser_surface::imp::surface_set_wanted(true);
        glib::MainContext::default().invoke(update_visibility);
    }
    pub fn hide() {
        crate::browser_surface::imp::surface_set_wanted(false);
        glib::MainContext::default().invoke(update_visibility);
    }
    pub fn back() {
        with_active_webview(|wv| {
            use webkit2gtk::WebViewExt;
            wv.go_back()
        });
    }
    pub fn forward() {
        with_active_webview(|wv| {
            use webkit2gtk::WebViewExt;
            wv.go_forward()
        });
    }
    pub fn reload() {
        with_active_webview(|wv| {
            use webkit2gtk::WebViewExt;
            wv.reload()
        });
    }
    pub fn eval_js(script: String) {
        with_active_webview(move |wv| {
            use webkit2gtk::WebViewExt;
            wv.run_javascript(&script, None::<&gtk::gio::Cancellable>, |_| {});
        });
    }
}

#[cfg(target_os = "linux")]
pub fn navigate(
    window: tauri::Window,
    tab_id: i64,
    url: String,
    bounds: Option<WebviewBounds>,
) -> Result<(), String> {
    imp::navigate(window, tab_id, url, bounds)
}
#[cfg(target_os = "linux")]
pub fn push_trust(origins: Vec<String>) {
    imp::push_trust(origins)
}
#[cfg(target_os = "linux")]
pub fn apply_privacy(cfg: crate::browser_surface::PrivacyConfig) {
    imp::apply_privacy(cfg)
}
#[cfg(target_os = "linux")]
pub fn set_bounds(bounds: WebviewBounds) {
    imp::set_bounds(bounds)
}
#[cfg(target_os = "linux")]
pub fn show() {
    imp::show()
}
#[cfg(target_os = "linux")]
pub fn hide() {
    imp::hide()
}
#[cfg(target_os = "linux")]
pub fn back() {
    imp::back()
}
#[cfg(target_os = "linux")]
pub fn forward() {
    imp::forward()
}
#[cfg(target_os = "linux")]
pub fn reload() {
    imp::reload()
}
#[cfg(target_os = "linux")]
pub fn eval_js(script: String) {
    imp::eval_js(script)
}
/// The active tab's authoritative committed URL (origin source of truth for fills).
#[cfg(target_os = "linux")]
pub fn active_page_url() -> Option<String> {
    imp::active_page_url()
}
/// True if `wv` is the active tab's webview (used by the load-reveal gating in
/// `browser_surface` so a background tab's load doesn't pop over the active page).
#[cfg(target_os = "linux")]
pub fn is_active_webview(wv: &webkit2gtk::WebView) -> bool {
    imp::is_active_webview(wv)
}

#[cfg(not(target_os = "linux"))]
pub fn navigate(
    _window: tauri::Window,
    _tab_id: i64,
    _url: String,
    _bounds: Option<WebviewBounds>,
) -> Result<(), String> {
    Err("overlay embed only supported on Linux".to_string())
}
#[cfg(not(target_os = "linux"))]
pub fn push_trust(_origins: Vec<String>) {}
#[cfg(not(target_os = "linux"))]
pub fn apply_privacy(_cfg: crate::browser_surface::PrivacyConfig) {}
#[cfg(not(target_os = "linux"))]
pub fn set_bounds(_bounds: WebviewBounds) {}
#[cfg(not(target_os = "linux"))]
pub fn show() {}
#[cfg(not(target_os = "linux"))]
pub fn hide() {}
#[cfg(not(target_os = "linux"))]
pub fn back() {}
#[cfg(not(target_os = "linux"))]
pub fn forward() {}
#[cfg(not(target_os = "linux"))]
pub fn reload() {}
#[cfg(not(target_os = "linux"))]
pub fn eval_js(_script: String) {}
#[cfg(not(target_os = "linux"))]
pub fn active_page_url() -> Option<String> {
    None
}
