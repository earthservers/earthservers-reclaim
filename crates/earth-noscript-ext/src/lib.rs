//! Earth Reclaim NoScript — WebKit web-process extension.
//!
//! Loaded into WebKit's web process (the only place sub-resource requests can be
//! cancelled). Responsibilities:
//!   * OBSERVE: report each distinct request origin per page to the UI via a
//!     `noscript:seen` message — payload `(origin, is_first_party)`.
//!   * ENFORCE: block requests from THIRD-PARTY origins that are not in the
//!     trusted set. The first-party origin is always allowed (blocking it would
//!     break the document); first-party JS is governed separately by the UI's
//!     per-page `enable_javascript` toggle.
//!
//! The trusted set is pushed from the UI process via
//! `WebContext::send_message_to_all_extensions` (`noscript:set-trust`, payload =
//! array of trusted origin strings) and received here on `WebExtension`.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use glib::ToVariant;
use webkit2gtk_webextension::traits::*;
use webkit2gtk_webextension::{UserMessage, WebExtension};

webkit2gtk_webextension::web_extension_init!();

thread_local! {
    /// Origins trusted to load (union of the UI's persistent + temp trust sets).
    /// Single web-process thread → RefCell is sufficient.
    static TRUSTED: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

/// Host (registrable origin) for an http(s) URL, lowercased; None otherwise.
fn origin_of(uri: &str) -> Option<String> {
    let parsed = url::Url::parse(uri).ok()?;
    match parsed.scheme() {
        "http" | "https" => parsed.host_str().map(|h| h.to_ascii_lowercase()),
        _ => None,
    }
}

pub fn web_extension_initialize(extension: &WebExtension) {
    eprintln!("[noscript-ext] loaded into web process");

    // UI -> extension: replace the trusted set whenever the UI pushes it.
    extension.connect_user_message_received(|_ext, msg| {
        if msg.name().as_deref() == Some("noscript:set-trust") {
            if let Some(list) = msg.parameters().and_then(|p| p.get::<Vec<String>>()) {
                TRUSTED.with(|t| {
                    let mut set = t.borrow_mut();
                    set.clear();
                    set.extend(list.into_iter().map(|o| o.to_ascii_lowercase()));
                });
            }
        }
    });

    extension.connect_page_created(|_ext, page| {
        // Distinct origins already reported for this page (dedupe the messages).
        let seen: Rc<RefCell<HashSet<String>>> = Rc::new(RefCell::new(HashSet::new()));
        // Track the page's first-party origin; when it changes (a new top-level
        // navigation), reset `seen` so origins are re-reported for the new page.
        let last_fp: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

        page.connect_send_request(move |page, request, _response| {
            let uri = match request.uri() {
                Some(u) => u,
                None => return false,
            };
            let origin = match origin_of(uri.as_str()) {
                Some(o) => o,
                None => return false, // non-http(s): leave alone
            };

            // First-party = the page's own document origin. Always allowed.
            let first_party = page.uri().and_then(|u| origin_of(u.as_str()));
            if *last_fp.borrow() != first_party {
                seen.borrow_mut().clear();
                *last_fp.borrow_mut() = first_party.clone();
            }
            let is_first_party = first_party.as_deref() == Some(origin.as_str());

            // Report newly seen origins to the UI (origin + first-party flag).
            if seen.borrow_mut().insert(origin.clone()) {
                eprintln!("[noscript-ext] seen origin: {origin} (first_party={is_first_party})");
                let payload = (origin.clone(), is_first_party).to_variant();
                let m = UserMessage::new("noscript:seen", Some(&payload));
                page.send_message_to_view(&m, None::<&gio::Cancellable>, |_res| {});
            }

            // Enforce: block third-party origins that aren't trusted.
            if is_first_party {
                return false;
            }
            let trusted = TRUSTED.with(|t| t.borrow().contains(&origin));
            if trusted {
                false
            } else {
                eprintln!("[noscript-ext] BLOCKED {origin}");
                true
            }
        });
    });
}
