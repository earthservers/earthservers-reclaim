// Community Trust & Bias Rating System for EarthSearch
// Manages domain ratings, aggregates, and subdomain-specific ratings

use rusqlite::{Connection, Result, params};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ==================== Data Structures ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainRating {
    pub id: Option<i64>,
    pub domain_id: i64,
    pub user_id: String,
    pub trust_rating: i32,        // 1-5 (1=Trusted, 5=Sketchy)
    pub bias_rating: i32,         // 1-5 (1=Far Left, 3=Center, 5=Far Right)
    pub independence_rating: i32, // 1-4 (1=Biased, 2=Neutral, 3=Independent, 4=Unbiased)
    pub review_text: Option<String>,
    pub created_at: String,
    pub updated_at: Option<String>,
    pub helpful_count: i32,
    pub reported: bool,
    pub device_fingerprint: Option<String>, // Hardware fingerprint for deduplication
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RatingAggregate {
    pub domain_id: i64,
    pub avg_trust: f64,
    pub avg_bias: f64,
    pub avg_independence: f64,
    pub total_ratings: i64,
    pub trust_distribution: Vec<i64>,       // [count_1, count_2, count_3, count_4, count_5]
    pub bias_distribution: Vec<i64>,        // [count_far_left, count_left, count_center, count_right, count_far_right]
    pub independence_distribution: Vec<i64>, // [count_biased, count_neutral, count_independent, count_unbiased]
    pub last_updated: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubdomainRating {
    pub id: Option<i64>,
    pub parent_domain_id: i64,
    pub subdomain: String,
    pub avg_trust: f64,
    pub avg_bias: f64,
    pub total_ratings: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RatingCategory {
    pub id: Option<i64>,
    pub domain_rating_id: i64,
    pub category: String,  // e.g., "accuracy", "transparency", "sourcing"
    pub score: i32,        // 1-5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RatingSummary {
    pub domain_url: String,
    pub avg_trust: f64,
    pub avg_bias: f64,
    pub avg_independence: f64,
    pub total_ratings: i64,
    pub trust_label: String,       // "Trusted", "Mostly Trusted", "Mixed", "Questionable", "Sketchy"
    pub bias_label: String,        // "Far Left", "Left", "Center", "Right", "Far Right"
    pub independence_label: String, // "Biased", "Neutral", "Independent", "Unbiased"
    pub category_scores: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRatingHistory {
    pub ratings: Vec<DomainRating>,
    pub total_ratings: i64,
    pub avg_trust_given: f64,
    pub avg_bias_given: f64,
}

// ==================== Rating Manager ====================

pub struct RatingManager {
    db_path: String,
}

impl RatingManager {
    pub fn new(db_path: String) -> Self {
        RatingManager { db_path }
    }

    // ==================== Rating CRUD ====================

    /// Submit or update a domain rating
    pub fn submit_rating(&self, rating: &DomainRating) -> Result<DomainRating> {
        let conn = Connection::open(&self.db_path)?;
        let now = chrono_now();

        // Check if user already rated this domain (by user_id or device_fingerprint for deduplication)
        let existing: Option<i64> = conn.query_row(
            "SELECT id FROM domain_ratings WHERE domain_id = ?1 AND (user_id = ?2 OR device_fingerprint = ?3)",
            params![rating.domain_id, rating.user_id, rating.device_fingerprint],
            |row| row.get(0),
        ).ok();

        let id = if let Some(existing_id) = existing {
            // Update existing rating
            conn.execute(
                "UPDATE domain_ratings SET
                    trust_rating = ?1, bias_rating = ?2, independence_rating = ?3,
                    review_text = ?4, updated_at = ?5, device_fingerprint = ?6
                 WHERE id = ?7",
                params![
                    rating.trust_rating,
                    rating.bias_rating,
                    rating.independence_rating,
                    rating.review_text,
                    now,
                    rating.device_fingerprint,
                    existing_id
                ],
            )?;
            existing_id
        } else {
            // Insert new rating
            conn.execute(
                "INSERT INTO domain_ratings
                    (domain_id, user_id, trust_rating, bias_rating, independence_rating, review_text, created_at, helpful_count, reported, device_fingerprint)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, FALSE, ?8)",
                params![
                    rating.domain_id,
                    rating.user_id,
                    rating.trust_rating,
                    rating.bias_rating,
                    rating.independence_rating,
                    rating.review_text,
                    now,
                    rating.device_fingerprint
                ],
            )?;
            conn.last_insert_rowid()
        };

        // Update aggregates
        self.update_aggregates(rating.domain_id)?;

        Ok(DomainRating {
            id: Some(id),
            domain_id: rating.domain_id,
            user_id: rating.user_id.clone(),
            trust_rating: rating.trust_rating,
            bias_rating: rating.bias_rating,
            independence_rating: rating.independence_rating,
            review_text: rating.review_text.clone(),
            created_at: if existing.is_some() { rating.created_at.clone() } else { now.clone() },
            updated_at: if existing.is_some() { Some(now) } else { None },
            helpful_count: rating.helpful_count,
            reported: rating.reported,
            device_fingerprint: rating.device_fingerprint.clone(),
        })
    }

    /// Get a user's rating for a specific domain
    pub fn get_user_rating(&self, domain_id: i64, user_id: &str) -> Result<Option<DomainRating>> {
        let conn = Connection::open(&self.db_path)?;

        let result = conn.query_row(
            "SELECT id, domain_id, user_id, trust_rating, bias_rating, independence_rating, review_text,
                    created_at, updated_at, helpful_count, reported, device_fingerprint
             FROM domain_ratings WHERE domain_id = ?1 AND user_id = ?2",
            params![domain_id, user_id],
            |row| {
                Ok(DomainRating {
                    id: Some(row.get(0)?),
                    domain_id: row.get(1)?,
                    user_id: row.get(2)?,
                    trust_rating: row.get(3)?,
                    bias_rating: row.get(4)?,
                    independence_rating: row.get(5)?,
                    review_text: row.get(6)?,
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                    helpful_count: row.get(9)?,
                    reported: row.get(10)?,
                    device_fingerprint: row.get(11)?,
                })
            },
        );

        match result {
            Ok(rating) => Ok(Some(rating)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Get all ratings for a domain
    pub fn get_domain_ratings(&self, domain_id: i64, limit: Option<i64>) -> Result<Vec<DomainRating>> {
        let conn = Connection::open(&self.db_path)?;
        let limit_clause = limit.map(|l| format!(" LIMIT {}", l)).unwrap_or_default();

        let mut stmt = conn.prepare(&format!(
            "SELECT id, domain_id, user_id, trust_rating, bias_rating, independence_rating, review_text,
                    created_at, updated_at, helpful_count, reported, device_fingerprint
             FROM domain_ratings WHERE domain_id = ?1
             ORDER BY helpful_count DESC, created_at DESC{}",
            limit_clause
        ))?;

        let ratings = stmt.query_map(params![domain_id], |row| {
            Ok(DomainRating {
                id: Some(row.get(0)?),
                domain_id: row.get(1)?,
                user_id: row.get(2)?,
                trust_rating: row.get(3)?,
                bias_rating: row.get(4)?,
                independence_rating: row.get(5)?,
                review_text: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
                helpful_count: row.get(9)?,
                reported: row.get(10)?,
                device_fingerprint: row.get(11)?,
            })
        })?;

        ratings.collect()
    }

    /// Delete a rating
    pub fn delete_rating(&self, rating_id: i64, user_id: &str) -> Result<bool> {
        let conn = Connection::open(&self.db_path)?;

        // Get domain_id before deletion for aggregate update
        let domain_id: Option<i64> = conn.query_row(
            "SELECT domain_id FROM domain_ratings WHERE id = ?1 AND user_id = ?2",
            params![rating_id, user_id],
            |row| row.get(0),
        ).ok();

        let affected = conn.execute(
            "DELETE FROM domain_ratings WHERE id = ?1 AND user_id = ?2",
            params![rating_id, user_id],
        )?;

        if affected > 0 {
            if let Some(did) = domain_id {
                self.update_aggregates(did)?;
            }
        }

        Ok(affected > 0)
    }

    // ==================== Aggregates ====================

    /// Update aggregated ratings for a domain
    pub fn update_aggregates(&self, domain_id: i64) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;
        let now = chrono_now();

        // Calculate averages and distributions
        let (avg_trust, avg_bias, avg_independence, total): (f64, f64, f64, i64) = conn.query_row(
            "SELECT
                COALESCE(AVG(CAST(trust_rating AS REAL)), 3.0),
                COALESCE(AVG(CAST(bias_rating AS REAL)), 3.0),
                COALESCE(AVG(CAST(independence_rating AS REAL)), 2.5),
                COUNT(*)
             FROM domain_ratings WHERE domain_id = ?1",
            params![domain_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;

        // Trust distribution (1-5)
        let mut trust_dist = vec![0i64; 5];
        let mut stmt = conn.prepare(
            "SELECT trust_rating, COUNT(*) FROM domain_ratings
             WHERE domain_id = ?1 GROUP BY trust_rating"
        )?;
        let mut rows = stmt.query(params![domain_id])?;
        while let Some(row) = rows.next()? {
            let rating: i32 = row.get(0)?;
            let count: i64 = row.get(1)?;
            if rating >= 1 && rating <= 5 {
                trust_dist[(rating - 1) as usize] = count;
            }
        }

        // Bias distribution (1-5)
        let mut bias_dist = vec![0i64; 5];
        let mut stmt = conn.prepare(
            "SELECT bias_rating, COUNT(*) FROM domain_ratings
             WHERE domain_id = ?1 GROUP BY bias_rating"
        )?;
        let mut rows = stmt.query(params![domain_id])?;
        while let Some(row) = rows.next()? {
            let rating: i32 = row.get(0)?;
            let count: i64 = row.get(1)?;
            if rating >= 1 && rating <= 5 {
                bias_dist[(rating - 1) as usize] = count;
            }
        }

        // Independence distribution (1-4)
        let mut independence_dist = vec![0i64; 4];
        let mut stmt = conn.prepare(
            "SELECT independence_rating, COUNT(*) FROM domain_ratings
             WHERE domain_id = ?1 GROUP BY independence_rating"
        )?;
        let mut rows = stmt.query(params![domain_id])?;
        while let Some(row) = rows.next()? {
            let rating: i32 = row.get(0)?;
            let count: i64 = row.get(1)?;
            if rating >= 1 && rating <= 4 {
                independence_dist[(rating - 1) as usize] = count;
            }
        }

        let trust_json = serde_json::to_string(&trust_dist).unwrap_or_default();
        let bias_json = serde_json::to_string(&bias_dist).unwrap_or_default();
        let independence_json = serde_json::to_string(&independence_dist).unwrap_or_default();

        // Upsert aggregate
        conn.execute(
            "INSERT INTO domain_rating_aggregates
                (domain_id, avg_trust, avg_bias, avg_independence, total_ratings, trust_distribution, bias_distribution, independence_distribution, last_updated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(domain_id) DO UPDATE SET
                avg_trust = excluded.avg_trust,
                avg_bias = excluded.avg_bias,
                avg_independence = excluded.avg_independence,
                total_ratings = excluded.total_ratings,
                trust_distribution = excluded.trust_distribution,
                bias_distribution = excluded.bias_distribution,
                independence_distribution = excluded.independence_distribution,
                last_updated = excluded.last_updated",
            params![domain_id, avg_trust, avg_bias, avg_independence, total, trust_json, bias_json, independence_json, now],
        )?;

        Ok(())
    }

    /// Get aggregated ratings for a domain
    pub fn get_aggregate(&self, domain_id: i64) -> Result<Option<RatingAggregate>> {
        let conn = Connection::open(&self.db_path)?;

        let result = conn.query_row(
            "SELECT domain_id, avg_trust, avg_bias, avg_independence, total_ratings, trust_distribution, bias_distribution, independence_distribution, last_updated
             FROM domain_rating_aggregates WHERE domain_id = ?1",
            params![domain_id],
            |row| {
                let trust_json: String = row.get(5)?;
                let bias_json: String = row.get(6)?;
                let independence_json: String = row.get::<_, Option<String>>(7)?.unwrap_or_default();

                let trust_dist: Vec<i64> = serde_json::from_str(&trust_json).unwrap_or_else(|_| vec![0; 5]);
                let bias_dist: Vec<i64> = serde_json::from_str(&bias_json).unwrap_or_else(|_| vec![0; 5]);
                let independence_dist: Vec<i64> = serde_json::from_str(&independence_json).unwrap_or_else(|_| vec![0; 4]);

                Ok(RatingAggregate {
                    domain_id: row.get(0)?,
                    avg_trust: row.get(1)?,
                    avg_bias: row.get(2)?,
                    avg_independence: row.get::<_, Option<f64>>(3)?.unwrap_or(2.5),
                    total_ratings: row.get(4)?,
                    trust_distribution: trust_dist,
                    bias_distribution: bias_dist,
                    independence_distribution: independence_dist,
                    last_updated: row.get(8)?,
                })
            },
        );

        match result {
            Ok(agg) => Ok(Some(agg)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    // ==================== Rating Summary ====================

    /// Get a complete rating summary for a domain
    pub fn get_rating_summary(&self, domain_id: i64, domain_url: &str) -> Result<RatingSummary> {
        let aggregate = self.get_aggregate(domain_id)?;

        let (avg_trust, avg_bias, avg_independence, total_ratings) = match &aggregate {
            Some(agg) => (agg.avg_trust, agg.avg_bias, agg.avg_independence, agg.total_ratings),
            None => (3.0, 3.0, 2.5, 0),
        };

        // Trust: 1=Trusted, 5=Sketchy
        let trust_label = match avg_trust {
            t if t < 1.5 => "Trusted",
            t if t < 2.5 => "Mostly Trusted",
            t if t < 3.5 => "Mixed",
            t if t < 4.5 => "Questionable",
            _ => "Sketchy",
        }.to_string();

        // Bias: 1=Far Left, 3=Center, 5=Far Right
        let bias_label = match avg_bias {
            b if b < 1.5 => "Far Left",
            b if b < 2.5 => "Left",
            b if b < 3.5 => "Center",
            b if b < 4.5 => "Right",
            _ => "Far Right",
        }.to_string();

        // Independence: 1=Biased, 2=Neutral, 3=Independent, 4=Unbiased
        let independence_label = match avg_independence {
            i if i < 1.5 => "Biased",
            i if i < 2.5 => "Neutral",
            i if i < 3.5 => "Independent",
            _ => "Unbiased",
        }.to_string();

        // Get category scores if available
        let category_scores = self.get_category_averages(domain_id)?;

        Ok(RatingSummary {
            domain_url: domain_url.to_string(),
            avg_trust,
            avg_bias,
            avg_independence,
            total_ratings,
            trust_label,
            bias_label,
            independence_label,
            category_scores,
        })
    }

    /// Get average scores per category for a domain
    fn get_category_averages(&self, domain_id: i64) -> Result<HashMap<String, f64>> {
        let conn = Connection::open(&self.db_path)?;

        let mut stmt = conn.prepare(
            "SELECT rc.category, AVG(CAST(rc.score AS REAL))
             FROM rating_categories rc
             INNER JOIN domain_ratings dr ON rc.domain_rating_id = dr.id
             WHERE dr.domain_id = ?1
             GROUP BY rc.category"
        )?;

        let mut map = HashMap::new();
        let rows = stmt.query_map(params![domain_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
        })?;

        for row in rows {
            if let Ok((cat, score)) = row {
                map.insert(cat, score);
            }
        }

        Ok(map)
    }

    // ==================== Subdomain Ratings ====================

    /// Submit a subdomain-specific rating
    pub fn submit_subdomain_rating(&self, parent_domain_id: i64, subdomain: &str, trust: f64, bias: f64) -> Result<SubdomainRating> {
        let conn = Connection::open(&self.db_path)?;

        // Check if subdomain already exists
        let existing: Option<(i64, i64)> = conn.query_row(
            "SELECT id, total_ratings FROM subdomain_ratings
             WHERE parent_domain_id = ?1 AND subdomain = ?2",
            params![parent_domain_id, subdomain],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).ok();

        if let Some((id, total)) = existing {
            // Update with weighted average
            let new_total = total + 1;
            conn.execute(
                "UPDATE subdomain_ratings SET
                    avg_trust = (avg_trust * ?1 + ?2) / ?3,
                    avg_bias = (avg_bias * ?1 + ?4) / ?3,
                    total_ratings = ?3
                 WHERE id = ?5",
                params![total, trust, new_total, bias, id],
            )?;

            Ok(SubdomainRating {
                id: Some(id),
                parent_domain_id,
                subdomain: subdomain.to_string(),
                avg_trust: trust,
                avg_bias: bias,
                total_ratings: new_total,
            })
        } else {
            conn.execute(
                "INSERT INTO subdomain_ratings (parent_domain_id, subdomain, avg_trust, avg_bias, total_ratings)
                 VALUES (?1, ?2, ?3, ?4, 1)",
                params![parent_domain_id, subdomain, trust, bias],
            )?;

            let id = conn.last_insert_rowid();
            Ok(SubdomainRating {
                id: Some(id),
                parent_domain_id,
                subdomain: subdomain.to_string(),
                avg_trust: trust,
                avg_bias: bias,
                total_ratings: 1,
            })
        }
    }

    /// Get subdomain ratings for a parent domain
    pub fn get_subdomain_ratings(&self, parent_domain_id: i64) -> Result<Vec<SubdomainRating>> {
        let conn = Connection::open(&self.db_path)?;

        let mut stmt = conn.prepare(
            "SELECT id, parent_domain_id, subdomain, avg_trust, avg_bias, total_ratings
             FROM subdomain_ratings WHERE parent_domain_id = ?1 ORDER BY total_ratings DESC"
        )?;

        let ratings = stmt.query_map(params![parent_domain_id], |row| {
            Ok(SubdomainRating {
                id: Some(row.get(0)?),
                parent_domain_id: row.get(1)?,
                subdomain: row.get(2)?,
                avg_trust: row.get(3)?,
                avg_bias: row.get(4)?,
                total_ratings: row.get(5)?,
            })
        })?;

        ratings.collect()
    }

    // ==================== Helpful / Report ====================

    /// Mark a rating as helpful
    pub fn mark_helpful(&self, rating_id: i64) -> Result<i32> {
        let conn = Connection::open(&self.db_path)?;

        conn.execute(
            "UPDATE domain_ratings SET helpful_count = helpful_count + 1 WHERE id = ?1",
            params![rating_id],
        )?;

        let count: i32 = conn.query_row(
            "SELECT helpful_count FROM domain_ratings WHERE id = ?1",
            params![rating_id],
            |row| row.get(0),
        )?;

        Ok(count)
    }

    /// Report a rating
    pub fn report_rating(&self, rating_id: i64) -> Result<bool> {
        let conn = Connection::open(&self.db_path)?;

        let affected = conn.execute(
            "UPDATE domain_ratings SET reported = TRUE WHERE id = ?1",
            params![rating_id],
        )?;

        Ok(affected > 0)
    }

    // ==================== User History ====================

    /// Get a user's rating history
    pub fn get_user_history(&self, user_id: &str) -> Result<UserRatingHistory> {
        let conn = Connection::open(&self.db_path)?;

        let mut stmt = conn.prepare(
            "SELECT id, domain_id, user_id, trust_rating, bias_rating, independence_rating, review_text,
                    created_at, updated_at, helpful_count, reported, device_fingerprint
             FROM domain_ratings WHERE user_id = ?1 ORDER BY created_at DESC"
        )?;

        let ratings: Vec<DomainRating> = stmt.query_map(params![user_id], |row| {
            Ok(DomainRating {
                id: Some(row.get(0)?),
                domain_id: row.get(1)?,
                user_id: row.get(2)?,
                trust_rating: row.get(3)?,
                bias_rating: row.get(4)?,
                independence_rating: row.get(5)?,
                review_text: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
                helpful_count: row.get(9)?,
                reported: row.get(10)?,
                device_fingerprint: row.get(11)?,
            })
        })?.filter_map(|r| r.ok()).collect();

        let total = ratings.len() as i64;
        let (avg_trust, avg_bias) = if total > 0 {
            let sum_trust: i32 = ratings.iter().map(|r| r.trust_rating).sum();
            let sum_bias: i32 = ratings.iter().map(|r| r.bias_rating).sum();
            (sum_trust as f64 / total as f64, sum_bias as f64 / total as f64)
        } else {
            (3.0, 3.0)
        };

        Ok(UserRatingHistory {
            ratings,
            total_ratings: total,
            avg_trust_given: avg_trust,
            avg_bias_given: avg_bias,
        })
    }

    // ==================== Category Ratings ====================

    /// Add category scores to a rating
    pub fn add_category_scores(&self, rating_id: i64, categories: Vec<(String, i32)>) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;

        // Clear existing categories for this rating
        conn.execute(
            "DELETE FROM rating_categories WHERE domain_rating_id = ?1",
            params![rating_id],
        )?;

        // Insert new categories
        for (category, score) in categories {
            conn.execute(
                "INSERT INTO rating_categories (domain_rating_id, category, score) VALUES (?1, ?2, ?3)",
                params![rating_id, category, score],
            )?;
        }

        Ok(())
    }

    /// Get category scores for a rating
    pub fn get_rating_categories(&self, rating_id: i64) -> Result<Vec<RatingCategory>> {
        let conn = Connection::open(&self.db_path)?;

        let mut stmt = conn.prepare(
            "SELECT id, domain_rating_id, category, score FROM rating_categories WHERE domain_rating_id = ?1"
        )?;

        let categories = stmt.query_map(params![rating_id], |row| {
            Ok(RatingCategory {
                id: Some(row.get(0)?),
                domain_rating_id: row.get(1)?,
                category: row.get(2)?,
                score: row.get(3)?,
            })
        })?;

        categories.collect()
    }
}

fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", duration.as_secs())
}
