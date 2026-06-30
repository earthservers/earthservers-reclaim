//! Typed content kinds + the "comments & discussions" filter preset.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentKind {
    Article,
    Post,
    Comment,
    ForumPost,
    ForumComment,
}

impl ContentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ContentKind::Article => "article",
            ContentKind::Post => "post",
            ContentKind::Comment => "comment",
            ContentKind::ForumPost => "forum_post",
            ContentKind::ForumComment => "forum_comment",
        }
    }

    pub fn parse(s: &str) -> Option<ContentKind> {
        match s.trim().to_ascii_lowercase().as_str() {
            "article" => Some(ContentKind::Article),
            "post" => Some(ContentKind::Post),
            "comment" => Some(ContentKind::Comment),
            "forum_post" => Some(ContentKind::ForumPost),
            "forum_comment" => Some(ContentKind::ForumComment),
            _ => None,
        }
    }
}

/// The "Comments & discussions" preset = social comments + posts + forum posts +
/// forum comments. (Reddit posts are `post`; forum threads are `forum_post`.)
pub const COMMENTS_AND_DISCUSSIONS: &[&str] =
    &["comment", "forum_comment", "post", "forum_post"];

/// Normalize a kinds filter from the frontend into a deduped list of valid kind
/// strings. Returns None for "everything" (no filter). Unknown kinds are dropped;
/// if that leaves nothing, also treat as no filter (fail open to all).
pub fn normalize(kinds: &Option<Vec<String>>) -> Option<Vec<String>> {
    let raw = kinds.as_ref()?;
    if raw.is_empty() {
        return None;
    }
    let mut out: Vec<String> = Vec::new();
    for k in raw {
        if let Some(ck) = ContentKind::parse(k) {
            let s = ck.as_str().to_string();
            if !out.contains(&s) {
                out.push(s);
            }
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_maps_to_expected_kinds() {
        let n = normalize(&Some(COMMENTS_AND_DISCUSSIONS.iter().map(|s| s.to_string()).collect())).unwrap();
        assert!(n.contains(&"comment".to_string()));
        assert!(n.contains(&"forum_comment".to_string()));
        assert!(n.contains(&"post".to_string()));
        assert!(n.contains(&"forum_post".to_string()));
        assert!(!n.contains(&"article".to_string()));
    }

    #[test]
    fn normalize_fails_open() {
        assert_eq!(normalize(&None), None);
        assert_eq!(normalize(&Some(vec![])), None);
        assert_eq!(normalize(&Some(vec!["bogus".into()])), None, "all-invalid → no filter");
        assert_eq!(
            normalize(&Some(vec!["comment".into(), "comment".into(), "junk".into()])),
            Some(vec!["comment".into()]),
            "dedups + drops invalid"
        );
    }
}
