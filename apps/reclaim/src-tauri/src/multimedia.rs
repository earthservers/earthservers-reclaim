// EarthMultiMedia - Privacy-focused media player
// Supports video, image, and audio with optional encrypted history

use rusqlite::{Connection, Result, params};
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use rand::Rng;
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

// ==================== Types ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MediaType {
    #[serde(rename = "video")]
    Video,
    #[serde(rename = "image")]
    Image,
    #[serde(rename = "audio")]
    Audio,
}

impl From<&str> for MediaType {
    fn from(s: &str) -> Self {
        match s {
            "image" => MediaType::Image,
            "audio" => MediaType::Audio,
            _ => MediaType::Video,
        }
    }
}

impl ToString for MediaType {
    fn to_string(&self) -> String {
        match self {
            MediaType::Video => "video".to_string(),
            MediaType::Image => "image".to_string(),
            MediaType::Audio => "audio".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaItem {
    pub id: Option<i64>,
    pub profile_id: i64,
    pub media_type: MediaType,
    pub source: String,           // File path or URL
    pub title: Option<String>,
    pub thumbnail: Option<String>,
    pub duration: Option<i64>,    // In seconds for video/audio
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub file_size: Option<i64>,
    pub metadata: Option<String>, // JSON metadata
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaHistoryEntry {
    pub id: Option<i64>,
    pub profile_id: i64,
    pub media_id: Option<i64>,
    pub source: String,
    pub media_type: MediaType,
    pub title: Option<String>,
    pub thumbnail: Option<String>,
    pub position: i64,            // Playback position in seconds
    pub duration: Option<i64>,
    pub played_at: String,
    pub encrypted: bool,          // If true, data is encrypted
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    pub id: Option<i64>,
    pub profile_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub thumbnail: Option<String>,
    pub is_encrypted: bool,
    pub created_at: String,
    pub updated_at: Option<String>,
    pub item_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistItem {
    pub id: Option<i64>,
    pub playlist_id: i64,
    pub source: String,
    pub media_type: MediaType,
    pub title: Option<String>,
    pub thumbnail: Option<String>,
    pub position: i32,
    pub added_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacySettings {
    pub profile_id: i64,
    pub history_enabled: bool,           // Default: false (privacy-first)
    pub playlist_history_enabled: bool,  // Default: false
    pub require_password: bool,          // Require password to access history
    pub require_otp: bool,               // Require OTP for sensitive actions
    pub password_hash: Option<String>,   // Hashed password
    pub otp_secret: Option<String>,      // TOTP secret (encrypted)
    pub auto_clear_history_days: Option<i32>, // Auto-clear after N days
}

impl Default for PrivacySettings {
    fn default() -> Self {
        PrivacySettings {
            profile_id: 1,
            history_enabled: false,        // Privacy-first: no history by default
            playlist_history_enabled: false,
            require_password: false,
            require_otp: false,
            password_hash: None,
            otp_secret: None,
            auto_clear_history_days: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaStats {
    pub total_played: i32,
    pub total_time_watched: i64,  // In seconds
    pub videos_watched: i32,
    pub images_viewed: i32,
    pub audio_played: i32,
    pub playlists_count: i32,
}

// ==================== Manager ====================

pub struct MultimediaManager {
    db_path: String,
}

impl MultimediaManager {
    pub fn new(db_path: String) -> Self {
        MultimediaManager { db_path }
    }

    // ==================== Privacy Settings ====================

    /// Get privacy settings for a profile
    pub fn get_privacy_settings(&self, profile_id: i64) -> Result<PrivacySettings> {
        let conn = Connection::open(&self.db_path)?;

        let result = conn.query_row(
            "SELECT profile_id, history_enabled, playlist_history_enabled, require_password,
                    require_otp, password_hash, otp_secret, auto_clear_history_days
             FROM multimedia_privacy WHERE profile_id = ?1",
            params![profile_id],
            |row| {
                Ok(PrivacySettings {
                    profile_id: row.get(0)?,
                    history_enabled: row.get(1)?,
                    playlist_history_enabled: row.get(2)?,
                    require_password: row.get(3)?,
                    require_otp: row.get(4)?,
                    password_hash: row.get(5)?,
                    otp_secret: row.get(6)?,
                    auto_clear_history_days: row.get(7)?,
                })
            },
        );

        match result {
            Ok(settings) => Ok(settings),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                // Create default settings (privacy-first)
                self.create_default_privacy_settings(profile_id)
            }
            Err(e) => Err(e),
        }
    }

    /// Create default privacy settings
    fn create_default_privacy_settings(&self, profile_id: i64) -> Result<PrivacySettings> {
        let conn = Connection::open(&self.db_path)?;

        conn.execute(
            "INSERT INTO multimedia_privacy (profile_id, history_enabled, playlist_history_enabled,
                require_password, require_otp)
             VALUES (?1, 0, 0, 0, 0)",
            params![profile_id],
        )?;

        Ok(PrivacySettings {
            profile_id,
            ..Default::default()
        })
    }

    /// Update privacy settings
    pub fn update_privacy_settings(&self, settings: &PrivacySettings) -> Result<PrivacySettings> {
        let conn = Connection::open(&self.db_path)?;

        // Ensure settings exist
        let _ = self.get_privacy_settings(settings.profile_id)?;

        conn.execute(
            "UPDATE multimedia_privacy SET
                history_enabled = ?1,
                playlist_history_enabled = ?2,
                require_password = ?3,
                require_otp = ?4,
                auto_clear_history_days = ?5
             WHERE profile_id = ?6",
            params![
                settings.history_enabled,
                settings.playlist_history_enabled,
                settings.require_password,
                settings.require_otp,
                settings.auto_clear_history_days,
                settings.profile_id
            ],
        )?;

        self.get_privacy_settings(settings.profile_id)
    }

    /// Set password for media history access
    pub fn set_password(&self, profile_id: i64, password: &str) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;

        // Hash password with SHA256
        let mut hasher = Sha256::new();
        hasher.update(password.as_bytes());
        let hash = format!("{:x}", hasher.finalize());

        conn.execute(
            "UPDATE multimedia_privacy SET password_hash = ?1, require_password = 1 WHERE profile_id = ?2",
            params![hash, profile_id],
        )?;

        Ok(())
    }

    /// Verify password
    pub fn verify_password(&self, profile_id: i64, password: &str) -> Result<bool> {
        let settings = self.get_privacy_settings(profile_id)?;

        if let Some(stored_hash) = settings.password_hash {
            let mut hasher = Sha256::new();
            hasher.update(password.as_bytes());
            let hash = format!("{:x}", hasher.finalize());
            Ok(hash == stored_hash)
        } else {
            Ok(true) // No password set
        }
    }

    /// Generate OTP secret for TOTP
    pub fn generate_otp_secret(&self, profile_id: i64) -> Result<String> {
        let conn = Connection::open(&self.db_path)?;

        // Generate random 20-byte secret
        let secret: [u8; 20] = rand::thread_rng().gen();
        let secret_base32 = base32_encode(&secret);

        conn.execute(
            "UPDATE multimedia_privacy SET otp_secret = ?1, require_otp = 1 WHERE profile_id = ?2",
            params![secret_base32, profile_id],
        )?;

        Ok(secret_base32)
    }

    /// Verify OTP code
    pub fn verify_otp(&self, profile_id: i64, code: &str) -> Result<bool> {
        let settings = self.get_privacy_settings(profile_id)?;

        if let Some(secret) = settings.otp_secret {
            // Simple TOTP verification (30-second window)
            let time_step = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() / 30;

            let expected = generate_totp(&secret, time_step);
            Ok(code == expected)
        } else {
            Ok(true) // No OTP set
        }
    }

    // ==================== History Management ====================

    /// Add history entry (only if history is enabled)
    /// If password is provided and require_password is enabled, the entry will be encrypted
    pub fn add_history_entry(&self, entry: &MediaHistoryEntry, password: Option<&str>) -> Result<Option<MediaHistoryEntry>> {
        let settings = self.get_privacy_settings(entry.profile_id)?;

        if !settings.history_enabled {
            return Ok(None); // History disabled, don't save
        }

        let conn = Connection::open(&self.db_path)?;
        let now = chrono::Utc::now().to_rfc3339();

        // Encrypt if password protection is enabled and password is provided
        let (final_entry, is_encrypted) = if settings.require_password {
            if let Some(pwd) = password {
                match encrypt_history_entry(entry, pwd) {
                    Ok(encrypted) => (encrypted, true),
                    Err(_) => (entry.clone(), false),
                }
            } else {
                (entry.clone(), false)
            }
        } else {
            (entry.clone(), false)
        };

        conn.execute(
            "INSERT INTO multimedia_history (profile_id, media_id, source, media_type, title,
                thumbnail, position, duration, played_at, encrypted)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                final_entry.profile_id,
                final_entry.media_id,
                final_entry.source,
                final_entry.media_type.to_string(),
                final_entry.title,
                final_entry.thumbnail,
                final_entry.position,
                final_entry.duration,
                now,
                is_encrypted
            ],
        )?;

        let id = conn.last_insert_rowid();

        Ok(Some(MediaHistoryEntry {
            id: Some(id),
            played_at: now,
            encrypted: is_encrypted,
            ..entry.clone()
        }))
    }

    /// Get history entries
    /// If password is provided, encrypted entries will be decrypted
    pub fn get_history(&self, profile_id: i64, limit: i32, password: Option<&str>) -> Result<Vec<MediaHistoryEntry>> {
        let settings = self.get_privacy_settings(profile_id)?;

        // Check password if required
        if settings.require_password {
            if let Some(pwd) = password {
                if !self.verify_password(profile_id, pwd)? {
                    return Err(rusqlite::Error::InvalidQuery);
                }
            } else {
                return Err(rusqlite::Error::InvalidQuery);
            }
        }

        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT id, profile_id, media_id, source, media_type, title, thumbnail,
                    position, duration, played_at, encrypted
             FROM multimedia_history
             WHERE profile_id = ?1
             ORDER BY played_at DESC
             LIMIT ?2"
        )?;

        let entries: Vec<MediaHistoryEntry> = stmt.query_map(params![profile_id, limit], |row| {
            let media_type_str: String = row.get(4)?;
            Ok(MediaHistoryEntry {
                id: row.get(0)?,
                profile_id: row.get(1)?,
                media_id: row.get(2)?,
                source: row.get(3)?,
                media_type: MediaType::from(media_type_str.as_str()),
                title: row.get(5)?,
                thumbnail: row.get(6)?,
                position: row.get(7)?,
                duration: row.get(8)?,
                played_at: row.get(9)?,
                encrypted: row.get(10)?,
            })
        })?.collect::<Result<Vec<_>>>()?;

        // Decrypt entries if password provided
        if let Some(pwd) = password {
            Ok(entries.into_iter().map(|entry| {
                if entry.encrypted {
                    decrypt_history_entry(&entry, pwd).unwrap_or(entry)
                } else {
                    entry
                }
            }).collect())
        } else {
            Ok(entries)
        }
    }

    /// Clear all history
    pub fn clear_history(&self, profile_id: i64) -> Result<i32> {
        let conn = Connection::open(&self.db_path)?;
        let count = conn.execute(
            "DELETE FROM multimedia_history WHERE profile_id = ?1",
            params![profile_id],
        )?;
        Ok(count as i32)
    }

    /// Delete single history entry
    pub fn delete_history_entry(&self, entry_id: i64) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute(
            "DELETE FROM multimedia_history WHERE id = ?1",
            params![entry_id],
        )?;
        Ok(())
    }

    // ==================== Playlist Management ====================

    /// Create playlist
    pub fn create_playlist(&self, profile_id: i64, name: &str, description: Option<&str>, encrypted: bool) -> Result<Playlist> {
        let conn = Connection::open(&self.db_path)?;
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO multimedia_playlists (profile_id, name, description, is_encrypted, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![profile_id, name, description, encrypted, now],
        )?;

        let id = conn.last_insert_rowid();

        Ok(Playlist {
            id: Some(id),
            profile_id,
            name: name.to_string(),
            description: description.map(String::from),
            thumbnail: None,
            is_encrypted: encrypted,
            created_at: now,
            updated_at: None,
            item_count: 0,
        })
    }

    /// Get all playlists
    pub fn get_playlists(&self, profile_id: i64) -> Result<Vec<Playlist>> {
        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT p.id, p.profile_id, p.name, p.description, p.thumbnail, p.is_encrypted,
                    p.created_at, p.updated_at, COUNT(i.id) as item_count
             FROM multimedia_playlists p
             LEFT JOIN multimedia_playlist_items i ON p.id = i.playlist_id
             WHERE p.profile_id = ?1
             GROUP BY p.id
             ORDER BY p.created_at DESC"
        )?;

        let playlists = stmt.query_map(params![profile_id], |row| {
            Ok(Playlist {
                id: row.get(0)?,
                profile_id: row.get(1)?,
                name: row.get(2)?,
                description: row.get(3)?,
                thumbnail: row.get(4)?,
                is_encrypted: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
                item_count: row.get(8)?,
            })
        })?;

        playlists.collect()
    }

    /// Delete playlist
    pub fn delete_playlist(&self, playlist_id: i64) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;

        // Delete items first
        conn.execute(
            "DELETE FROM multimedia_playlist_items WHERE playlist_id = ?1",
            params![playlist_id],
        )?;

        // Delete playlist
        conn.execute(
            "DELETE FROM multimedia_playlists WHERE id = ?1",
            params![playlist_id],
        )?;

        Ok(())
    }

    /// Add item to playlist
    pub fn add_to_playlist(&self, playlist_id: i64, source: &str, media_type: &str, title: Option<&str>, thumbnail: Option<&str>) -> Result<PlaylistItem> {
        let conn = Connection::open(&self.db_path)?;
        let now = chrono::Utc::now().to_rfc3339();

        // Get next position
        let position: i32 = conn.query_row(
            "SELECT COALESCE(MAX(position), 0) + 1 FROM multimedia_playlist_items WHERE playlist_id = ?1",
            params![playlist_id],
            |row| row.get(0),
        )?;

        // Store the filepath encrypted at rest (transparent, device-keyed).
        // Fall back to plaintext only if encryption itself fails.
        let stored_source = encrypt_data(source, &playlist_storage_key()).unwrap_or_else(|_| source.to_string());

        conn.execute(
            "INSERT INTO multimedia_playlist_items (playlist_id, source, media_type, title, thumbnail, position, added_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![playlist_id, stored_source, media_type, title, thumbnail, position, now],
        )?;

        let id = conn.last_insert_rowid();

        // Update playlist timestamp
        conn.execute(
            "UPDATE multimedia_playlists SET updated_at = ?1 WHERE id = ?2",
            params![now, playlist_id],
        )?;

        Ok(PlaylistItem {
            id: Some(id),
            playlist_id,
            source: source.to_string(),
            media_type: MediaType::from(media_type),
            title: title.map(String::from),
            thumbnail: thumbnail.map(String::from),
            position,
            added_at: now,
        })
    }

    /// Get playlist items
    pub fn get_playlist_items(&self, playlist_id: i64) -> Result<Vec<PlaylistItem>> {
        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT id, playlist_id, source, media_type, title, thumbnail, position, added_at
             FROM multimedia_playlist_items
             WHERE playlist_id = ?1
             ORDER BY position ASC"
        )?;

        let key = playlist_storage_key();
        let items = stmt.query_map(params![playlist_id], |row| {
            let media_type_str: String = row.get(3)?;
            // Decrypt the stored filepath: try the keyring secret, then the legacy
            // machine-id key (pre-migration rows), then fall back to the raw value
            // for any legacy plaintext rows written before encryption was added.
            let stored_source: String = row.get(2)?;
            let source = decrypt_data(&stored_source, &key)
                .or_else(|_| decrypt_data(&stored_source, &legacy_playlist_storage_key()))
                .unwrap_or(stored_source);
            Ok(PlaylistItem {
                id: row.get(0)?,
                playlist_id: row.get(1)?,
                source,
                media_type: MediaType::from(media_type_str.as_str()),
                title: row.get(4)?,
                thumbnail: row.get(5)?,
                position: row.get(6)?,
                added_at: row.get(7)?,
            })
        })?;

        items.collect()
    }

    /// Remove item from playlist
    pub fn remove_from_playlist(&self, item_id: i64) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute(
            "DELETE FROM multimedia_playlist_items WHERE id = ?1",
            params![item_id],
        )?;
        Ok(())
    }

    /// Reorder playlist items
    pub fn reorder_playlist_items(&self, playlist_id: i64, item_ids: Vec<i64>) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;

        for (position, item_id) in item_ids.iter().enumerate() {
            conn.execute(
                "UPDATE multimedia_playlist_items SET position = ?1 WHERE id = ?2 AND playlist_id = ?3",
                params![position as i32, item_id, playlist_id],
            )?;
        }

        Ok(())
    }

    // ==================== Stats ====================

    /// Get media stats
    pub fn get_stats(&self, profile_id: i64) -> Result<MediaStats> {
        let conn = Connection::open(&self.db_path)?;

        let total_played: i32 = conn.query_row(
            "SELECT COUNT(*) FROM multimedia_history WHERE profile_id = ?1",
            params![profile_id],
            |row| row.get(0),
        ).unwrap_or(0);

        let total_time: i64 = conn.query_row(
            "SELECT COALESCE(SUM(position), 0) FROM multimedia_history WHERE profile_id = ?1",
            params![profile_id],
            |row| row.get(0),
        ).unwrap_or(0);

        let videos: i32 = conn.query_row(
            "SELECT COUNT(*) FROM multimedia_history WHERE profile_id = ?1 AND media_type = 'video'",
            params![profile_id],
            |row| row.get(0),
        ).unwrap_or(0);

        let images: i32 = conn.query_row(
            "SELECT COUNT(*) FROM multimedia_history WHERE profile_id = ?1 AND media_type = 'image'",
            params![profile_id],
            |row| row.get(0),
        ).unwrap_or(0);

        let audio: i32 = conn.query_row(
            "SELECT COUNT(*) FROM multimedia_history WHERE profile_id = ?1 AND media_type = 'audio'",
            params![profile_id],
            |row| row.get(0),
        ).unwrap_or(0);

        let playlists: i32 = conn.query_row(
            "SELECT COUNT(*) FROM multimedia_playlists WHERE profile_id = ?1",
            params![profile_id],
            |row| row.get(0),
        ).unwrap_or(0);

        Ok(MediaStats {
            total_played,
            total_time_watched: total_time,
            videos_watched: videos,
            images_viewed: images,
            audio_played: audio,
            playlists_count: playlists,
        })
    }
}

// ==================== Encryption Helper Functions ====================

/// A stable, RANDOM secret for encrypting low-sensitivity data at rest (the JS
/// allowlist, playlist filepaths, bookmark metadata) WITHOUT a user password.
///
/// It's a 256-bit random key kept in the OS keyring (Secret Service), generated
/// once on first use. Unlike a machine-id-derived key, it is NOT derivable from
/// any world-readable file — so the encrypted data can't be decrypted just by
/// reading the DB plus public machine info. Cached for the process lifetime.
///
/// If no keyring is available (e.g. headless), it degrades to a machine-id value
/// so the app still works (logged once) — that path is no worse than before.
pub fn local_data_secret() -> String {
    use std::sync::OnceLock;
    static CACHE: OnceLock<String> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            if let Ok(entry) = keyring::Entry::new("com.earthservers.reclaim", "local-data-key") {
                match entry.get_password() {
                    Ok(k) if !k.is_empty() => return k,
                    _ => {
                        let key: [u8; 32] = rand::random();
                        let hex = hex::encode(key);
                        if entry.set_password(&hex).is_ok() {
                            return hex;
                        }
                    }
                }
            }
            eprintln!("[security] OS keyring unavailable — falling back to a machine-id key for at-rest encryption of low-sensitivity data");
            format!(
                "earthservers::local::{}",
                machine_uid::get().unwrap_or_else(|_| "default-device".to_string())
            )
        })
        .clone()
}

/// Legacy machine-id playlist key — retained ONLY to decrypt + migrate data
/// written before `local_data_secret`. Never used for new writes.
pub fn legacy_playlist_storage_key() -> String {
    let machine = machine_uid::get().unwrap_or_else(|_| "earthservers-default-device".to_string());
    format!("EarthMultiMedia::playlist::{}", machine)
}

/// Key for encrypting playlist filepaths at rest. Now a random keyring secret
/// (see `local_data_secret`); `legacy_playlist_storage_key` migrates old data.
pub fn playlist_storage_key() -> String {
    local_data_secret()
}

/// Derives a 32-byte key from a password using SHA256
fn derive_key_from_password(password: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    // Add salt for better security
    hasher.update(b"EarthMultiMedia_v1_salt");
    let result = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&result);
    key
}

/// Encrypts data using AES-256-GCM
pub fn encrypt_data(plaintext: &str, password: &str) -> Result<String, String> {
    let key = derive_key_from_password(password);
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| format!("Failed to create cipher: {}", e))?;

    // Generate random 12-byte nonce
    let nonce_bytes: [u8; 12] = rand::thread_rng().gen();
    let nonce = Nonce::from_slice(&nonce_bytes);

    // Encrypt
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| format!("Encryption failed: {}", e))?;

    // Prepend nonce to ciphertext and encode as base64
    let mut result = nonce_bytes.to_vec();
    result.extend(ciphertext);
    Ok(BASE64.encode(&result))
}

/// Decrypts data using AES-256-GCM
pub fn decrypt_data(encrypted: &str, password: &str) -> Result<String, String> {
    let key = derive_key_from_password(password);
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| format!("Failed to create cipher: {}", e))?;

    // Decode from base64
    let data = BASE64.decode(encrypted)
        .map_err(|e| format!("Invalid base64: {}", e))?;

    if data.len() < 12 {
        return Err("Invalid encrypted data: too short".to_string());
    }

    // Extract nonce (first 12 bytes) and ciphertext
    let nonce = Nonce::from_slice(&data[..12]);
    let ciphertext = &data[12..];

    // Decrypt
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| "Decryption failed: invalid password or corrupted data".to_string())?;

    String::from_utf8(plaintext)
        .map_err(|e| format!("Invalid UTF-8: {}", e))
}

/// Encrypts a MediaHistoryEntry's sensitive fields
pub fn encrypt_history_entry(entry: &MediaHistoryEntry, password: &str) -> Result<MediaHistoryEntry, String> {
    let mut encrypted = entry.clone();
    encrypted.source = encrypt_data(&entry.source, password)?;
    if let Some(ref title) = entry.title {
        encrypted.title = Some(encrypt_data(title, password)?);
    }
    if let Some(ref thumbnail) = entry.thumbnail {
        encrypted.thumbnail = Some(encrypt_data(thumbnail, password)?);
    }
    encrypted.encrypted = true;
    Ok(encrypted)
}

/// Decrypts a MediaHistoryEntry's sensitive fields
pub fn decrypt_history_entry(entry: &MediaHistoryEntry, password: &str) -> Result<MediaHistoryEntry, String> {
    if !entry.encrypted {
        return Ok(entry.clone());
    }

    let mut decrypted = entry.clone();
    decrypted.source = decrypt_data(&entry.source, password)?;
    if let Some(ref title) = entry.title {
        decrypted.title = Some(decrypt_data(title, password)?);
    }
    if let Some(ref thumbnail) = entry.thumbnail {
        decrypted.thumbnail = Some(decrypt_data(thumbnail, password)?);
    }
    decrypted.encrypted = false;
    Ok(decrypted)
}

/// Encrypts a PlaylistItem's sensitive fields
#[allow(dead_code)]
pub fn encrypt_playlist_item(item: &PlaylistItem, password: &str) -> Result<PlaylistItem, String> {
    let mut encrypted = item.clone();
    encrypted.source = encrypt_data(&item.source, password)?;
    if let Some(ref title) = item.title {
        encrypted.title = Some(encrypt_data(title, password)?);
    }
    if let Some(ref thumbnail) = item.thumbnail {
        encrypted.thumbnail = Some(encrypt_data(thumbnail, password)?);
    }
    Ok(encrypted)
}

/// Decrypts a PlaylistItem's sensitive fields
#[allow(dead_code)]
pub fn decrypt_playlist_item(item: &PlaylistItem, password: &str) -> Result<PlaylistItem, String> {
    let mut decrypted = item.clone();
    decrypted.source = decrypt_data(&item.source, password)?;
    if let Some(ref title) = item.title {
        decrypted.title = Some(decrypt_data(title, password)?);
    }
    if let Some(ref thumbnail) = item.thumbnail {
        decrypted.thumbnail = Some(decrypt_data(thumbnail, password)?);
    }
    Ok(decrypted)
}

// ==================== Helper Functions ====================

/// Simple base32 encoding for OTP secrets
fn base32_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut result = String::new();

    for chunk in data.chunks(5) {
        let mut buffer = [0u8; 8];
        let len = chunk.len();

        for (i, &byte) in chunk.iter().enumerate() {
            let shift = (4 - i) * 8;
            if shift < 40 {
                let idx = shift / 8;
                buffer[idx] |= byte >> (shift % 8);
                if shift % 8 != 0 && idx + 1 < 8 {
                    buffer[idx + 1] |= byte << (8 - shift % 8);
                }
            }
        }

        let bits = len * 8;
        let chars = (bits + 4) / 5;

        for i in 0..chars {
            let idx = (i * 5) / 8;
            let bit_offset = (i * 5) % 8;
            let value = if bit_offset <= 3 {
                (buffer[idx] >> (3 - bit_offset)) & 0x1F
            } else {
                let low = (buffer[idx] << (bit_offset - 3)) & 0x1F;
                let high = if idx + 1 < 8 { buffer[idx + 1] >> (11 - bit_offset) } else { 0 };
                low | high
            };
            result.push(ALPHABET[value as usize] as char);
        }
    }

    result
}

/// Generate TOTP code
fn generate_totp(secret: &str, time_step: u64) -> String {
    use sha2::Sha256;
    use hmac::{Hmac, Mac};

    type HmacSha256 = Hmac<Sha256>;

    // Decode base32 secret (simplified)
    let secret_bytes: Vec<u8> = secret.bytes().take(20).collect();

    // Create HMAC using Mac trait's new_from_slice
    let mut mac = <HmacSha256 as Mac>::new_from_slice(&secret_bytes).unwrap();
    mac.update(&time_step.to_be_bytes());
    let result = mac.finalize().into_bytes();

    // Dynamic truncation
    let offset = (result[result.len() - 1] & 0x0f) as usize;
    let code = ((result[offset] as u32 & 0x7f) << 24)
        | ((result[offset + 1] as u32) << 16)
        | ((result[offset + 2] as u32) << 8)
        | (result[offset + 3] as u32);

    format!("{:06}", code % 1_000_000)
}
