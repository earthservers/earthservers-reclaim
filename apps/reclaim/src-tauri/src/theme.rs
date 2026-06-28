// Theme management for EarthServers Local
// Handles per-profile theme customization and preset themes

use rusqlite::{Connection, Result, params};
use serde::{Deserialize, Serialize};

/// Theme configuration stored in database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    pub id: Option<i64>,
    pub profile_id: i64,
    pub name: String,
    pub is_active: bool,
    pub base_preset: String,
    // Core colors
    pub primary_color: String,
    pub secondary_color: String,
    pub accent_color: String,
    pub text_color: String,
    // Background settings
    pub background_color: String,
    pub background_gradient_enabled: bool,
    pub background_gradient_angle: i32,
    pub background_gradient_from: Option<String>,
    pub background_gradient_to: Option<String>,
    // Card settings
    pub card_bg_color: String,
    pub card_opacity: i32,
    pub card_gradient_enabled: bool,
    pub card_gradient_color1: Option<String>,
    pub card_gradient_color2: Option<String>,
    // Navbar settings
    pub navbar_color: Option<String>,
    pub navbar_opacity: i32,
    // Extra
    pub custom_css: Option<String>,
    pub extra_settings: Option<String>,
    pub created_at: String,
    pub updated_at: Option<String>,
}

/// Preset theme definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetTheme {
    pub id: String,
    pub name: String,
    pub primary_color: String,
    pub secondary_color: String,
    pub accent_color: String,
    pub text_color: String,
    pub background_color: String,
    pub background_gradient_from: String,
    pub background_gradient_to: String,
    pub card_bg_color: String,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            id: None,
            profile_id: 1,
            name: "Default".to_string(),
            is_active: true,
            base_preset: "earthservers-default".to_string(),
            primary_color: "#0fab89".to_string(),
            secondary_color: "#e91e63".to_string(),
            accent_color: "#0178C6".to_string(),
            text_color: "#f0f0f0".to_string(),
            background_color: "#0a0a0f".to_string(),
            background_gradient_enabled: true,
            background_gradient_angle: 135,
            background_gradient_from: Some("#0a0a0f".to_string()),
            background_gradient_to: Some("#1a1a2e".to_string()),
            card_bg_color: "#1a1a2e".to_string(),
            card_opacity: 80,
            card_gradient_enabled: false,
            card_gradient_color1: Some("#1a1a2e".to_string()),
            card_gradient_color2: Some("#2a2a3e".to_string()),
            navbar_color: Some("#0a0a0f".to_string()),
            navbar_opacity: 90,
            custom_css: None,
            extra_settings: None,
            created_at: String::new(),
            updated_at: None,
        }
    }
}

/// Get all preset themes
pub fn get_preset_themes() -> Vec<PresetTheme> {
    vec![
        PresetTheme {
            id: "earthservers-default".to_string(),
            name: "EarthServers Default".to_string(),
            primary_color: "#0fab89".to_string(),
            secondary_color: "#e91e63".to_string(),
            accent_color: "#0178C6".to_string(),
            text_color: "#f0f0f0".to_string(),
            background_color: "#0a0a0f".to_string(),
            background_gradient_from: "#0a0a0f".to_string(),
            background_gradient_to: "#1a1a2e".to_string(),
            card_bg_color: "#1a1a2e".to_string(),
        },
        PresetTheme {
            id: "ocean-turtle".to_string(),
            name: "Ocean Turtle".to_string(),
            primary_color: "#0d4f4f".to_string(),
            secondary_color: "#1a8f8f".to_string(),
            accent_color: "#2dd4bf".to_string(),
            text_color: "#e0f2f1".to_string(),
            background_color: "#042f2e".to_string(),
            background_gradient_from: "#042f2e".to_string(),
            background_gradient_to: "#0d4f4f".to_string(),
            card_bg_color: "#0d4f4f".to_string(),
        },
        PresetTheme {
            id: "mountain-eagle".to_string(),
            name: "Mountain Eagle".to_string(),
            primary_color: "#374151".to_string(),
            secondary_color: "#6b7280".to_string(),
            accent_color: "#f59e0b".to_string(),
            text_color: "#f9fafb".to_string(),
            background_color: "#111827".to_string(),
            background_gradient_from: "#111827".to_string(),
            background_gradient_to: "#1f2937".to_string(),
            card_bg_color: "#1f2937".to_string(),
        },
        PresetTheme {
            id: "sun-fire".to_string(),
            name: "Sun Fire".to_string(),
            primary_color: "#7c2d12".to_string(),
            secondary_color: "#c2410c".to_string(),
            accent_color: "#fb923c".to_string(),
            text_color: "#fef3c7".to_string(),
            background_color: "#431407".to_string(),
            background_gradient_from: "#431407".to_string(),
            background_gradient_to: "#7c2d12".to_string(),
            card_bg_color: "#7c2d12".to_string(),
        },
        PresetTheme {
            id: "air-clouds".to_string(),
            name: "Air Clouds".to_string(),
            primary_color: "#1e3a5f".to_string(),
            secondary_color: "#3b82f6".to_string(),
            accent_color: "#93c5fd".to_string(),
            text_color: "#f0f9ff".to_string(),
            background_color: "#0c1929".to_string(),
            background_gradient_from: "#0c1929".to_string(),
            background_gradient_to: "#1e3a5f".to_string(),
            card_bg_color: "#1e3a5f".to_string(),
        },
        PresetTheme {
            id: "lightning-bolt".to_string(),
            name: "Lightning Bolt".to_string(),
            primary_color: "#4c1d95".to_string(),
            secondary_color: "#7c3aed".to_string(),
            accent_color: "#a78bfa".to_string(),
            text_color: "#f5f3ff".to_string(),
            background_color: "#2e1065".to_string(),
            background_gradient_from: "#2e1065".to_string(),
            background_gradient_to: "#4c1d95".to_string(),
            card_bg_color: "#4c1d95".to_string(),
        },
    ]
}

pub struct ThemeManager {
    db_path: String,
}

impl ThemeManager {
    pub fn new(db_path: String) -> Self {
        ThemeManager { db_path }
    }

    /// Initialize theme table
    pub fn init(&self) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS themes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                profile_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                is_active INTEGER NOT NULL DEFAULT 0,
                base_preset TEXT DEFAULT 'earthservers-default',
                primary_color TEXT NOT NULL DEFAULT '#0fab89',
                secondary_color TEXT NOT NULL DEFAULT '#e91e63',
                accent_color TEXT NOT NULL DEFAULT '#0178C6',
                text_color TEXT NOT NULL DEFAULT '#f0f0f0',
                background_color TEXT NOT NULL DEFAULT '#0a0a0f',
                background_gradient_enabled INTEGER NOT NULL DEFAULT 1,
                background_gradient_angle INTEGER NOT NULL DEFAULT 135,
                background_gradient_from TEXT DEFAULT '#0a0a0f',
                background_gradient_to TEXT DEFAULT '#1a1a2e',
                card_bg_color TEXT NOT NULL DEFAULT '#1a1a2e',
                card_opacity INTEGER NOT NULL DEFAULT 80,
                card_gradient_enabled INTEGER NOT NULL DEFAULT 0,
                card_gradient_color1 TEXT DEFAULT '#1a1a2e',
                card_gradient_color2 TEXT DEFAULT '#2a2a3e',
                navbar_color TEXT DEFAULT '#0a0a0f',
                navbar_opacity INTEGER NOT NULL DEFAULT 90,
                custom_css TEXT,
                extra_settings TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT,
                FOREIGN KEY (profile_id) REFERENCES profiles(id) ON DELETE CASCADE,
                UNIQUE(profile_id, name)
            )",
            [],
        )?;

        // Create indexes
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_themes_profile ON themes(profile_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_themes_active ON themes(profile_id, is_active)",
            [],
        )?;

        Ok(())
    }

    /// Ensure a profile has at least one theme
    pub fn ensure_default_theme(&self, profile_id: i64) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM themes WHERE profile_id = ?1",
            params![profile_id],
            |row| row.get(0),
        )?;

        if count == 0 {
            let now = chrono_now();
            conn.execute(
                "INSERT INTO themes (profile_id, name, is_active, created_at) VALUES (?1, 'Default', 1, ?2)",
                params![profile_id, now],
            )?;
        }

        Ok(())
    }

    /// Get all themes for a profile
    pub fn get_themes(&self, profile_id: i64) -> Result<Vec<Theme>> {
        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT id, profile_id, name, is_active, base_preset,
                    primary_color, secondary_color, accent_color, text_color,
                    background_color, background_gradient_enabled, background_gradient_angle,
                    background_gradient_from, background_gradient_to,
                    card_bg_color, card_opacity, card_gradient_enabled,
                    card_gradient_color1, card_gradient_color2,
                    navbar_color, navbar_opacity, custom_css, extra_settings,
                    created_at, updated_at
             FROM themes WHERE profile_id = ?1 ORDER BY created_at ASC"
        )?;

        let themes = stmt.query_map(params![profile_id], |row| {
            Ok(Theme {
                id: Some(row.get(0)?),
                profile_id: row.get(1)?,
                name: row.get(2)?,
                is_active: row.get::<_, i64>(3)? == 1,
                base_preset: row.get(4)?,
                primary_color: row.get(5)?,
                secondary_color: row.get(6)?,
                accent_color: row.get(7)?,
                text_color: row.get(8)?,
                background_color: row.get(9)?,
                background_gradient_enabled: row.get::<_, i64>(10)? == 1,
                background_gradient_angle: row.get(11)?,
                background_gradient_from: row.get(12)?,
                background_gradient_to: row.get(13)?,
                card_bg_color: row.get(14)?,
                card_opacity: row.get(15)?,
                card_gradient_enabled: row.get::<_, i64>(16)? == 1,
                card_gradient_color1: row.get(17)?,
                card_gradient_color2: row.get(18)?,
                navbar_color: row.get(19)?,
                navbar_opacity: row.get(20)?,
                custom_css: row.get(21)?,
                extra_settings: row.get(22)?,
                created_at: row.get(23)?,
                updated_at: row.get(24)?,
            })
        })?;

        themes.collect()
    }

    /// Get active theme for a profile
    pub fn get_active_theme(&self, profile_id: i64) -> Result<Option<Theme>> {
        self.ensure_default_theme(profile_id)?;

        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT id, profile_id, name, is_active, base_preset,
                    primary_color, secondary_color, accent_color, text_color,
                    background_color, background_gradient_enabled, background_gradient_angle,
                    background_gradient_from, background_gradient_to,
                    card_bg_color, card_opacity, card_gradient_enabled,
                    card_gradient_color1, card_gradient_color2,
                    navbar_color, navbar_opacity, custom_css, extra_settings,
                    created_at, updated_at
             FROM themes WHERE profile_id = ?1 AND is_active = 1"
        )?;

        let mut themes = stmt.query_map(params![profile_id], |row| {
            Ok(Theme {
                id: Some(row.get(0)?),
                profile_id: row.get(1)?,
                name: row.get(2)?,
                is_active: true,
                base_preset: row.get(4)?,
                primary_color: row.get(5)?,
                secondary_color: row.get(6)?,
                accent_color: row.get(7)?,
                text_color: row.get(8)?,
                background_color: row.get(9)?,
                background_gradient_enabled: row.get::<_, i64>(10)? == 1,
                background_gradient_angle: row.get(11)?,
                background_gradient_from: row.get(12)?,
                background_gradient_to: row.get(13)?,
                card_bg_color: row.get(14)?,
                card_opacity: row.get(15)?,
                card_gradient_enabled: row.get::<_, i64>(16)? == 1,
                card_gradient_color1: row.get(17)?,
                card_gradient_color2: row.get(18)?,
                navbar_color: row.get(19)?,
                navbar_opacity: row.get(20)?,
                custom_css: row.get(21)?,
                extra_settings: row.get(22)?,
                created_at: row.get(23)?,
                updated_at: row.get(24)?,
            })
        })?;

        match themes.next() {
            Some(Ok(theme)) => Ok(Some(theme)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    /// Save/update a theme
    pub fn save_theme(&self, theme: &Theme) -> Result<Theme> {
        let conn = Connection::open(&self.db_path)?;
        let now = chrono_now();

        if let Some(id) = theme.id {
            // Update existing theme
            conn.execute(
                "UPDATE themes SET
                    name = ?1, base_preset = ?2,
                    primary_color = ?3, secondary_color = ?4, accent_color = ?5, text_color = ?6,
                    background_color = ?7, background_gradient_enabled = ?8, background_gradient_angle = ?9,
                    background_gradient_from = ?10, background_gradient_to = ?11,
                    card_bg_color = ?12, card_opacity = ?13, card_gradient_enabled = ?14,
                    card_gradient_color1 = ?15, card_gradient_color2 = ?16,
                    navbar_color = ?17, navbar_opacity = ?18,
                    custom_css = ?19, extra_settings = ?20, updated_at = ?21
                 WHERE id = ?22",
                params![
                    theme.name, theme.base_preset,
                    theme.primary_color, theme.secondary_color, theme.accent_color, theme.text_color,
                    theme.background_color, theme.background_gradient_enabled as i64, theme.background_gradient_angle,
                    theme.background_gradient_from, theme.background_gradient_to,
                    theme.card_bg_color, theme.card_opacity, theme.card_gradient_enabled as i64,
                    theme.card_gradient_color1, theme.card_gradient_color2,
                    theme.navbar_color, theme.navbar_opacity,
                    theme.custom_css, theme.extra_settings, now,
                    id
                ],
            )?;

            let mut updated_theme = theme.clone();
            updated_theme.updated_at = Some(now);
            Ok(updated_theme)
        } else {
            // Insert new theme
            conn.execute(
                "INSERT INTO themes (
                    profile_id, name, is_active, base_preset,
                    primary_color, secondary_color, accent_color, text_color,
                    background_color, background_gradient_enabled, background_gradient_angle,
                    background_gradient_from, background_gradient_to,
                    card_bg_color, card_opacity, card_gradient_enabled,
                    card_gradient_color1, card_gradient_color2,
                    navbar_color, navbar_opacity,
                    custom_css, extra_settings, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)",
                params![
                    theme.profile_id, theme.name, theme.is_active as i64, theme.base_preset,
                    theme.primary_color, theme.secondary_color, theme.accent_color, theme.text_color,
                    theme.background_color, theme.background_gradient_enabled as i64, theme.background_gradient_angle,
                    theme.background_gradient_from, theme.background_gradient_to,
                    theme.card_bg_color, theme.card_opacity, theme.card_gradient_enabled as i64,
                    theme.card_gradient_color1, theme.card_gradient_color2,
                    theme.navbar_color, theme.navbar_opacity,
                    theme.custom_css, theme.extra_settings, now
                ],
            )?;

            let mut new_theme = theme.clone();
            new_theme.id = Some(conn.last_insert_rowid());
            new_theme.created_at = now;
            Ok(new_theme)
        }
    }

    /// Set active theme for a profile
    pub fn set_active_theme(&self, profile_id: i64, theme_id: i64) -> Result<Theme> {
        let conn = Connection::open(&self.db_path)?;

        // Deactivate all themes for this profile
        conn.execute(
            "UPDATE themes SET is_active = 0 WHERE profile_id = ?1",
            params![profile_id],
        )?;

        // Activate the selected theme
        conn.execute(
            "UPDATE themes SET is_active = 1 WHERE id = ?1 AND profile_id = ?2",
            params![theme_id, profile_id],
        )?;

        // Return the now-active theme
        self.get_active_theme(profile_id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    /// Delete a theme
    pub fn delete_theme(&self, theme_id: i64, profile_id: i64) -> Result<bool> {
        let conn = Connection::open(&self.db_path)?;

        // Check if this is the only theme
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM themes WHERE profile_id = ?1",
            params![profile_id],
            |row| row.get(0),
        )?;

        if count <= 1 {
            return Ok(false); // Can't delete the last theme
        }

        // Check if deleting active theme
        let is_active: i64 = conn.query_row(
            "SELECT is_active FROM themes WHERE id = ?1",
            params![theme_id],
            |row| row.get(0),
        ).unwrap_or(0);

        // Delete the theme
        let affected = conn.execute(
            "DELETE FROM themes WHERE id = ?1 AND profile_id = ?2",
            params![theme_id, profile_id],
        )?;

        // If deleted theme was active, activate another one
        if is_active == 1 {
            conn.execute(
                "UPDATE themes SET is_active = 1 WHERE profile_id = ?1 AND id = (SELECT MIN(id) FROM themes WHERE profile_id = ?1)",
                params![profile_id],
            )?;
        }

        Ok(affected > 0)
    }

    /// Apply a preset theme
    pub fn apply_preset(&self, profile_id: i64, preset_id: &str) -> Result<Theme> {
        let presets = get_preset_themes();
        let preset = presets.iter()
            .find(|p| p.id == preset_id)
            .ok_or(rusqlite::Error::QueryReturnedNoRows)?;

        // Get active theme and update it with preset colors
        let active = self.get_active_theme(profile_id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)?;

        let mut updated = active;
        updated.base_preset = preset.id.clone();
        updated.primary_color = preset.primary_color.clone();
        updated.secondary_color = preset.secondary_color.clone();
        updated.accent_color = preset.accent_color.clone();
        updated.text_color = preset.text_color.clone();
        updated.background_color = preset.background_color.clone();
        updated.background_gradient_from = Some(preset.background_gradient_from.clone());
        updated.background_gradient_to = Some(preset.background_gradient_to.clone());
        updated.card_bg_color = preset.card_bg_color.clone();

        self.save_theme(&updated)
    }

    /// Export theme as JSON
    pub fn export_theme(&self, theme_id: i64) -> Result<String> {
        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT id, profile_id, name, is_active, base_preset,
                    primary_color, secondary_color, accent_color, text_color,
                    background_color, background_gradient_enabled, background_gradient_angle,
                    background_gradient_from, background_gradient_to,
                    card_bg_color, card_opacity, card_gradient_enabled,
                    card_gradient_color1, card_gradient_color2,
                    navbar_color, navbar_opacity, custom_css, extra_settings,
                    created_at, updated_at
             FROM themes WHERE id = ?1"
        )?;

        let theme = stmt.query_row(params![theme_id], |row| {
            Ok(Theme {
                id: Some(row.get(0)?),
                profile_id: row.get(1)?,
                name: row.get(2)?,
                is_active: row.get::<_, i64>(3)? == 1,
                base_preset: row.get(4)?,
                primary_color: row.get(5)?,
                secondary_color: row.get(6)?,
                accent_color: row.get(7)?,
                text_color: row.get(8)?,
                background_color: row.get(9)?,
                background_gradient_enabled: row.get::<_, i64>(10)? == 1,
                background_gradient_angle: row.get(11)?,
                background_gradient_from: row.get(12)?,
                background_gradient_to: row.get(13)?,
                card_bg_color: row.get(14)?,
                card_opacity: row.get(15)?,
                card_gradient_enabled: row.get::<_, i64>(16)? == 1,
                card_gradient_color1: row.get(17)?,
                card_gradient_color2: row.get(18)?,
                navbar_color: row.get(19)?,
                navbar_opacity: row.get(20)?,
                custom_css: row.get(21)?,
                extra_settings: row.get(22)?,
                created_at: row.get(23)?,
                updated_at: row.get(24)?,
            })
        })?;

        let export = serde_json::json!({
            "version": 1,
            "exported_at": chrono_now(),
            "theme": theme
        });

        Ok(serde_json::to_string_pretty(&export).unwrap_or_default())
    }
}

fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", duration.as_secs())
}
