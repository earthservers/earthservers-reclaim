//! File attachments & filesystem access for the local assistant.
//!
//! Two ways the assistant gets at files, both explicit and user-driven:
//!   • Attachments — files the user drag-drops (or picks) into the composer.
//!   • Filesystem access — an opt-in toggle that lets a path mentioned in the
//!     message be read/listed on demand (`assistant_resolve_paths`).
//!
//! Everything stays on-device: this is plain local IO, no network, no indexing.
//! Reads are capped and binary files are rejected so we never shove megabytes of
//! garbage into the model's context window.

use std::path::{Path, PathBuf};

/// Max bytes read from any single file. Text beyond this is truncated with a
/// marker so the model knows it isn't seeing the whole thing.
const MAX_FILE_BYTES: usize = 256 * 1024;
/// Max entries listed for a single directory.
const MAX_DIR_ENTRIES: usize = 200;
/// Max files auto-read when a directory is referenced with filesystem access on.
const MAX_DIR_FILES_READ: usize = 20;

/// One attached/resolved file or directory, ready to be folded into the prompt.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AttachedFile {
    pub path: String,
    pub name: String,
    /// "file" | "dir" | "error"
    pub kind: String,
    /// Extracted text (file), listing (dir), or the error message.
    pub content: String,
    pub bytes: u64,
    pub truncated: bool,
}

/// Whether a file's leading bytes read as text. Sniffed rather than trusted by
/// extension, so unlabelled text files still work and images/video/archives stay
/// out of the model's context.
fn looks_textual(head: &[u8]) -> bool {
    // A NUL byte in the first chunk is the classic binary tell.
    if head.contains(&0) {
        return false;
    }
    // Mostly-valid UTF-8 (or ASCII) reads as text.
    match std::str::from_utf8(head) {
        Ok(_) => true,
        // A trailing multi-byte char cut by the chunk boundary is still text.
        Err(e) => e.valid_up_to() + 4 >= head.len(),
    }
}

fn file_name_of(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| p.to_string_lossy().to_string())
}

fn err_entry(path: &Path, msg: impl Into<String>) -> AttachedFile {
    AttachedFile {
        path: path.to_string_lossy().to_string(),
        name: file_name_of(path),
        kind: "error".to_string(),
        content: msg.into(),
        bytes: 0,
        truncated: false,
    }
}

/// Read one file as text, capped at MAX_FILE_BYTES.
pub fn read_text_file(path: &Path) -> AttachedFile {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) => return err_entry(path, format!("Can't read: {}", e)),
    };
    let size = meta.len();
    // Only regular files. Devices (/dev/zero), FIFOs, sockets and the like would
    // either block the reader forever or never stop producing bytes.
    if !meta.is_file() {
        return err_entry(path, "Skipped — not a regular file");
    }

    // Bounded read: pull at most MAX_FILE_BYTES + 1 so a multi-GB file (or a
    // /proc entry that lies about its size) can't be slurped into memory just
    // to be truncated afterwards. The extra byte tells us whether we truncated.
    let data = {
        use std::io::Read;
        let file = match std::fs::File::open(path) {
            Ok(f) => f,
            Err(e) => return err_entry(path, format!("Can't read: {}", e)),
        };
        let mut buf = Vec::with_capacity((size as usize).min(MAX_FILE_BYTES + 1));
        if let Err(e) = file.take(MAX_FILE_BYTES as u64 + 1).read_to_end(&mut buf) {
            return err_entry(path, format!("Can't read: {}", e));
        }
        buf
    };
    let head_len = data.len().min(8192);
    if !looks_textual(&data[..head_len]) {
        return err_entry(
            path,
            format!(
                "Skipped — looks like a binary file ({}). The assistant reads text files only.",
                human_bytes(size)
            ),
        );
    }

    let truncated = data.len() > MAX_FILE_BYTES;
    let slice = if truncated { &data[..MAX_FILE_BYTES] } else { &data[..] };
    let mut text = String::from_utf8_lossy(slice).to_string();
    if truncated {
        text.push_str("\n\n… [truncated — file is larger than 256 KB]");
    }

    AttachedFile {
        path: path.to_string_lossy().to_string(),
        name: file_name_of(path),
        kind: "file".to_string(),
        content: text,
        bytes: size,
        truncated,
    }
}

fn human_bytes(n: u64) -> String {
    if n >= 1 << 20 {
        format!("{:.1} MB", n as f64 / (1u64 << 20) as f64)
    } else if n >= 1 << 10 {
        format!("{:.0} KB", n as f64 / 1024.0)
    } else {
        format!("{} B", n)
    }
}

/// List a directory (names + sizes), non-recursive.
pub fn list_dir(path: &Path) -> AttachedFile {
    let rd = match std::fs::read_dir(path) {
        Ok(r) => r,
        Err(e) => return err_entry(path, format!("Can't list directory: {}", e)),
    };
    let mut entries: Vec<String> = Vec::new();
    let mut count = 0usize;
    let mut more = false;
    for e in rd.flatten() {
        if count >= MAX_DIR_ENTRIES {
            more = true;
            break;
        }
        let p = e.path();
        let is_dir = p.is_dir();
        let size = e.metadata().map(|m| m.len()).unwrap_or(0);
        entries.push(if is_dir {
            format!("{}/", file_name_of(&p))
        } else {
            format!("{}  ({})", file_name_of(&p), human_bytes(size))
        });
        count += 1;
    }
    entries.sort();
    let mut content = entries.join("\n");
    if more {
        content.push_str(&format!("\n… [more than {} entries — list truncated]", MAX_DIR_ENTRIES));
    }
    AttachedFile {
        path: path.to_string_lossy().to_string(),
        name: file_name_of(path),
        kind: "dir".to_string(),
        content,
        bytes: 0,
        truncated: more,
    }
}

/// Read a set of explicit paths (the drag-dropped / picked attachments).
/// Directories become a listing plus the text of their first few files.
#[tauri::command(rename_all = "camelCase")]
pub async fn assistant_read_files(paths: Vec<String>) -> Result<Vec<AttachedFile>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut out = Vec::new();
        for p in paths {
            let path = PathBuf::from(&p);
            if path.is_dir() {
                out.push(list_dir(&path));
                // Pull in the text of the first few files so "look at this folder"
                // actually gives the model something to work with.
                if let Ok(rd) = std::fs::read_dir(&path) {
                    let mut read = 0usize;
                    for e in rd.flatten() {
                        if read >= MAX_DIR_FILES_READ {
                            break;
                        }
                        let fp = e.path();
                        if fp.is_file() {
                            let f = read_text_file(&fp);
                            if f.kind == "file" {
                                out.push(f);
                                read += 1;
                            }
                        }
                    }
                }
            } else if path.exists() {
                out.push(read_text_file(&path));
            } else {
                out.push(err_entry(&path, "No such file or directory"));
            }
        }
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Extract filesystem paths mentioned in a message and read them. Used when the
/// "Filesystem access" toggle is on, so referencing a directory in prose ("look
/// at ~/projects/foo") actually reaches the disk.
///
/// Only absolute paths, `~`-paths, and existing relative paths are considered —
/// we never guess. Nothing is read unless it exists on disk.
#[tauri::command(rename_all = "camelCase")]
pub async fn assistant_resolve_paths(message: String) -> Result<Vec<AttachedFile>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut out = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        for cand in extract_paths(&message) {
            let expanded = expand_tilde(&cand);
            let path = PathBuf::from(&expanded);
            if !path.exists() {
                continue;
            }
            let key = path.to_string_lossy().to_string();
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            if path.is_dir() {
                out.push(list_dir(&path));
            } else {
                out.push(read_text_file(&path));
            }
            if out.len() >= 10 {
                break;
            }
        }
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn expand_tilde(p: &str) -> String {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = dirs_home() {
            return format!("{}/{}", home.trim_end_matches('/'), rest);
        }
    } else if p == "~" {
        if let Some(home) = dirs_home() {
            return home;
        }
    }
    p.to_string()
}

fn dirs_home() -> Option<String> {
    std::env::var("HOME").ok().or_else(|| std::env::var("USERPROFILE").ok())
}

/// Pull path-looking tokens out of free text. Handles quoted paths (which may
/// contain spaces) and bare absolute/tilde/relative paths.
fn extract_paths(msg: &str) -> Vec<String> {
    let mut found = Vec::new();

    // Quoted paths first — the only reliable way to catch paths with spaces.
    for quote in ['"', '\''] {
        let mut rest = msg;
        while let Some(start) = rest.find(quote) {
            let after = &rest[start + 1..];
            if let Some(end) = after.find(quote) {
                let inner = &after[..end];
                if inner.contains('/') || inner.contains('\\') {
                    found.push(inner.to_string());
                }
                rest = &after[end + 1..];
            } else {
                break;
            }
        }
    }

    // Bare tokens that look like paths.
    for tok in msg.split_whitespace() {
        let t = tok.trim_matches(|c: char| {
            matches!(c, '"' | '\'' | '`' | ',' | ';' | '(' | ')' | '[' | ']' | '<' | '>')
        });
        // Trailing sentence punctuation, but keep a trailing slash.
        let t = t.trim_end_matches(|c: char| matches!(c, '.' | '!' | '?' | ':'));
        if t.is_empty() || t.len() < 2 {
            continue;
        }
        // Skip URLs — those are the research tool's job, not the filesystem's.
        if t.contains("://") {
            continue;
        }
        let looks_pathy = t.starts_with('/')
            || t.starts_with("~/")
            || t.starts_with("./")
            || t.starts_with("../")
            // Windows drive paths, e.g. C:\Users\...
            || (t.len() > 3 && t.as_bytes()[1] == b':' && (t.contains('\\') || t.contains('/')));
        if looks_pathy {
            found.push(t.to_string());
        }
    }
    found
}

/// Render attachments into the prompt block the model sees. Kept here so both
/// chat and research mode format them identically.
pub fn format_attachments(files: &[AttachedFile]) -> String {
    if files.is_empty() {
        return String::new();
    }
    let mut s = String::from("--- Files the user attached / referenced ---\n");
    for f in files {
        match f.kind.as_str() {
            "dir" => {
                s.push_str(&format!("\n[Directory] {}\n{}\n", f.path, f.content));
            }
            "error" => {
                s.push_str(&format!("\n[Unavailable] {} — {}\n", f.path, f.content));
            }
            _ => {
                s.push_str(&format!(
                    "\n[File] {} ({})\n```\n{}\n```\n",
                    f.path,
                    human_bytes(f.bytes),
                    f.content
                ));
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_absolute_and_tilde_paths() {
        let p = extract_paths("please look at /etc/hosts and ~/notes.txt today");
        assert!(p.contains(&"/etc/hosts".to_string()));
        assert!(p.contains(&"~/notes.txt".to_string()));
    }

    #[test]
    fn ignores_urls() {
        let p = extract_paths("check https://example.com/foo/bar");
        assert!(p.is_empty(), "URLs must not be treated as paths: {:?}", p);
    }

    #[test]
    fn extracts_quoted_path_with_spaces() {
        let p = extract_paths("open \"/home/me/My Docs/report.md\" please");
        assert!(p.contains(&"/home/me/My Docs/report.md".to_string()));
    }

    #[test]
    fn strips_trailing_sentence_punctuation() {
        let p = extract_paths("summarize /tmp/a.txt.");
        assert!(p.contains(&"/tmp/a.txt".to_string()));
    }

    #[test]
    fn binary_content_is_not_textual() {
        assert!(!looks_textual(&[0x00, 0x01, 0x02]));
        assert!(looks_textual(b"hello world"));
    }
}

