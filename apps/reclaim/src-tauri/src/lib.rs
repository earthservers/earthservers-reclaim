// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod search;
mod memory;
mod ratings;
mod ai;
mod knowledge_graph;
mod profile;
mod privacy;
mod theme;
mod tabs;
mod bookmarks;
mod split_view;
mod multimedia;
mod webview;
mod scraper;
mod identity;
mod servo_browser;
mod media_player;
mod video_surface;
mod controls_server;
mod browser_surface;
mod browser_overlay;
mod router;
mod vault;
mod media_downloads;
mod assistant;
mod research;
mod ai_lock;

use std::sync::Mutex;
use tauri::{
    Manager, State,
    menu::{Menu, MenuItem},
    tray::{TrayIconBuilder, TrayIconEvent, MouseButton, MouseButtonState},
    WebviewUrl, WebviewWindowBuilder,
};

use profile::{Profile, ProfileManager, PrivacySettings};
use privacy::{PrivacyManager, HistoryEntry, HistoryStats};
use knowledge_graph::{KnowledgeGraph, Page, SearchResult as KGSearchResult};
use theme::{Theme, ThemeManager, PresetTheme, get_preset_themes};
use search::{Domain, DomainList, DomainStats, SearchManager};
use memory::{IndexedPage, PageNote, MemoryStats, MemoryManager};
use ratings::{DomainRating, RatingAggregate, RatingSummary, SubdomainRating, RatingManager, UserRatingHistory};
use tabs::{Tab, TabHistoryEntry, TabManager};
use bookmarks::{Bookmark, BookmarkFolder, BookmarkManager};
use split_view::{SplitViewConfig, SplitViewManager, PaneSizes};
use multimedia::{MediaHistoryEntry, Playlist, PlaylistItem, PrivacySettings as MediaPrivacySettings, MediaStats, MultimediaManager};
use scraper::{ScrapingJob, ScrapedPage, ContentSelector, ScraperManager};
use identity::{HardwareInfo, DeviceFingerprint};

// Application state managed by Tauri
pub struct AppState {
    pub db_path: String,
    pub profile_manager: ProfileManager,
    pub privacy_manager: PrivacyManager,
    pub knowledge_graph: KnowledgeGraph,
    pub theme_manager: ThemeManager,
    pub search_manager: SearchManager,
    pub memory_manager: MemoryManager,
    pub rating_manager: RatingManager,
    pub tab_manager: TabManager,
    pub bookmark_manager: BookmarkManager,
    pub split_view_manager: SplitViewManager,
    pub multimedia_manager: MultimediaManager,
    pub scraper_manager: ScraperManager,
    pub vault_manager: vault::VaultManager,
}

// ==================== Profile Commands ====================

#[tauri::command(rename_all = "camelCase")]
async fn get_profiles(state: State<'_, Mutex<AppState>>) -> Result<Vec<Profile>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.profile_manager
        .get_profiles()
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn get_active_profile(state: State<'_, Mutex<AppState>>) -> Result<Option<Profile>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.profile_manager
        .get_active_profile()
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn create_profile(
    state: State<'_, Mutex<AppState>>,
    name: String,
    icon: Option<String>,
    delete_pin: String,
) -> Result<Profile, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    let profile = state.profile_manager
        .create_profile(&name, icon.as_deref(), &delete_pin)
        .map_err(format_profile_pin_err)?;
    // A profile named "Incognito" is the dedicated, always-private profile.
    if name.trim().eq_ignore_ascii_case("incognito") {
        if let Some(id) = profile.id {
            PrivacyManager::mark_incognito_profile(id);
        }
    }
    Ok(profile)
}

#[tauri::command(rename_all = "camelCase")]
async fn switch_profile(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
) -> Result<Profile, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.profile_manager
        .switch_profile(profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn update_profile(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    name: String,
    icon: Option<String>,
) -> Result<Profile, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.profile_manager
        .update_profile(profile_id, &name, icon.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn delete_profile(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    pin: String,
) -> Result<(), String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.profile_manager
        .delete_profile(profile_id, &pin)
        .map_err(format_profile_pin_err)
}

/// Wipe ALL of a profile's data but keep the profile itself. The only destructive
/// option for the protected Default/Incognito profiles; gated by the delete code.
#[tauri::command(rename_all = "camelCase")]
async fn wipe_profile(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    pin: String,
) -> Result<(), String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.profile_manager
        .wipe_profile(profile_id, &pin)
        .map_err(format_profile_pin_err)
}

/// Whether a profile is protected (Default/Incognito — wipe-only, never deletable).
#[tauri::command(rename_all = "camelCase")]
async fn profile_is_protected(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
) -> Result<bool, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.profile_manager
        .is_protected_profile(profile_id)
        .map_err(|e| e.to_string())
}

/// Whether a profile already has a 4-digit delete code set.
#[tauri::command(rename_all = "camelCase")]
async fn profile_has_delete_pin(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
) -> Result<bool, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.profile_manager
        .has_delete_pin(profile_id)
        .map_err(|e| e.to_string())
}

/// Set (or change) a profile's 4-digit delete code.
#[tauri::command(rename_all = "camelCase")]
async fn set_profile_delete_pin(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    pin: String,
) -> Result<(), String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.profile_manager
        .set_delete_pin(profile_id, &pin)
        .map_err(format_profile_pin_err)
}

/// rusqlite surfaces our user-facing messages as `InvalidParameterName("…")`;
/// unwrap those to the bare message and pass everything else through.
fn format_profile_pin_err(e: rusqlite::Error) -> String {
    match e {
        rusqlite::Error::InvalidParameterName(m) => m,
        other => other.to_string(),
    }
}

#[tauri::command(rename_all = "camelCase")]
async fn get_privacy_settings(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
) -> Result<PrivacySettings, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.profile_manager
        .get_privacy_settings(profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn update_privacy_settings(
    state: State<'_, Mutex<AppState>>,
    settings: PrivacySettings,
) -> Result<(), String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.profile_manager
        .update_privacy_settings(&settings)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn export_profile(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
) -> Result<String, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.profile_manager
        .export_profile(profile_id)
        .map_err(|e| e.to_string())
}

// ==================== Per-Profile Incognito Commands ====================

#[tauri::command(rename_all = "camelCase")]
fn get_incognito_status(profile_id: i64) -> bool {
    PrivacyManager::is_incognito(profile_id)
}

#[tauri::command(rename_all = "camelCase")]
fn toggle_incognito(profile_id: i64) -> bool {
    PrivacyManager::toggle_incognito(profile_id)
}

#[tauri::command(rename_all = "camelCase")]
fn set_incognito(profile_id: i64, enabled: bool) {
    PrivacyManager::set_incognito(profile_id, enabled);
}

#[tauri::command(rename_all = "camelCase")]
fn get_incognito_profiles() -> Vec<i64> {
    PrivacyManager::get_incognito_profiles()
}

/// Whether a profile is FORCED incognito (the dedicated Incognito profile) — so
/// the UI can show the toggle as locked-on.
#[tauri::command(rename_all = "camelCase")]
fn incognito_is_forced(profile_id: i64) -> bool {
    PrivacyManager::is_forced_incognito(profile_id)
}

// ==================== Identity Commands (Hardware Fingerprinting) ====================

#[tauri::command(rename_all = "camelCase")]
fn get_hardware_info() -> HardwareInfo {
    identity::get_hardware_info()
}

#[tauri::command(rename_all = "camelCase")]
fn get_device_fingerprint() -> DeviceFingerprint {
    identity::generate_device_fingerprint()
}

// ==================== History Commands ====================

#[tauri::command(rename_all = "camelCase")]
async fn get_history(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    search_query: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<HistoryEntry>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.privacy_manager
        .get_history(
            profile_id,
            search_query.as_deref(),
            limit.unwrap_or(50),
            offset.unwrap_or(0),
        )
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn delete_history_entry(
    state: State<'_, Mutex<AppState>>,
    entry_id: i64,
    profile_id: i64,
) -> Result<bool, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.privacy_manager
        .delete_history_entry(entry_id, profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn delete_history_by_date_range(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    start_date: String,
    end_date: String,
) -> Result<i64, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.privacy_manager
        .delete_history_by_date_range(profile_id, &start_date, &end_date)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn clear_all_history(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
) -> Result<i64, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.privacy_manager
        .clear_all_history(profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn get_history_stats(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
) -> Result<HistoryStats, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.privacy_manager
        .get_history_stats(profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn export_history(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
) -> Result<String, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.privacy_manager
        .export_history(profile_id)
        .map_err(|e| e.to_string())
}

// ==================== Knowledge Graph Commands ====================

#[tauri::command(rename_all = "camelCase")]
async fn add_page(
    state: State<'_, Mutex<AppState>>,
    url: String,
    title: String,
    content: String,
    profile_id: i64,
) -> Result<Option<i64>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    let page = Page {
        id: None,
        url,
        title,
        content,
        visited_at: String::new(),
        embedding: None,
        profile_id: Some(profile_id),
    };
    state.knowledge_graph
        .add_page(&page, profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn search_knowledge_graph(
    state: State<'_, Mutex<AppState>>,
    query: String,
    profile_id: i64,
    limit: Option<i64>,
) -> Result<Vec<KGSearchResult>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.knowledge_graph
        .search_pages(&query, profile_id, limit.unwrap_or(20))
        .map_err(|e| e.to_string())
}

// ==================== Domain Commands (EarthSearch) ====================

#[tauri::command(rename_all = "camelCase")]
async fn get_domains(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
) -> Result<Vec<Domain>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.search_manager
        .get_domains(profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn add_domain_entry(
    state: State<'_, Mutex<AppState>>,
    url: String,
    category: String,
    trust_score: f64,
    profile_id: i64,
) -> Result<Domain, String> {
    let domain = Domain {
        id: None,
        url,
        category,
        trust_score,
        added_date: String::new(),
        metadata: None,
        profile_id: Some(profile_id),
    };
    let state = state.lock().map_err(|e| e.to_string())?;
    state.search_manager
        .add_domain(&domain, profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn update_domain(
    state: State<'_, Mutex<AppState>>,
    domain: Domain,
) -> Result<Domain, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.search_manager
        .update_domain(&domain)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn delete_domain_entry(
    state: State<'_, Mutex<AppState>>,
    domain_id: i64,
    profile_id: i64,
) -> Result<bool, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.search_manager
        .delete_domain(domain_id, profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn search_domain_list(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    query: String,
) -> Result<Vec<Domain>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.search_manager
        .search_domains(profile_id, &query)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn get_domain_lists(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
) -> Result<Vec<DomainList>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.search_manager
        .get_lists(profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn create_domain_list(
    state: State<'_, Mutex<AppState>>,
    name: String,
    description: Option<String>,
    profile_id: i64,
) -> Result<DomainList, String> {
    let list = DomainList {
        id: None,
        name,
        description,
        author: None,
        version: "1.0".to_string(),
        created_at: String::new(),
        profile_id: Some(profile_id),
        domain_count: None,
    };
    let state = state.lock().map_err(|e| e.to_string())?;
    state.search_manager
        .create_list(&list, profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn delete_domain_list(
    state: State<'_, Mutex<AppState>>,
    list_id: i64,
    profile_id: i64,
) -> Result<bool, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.search_manager
        .delete_list(list_id, profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn get_domain_stats(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
) -> Result<DomainStats, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.search_manager
        .get_stats(profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn get_domain_categories(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
) -> Result<Vec<String>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.search_manager
        .get_categories(profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn export_domains(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
) -> Result<String, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.search_manager
        .export_domains(profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn import_domains(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    json_data: String,
) -> Result<i64, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.search_manager
        .import_domains(profile_id, &json_data)
        .map_err(|e| e.to_string())
}

// ==================== Memory Commands (EarthMemory) ====================

#[tauri::command(rename_all = "camelCase")]
async fn get_indexed_pages(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<IndexedPage>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.memory_manager
        .get_pages(profile_id, limit, offset)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn index_page(
    state: State<'_, Mutex<AppState>>,
    page: IndexedPage,
    profile_id: i64,
) -> Result<IndexedPage, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.memory_manager
        .index_page(&page, profile_id)
        .map_err(|e| e.to_string())
}

/// Auto-curator entry point: summarize a freshly-visited page into EarthMemory
/// using the local model. Returns immediately — the fetch + summarize + store
/// runs in the background so navigation never waits, and it silently no-ops if
/// the page is non-http(s), the profile is incognito, or Ollama isn't running.
#[tauri::command(rename_all = "camelCase")]
async fn curate_page(
    app: tauri::AppHandle,
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    url: String,
    title: String,
) -> Result<(), String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Ok(());
    }
    if privacy::PrivacyManager::is_incognito(profile_id) {
        return Ok(());
    }
    let db_path = {
        let state = state.lock().map_err(|e| e.to_string())?;
        state.db_path.clone()
    };
    tauri::async_runtime::spawn(async move {
        match ai::curate(&db_path, profile_id, &url, &title).await {
            Ok(()) => {
                use tauri::Emitter;
                let _ = app.emit("memory-updated", ());
            }
            Err(e) => eprintln!("[curator] {url}: {e}"),
        }
    });
    Ok(())
}

/// Curate from VIEWED page text (sent by the in-page viewed-content bridge)
/// instead of re-fetching. Same no-op gating as `curate_page`; summarizes only
/// what the user actually scrolled through.
#[tauri::command(rename_all = "camelCase")]
async fn curate_viewed_page(
    app: tauri::AppHandle,
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    url: String,
    title: String,
    text: String,
) -> Result<(), String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Ok(());
    }
    if privacy::PrivacyManager::is_incognito(profile_id) {
        return Ok(());
    }
    let db_path = {
        let state = state.lock().map_err(|e| e.to_string())?;
        state.db_path.clone()
    };
    tauri::async_runtime::spawn(async move {
        match ai::curate_viewed(&db_path, profile_id, &url, &title, &text).await {
            Ok(()) => {
                use tauri::Emitter;
                let _ = app.emit("memory-updated", ());
            }
            Err(e) => eprintln!("[curator/viewed] {url}: {e}"),
        }
    });
    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
async fn search_memory(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    query: String,
) -> Result<Vec<IndexedPage>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.memory_manager
        .search_pages(profile_id, &query)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn get_favorite_pages(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
) -> Result<Vec<IndexedPage>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.memory_manager
        .get_favorites(profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn toggle_page_favorite(
    state: State<'_, Mutex<AppState>>,
    page_id: i64,
    profile_id: i64,
) -> Result<bool, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.memory_manager
        .toggle_favorite(page_id, profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn update_page_tags(
    state: State<'_, Mutex<AppState>>,
    page_id: i64,
    profile_id: i64,
    tags: String,
) -> Result<(), String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.memory_manager
        .update_tags(page_id, profile_id, &tags)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn delete_indexed_page(
    state: State<'_, Mutex<AppState>>,
    page_id: i64,
    profile_id: i64,
) -> Result<bool, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.memory_manager
        .delete_page(page_id, profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn add_page_note(
    state: State<'_, Mutex<AppState>>,
    page_id: i64,
    content: String,
    profile_id: i64,
) -> Result<PageNote, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.memory_manager
        .add_note(page_id, &content, profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn get_page_notes(
    state: State<'_, Mutex<AppState>>,
    page_id: i64,
) -> Result<Vec<PageNote>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.memory_manager
        .get_page_notes(page_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn update_page_note(
    state: State<'_, Mutex<AppState>>,
    note_id: i64,
    content: String,
    profile_id: i64,
) -> Result<(), String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.memory_manager
        .update_note(note_id, &content, profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn delete_page_note(
    state: State<'_, Mutex<AppState>>,
    note_id: i64,
    profile_id: i64,
) -> Result<bool, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.memory_manager
        .delete_note(note_id, profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn get_memory_stats(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
) -> Result<MemoryStats, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.memory_manager
        .get_stats(profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn get_memory_tags(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
) -> Result<Vec<String>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.memory_manager
        .get_all_tags(profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn export_memory(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
) -> Result<String, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.memory_manager
        .export_memory(profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn import_memory(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    json_data: String,
) -> Result<i64, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.memory_manager
        .import_memory(profile_id, &json_data)
        .map_err(|e| e.to_string())
}

// ==================== Domain Seeding Command ====================

#[tauri::command(rename_all = "camelCase")]
async fn seed_default_domains(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    resource_path: String,
) -> Result<i64, String> {
    let state = state.lock().map_err(|e| e.to_string())?;

    // Check multiple possible locations for domain-lists
    let provided = std::path::Path::new(&resource_path);
    let dev_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources");

    let possible_paths = [
        provided.join("domain-lists"),
        provided.join("resources").join("domain-lists"),
        dev_path.join("domain-lists"),
    ];

    let final_path = possible_paths
        .iter()
        .find(|p| p.exists())
        .map(|p| p.parent().unwrap().to_path_buf())
        .unwrap_or_else(|| {
            println!("Domain lists not found in any of: {:?}", possible_paths);
            provided.to_path_buf()
        });

    state.search_manager
        .seed_default_domains(profile_id, &final_path)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn force_reseed_domains(
    app: tauri::AppHandle,
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
) -> Result<i64, String> {
    let state = state.lock().map_err(|e| e.to_string())?;

    // Clear existing domains for this profile
    state.search_manager.clear_all_domains(profile_id).map_err(|e| e.to_string())?;

    // Get resource directory - try multiple locations
    let resource_dir = app.path().resource_dir().map_err(|e| e.to_string())?;
    let dev_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources");

    let possible_paths = [
        resource_dir.join("domain-lists"),
        resource_dir.join("resources").join("domain-lists"),
        dev_path.join("domain-lists"),
    ];

    let final_path = possible_paths
        .iter()
        .find(|p| p.exists())
        .map(|p| p.parent().unwrap().to_path_buf())
        .unwrap_or_else(|| {
            println!("Domain lists not found in any of: {:?}", possible_paths);
            resource_dir.clone()
        });

    println!("Force reseeding domains from: {:?}", final_path);

    // Seed from .earth files (search_manager handles the domain-lists subdirectory)
    state.search_manager
        .seed_default_domains(profile_id, &final_path)
        .map_err(|e| e.to_string())
}

// ==================== Rating Commands ====================

#[tauri::command(rename_all = "camelCase")]
async fn submit_rating(
    state: State<'_, Mutex<AppState>>,
    rating: DomainRating,
) -> Result<DomainRating, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.rating_manager
        .submit_rating(&rating)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn get_user_rating(
    state: State<'_, Mutex<AppState>>,
    domain_id: i64,
    user_id: String,
) -> Result<Option<DomainRating>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.rating_manager
        .get_user_rating(domain_id, &user_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn get_domain_ratings(
    state: State<'_, Mutex<AppState>>,
    domain_id: i64,
    limit: Option<i64>,
) -> Result<Vec<DomainRating>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.rating_manager
        .get_domain_ratings(domain_id, limit)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn delete_rating(
    state: State<'_, Mutex<AppState>>,
    rating_id: i64,
    user_id: String,
) -> Result<bool, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.rating_manager
        .delete_rating(rating_id, &user_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn get_rating_aggregate(
    state: State<'_, Mutex<AppState>>,
    domain_id: i64,
) -> Result<Option<RatingAggregate>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.rating_manager
        .get_aggregate(domain_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn get_rating_summary(
    state: State<'_, Mutex<AppState>>,
    domain_id: i64,
    domain_url: String,
) -> Result<RatingSummary, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.rating_manager
        .get_rating_summary(domain_id, &domain_url)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn submit_subdomain_rating(
    state: State<'_, Mutex<AppState>>,
    parent_domain_id: i64,
    subdomain: String,
    trust: f64,
    bias: f64,
) -> Result<SubdomainRating, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.rating_manager
        .submit_subdomain_rating(parent_domain_id, &subdomain, trust, bias)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn get_subdomain_ratings(
    state: State<'_, Mutex<AppState>>,
    parent_domain_id: i64,
) -> Result<Vec<SubdomainRating>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.rating_manager
        .get_subdomain_ratings(parent_domain_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn mark_rating_helpful(
    state: State<'_, Mutex<AppState>>,
    rating_id: i64,
) -> Result<i32, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.rating_manager
        .mark_helpful(rating_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn report_rating(
    state: State<'_, Mutex<AppState>>,
    rating_id: i64,
) -> Result<bool, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.rating_manager
        .report_rating(rating_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn get_user_rating_history(
    state: State<'_, Mutex<AppState>>,
    user_id: String,
) -> Result<UserRatingHistory, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.rating_manager
        .get_user_history(&user_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn add_rating_category_scores(
    state: State<'_, Mutex<AppState>>,
    rating_id: i64,
    categories: Vec<(String, i32)>,
) -> Result<(), String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.rating_manager
        .add_category_scores(rating_id, categories)
        .map_err(|e| e.to_string())
}

// ==================== Theme Commands ====================

#[tauri::command(rename_all = "camelCase")]
async fn get_themes(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
) -> Result<Vec<Theme>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.theme_manager
        .get_themes(profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn get_active_theme(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
) -> Result<Option<Theme>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.theme_manager
        .get_active_theme(profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn save_theme(
    state: State<'_, Mutex<AppState>>,
    theme: Theme,
) -> Result<Theme, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.theme_manager
        .save_theme(&theme)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn set_active_theme(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    theme_id: i64,
) -> Result<Theme, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.theme_manager
        .set_active_theme(profile_id, theme_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn delete_theme(
    state: State<'_, Mutex<AppState>>,
    theme_id: i64,
    profile_id: i64,
) -> Result<bool, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.theme_manager
        .delete_theme(theme_id, profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn apply_preset_theme(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    preset_id: String,
) -> Result<Theme, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.theme_manager
        .apply_preset(profile_id, &preset_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
fn get_theme_presets() -> Vec<PresetTheme> {
    get_preset_themes()
}

#[tauri::command(rename_all = "camelCase")]
async fn export_theme(
    state: State<'_, Mutex<AppState>>,
    theme_id: i64,
) -> Result<String, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.theme_manager
        .export_theme(theme_id)
        .map_err(|e| e.to_string())
}

// ==================== Tab Commands ====================

#[tauri::command(rename_all = "camelCase")]
async fn create_tab(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    url: String,
    title: Option<String>,
) -> Result<Tab, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.tab_manager
        .create_tab(profile_id, &url, title.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn close_tab(
    state: State<'_, Mutex<AppState>>,
    tab_id: i64,
) -> Result<(), String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.tab_manager
        .close_tab(tab_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn get_all_tabs(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
) -> Result<Vec<Tab>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.tab_manager
        .get_all_tabs(profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn update_tab(
    state: State<'_, Mutex<AppState>>,
    tab_id: i64,
    title: Option<String>,
    url: Option<String>,
    favicon: Option<String>,
) -> Result<Tab, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.tab_manager
        .update_tab(tab_id, title.as_deref(), url.as_deref(), favicon.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn reorder_tabs(
    state: State<'_, Mutex<AppState>>,
    tab_ids: Vec<i64>,
) -> Result<(), String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.tab_manager
        .reorder_tabs(tab_ids)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn pin_tab(
    state: State<'_, Mutex<AppState>>,
    tab_id: i64,
    pinned: bool,
) -> Result<Tab, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.tab_manager
        .pin_tab(tab_id, pinned)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn set_active_tab(
    state: State<'_, Mutex<AppState>>,
    tab_id: i64,
) -> Result<Tab, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.tab_manager
        .set_active_tab(tab_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn get_tab_history(
    state: State<'_, Mutex<AppState>>,
    tab_id: i64,
) -> Result<Vec<TabHistoryEntry>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.tab_manager
        .get_tab_history(tab_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn navigate_tab_back(
    state: State<'_, Mutex<AppState>>,
    tab_id: i64,
) -> Result<Option<String>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.tab_manager
        .navigate_back(tab_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn navigate_tab_forward(
    state: State<'_, Mutex<AppState>>,
    tab_id: i64,
) -> Result<Option<String>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.tab_manager
        .navigate_forward(tab_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn duplicate_tab(
    state: State<'_, Mutex<AppState>>,
    tab_id: i64,
) -> Result<Tab, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.tab_manager
        .duplicate_tab(tab_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn close_tabs_to_right(
    state: State<'_, Mutex<AppState>>,
    tab_id: i64,
) -> Result<(), String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.tab_manager
        .close_tabs_to_right(tab_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn close_unpinned_tabs(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
) -> Result<(), String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.tab_manager
        .close_unpinned_tabs(profile_id)
        .map_err(|e| e.to_string())
}

// ==================== Bookmark Commands ====================

#[tauri::command(rename_all = "camelCase")]
async fn add_bookmark(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    title: String,
    url: String,
    folder_id: Option<i64>,
    tags: Vec<String>,
    notes: Option<String>,
    location: Option<String>,
) -> Result<Bookmark, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    // Default to the toolbar when the caller doesn't specify a location.
    let location = location.as_deref().unwrap_or("toolbar");
    state.bookmark_manager
        .add_bookmark(profile_id, &title, &url, folder_id, tags, notes.as_deref(), location)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn delete_bookmark(
    state: State<'_, Mutex<AppState>>,
    bookmark_id: i64,
) -> Result<(), String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.bookmark_manager
        .delete_bookmark(bookmark_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn get_all_bookmarks(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
) -> Result<Vec<Bookmark>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.bookmark_manager
        .get_all_bookmarks(profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn has_private_bookmarks_password(state: State<'_, Mutex<AppState>>, profile_id: i64) -> Result<bool, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    Ok(state.bookmark_manager.has_private_password(profile_id))
}

#[tauri::command(rename_all = "camelCase")]
async fn set_private_bookmarks_password(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    password: String,
) -> Result<(), String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.bookmark_manager.set_private_password(profile_id, &password)
}

#[tauri::command(rename_all = "camelCase")]
async fn verify_private_bookmarks_password(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    password: String,
) -> Result<bool, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    Ok(state.bookmark_manager.verify_private_password(profile_id, &password))
}

#[tauri::command(rename_all = "camelCase")]
async fn get_bookmarks_by_folder(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    folder_id: Option<i64>,
) -> Result<Vec<Bookmark>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.bookmark_manager
        .get_bookmarks_by_folder(profile_id, folder_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn search_bookmarks(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    query: String,
) -> Result<Vec<Bookmark>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.bookmark_manager
        .search_bookmarks(profile_id, &query)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn update_bookmark(
    state: State<'_, Mutex<AppState>>,
    bookmark_id: i64,
    title: Option<String>,
    url: Option<String>,
    folder_id: Option<Option<i64>>,
    tags: Option<Vec<String>>,
    notes: Option<Option<String>>,
    favicon: Option<String>,
    location: Option<String>,
) -> Result<Bookmark, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.bookmark_manager
        .update_bookmark(
            bookmark_id,
            title.as_deref(),
            url.as_deref(),
            folder_id,
            tags,
            notes.as_ref().map(|n| n.as_deref()),
            favicon.as_deref(),
            location.as_deref(),
        )
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn is_url_bookmarked(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    url: String,
) -> Result<Option<i64>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.bookmark_manager
        .is_bookmarked(profile_id, &url)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn create_bookmark_folder(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    name: String,
    parent_id: Option<i64>,
) -> Result<BookmarkFolder, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.bookmark_manager
        .create_folder(profile_id, &name, parent_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn get_bookmark_folders(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
) -> Result<Vec<BookmarkFolder>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.bookmark_manager
        .get_all_folders(profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn delete_bookmark_folder(
    state: State<'_, Mutex<AppState>>,
    folder_id: i64,
) -> Result<(), String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.bookmark_manager
        .delete_folder(folder_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn rename_bookmark_folder(
    state: State<'_, Mutex<AppState>>,
    folder_id: i64,
    name: String,
) -> Result<BookmarkFolder, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.bookmark_manager
        .rename_folder(folder_id, &name)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn export_bookmarks(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    format: String,
) -> Result<String, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    match format.as_str() {
        "html" => state.bookmark_manager
            .export_bookmarks_html(profile_id)
            .map_err(|e| e.to_string()),
        _ => state.bookmark_manager
            .export_bookmarks_json(profile_id)
            .map_err(|e| e.to_string()),
    }
}

#[tauri::command(rename_all = "camelCase")]
async fn import_bookmarks(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    data: String,
    format: String,
) -> Result<i32, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    match format.as_str() {
        "html" => state.bookmark_manager
            .import_bookmarks_html(profile_id, &data)
            .map_err(|e| e.to_string()),
        _ => state.bookmark_manager
            .import_bookmarks_json(profile_id, &data)
            .map_err(|e| e.to_string()),
    }
}

// ==================== Split View Commands ====================

#[tauri::command(rename_all = "camelCase")]
async fn get_split_config(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
) -> Result<SplitViewConfig, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.split_view_manager
        .get_config(profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn set_split_layout(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    layout: String,
) -> Result<SplitViewConfig, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.split_view_manager
        .set_layout(profile_id, &layout)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn set_pane_tab(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    pane_number: i32,
    tab_id: Option<i64>,
) -> Result<SplitViewConfig, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.split_view_manager
        .set_pane_tab(profile_id, pane_number, tab_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn set_active_pane(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    pane_number: i32,
) -> Result<SplitViewConfig, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.split_view_manager
        .set_active_pane(profile_id, pane_number)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn cycle_pane(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    direction: i32,
) -> Result<SplitViewConfig, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.split_view_manager
        .cycle_pane(profile_id, direction)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn update_pane_sizes(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    sizes: PaneSizes,
) -> Result<SplitViewConfig, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.split_view_manager
        .update_pane_sizes(profile_id, sizes)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn swap_panes(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    pane_a: i32,
    pane_b: i32,
) -> Result<SplitViewConfig, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.split_view_manager
        .swap_panes(profile_id, pane_a, pane_b)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn reset_split_view(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
) -> Result<SplitViewConfig, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.split_view_manager
        .reset_to_single(profile_id)
        .map_err(|e| e.to_string())
}

// ==================== EarthMultiMedia Commands ====================

#[tauri::command(rename_all = "camelCase")]
async fn get_media_privacy_settings(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
) -> Result<MediaPrivacySettings, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.multimedia_manager
        .get_privacy_settings(profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn update_media_privacy_settings(
    state: State<'_, Mutex<AppState>>,
    settings: MediaPrivacySettings,
) -> Result<MediaPrivacySettings, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.multimedia_manager
        .update_privacy_settings(&settings)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn set_media_password(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    password: String,
) -> Result<(), String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.multimedia_manager
        .set_password(profile_id, &password)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn verify_media_password(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    password: String,
) -> Result<bool, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.multimedia_manager
        .verify_password(profile_id, &password)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn generate_media_otp_secret(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
) -> Result<String, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.multimedia_manager
        .generate_otp_secret(profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn verify_media_otp(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    code: String,
) -> Result<bool, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.multimedia_manager
        .verify_otp(profile_id, &code)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn add_media_history_entry(
    state: State<'_, Mutex<AppState>>,
    entry: MediaHistoryEntry,
    password: Option<String>,
) -> Result<Option<MediaHistoryEntry>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.multimedia_manager
        .add_history_entry(&entry, password.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn get_media_history(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    limit: i32,
    password: Option<String>,
) -> Result<Vec<MediaHistoryEntry>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.multimedia_manager
        .get_history(profile_id, limit, password.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn clear_media_history(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
) -> Result<i32, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.multimedia_manager
        .clear_history(profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn delete_media_history_entry(
    state: State<'_, Mutex<AppState>>,
    entry_id: i64,
) -> Result<(), String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.multimedia_manager
        .delete_history_entry(entry_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn create_media_playlist(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    name: String,
    description: Option<String>,
    encrypted: bool,
) -> Result<Playlist, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.multimedia_manager
        .create_playlist(profile_id, &name, description.as_deref(), encrypted)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn get_media_playlists(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
) -> Result<Vec<Playlist>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.multimedia_manager
        .get_playlists(profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn delete_media_playlist(
    state: State<'_, Mutex<AppState>>,
    playlist_id: i64,
) -> Result<(), String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.multimedia_manager
        .delete_playlist(playlist_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn add_to_media_playlist(
    state: State<'_, Mutex<AppState>>,
    playlist_id: i64,
    source: String,
    media_type: String,
    title: Option<String>,
    thumbnail: Option<String>,
) -> Result<PlaylistItem, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.multimedia_manager
        .add_to_playlist(playlist_id, &source, &media_type, title.as_deref(), thumbnail.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn get_media_playlist_items(
    state: State<'_, Mutex<AppState>>,
    playlist_id: i64,
) -> Result<Vec<PlaylistItem>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.multimedia_manager
        .get_playlist_items(playlist_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn remove_from_media_playlist(
    state: State<'_, Mutex<AppState>>,
    item_id: i64,
) -> Result<(), String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.multimedia_manager
        .remove_from_playlist(item_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn reorder_media_playlist_items(
    state: State<'_, Mutex<AppState>>,
    playlist_id: i64,
    item_ids: Vec<i64>,
) -> Result<(), String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.multimedia_manager
        .reorder_playlist_items(playlist_id, item_ids)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn get_media_stats(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
) -> Result<MediaStats, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.multimedia_manager
        .get_stats(profile_id)
        .map_err(|e| e.to_string())
}

// ==================== Web Scraper Commands ====================

/// Run a scraping job in the background. The crawl updates the job's status
/// (running -> completed/failed) and pages_scraped in the DB; a
/// `scraping-jobs-changed` event is emitted at start and finish so the UI can
/// refresh. The manager is cloned out so we never hold the AppState lock across
/// the (long-running) crawl.
fn spawn_scraping_run(app: tauri::AppHandle, manager: scraper::ScraperManager, job_id: i64) {
    tauri::async_runtime::spawn(async move {
        use tauri::Emitter;
        let _ = app.emit("scraping-jobs-changed", ());
        if let Err(e) = manager.run_job(job_id).await {
            eprintln!("[scraper] job {job_id} failed: {e}");
            let pages = manager.get_job(job_id).map(|j| j.pages_scraped).unwrap_or(0);
            let _ = manager.update_job_status(job_id, "failed", pages);
        }
        let _ = app.emit("scraping-jobs-changed", ());
    });
}

#[tauri::command(rename_all = "camelCase")]
async fn create_scraping_job(
    app: tauri::AppHandle,
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    name: String,
    base_url: String,
    url_pattern: Option<String>,
    max_depth: i32,
    max_pages: i32,
    content_selectors: Vec<ContentSelector>,
    add_to_ai: Option<bool>,
) -> Result<i64, String> {
    let (job_id, manager) = {
        let state = state.lock().map_err(|e| e.to_string())?;
        let id = state.scraper_manager
            .create_job(
                profile_id,
                &name,
                &base_url,
                url_pattern.as_deref(),
                max_depth,
                max_pages,
                content_selectors,
                add_to_ai.unwrap_or(false),
            )
            .map_err(|e| e.to_string())?;
        (id, state.scraper_manager.clone())
    };
    // Start scraping immediately — a newly created job shouldn't sit "pending".
    spawn_scraping_run(app, manager, job_id);
    Ok(job_id)
}

/// (Re)run an existing scraping job.
#[tauri::command(rename_all = "camelCase")]
async fn run_scraping_job(
    app: tauri::AppHandle,
    state: State<'_, Mutex<AppState>>,
    job_id: i64,
) -> Result<(), String> {
    let manager = {
        let state = state.lock().map_err(|e| e.to_string())?;
        state.scraper_manager.clone()
    };
    spawn_scraping_run(app, manager, job_id);
    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
async fn get_scraping_jobs(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
) -> Result<Vec<ScrapingJob>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.scraper_manager
        .get_jobs(profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn get_scraping_job(
    state: State<'_, Mutex<AppState>>,
    job_id: i64,
) -> Result<ScrapingJob, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.scraper_manager
        .get_job(job_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn delete_scraping_job(
    state: State<'_, Mutex<AppState>>,
    job_id: i64,
) -> Result<(), String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.scraper_manager
        .delete_job(job_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn get_scraped_pages(
    state: State<'_, Mutex<AppState>>,
    job_id: i64,
    limit: i32,
) -> Result<Vec<ScrapedPage>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.scraper_manager
        .get_pages(job_id, limit)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn search_scraped_content(
    state: State<'_, Mutex<AppState>>,
    profile_id: i64,
    query: String,
    limit: i32,
) -> Result<Vec<ScrapedPage>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.scraper_manager
        .search_content(profile_id, &query, limit)
        .map_err(|e| e.to_string())
}

// ==================== Window Management Commands ====================

/// Create a new window with a specific tab (for tab drag-out functionality)
#[tauri::command(rename_all = "camelCase")]
async fn create_detached_window(
    app: tauri::AppHandle,
    tab_id: i64,
    url: String,
    title: String,
    x: Option<i32>,
    y: Option<i32>,
) -> Result<String, String> {
    let window_id = format!("reclaim-{}", tab_id);

    // Build the window with the tab data passed via URL fragment
    let window_url = format!("{}#tab={}", url, tab_id);

    let builder = WebviewWindowBuilder::new(
        &app,
        &window_id,
        WebviewUrl::App(window_url.parse().map_err(|e: std::convert::Infallible| e.to_string())?),
    )
    .title(&title)
    .inner_size(1280.0, 720.0)
    .min_inner_size(800.0, 600.0)
    .decorations(false)
    .resizable(true)
    .transparent(false);

    // Set position if provided (for drag-out to specific location)
    let builder = if let (Some(x), Some(y)) = (x, y) {
        builder.position(x as f64, y as f64)
    } else {
        builder
    };

    builder
        .build()
        .map_err(|e| e.to_string())?;

    Ok(window_id)
}

/// Close a specific window by its label
#[tauri::command(rename_all = "camelCase")]
async fn close_window_by_label(app: tauri::AppHandle, label: String) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(&label) {
        window.close().map_err(|e| e.to_string())
    } else {
        Err(format!("Window not found: {}", label))
    }
}

/// Get all open window labels
#[tauri::command(rename_all = "camelCase")]
async fn get_all_windows(app: tauri::AppHandle) -> Vec<String> {
    app.webview_windows()
        .keys()
        .cloned()
        .collect()
}

/// Toggle developer tools for the main window (opens in separate window on Linux)
#[tauri::command(rename_all = "camelCase")]
async fn toggle_devtools(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        // Use tracked state since is_devtools_open() is unreliable on WebKitGTK after detach
        let is_open = webview::is_devtools_open_tracked();

        if is_open {
            window.close_devtools();
            webview::set_devtools_open(false);
        } else {
            window.open_devtools();
            webview::set_devtools_open(true);
        }
        Ok(())
    } else {
        Err("Main window not found".to_string())
    }
}

/// Toggle fullscreen mode for the main window
#[tauri::command(rename_all = "camelCase")]
async fn toggle_fullscreen(app: tauri::AppHandle) -> Result<bool, String> {
    if let Some(window) = app.get_webview_window("main") {
        let is_fullscreen = window.is_fullscreen().map_err(|e| e.to_string())?;
        window.set_fullscreen(!is_fullscreen).map_err(|e| e.to_string())?;
        Ok(!is_fullscreen)
    } else {
        Err("Main window not found".to_string())
    }
}

// ==================== Legacy Commands (for compatibility) ====================

#[tauri::command(rename_all = "camelCase")]
fn greet(name: &str) -> String {
    format!("Hello, {}! Welcome to EarthServers Local.", name)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateInfo {
    current: String,
    latest: String,
    update_available: bool,
    url: String,
    notes: String,
}

/// Compare dotted versions numerically (ignores any pre-release suffix).
fn version_is_newer(latest: &str, current: &str) -> bool {
    fn parts(s: &str) -> Vec<u64> {
        s.trim_start_matches('v')
            .split('.')
            .map(|p| {
                p.chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse()
                    .unwrap_or(0)
            })
            .collect()
    }
    let (l, c) = (parts(latest), parts(current));
    for i in 0..l.len().max(c.len()) {
        let (lv, cv) = (l.get(i).copied().unwrap_or(0), c.get(i).copied().unwrap_or(0));
        if lv != cv {
            return lv > cv;
        }
    }
    false
}

/// Check the GitHub releases for a newer version. Returns the current/latest
/// versions, whether an update is available, and the release page URL.
#[tauri::command(rename_all = "camelCase")]
async fn check_for_update() -> Result<UpdateInfo, String> {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let none = UpdateInfo {
        current: current.clone(),
        latest: current.clone(),
        update_available: false,
        url: String::new(),
        notes: String::new(),
    };
    let client = reqwest::Client::new();
    let resp = match client
        .get("https://api.github.com/repos/earthservers/earthservers-reclaim/releases/latest")
        .header("User-Agent", "Reclaim")
        .header("Accept", "application/vnd.github+json")
        .timeout(std::time::Duration::from_secs(8))
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => r,
        _ => return Ok(none), // no releases yet / offline — treat as up to date
    };
    let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let latest = v["tag_name"].as_str().unwrap_or("").trim_start_matches('v').to_string();
    let url = v["html_url"].as_str().unwrap_or("").to_string();
    let notes = v["body"].as_str().unwrap_or("").to_string();
    if latest.is_empty() {
        return Ok(none);
    }
    let update_available = version_is_newer(&latest, &current);
    Ok(UpdateInfo { current, latest, update_available, url, notes })
}

#[tauri::command(rename_all = "camelCase")]
async fn search_domains(query: String) -> Result<String, String> {
    Ok(format!("Searching for: {}", query))
}

#[tauri::command(rename_all = "camelCase")]
async fn add_domain(domain: String) -> Result<String, String> {
    Ok(format!("Added domain: {}", domain))
}

#[tauri::command(rename_all = "camelCase")]
async fn query_knowledge_graph(query: String) -> Result<String, String> {
    Ok(format!("Knowledge graph query: {}", query))
}

/// Locate the built NoScript web-extension `.so`. Search order:
///   1. `EARTH_NOSCRIPT_EXT` (full path override)
///   2. the bundled resource dir — this is where it lives in an INSTALLED build
///      (.rpm/.deb/AppImage); bundled via `bundle.resources` in tauri.conf.json
///   3. next to the executable
///   4. the dev source tree (the standalone crate's own target dir)
/// Returns None if not built/bundled.
#[cfg(target_os = "linux")]
fn locate_noscript_so(resource_dir: Option<&std::path::Path>) -> Option<std::path::PathBuf> {
    const SO: &str = "libearth_noscript_ext.so";

    if let Ok(p) = std::env::var("EARTH_NOSCRIPT_EXT") {
        let p = std::path::PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }

    // Installed build: bundled as a resource. Tauri's exact layout can vary, so
    // check the common sub-paths rather than assuming one.
    if let Some(res) = resource_dir {
        for rel in [
            format!("noscript/{SO}"),
            format!("resources/noscript/{SO}"),
            SO.to_string(),
        ] {
            let cand = res.join(rel);
            if cand.exists() {
                return Some(cand);
            }
        }
    }

    // Alongside the executable (e.g. /usr/lib/<app>/).
    if let Some(exe_dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())) {
        let cand = exe_dir.join(SO);
        if cand.exists() {
            return Some(cand);
        }
    }

    // Dev: walk up from the executable to the standalone crate's target dir.
    let exe = std::env::current_exe().ok()?;
    let mut dir: Option<&std::path::Path> = exe.parent();
    while let Some(d) = dir {
        for profile in ["debug", "release"] {
            let cand = d
                .join("crates/earth-noscript-ext/target")
                .join(profile)
                .join(SO);
            if cand.exists() {
                return Some(cand);
            }
        }
        dir = d.parent();
    }
    None
}

#[cfg(not(target_os = "linux"))]
fn locate_noscript_so(_resource_dir: Option<&std::path::Path>) -> Option<std::path::PathBuf> {
    None
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Note: GPU environment variables are set in main.rs BEFORE this function
    // to ensure they're set before any GTK/WebKit initialization

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            // Initialize database directory
            let app_dir = app.path().app_data_dir()
                .expect("Failed to get app data directory");

            println!("App data directory: {:?}", app_dir);

            std::fs::create_dir_all(&app_dir)
                .expect("Failed to create app data directory");

            // Set up database path
            let db_path = app_dir.join("earthservers.db");
            let db_path_str = db_path.to_string_lossy().to_string();

            // Initialize managers
            let profile_manager = ProfileManager::new(db_path_str.clone());
            let privacy_manager = PrivacyManager::new(db_path_str.clone());
            let knowledge_graph = KnowledgeGraph::new(db_path_str.clone());
            let theme_manager = ThemeManager::new(db_path_str.clone());
            let search_manager = SearchManager::new(db_path_str.clone());
            let memory_manager = MemoryManager::new(db_path_str.clone());
            let rating_manager = RatingManager::new(db_path_str.clone());
            let tab_manager = TabManager::new(db_path_str.clone());
            let bookmark_manager = BookmarkManager::new(db_path_str.clone());
            let split_view_manager = SplitViewManager::new(db_path_str.clone());
            let multimedia_manager = MultimediaManager::new(db_path_str.clone());
            let scraper_manager = ScraperManager::new(db_path_str.clone());
            let vault_manager = vault::VaultManager::new(db_path_str.clone());

            // Initialize database tables
            profile_manager.init().expect("Failed to initialize profile tables");
            // Restore persisted per-profile incognito flags and force the dedicated
            // Incognito profile permanently on. Must run after profile tables exist.
            PrivacyManager::init_incognito_persistence(&db_path_str);
            knowledge_graph.init().expect("Failed to initialize knowledge graph");
            theme_manager.init().expect("Failed to initialize theme tables");
            search_manager.init().expect("Failed to initialize search tables");
            memory_manager.init().expect("Failed to initialize memory tables");
            if let Err(e) = vault_manager.init() {
                log::error!("Failed to initialize vault tables: {}", e);
            }
            if let Err(e) = media_downloads::init(&db_path_str) {
                log::error!("Failed to initialize media_downloads table: {}", e);
            }

            // Seed default domains and bookmarks for the active profile
            if let Ok(Some(active_profile)) = profile_manager.get_active_profile() {
                let profile_id = active_profile.id.unwrap_or(1);
                println!("Active profile ID: {}", profile_id);

                // Get resource directory and seed default domains
                match app.path().resource_dir() {
                    Ok(resource_dir) => {
                        println!("Resource directory: {:?}", resource_dir);
                        let domain_lists_path = resource_dir.join("domain-lists");
                        println!("Domain lists path: {:?}, exists: {}", domain_lists_path, domain_lists_path.exists());

                        match search_manager.seed_default_domains(profile_id, &resource_dir) {
                            Ok(imported) => {
                                if imported > 0 {
                                    println!("Seeded {} default domains", imported);
                                } else {
                                    println!("No domains seeded (may already exist or no .earth files found)");
                                }
                            }
                            Err(e) => println!("Error seeding domains: {:?}", e),
                        }
                    }
                    Err(e) => println!("Error getting resource directory: {:?}", e),
                }

                // Seed default bookmarks
                if let Ok(seeded) = bookmark_manager.seed_default_bookmarks(profile_id) {
                    if seeded > 0 {
                        println!("Seeded {} default bookmarks", seeded);
                    }
                }
            }

            // Store state
            let state = AppState {
                db_path: db_path_str,
                profile_manager,
                privacy_manager,
                knowledge_graph,
                theme_manager,
                search_manager,
                memory_manager,
                rating_manager,
                tab_manager,
                bookmark_manager,
                split_view_manager,
                multimedia_manager,
                scraper_manager,
                vault_manager,
            };

            app.manage(Mutex::new(state));

            // Browser router: single navigation front door (resolution + render
            // axes). Managed separately from AppState so navigation never
            // contends on the DB lock. Phase 1: empty resolver chain + WebKitGTK.
            app.manage(router::Router::new(db_path.to_string_lossy().into_owned()));

            // NoScript: load the encrypted per-site JS allowlist into memory.
            browser_surface::init_js_policy(db_path.to_string_lossy().into_owned());

            // NoScript per-script engine: stage the WebKit web-process extension
            // (.so) into a clean directory and point the browser surface at it.
            // The extension observes (Phase 1) per-origin requests in the web
            // process. In dev it's found in the crate's target dir; in an installed
            // build it's bundled as a resource (see bundle.resources + the
            // build:noscript step).
            let res_dir = app.path().resource_dir().ok();
            if let Some(so) = locate_noscript_so(res_dir.as_deref()) {
                let ext_dir = app_dir.join("web-extensions");
                let _ = std::fs::create_dir_all(&ext_dir);
                let dest = ext_dir.join("libearth_noscript_ext.so");
                match std::fs::copy(&so, &dest) {
                    Ok(_) => {
                        browser_surface::set_noscript_ext_dir(ext_dir.to_string_lossy().into_owned());
                        eprintln!("[noscript] extension staged: {:?}", dest);
                    }
                    Err(e) => eprintln!("[noscript] failed to stage extension: {}", e),
                }
            } else {
                eprintln!("[noscript] extension .so NOT found; build crates/earth-noscript-ext");
            }

            // Initialize GStreamer media player states
            app.manage(media_player::MediaPlayerState::new());
            app.manage(media_player::MediaPlayerManagerState::new());

            // Start WebSocket server for media controls communication
            // This enables real-time bidirectional communication with floating controls
            tauri::async_runtime::spawn(async {
                if let Err(e) = controls_server::init_controls_server().await {
                    log::error!("Failed to start controls WebSocket server: {}", e);
                } else {
                    // Set up command handler to forward commands to media player
                    controls_server::set_controls_command_handler(|cmd| {
                        use controls_server::ControlCommand;

                        // Get the default player ID if not specified
                        let get_player_id = |id: Option<String>| id.unwrap_or_else(|| "pane-0".to_string());

                        tauri::async_runtime::spawn(async move {
                            match cmd {
                                ControlCommand::Play { player_id } => {
                                    let _ = media_player::player_play_internal(&get_player_id(player_id)).await;
                                }
                                ControlCommand::Pause { player_id } => {
                                    let _ = media_player::player_pause_internal(&get_player_id(player_id)).await;
                                }
                                ControlCommand::Stop { player_id } => {
                                    let _ = media_player::player_stop_internal(&get_player_id(player_id)).await;
                                }
                                ControlCommand::TogglePlay { player_id } => {
                                    let pid = get_player_id(player_id);
                                    if let Ok(status) = media_player::player_get_status_internal(&pid).await {
                                        if matches!(status.state, earth_media::PlaybackState::Playing) {
                                            let _ = media_player::player_pause_internal(&pid).await;
                                        } else {
                                            let _ = media_player::player_play_internal(&pid).await;
                                        }
                                    }
                                }
                                ControlCommand::Seek { player_id, position_ms } => {
                                    let _ = media_player::player_seek_internal(&get_player_id(player_id), position_ms).await;
                                }
                                ControlCommand::SetVolume { player_id, volume } => {
                                    let _ = media_player::player_set_volume_internal(&get_player_id(player_id), volume).await;
                                }
                                ControlCommand::ToggleMute { player_id } => {
                                    let pid = get_player_id(player_id);
                                    if let Ok(status) = media_player::player_get_status_internal(&pid).await {
                                        let _ = media_player::player_set_muted_internal(&pid, !status.muted).await;
                                    }
                                }
                                ControlCommand::SkipForward { player_id, seconds } => {
                                    let _ = media_player::player_skip_forward_internal(&get_player_id(player_id), seconds).await;
                                }
                                ControlCommand::SkipBackward { player_id, seconds } => {
                                    let _ = media_player::player_skip_backward_internal(&get_player_id(player_id), seconds).await;
                                }
                                ControlCommand::GetStatus => {
                                    // Status is automatically sent on connect
                                }
                                ControlCommand::MoveWindow { delta_x, delta_y } => {
                                    // Move the X11 webview controls window
                                    if let Err(e) = video_surface::move_x11_webview_controls_by_delta(delta_x, delta_y) {
                                        log::warn!("Failed to move controls window: {}", e);
                                    }
                                }
                                ControlCommand::ResizeWindow { width, height } => {
                                    // Resize the controls window to fit collapsed/expanded content.
                                    if let Err(e) = video_surface::resize_x11_webview_controls(width, height).await {
                                        log::warn!("Failed to resize controls window: {}", e);
                                    }
                                }
                                ControlCommand::ToggleShuffle { player_id: _ } => {
                                    // TODO: Implement shuffle toggle when playlist system is ready
                                    log::info!("Shuffle toggle requested (not yet implemented)");
                                }
                                ControlCommand::ToggleRepeat { player_id: _ } => {
                                    // TODO: Implement repeat toggle when playlist system is ready
                                    log::info!("Repeat toggle requested (not yet implemented)");
                                }
                                ControlCommand::TogglePlaylist => {
                                    // TODO: Emit event to toggle playlist panel in frontend
                                    log::info!("Playlist toggle requested (not yet implemented)");
                                }
                            }
                        });
                    }).await;

                    log::info!("Controls WebSocket server started on ws://127.0.0.1:9876");

                    // Start status broadcast loop - push player status every 250ms
                    tokio::spawn(async {
                        use std::time::Duration;
                        use controls_server::{broadcast_player_status, PlayerStatus};

                        loop {
                            // The focused pane is set from the frontend; read it
                            // each tick so the controls follow the active pane.
                            let active_player_id = controls_server::get_active_player_id();
                            // Get status from the active player
                            if let Ok(status) = media_player::player_get_status_internal(&active_player_id).await {
                                broadcast_player_status(PlayerStatus {
                                    player_id: active_player_id.clone(),
                                    is_playing: matches!(status.state, earth_media::PlaybackState::Playing),
                                    current_time: status.position_ms,
                                    duration: status.duration_ms,
                                    volume: status.volume,
                                    is_muted: status.muted,
                                    title: status.info.title.clone().unwrap_or_default(),
                                    is_shuffled: false, // TODO: Get from playlist state when implemented
                                    repeat_mode: "none".to_string(), // TODO: Get from playlist state when implemented
                                });
                            }

                            tokio::time::sleep(Duration::from_millis(250)).await;
                        }
                    });
                }
            });

            // Privacy-first: Log that incognito mode is enabled
            // The WebView is configured with incognito: true in tauri.conf.json
            // This ensures:
            // - No disk cache (memory-only caching)
            // - No persistent cookies
            // - No localStorage persistence
            // - All data cleared on exit
            println!("Privacy mode enabled: WebView running in incognito mode (no disk cache)");

            // NOTE: Don't auto-open devtools - it interferes with the GTK VBox layout
            // DevTools can be opened manually with F12

            // Set up system tray with right-click menu
            let quit_item = MenuItem::with_id(app, "quit", "Quit Reclaim", true, None::<&str>)?;
            let new_window_item = MenuItem::with_id(app, "new_window", "New Window", true, None::<&str>)?;
            let show_item = MenuItem::with_id(app, "show", "Show Reclaim", true, None::<&str>)?;
            let hide_item = MenuItem::with_id(app, "hide", "Hide to Tray", true, None::<&str>)?;

            let tray_menu = Menu::with_items(app, &[
                &new_window_item,
                &show_item,
                &hide_item,
                &quit_item,
            ])?;

            let app_handle = app.handle().clone();
            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().cloned().expect("No icon found"))
                .menu(&tray_menu)
                .on_menu_event(move |app, event| {
                    match event.id.as_ref() {
                        "quit" => {
                            app.exit(0);
                        }
                        "new_window" => {
                            let window_id = format!("reclaim-{}", std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_millis());
                            let _ = WebviewWindowBuilder::new(
                                app,
                                &window_id,
                                WebviewUrl::App("index.html".into()),
                            )
                            .title("Reclaim")
                            .inner_size(1280.0, 720.0)
                            .min_inner_size(800.0, 600.0)
                            .decorations(false)
                            .transparent(false)
                            .build();
                        }
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        "hide" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.hide();
                            }
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(move |_tray, event| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                        // Left click shows the main window
                        if let Some(window) = app_handle.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .tooltip("Reclaim Browser")
                .build(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Profile commands
            get_profiles,
            get_active_profile,
            create_profile,
            switch_profile,
            update_profile,
            delete_profile,
            wipe_profile,
            profile_is_protected,
            profile_has_delete_pin,
            set_profile_delete_pin,
            get_privacy_settings,
            update_privacy_settings,
            export_profile,
            // Per-profile incognito commands
            get_incognito_status,
            toggle_incognito,
            set_incognito,
            get_incognito_profiles,
            incognito_is_forced,
            // Identity commands (hardware fingerprinting)
            get_hardware_info,
            get_device_fingerprint,
            // History commands
            get_history,
            delete_history_entry,
            delete_history_by_date_range,
            clear_all_history,
            get_history_stats,
            export_history,
            // Knowledge graph commands
            add_page,
            search_knowledge_graph,
            // Theme commands
            get_themes,
            get_active_theme,
            save_theme,
            set_active_theme,
            delete_theme,
            apply_preset_theme,
            get_theme_presets,
            export_theme,
            // Domain commands (EarthSearch)
            get_domains,
            add_domain_entry,
            update_domain,
            delete_domain_entry,
            search_domain_list,
            get_domain_lists,
            create_domain_list,
            delete_domain_list,
            get_domain_stats,
            get_domain_categories,
            export_domains,
            import_domains,
            // Memory commands (EarthMemory)
            get_indexed_pages,
            index_page,
            curate_page,
            curate_viewed_page,
            search_memory,
            get_favorite_pages,
            toggle_page_favorite,
            update_page_tags,
            delete_indexed_page,
            add_page_note,
            get_page_notes,
            update_page_note,
            delete_page_note,
            get_memory_stats,
            get_memory_tags,
            export_memory,
            import_memory,
            // Rating commands
            submit_rating,
            get_user_rating,
            get_domain_ratings,
            delete_rating,
            get_rating_aggregate,
            get_rating_summary,
            submit_subdomain_rating,
            get_subdomain_ratings,
            mark_rating_helpful,
            report_rating,
            get_user_rating_history,
            add_rating_category_scores,
            // Domain seeding
            seed_default_domains,
            force_reseed_domains,
            // Tab commands
            create_tab,
            close_tab,
            get_all_tabs,
            update_tab,
            reorder_tabs,
            pin_tab,
            set_active_tab,
            get_tab_history,
            navigate_tab_back,
            navigate_tab_forward,
            duplicate_tab,
            close_tabs_to_right,
            close_unpinned_tabs,
            // Bookmark commands
            add_bookmark,
            delete_bookmark,
            get_all_bookmarks,
            has_private_bookmarks_password,
            set_private_bookmarks_password,
            verify_private_bookmarks_password,
            get_bookmarks_by_folder,
            search_bookmarks,
            update_bookmark,
            is_url_bookmarked,
            create_bookmark_folder,
            get_bookmark_folders,
            delete_bookmark_folder,
            rename_bookmark_folder,
            export_bookmarks,
            import_bookmarks,
            // Split view commands
            get_split_config,
            set_split_layout,
            set_pane_tab,
            set_active_pane,
            cycle_pane,
            update_pane_sizes,
            swap_panes,
            reset_split_view,
            // EarthMultiMedia commands
            get_media_privacy_settings,
            update_media_privacy_settings,
            set_media_password,
            verify_media_password,
            generate_media_otp_secret,
            verify_media_otp,
            add_media_history_entry,
            get_media_history,
            clear_media_history,
            delete_media_history_entry,
            create_media_playlist,
            get_media_playlists,
            delete_media_playlist,
            add_to_media_playlist,
            get_media_playlist_items,
            remove_from_media_playlist,
            reorder_media_playlist_items,
            get_media_stats,
            // Web Scraper commands
            check_for_update,
            assistant::assistant_status,
            assistant::assistant_models,
            assistant::assistant_chat,
            assistant::assistant_chat_stream,
            assistant::assistant_research_stream,
            research::web_search,
            research::fetch_url,
            research::research_status,
            ai_lock::ai_lock_has_password,
            ai_lock::ai_lock_verify_password,
            ai_lock::ai_lock_set_password,
            ai_lock::ai_lock_remove_password,
            media_downloads::download_media,
            media_downloads::list_media_downloads,
            media_downloads::ytdlp_available,
            media_downloads::download_video_ytdlp,
            browser_surface::browser_collect_media,
            create_scraping_job,
            run_scraping_job,
            get_scraping_jobs,
            get_scraping_job,
            delete_scraping_job,
            get_scraped_pages,
            search_scraped_content,
            // Window management commands
            create_detached_window,
            close_window_by_label,
            get_all_windows,
            toggle_devtools,
            toggle_fullscreen,
            // Password Manager + OTP Authenticator vaults
            vault::has_password_manager_master,
            vault::verify_password_manager_master,
            vault::set_password_manager_master,
            vault::lock_password_manager,
            vault::lock_all_vaults,
            vault::vault_has_app_password,
            vault::vault_get_app_password,
            vault::vault_set_app_password,
            vault::change_password_manager_master,
            vault::change_otp_master,
            vault::get_password_entries,
            vault::add_password_entry,
            vault::update_password_entry,
            vault::delete_password_entry,
            vault::has_otp_master,
            vault::verify_otp_master,
            vault::set_otp_master,
            vault::lock_otp,
            vault::get_otp_entries,
            vault::add_otp_entry,
            vault::update_otp_entry,
            vault::delete_otp_entry,
            vault::vault_find_login,
            vault::vault_login_hint,
            vault::vault_autofill,
            vault::vault_autosave_is_new,
            vault::vault_autosave_confirm,
            vault::vault_autosave_dismiss,
            // Router — single navigation front door (resolution + render axes)
            router::navigate,
            router::router_seed_cache,
            router::router_clear_cache,
            // Embedded browser surface (GTK/X11 reparented WebKitGTK) controls
            browser_surface::open_download,
            browser_surface::open_download_location,
            browser_surface::browser_surface_set_bounds,
            browser_surface::browser_surface_show,
            browser_surface::browser_surface_hide,
            browser_surface::browser_surface_back,
            browser_surface::browser_surface_forward,
            browser_surface::browser_surface_reload,
            browser_surface::browser_surface_destroy,
            browser_surface::browser_surface_set_js,
            browser_surface::browser_surface_get_js,
            browser_surface::browser_surface_list_js,
            browser_surface::noscript_list_origins,
            browser_surface::noscript_get_trust,
            browser_surface::noscript_set_trust,
            browser_surface::privacy_get_config,
            browser_surface::privacy_set_config,
            // Webview commands - single webview navigation pattern
            webview::navigate_main_window,
            webview::navigate_to_app,
            webview::is_external_browsing,
            webview::get_current_external_url,
            webview::execute_js_in_main,
            webview::reload_main_window,
            webview::main_window_go_back,
            webview::main_window_go_forward,
            webview::open_in_system_browser,
            webview::toggle_browser_devtools,
            webview::open_browser_devtools,
            webview::close_browser_devtools,
            // Legacy webview commands (for compatibility)
            webview::create_browser_webview,
            webview::browser_navigate,
            webview::switch_tab_webview,
            webview::hide_browser_webview,
            webview::show_browser_webview,
            webview::update_browser_bounds,
            webview::has_browser_webview,
            webview::browser_go_back,
            webview::browser_go_forward,
            webview::browser_reload,
            webview::browser_get_url,
            webview::browser_execute_js,
            webview::detach_browser_to_window,
            webview::close_browser_webview,
            webview::clear_tab_state,
            // Legacy webview commands (redirect to single webview)
            webview::create_tab_webview,
            webview::webview_navigate,
            webview::show_tab_webview,
            webview::hide_tab_webview,
            webview::close_tab_webview,
            webview::update_webview_bounds,
            webview::webview_go_back,
            webview::webview_go_forward,
            webview::webview_reload,
            webview::webview_get_html,
            webview::webview_get_url,
            webview::webview_get_title,
            webview::webview_execute_js,
            webview::has_tab_webview,
            webview::is_webview_embedded,
            webview::detach_tab_webview,
            // Servo browser commands (process-based)
            servo_browser::create_servo_browser,
            servo_browser::servo_navigate,
            servo_browser::servo_close,
            servo_browser::servo_close_all,
            servo_browser::servo_get_url,
            servo_browser::servo_has_webview,
            servo_browser::servo_list_webviews,
            // GStreamer media player commands
            media_player::media_load,
            media_player::media_play,
            media_player::media_pause,
            media_player::media_stop,
            media_player::media_seek,
            media_player::media_set_volume,
            media_player::media_get_status,
            media_player::media_skip_forward,
            media_player::media_skip_backward,
            media_player::media_check_gstreamer,
            media_player::media_set_muted,
            // YouTube commands
            media_player::play_youtube,
            media_player::get_youtube_info,
            media_player::check_youtube_available,
            media_player::is_youtube_url,
            // Multi-player commands (for multi-pane support)
            media_player::player_load,
            media_player::player_play,
            media_player::player_pause,
            media_player::player_stop,
            media_player::player_seek,
            media_player::player_set_volume,
            media_player::player_set_muted,
            media_player::player_get_status,
            media_player::player_skip_forward,
            media_player::player_skip_backward,
            media_player::player_play_youtube,
            media_player::player_remove,
            media_player::player_get_all_statuses,
            media_player::player_stop_all,
            media_player::player_list,
            media_player::player_set_window_handle,
            media_player::player_expose,
            media_player::set_active_media_player,
            media_player::get_window_xid,
            // Video surface for embedded playback
            video_surface::create_video_surface,
            video_surface::update_video_surface,
            video_surface::show_video_surface,
            video_surface::hide_video_surface,
            video_surface::destroy_video_surface,
            video_surface::get_video_surface_xid,
            // Controls overlay for embedded playback (GTK version)
            video_surface::create_controls_overlay,
            video_surface::update_controls_overlay,
            video_surface::show_controls_overlay,
            video_surface::hide_controls_overlay,
            video_surface::destroy_controls_overlay,
            // Floating webview controls window
            video_surface::create_floating_controls_window,
            video_surface::update_floating_controls_window,
            video_surface::show_floating_controls_window,
            video_surface::hide_floating_controls_window,
            video_surface::destroy_floating_controls_window,
            // X11 window with WebKitGTK webview for HTML controls
            video_surface::create_x11_webview_controls,
            video_surface::update_x11_webview_controls,
            video_surface::show_x11_webview_controls,
            video_surface::resize_x11_webview_controls,
            video_surface::hide_x11_webview_controls,
            video_surface::destroy_x11_webview_controls,
            // Legacy commands
            greet,
            search_domains,
            add_domain,
            query_knowledge_graph
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
