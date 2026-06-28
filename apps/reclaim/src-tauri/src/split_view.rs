// Split view management for Earth Reclaim
// Multi-pane layout system for viewing multiple tabs simultaneously

use rusqlite::{Connection, Result, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SplitLayout {
    #[serde(rename = "single")]
    Single,
    #[serde(rename = "horizontal")]
    Horizontal,
    #[serde(rename = "vertical")]
    Vertical,
    #[serde(rename = "quad")]
    Quad,
}

impl From<&str> for SplitLayout {
    fn from(s: &str) -> Self {
        match s {
            "horizontal" => SplitLayout::Horizontal,
            "vertical" => SplitLayout::Vertical,
            "quad" => SplitLayout::Quad,
            _ => SplitLayout::Single,
        }
    }
}

impl ToString for SplitLayout {
    fn to_string(&self) -> String {
        match self {
            SplitLayout::Single => "single".to_string(),
            SplitLayout::Horizontal => "horizontal".to_string(),
            SplitLayout::Vertical => "vertical".to_string(),
            SplitLayout::Quad => "quad".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitViewConfig {
    pub profile_id: i64,
    pub layout: SplitLayout,
    pub pane_1_tab_id: Option<i64>,
    pub pane_2_tab_id: Option<i64>,
    pub pane_3_tab_id: Option<i64>,
    pub pane_4_tab_id: Option<i64>,
    pub active_pane: i32,
    pub pane_sizes: Option<PaneSizes>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneSizes {
    pub pane_1: f64,
    pub pane_2: f64,
    pub pane_3: Option<f64>,
    pub pane_4: Option<f64>,
}

impl Default for PaneSizes {
    fn default() -> Self {
        PaneSizes {
            pane_1: 50.0,
            pane_2: 50.0,
            pane_3: None,
            pane_4: None,
        }
    }
}

pub struct SplitViewManager {
    db_path: String,
}

impl SplitViewManager {
    pub fn new(db_path: String) -> Self {
        SplitViewManager { db_path }
    }

    /// Get or create split view config for a profile
    pub fn get_config(&self, profile_id: i64) -> Result<SplitViewConfig> {
        let conn = Connection::open(&self.db_path)?;

        let result = conn.query_row(
            "SELECT profile_id, layout, pane_1_tab_id, pane_2_tab_id, pane_3_tab_id, pane_4_tab_id, active_pane, pane_sizes
             FROM split_view_config WHERE profile_id = ?1",
            params![profile_id],
            |row| {
                let layout_str: String = row.get(1)?;
                let pane_sizes_str: Option<String> = row.get(7)?;
                let pane_sizes: Option<PaneSizes> = pane_sizes_str
                    .and_then(|s| serde_json::from_str(&s).ok());

                Ok(SplitViewConfig {
                    profile_id: row.get(0)?,
                    layout: SplitLayout::from(layout_str.as_str()),
                    pane_1_tab_id: row.get(2)?,
                    pane_2_tab_id: row.get(3)?,
                    pane_3_tab_id: row.get(4)?,
                    pane_4_tab_id: row.get(5)?,
                    active_pane: row.get(6)?,
                    pane_sizes,
                })
            },
        );

        match result {
            Ok(config) => Ok(config),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                // Create default config
                self.create_default_config(profile_id)
            }
            Err(e) => Err(e),
        }
    }

    /// Create default split view config
    fn create_default_config(&self, profile_id: i64) -> Result<SplitViewConfig> {
        let conn = Connection::open(&self.db_path)?;

        conn.execute(
            "INSERT INTO split_view_config (profile_id, layout, active_pane)
             VALUES (?1, 'single', 1)",
            params![profile_id],
        )?;

        Ok(SplitViewConfig {
            profile_id,
            layout: SplitLayout::Single,
            pane_1_tab_id: None,
            pane_2_tab_id: None,
            pane_3_tab_id: None,
            pane_4_tab_id: None,
            active_pane: 1,
            pane_sizes: None,
        })
    }

    /// Set the split layout
    pub fn set_layout(&self, profile_id: i64, layout: &str) -> Result<SplitViewConfig> {
        let conn = Connection::open(&self.db_path)?;
        let split_layout = SplitLayout::from(layout);

        // Ensure config exists
        let _ = self.get_config(profile_id)?;

        // Clear unused panes based on layout
        let (_pane_3, _pane_4) = match split_layout {
            SplitLayout::Single | SplitLayout::Horizontal | SplitLayout::Vertical => {
                // Clear panes 3 and 4
                conn.execute(
                    "UPDATE split_view_config SET pane_3_tab_id = NULL, pane_4_tab_id = NULL WHERE profile_id = ?1",
                    params![profile_id],
                )?;
                (None::<i64>, None::<i64>)
            }
            SplitLayout::Quad => {
                // Keep all panes
                let config = self.get_config(profile_id)?;
                (config.pane_3_tab_id, config.pane_4_tab_id)
            }
        };

        let _pane_2 = match split_layout {
            SplitLayout::Single => {
                // Clear pane 2
                conn.execute(
                    "UPDATE split_view_config SET pane_2_tab_id = NULL WHERE profile_id = ?1",
                    params![profile_id],
                )?;
                None
            }
            _ => {
                let config = self.get_config(profile_id)?;
                config.pane_2_tab_id
            }
        };

        conn.execute(
            "UPDATE split_view_config SET layout = ?1 WHERE profile_id = ?2",
            params![split_layout.to_string(), profile_id],
        )?;

        self.get_config(profile_id)
    }

    /// Set which tab is shown in a pane
    pub fn set_pane_tab(&self, profile_id: i64, pane_number: i32, tab_id: Option<i64>) -> Result<SplitViewConfig> {
        let conn = Connection::open(&self.db_path)?;

        // Ensure config exists
        let _ = self.get_config(profile_id)?;

        let column = match pane_number {
            1 => "pane_1_tab_id",
            2 => "pane_2_tab_id",
            3 => "pane_3_tab_id",
            4 => "pane_4_tab_id",
            _ => return Err(rusqlite::Error::InvalidParameterName("Invalid pane number".to_string())),
        };

        conn.execute(
            &format!("UPDATE split_view_config SET {} = ?1 WHERE profile_id = ?2", column),
            params![tab_id, profile_id],
        )?;

        self.get_config(profile_id)
    }

    /// Set the active pane (which has focus)
    pub fn set_active_pane(&self, profile_id: i64, pane_number: i32) -> Result<SplitViewConfig> {
        let conn = Connection::open(&self.db_path)?;

        // Validate pane number based on current layout
        let config = self.get_config(profile_id)?;
        let max_panes = match config.layout {
            SplitLayout::Single => 1,
            SplitLayout::Horizontal | SplitLayout::Vertical => 2,
            SplitLayout::Quad => 4,
        };

        if pane_number < 1 || pane_number > max_panes {
            return Err(rusqlite::Error::InvalidParameterName(
                format!("Pane number must be between 1 and {}", max_panes)
            ));
        }

        conn.execute(
            "UPDATE split_view_config SET active_pane = ?1 WHERE profile_id = ?2",
            params![pane_number, profile_id],
        )?;

        self.get_config(profile_id)
    }

    /// Cycle to next pane
    pub fn cycle_pane(&self, profile_id: i64, direction: i32) -> Result<SplitViewConfig> {
        let config = self.get_config(profile_id)?;

        let max_panes = match config.layout {
            SplitLayout::Single => 1,
            SplitLayout::Horizontal | SplitLayout::Vertical => 2,
            SplitLayout::Quad => 4,
        };

        let new_pane = if direction > 0 {
            // Next pane
            if config.active_pane >= max_panes {
                1
            } else {
                config.active_pane + 1
            }
        } else {
            // Previous pane
            if config.active_pane <= 1 {
                max_panes
            } else {
                config.active_pane - 1
            }
        };

        self.set_active_pane(profile_id, new_pane)
    }

    /// Update pane sizes
    pub fn update_pane_sizes(&self, profile_id: i64, sizes: PaneSizes) -> Result<SplitViewConfig> {
        let conn = Connection::open(&self.db_path)?;

        let sizes_json = serde_json::to_string(&sizes)
            .unwrap_or_else(|_| "{}".to_string());

        conn.execute(
            "UPDATE split_view_config SET pane_sizes = ?1 WHERE profile_id = ?2",
            params![sizes_json, profile_id],
        )?;

        self.get_config(profile_id)
    }

    /// Swap tabs between two panes
    pub fn swap_panes(&self, profile_id: i64, pane_a: i32, pane_b: i32) -> Result<SplitViewConfig> {
        let config = self.get_config(profile_id)?;

        let tab_a = match pane_a {
            1 => config.pane_1_tab_id,
            2 => config.pane_2_tab_id,
            3 => config.pane_3_tab_id,
            4 => config.pane_4_tab_id,
            _ => None,
        };

        let tab_b = match pane_b {
            1 => config.pane_1_tab_id,
            2 => config.pane_2_tab_id,
            3 => config.pane_3_tab_id,
            4 => config.pane_4_tab_id,
            _ => None,
        };

        self.set_pane_tab(profile_id, pane_a, tab_b)?;
        self.set_pane_tab(profile_id, pane_b, tab_a)
    }

    /// Get tab ID for active pane
    pub fn get_active_tab(&self, profile_id: i64) -> Result<Option<i64>> {
        let config = self.get_config(profile_id)?;

        Ok(match config.active_pane {
            1 => config.pane_1_tab_id,
            2 => config.pane_2_tab_id,
            3 => config.pane_3_tab_id,
            4 => config.pane_4_tab_id,
            _ => None,
        })
    }

    /// Reset to single pane view
    pub fn reset_to_single(&self, profile_id: i64) -> Result<SplitViewConfig> {
        let conn = Connection::open(&self.db_path)?;

        conn.execute(
            "UPDATE split_view_config SET
                layout = 'single',
                pane_2_tab_id = NULL,
                pane_3_tab_id = NULL,
                pane_4_tab_id = NULL,
                active_pane = 1,
                pane_sizes = NULL
             WHERE profile_id = ?1",
            params![profile_id],
        )?;

        self.get_config(profile_id)
    }
}
