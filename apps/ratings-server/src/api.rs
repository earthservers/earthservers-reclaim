use crate::models::*;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use sqlx::PgPool;

pub async fn submit_rating(
    State(pool): State<PgPool>,
    Json(req): Json<SubmitRatingRequest>,
) -> Result<Json<Rating>, StatusCode> {
    // Validate input
    if req.trust_level < 1 || req.trust_level > 5 {
        return Err(StatusCode::BAD_REQUEST);
    }
    if req.bias_level < 1 || req.bias_level > 4 {
        return Err(StatusCode::BAD_REQUEST);
    }
    if let Some(independence) = req.independence_level {
        if independence < 1 || independence > 4 {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    // Check rate limiting by device fingerprint (15 ratings per hour)
    if let Some(ref fingerprint) = req.device_fingerprint {
        let hour_ago = chrono::Utc::now() - chrono::Duration::hours(1);
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM domain_ratings WHERE device_fingerprint = $1 AND created_at > $2"
        )
        .bind(fingerprint)
        .bind(hour_ago)
        .fetch_one(&pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error during rate limit check: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        if count.0 >= 15 {
            tracing::warn!("Rate limit exceeded for device fingerprint");
            return Err(StatusCode::TOO_MANY_REQUESTS);
        }
    }

    // Insert or update rating
    // Uses device_fingerprint for deduplication if available, otherwise falls back to user_hash
    let rating = if req.device_fingerprint.is_some() {
        sqlx::query_as!(
            Rating,
            r#"
            INSERT INTO domain_ratings (domain_url, user_hash, device_fingerprint, trust_level, bias_level, independence_level, comment)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (domain_url, device_fingerprint) WHERE device_fingerprint IS NOT NULL
            DO UPDATE SET
                user_hash = $2,
                trust_level = $4,
                bias_level = $5,
                independence_level = $6,
                comment = $7,
                updated_at = NOW()
            RETURNING id, domain_url, user_hash, device_fingerprint, trust_level, bias_level, independence_level, comment, created_at, updated_at
            "#,
            req.domain_url,
            req.user_hash,
            req.device_fingerprint,
            req.trust_level,
            req.bias_level,
            req.independence_level,
            req.comment,
        )
        .fetch_one(&pool)
        .await
    } else {
        sqlx::query_as!(
            Rating,
            r#"
            INSERT INTO domain_ratings (domain_url, user_hash, device_fingerprint, trust_level, bias_level, independence_level, comment)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (domain_url, user_hash)
            DO UPDATE SET
                trust_level = $4,
                bias_level = $5,
                independence_level = $6,
                comment = $7,
                updated_at = NOW()
            RETURNING id, domain_url, user_hash, device_fingerprint, trust_level, bias_level, independence_level, comment, created_at, updated_at
            "#,
            req.domain_url,
            req.user_hash,
            req.device_fingerprint,
            req.trust_level,
            req.bias_level,
            req.independence_level,
            req.comment,
        )
        .fetch_one(&pool)
        .await
    }.map_err(|e| {
        tracing::error!("Database error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Refresh aggregates
    refresh_aggregates(&pool, &req.domain_url).await?;

    Ok(Json(rating))
}

pub async fn get_domain_rating(
    State(pool): State<PgPool>,
    Path(domain): Path<String>,
) -> Result<Json<RatingAggregate>, StatusCode> {
    let aggregate = sqlx::query_as!(
        RatingAggregate,
        r#"
        SELECT
            domain_url,
            avg_trust_level,
            avg_bias_level,
            avg_independence_level,
            total_ratings,
            trust_distribution,
            bias_distribution,
            independence_distribution
        FROM domain_rating_aggregates
        WHERE domain_url = $1
        "#,
        domain
    )
    .fetch_optional(&pool)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    match aggregate {
        Some(agg) => Ok(Json(agg)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn get_domain_reviews(
    State(pool): State<PgPool>,
    Path(domain): Path<String>,
) -> Result<Json<Vec<Rating>>, StatusCode> {
    let ratings = sqlx::query_as!(
        Rating,
        r#"
        SELECT id, domain_url, user_hash, device_fingerprint, trust_level, bias_level, independence_level, comment, created_at, updated_at
        FROM domain_ratings
        WHERE domain_url = $1
        ORDER BY created_at DESC
        LIMIT 50
        "#,
        domain
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(ratings))
}

pub async fn vote_helpful(
    State(pool): State<PgPool>,
    Path(rating_id): Path<i64>,
    Json(req): Json<VoteRequest>,
) -> Result<StatusCode, StatusCode> {
    sqlx::query!(
        r#"
        INSERT INTO rating_votes (rating_id, voter_hash, is_helpful)
        VALUES ($1, $2, $3)
        ON CONFLICT (rating_id, voter_hash)
        DO UPDATE SET is_helpful = $3
        "#,
        rating_id,
        req.voter_hash,
        req.is_helpful,
    )
    .execute(&pool)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(StatusCode::CREATED)
}

pub async fn report_rating(
    State(pool): State<PgPool>,
    Path(rating_id): Path<i64>,
    Json(req): Json<ReportRequest>,
) -> Result<StatusCode, StatusCode> {
    sqlx::query!(
        r#"
        INSERT INTO rating_reports (rating_id, reporter_hash, reason)
        VALUES ($1, $2, $3)
        "#,
        rating_id,
        req.reporter_hash,
        req.reason,
    )
    .execute(&pool)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(StatusCode::CREATED)
}

async fn refresh_aggregates(pool: &PgPool, domain_url: &str) -> Result<(), StatusCode> {
    // Calculate and update aggregates including independence distribution
    sqlx::query!(
        r#"
        INSERT INTO domain_rating_aggregates
            (domain_url, avg_trust_level, avg_bias_level, avg_independence_level, total_ratings, trust_distribution, bias_distribution, independence_distribution)
        SELECT
            $1 as domain_url,
            COALESCE(AVG(trust_level::float), 0) as avg_trust_level,
            COALESCE(AVG(bias_level::float), 0) as avg_bias_level,
            COALESCE(AVG(independence_level::float), 0) as avg_independence_level,
            COUNT(*) as total_ratings,
            COALESCE(
                jsonb_object_agg(
                    trust_level::text,
                    trust_count
                ) FILTER (WHERE trust_level IS NOT NULL),
                '{}'::jsonb
            ) as trust_distribution,
            COALESCE(
                jsonb_object_agg(
                    bias_level::text,
                    bias_count
                ) FILTER (WHERE bias_level IS NOT NULL),
                '{}'::jsonb
            ) as bias_distribution,
            COALESCE(
                jsonb_object_agg(
                    independence_level::text,
                    independence_count
                ) FILTER (WHERE independence_level IS NOT NULL),
                '{}'::jsonb
            ) as independence_distribution
        FROM (
            SELECT
                trust_level,
                bias_level,
                independence_level,
                COUNT(*) OVER (PARTITION BY trust_level) as trust_count,
                COUNT(*) OVER (PARTITION BY bias_level) as bias_count,
                COUNT(*) OVER (PARTITION BY independence_level) as independence_count
            FROM domain_ratings
            WHERE domain_url = $1
        ) sub
        GROUP BY 1
        ON CONFLICT (domain_url)
        DO UPDATE SET
            avg_trust_level = EXCLUDED.avg_trust_level,
            avg_bias_level = EXCLUDED.avg_bias_level,
            avg_independence_level = EXCLUDED.avg_independence_level,
            total_ratings = EXCLUDED.total_ratings,
            trust_distribution = EXCLUDED.trust_distribution,
            bias_distribution = EXCLUDED.bias_distribution,
            independence_distribution = EXCLUDED.independence_distribution,
            updated_at = NOW()
        "#,
        domain_url
    )
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to refresh aggregates: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(())
}
