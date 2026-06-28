-- Add device fingerprint for deduplication and independence rating dimension

-- Add device_fingerprint and independence_level to domain_ratings
ALTER TABLE domain_ratings
ADD COLUMN device_fingerprint VARCHAR(64),
ADD COLUMN independence_level INTEGER CHECK (independence_level BETWEEN 1 AND 4);

-- Add independence distribution to aggregates
ALTER TABLE domain_rating_aggregates
ADD COLUMN avg_independence_level FLOAT NOT NULL DEFAULT 0,
ADD COLUMN independence_distribution JSONB NOT NULL DEFAULT '{}';

-- Create index on device_fingerprint for deduplication lookups
CREATE INDEX idx_domain_ratings_device_fingerprint ON domain_ratings(device_fingerprint);

-- Create a unique constraint on domain + device_fingerprint for deduplication
-- This allows a user to rate a domain once per device
CREATE UNIQUE INDEX idx_domain_ratings_domain_device
ON domain_ratings(domain_url, device_fingerprint)
WHERE device_fingerprint IS NOT NULL;

-- Create rate limiting table
CREATE TABLE rating_rate_limits (
    id BIGSERIAL PRIMARY KEY,
    device_fingerprint VARCHAR(64) NOT NULL,
    window_start TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    rating_count INTEGER NOT NULL DEFAULT 1,
    UNIQUE(device_fingerprint, window_start)
);

CREATE INDEX idx_rate_limits_device ON rating_rate_limits(device_fingerprint);
CREATE INDEX idx_rate_limits_window ON rating_rate_limits(window_start);
