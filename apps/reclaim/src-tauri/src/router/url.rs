//! URL parsing + domain classification.
//!
//! This is the RESOLUTION-independent step: we work out the host and its
//! `DomainClass` once, up front, and flow that class through unchanged. The
//! render axis keys off the class, never off which resolver answered — that is
//! what keeps resolution and rendering orthogonal.

/// Which render engine a host ultimately wants.
///
/// `.earth` -> Servo (Phase 4); `.click` and all legacy TLDs -> WebKitGTK.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DomainClass {
    Earth,
    Click,
    Legacy,
}

/// The result of normalizing + classifying a raw navigation input.
#[derive(Debug, Clone)]
pub struct ParsedUrl {
    /// Exactly what the caller passed in.
    pub original: String,
    /// Normalized absolute URL the engine should load.
    pub url: String,
    /// URL scheme ("https", "earth", "tauri", ...).
    pub scheme: String,
    /// Registrable host, or "" for internal/search inputs.
    pub host: String,
    pub class: DomainClass,
    /// Internal app route (earth:// / tauri://) — bypasses resolve + external render.
    pub is_internal: bool,
}

/// App-internal schemes that should NOT hit the resolver chain or an external
/// render engine. `earth://search` is the home/search route.
fn is_internal_scheme(scheme: &str) -> bool {
    matches!(scheme, "earth" | "tauri")
}

/// Classify a host by its TLD.
pub fn classify_tld(host: &str) -> DomainClass {
    let h = host.trim_end_matches('.').to_ascii_lowercase();
    if h == "earth" || h.ends_with(".earth") {
        DomainClass::Earth
    } else if h == "click" || h.ends_with(".click") {
        DomainClass::Click
    } else {
        DomainClass::Legacy
    }
}

/// Normalize raw user input into an absolute URL and classify it.
///
/// Owns the "add `https://` to a bare host vs build a DuckDuckGo search"
/// heuristic that previously lived in the frontend, so every caller of the
/// `navigate` front door behaves identically.
pub fn parse(input: &str) -> ParsedUrl {
    let trimmed = input.trim();

    // Detect an explicit `scheme://` prefix without a full parse first.
    let explicit_scheme = trimmed.split_once("://").map(|(s, _)| s.to_ascii_lowercase()).filter(|s| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
    });

    // Internal app routes: keep verbatim, no resolve/render.
    if let Some(scheme) = explicit_scheme.as_deref() {
        if is_internal_scheme(scheme) {
            return ParsedUrl {
                original: input.to_string(),
                url: trimmed.to_string(),
                scheme: scheme.to_string(),
                host: String::new(),
                class: DomainClass::Legacy,
                is_internal: true,
            };
        }
    }

    // Build an absolute http(s) URL when the user typed a bare host or a query.
    let absolute = if explicit_scheme.is_some() {
        trimmed.to_string()
    } else if looks_like_host(trimmed) {
        format!("https://{trimmed}")
    } else {
        format!("https://duckduckgo.com/?q={}", urlencode(trimmed))
    };

    let (scheme, host) = match url::Url::parse(&absolute) {
        Ok(u) => (u.scheme().to_string(), u.host_str().unwrap_or_default().to_string()),
        Err(_) => ("https".to_string(), String::new()),
    };

    let class = classify_tld(&host);

    ParsedUrl {
        original: input.to_string(),
        url: absolute,
        scheme,
        host,
        class,
        is_internal: false,
    }
}

/// A single dot-bearing, space-free token is treated as a host; anything else
/// (multiple words, no dot) is treated as a search query.
fn looks_like_host(s: &str) -> bool {
    s.contains('.') && !s.contains(char::is_whitespace)
}

/// Minimal percent-encoder for the search-query path (avoids a new dependency).
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_tlds() {
        assert_eq!(classify_tld("foo.earth"), DomainClass::Earth);
        assert_eq!(classify_tld("foo.click"), DomainClass::Click);
        assert_eq!(classify_tld("example.com"), DomainClass::Legacy);
        assert_eq!(classify_tld("a.b.earth"), DomainClass::Earth);
    }

    #[test]
    fn internal_schemes_bypass() {
        let p = parse("earth://search");
        assert!(p.is_internal);
        assert_eq!(p.scheme, "earth");
    }

    #[test]
    fn bare_host_gets_https() {
        let p = parse("example.com");
        assert_eq!(p.url, "https://example.com");
        assert_eq!(p.host, "example.com");
        assert_eq!(p.class, DomainClass::Legacy);
    }

    #[test]
    fn multiword_becomes_search() {
        let p = parse("hello world");
        assert!(p.url.starts_with("https://duckduckgo.com/?q="));
        assert_eq!(p.host, "duckduckgo.com");
    }

    #[test]
    fn earth_and_click_hosts() {
        assert_eq!(parse("mysite.earth").class, DomainClass::Earth);
        assert_eq!(parse("cool.click").class, DomainClass::Click);
    }
}
