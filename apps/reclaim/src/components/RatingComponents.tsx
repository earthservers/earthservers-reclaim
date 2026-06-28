// Rating Components for Community Trust & Bias Rating System
// RatingBadge, RatingForm, RatingDisplay for domain ratings

import { useState, useEffect } from 'react';
import { invoke } from '../lib/tauri';
import { getRatingSubmissionIdentity, getViewingIdentity } from '../lib/userIdentity';

// ==================== Types ====================

interface RatingAggregate {
  domain_id: number;
  avg_trust: number;
  avg_bias: number;
  avg_independence: number;
  total_ratings: number;
  trust_distribution: number[];
  bias_distribution: number[];
  independence_distribution: number[];
  last_updated: string | null;
}

interface RatingSummary {
  domain_url: string;
  avg_trust: number;
  avg_bias: number;
  avg_independence: number;
  total_ratings: number;
  trust_label: string;
  bias_label: string;
  independence_label: string;
  category_scores: Record<string, number>;
}

interface DomainRating {
  id: number | null;
  domain_id: number;
  user_id: string;
  trust_rating: number;
  bias_rating: number;
  independence_rating: number;
  review_text: string | null;
  created_at: string;
  updated_at: string | null;
  helpful_count: number;
  reported: boolean;
  device_fingerprint: string | null;
}

// ==================== Rating Badge ====================

interface RatingBadgeProps {
  domainId: number;
  size?: 'sm' | 'md' | 'lg';
  showBias?: boolean;
  onClick?: () => void;
}

const trustColors: Record<string, string> = {
  'Trusted': '#10b981',
  'Mostly Trusted': '#22c55e',
  'Mixed': '#eab308',
  'Questionable': '#f97316',
  'Sketchy': '#ef4444',
};

const biasColors: Record<string, string> = {
  'Far Left': '#1d4ed8',
  'Left': '#3b82f6',
  'Center': '#8b5cf6',
  'Right': '#ec4899',
  'Far Right': '#ef4444',
};

const independenceColors: Record<string, string> = {
  'Biased': '#ef4444',
  'Neutral': '#eab308',
  'Independent': '#3b82f6',
  'Unbiased': '#10b981',
};

export function RatingBadge({ domainId, size = 'md', showBias = true, onClick }: RatingBadgeProps) {
  const [summary, setSummary] = useState<RatingSummary | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    loadSummary();
  }, [domainId]);

  const loadSummary = async () => {
    try {
      const data = await invoke<RatingSummary>('get_rating_summary', {
        domainId,
        domainUrl: '',
      });
      setSummary(data);
    } catch (err) {
      console.error('Failed to load rating summary:', err);
    } finally {
      setLoading(false);
    }
  };

  if (loading) {
    return <div className={`rating-badge rating-badge--${size} rating-badge--loading`}>...</div>;
  }

  if (!summary || summary.total_ratings === 0) {
    return (
      <div
        className={`rating-badge rating-badge--${size} rating-badge--no-ratings`}
        onClick={onClick}
        style={{ cursor: onClick ? 'pointer' : 'default' }}
      >
        <span className="rating-badge__trust">No ratings</span>
      </div>
    );
  }

  const trustColor = trustColors[summary.trust_label] || '#6b7280';
  const biasColor = biasColors[summary.bias_label] || '#6b7280';

  return (
    <div
      className={`rating-badge rating-badge--${size}`}
      onClick={onClick}
      style={{ cursor: onClick ? 'pointer' : 'default' }}
    >
      <div className="rating-badge__trust" style={{ backgroundColor: trustColor }}>
        <span className="rating-badge__score">{summary.avg_trust.toFixed(1)}</span>
        <span className="rating-badge__label">{summary.trust_label}</span>
      </div>
      {showBias && (
        <div className="rating-badge__bias" style={{ backgroundColor: biasColor }}>
          <span className="rating-badge__bias-label">{summary.bias_label}</span>
        </div>
      )}
      <span className="rating-badge__count">({summary.total_ratings})</span>

      <style>{`
        .rating-badge {
          display: inline-flex;
          align-items: center;
          gap: 4px;
          font-family: system-ui, -apple-system, sans-serif;
        }
        .rating-badge--sm { font-size: 11px; }
        .rating-badge--md { font-size: 13px; }
        .rating-badge--lg { font-size: 15px; }
        .rating-badge--loading { color: #6b7280; }
        .rating-badge--no-ratings { color: #9ca3af; font-style: italic; }
        .rating-badge__trust {
          display: flex;
          align-items: center;
          gap: 4px;
          padding: 2px 8px;
          border-radius: 4px;
          color: white;
          font-weight: 600;
        }
        .rating-badge__score { font-size: 1.1em; }
        .rating-badge__label { opacity: 0.9; }
        .rating-badge__bias {
          padding: 2px 6px;
          border-radius: 4px;
          color: white;
          font-size: 0.9em;
        }
        .rating-badge__count { color: #6b7280; font-size: 0.85em; }
      `}</style>
    </div>
  );
}

// ==================== Rating Form ====================

interface RatingFormProps {
  domainId?: number;
  domainUrl: string;
  onSubmit?: (rating: DomainRating) => void;
  onCancel?: () => void;
}

const TRUST_LABELS = ['Trusted', 'Mostly Trusted', 'Mixed', 'Questionable', 'Sketchy'];
const BIAS_LABELS = ['Far Left', 'Left', 'Center', 'Right', 'Far Right'];
const INDEPENDENCE_LABELS = ['Biased', 'Neutral', 'Independent', 'Unbiased'];
const CATEGORIES = ['Accuracy', 'Transparency', 'Sourcing', 'Editorial Standards', 'Fact-Checking'];

// Extract domain from URL
function extractDomain(url: string): string {
  try {
    return new URL(url).hostname;
  } catch {
    return url;
  }
}

export function RatingForm({ domainId, domainUrl, onSubmit, onCancel }: RatingFormProps) {
  const [trustRating, setTrustRating] = useState(3);
  const [biasRating, setBiasRating] = useState(3);
  const [independenceRating, setIndependenceRating] = useState(2);
  const [reviewText, setReviewText] = useState('');
  const [categoryScores, setCategoryScores] = useState<Record<string, number>>({});
  const [showCategories, setShowCategories] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [existingRating, setExistingRating] = useState<DomainRating | null>(null);
  const [canSubmit, setCanSubmit] = useState(true);

  const domain = extractDomain(domainUrl);

  useEffect(() => {
    initializeIdentity();
  }, [domainId, domainUrl]);

  const initializeIdentity = async () => {
    const viewIdentity = getViewingIdentity();
    setCanSubmit(viewIdentity.canSubmit);

    if (viewIdentity.userId && domainId) {
      await loadExistingRating(viewIdentity.userId);
    }
  };

  const loadExistingRating = async (uid: string) => {
    if (!domainId) return; // Skip if no domainId (URL-only mode)

    try {
      const rating = await invoke<DomainRating | null>('get_user_rating', {
        domainId,
        userId: uid,
      });
      if (rating) {
        setExistingRating(rating);
        setTrustRating(rating.trust_rating);
        setBiasRating(rating.bias_rating);
        setIndependenceRating(rating.independence_rating);
        setReviewText(rating.review_text || '');
      }
    } catch (err) {
      console.error('Failed to load existing rating:', err);
    }
  };

  const handleSubmit = async () => {
    if (!canSubmit) {
      console.error('Cannot submit rating: fingerprinting is disabled');
      return;
    }

    setSubmitting(true);
    try {
      const identity = await getRatingSubmissionIdentity();
      if (!identity) {
        console.error('Cannot get identity for rating submission');
        return;
      }

      // If we have a domainId, use local Tauri commands
      // Otherwise, submit to the ratings-server API
      if (domainId) {
        const rating = {
          id: existingRating?.id || null,
          domain_id: domainId,
          user_id: identity.userId,
          trust_rating: trustRating,
          bias_rating: biasRating,
          independence_rating: independenceRating,
          review_text: reviewText || null,
          created_at: existingRating?.created_at || '',
          updated_at: null,
          helpful_count: existingRating?.helpful_count || 0,
          reported: existingRating?.reported || false,
          device_fingerprint: identity.deviceFingerprint,
        };

        const savedRating = await invoke<DomainRating>('submit_rating', { rating });

        // Submit category scores if provided
        if (Object.keys(categoryScores).length > 0 && savedRating.id) {
          const categories = Object.entries(categoryScores).map(([cat, score]) => [cat, score] as [string, number]);
          await invoke('add_rating_category_scores', {
            ratingId: savedRating.id,
            categories,
          });
        }

        onSubmit?.(savedRating);
      } else {
        // Submit to ratings-server API
        const RATINGS_SERVER = 'https://ratings.earthreclaim.earth';

        const response = await fetch(`${RATINGS_SERVER}/api/ratings`, {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
          },
          body: JSON.stringify({
            domain: domain,
            user_id: identity.userId,
            device_fingerprint: identity.deviceFingerprint,
            trust_rating: trustRating,
            bias_rating: biasRating,
            independence_rating: independenceRating,
            review_text: reviewText || null,
          }),
        });

        if (!response.ok) {
          const errorData = await response.json().catch(() => ({}));
          throw new Error(errorData.error || `Server returned ${response.status}`);
        }

        const savedRating = await response.json();
        onSubmit?.(savedRating);
      }
    } catch (err) {
      console.error('Failed to submit rating:', err);
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="rating-form">
      <h3 className="rating-form__title">
        Rate: <span className="rating-form__domain">{domain}</span>
      </h3>

      {!canSubmit && (
        <div className="rating-form__disabled-notice">
          Rating submission is disabled because hardware fingerprinting is turned off in your privacy settings.
          You can still view community ratings.
        </div>
      )}

      {/* Trust Rating */}
      <div className="rating-form__section">
        <label className="rating-form__label">Trust Rating</label>
        <div className="rating-form__stars">
          {[1, 2, 3, 4, 5].map((value) => (
            <button
              key={value}
              type="button"
              className={`rating-form__star ${value <= trustRating ? 'rating-form__star--active' : ''}`}
              onClick={() => setTrustRating(value)}
              style={{ color: value <= trustRating ? trustColors[TRUST_LABELS[value - 1]] : '#4b5563' }}
            >
              ★
            </button>
          ))}
          <span className="rating-form__value">{TRUST_LABELS[trustRating - 1]}</span>
        </div>
      </div>

      {/* Bias Rating (5-point scale) */}
      <div className="rating-form__section">
        <label className="rating-form__label">Political Bias</label>
        <div className="rating-form__bias-slider">
          {BIAS_LABELS.map((label, index) => (
            <button
              key={label}
              type="button"
              className={`rating-form__bias-btn ${biasRating === index + 1 ? 'rating-form__bias-btn--active' : ''}`}
              onClick={() => setBiasRating(index + 1)}
              style={{
                backgroundColor: biasRating === index + 1 ? biasColors[label] : 'transparent',
                borderColor: biasColors[label],
              }}
            >
              {label}
            </button>
          ))}
        </div>
      </div>

      {/* Independence Rating */}
      <div className="rating-form__section">
        <label className="rating-form__label">Independence Rating</label>
        <div className="rating-form__independence">
          {INDEPENDENCE_LABELS.map((label, index) => (
            <button
              key={label}
              type="button"
              className={`rating-form__independence-btn ${independenceRating === index + 1 ? 'rating-form__independence-btn--active' : ''}`}
              onClick={() => setIndependenceRating(index + 1)}
              style={{
                backgroundColor: independenceRating === index + 1 ? independenceColors[label] : 'transparent',
                borderColor: independenceColors[label],
              }}
            >
              {label}
            </button>
          ))}
        </div>
        <p className="rating-form__hint">
          How independent is this source from corporate/political influence?
        </p>
      </div>

      {/* Review Text */}
      <div className="rating-form__section">
        <label className="rating-form__label">Review (optional)</label>
        <textarea
          className="rating-form__textarea"
          value={reviewText}
          onChange={(e) => setReviewText(e.target.value)}
          placeholder="Share your experience with this source..."
          rows={3}
        />
      </div>

      {/* Category Scores (Expandable) */}
      <div className="rating-form__section">
        <button
          type="button"
          className="rating-form__toggle"
          onClick={() => setShowCategories(!showCategories)}
        >
          {showCategories ? '▼' : '▶'} Detailed Category Ratings
        </button>

        {showCategories && (
          <div className="rating-form__categories">
            {CATEGORIES.map((category) => (
              <div key={category} className="rating-form__category">
                <span className="rating-form__category-name">{category}</span>
                <div className="rating-form__category-stars">
                  {[1, 2, 3, 4, 5].map((value) => (
                    <button
                      key={value}
                      type="button"
                      className={`rating-form__star rating-form__star--sm ${
                        (categoryScores[category] || 0) >= value ? 'rating-form__star--active' : ''
                      }`}
                      onClick={() =>
                        setCategoryScores((prev) => ({ ...prev, [category]: value }))
                      }
                    >
                      ★
                    </button>
                  ))}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Actions */}
      <div className="rating-form__actions">
        <button
          type="button"
          className="rating-form__btn rating-form__btn--cancel"
          onClick={onCancel}
          disabled={submitting}
        >
          Cancel
        </button>
        <button
          type="button"
          className="rating-form__btn rating-form__btn--submit"
          onClick={handleSubmit}
          disabled={submitting || !canSubmit}
        >
          {submitting ? 'Submitting...' : existingRating ? 'Update Rating' : 'Submit Rating'}
        </button>
      </div>

      <style>{`
        .rating-form {
          background: var(--color-card, #1a1a2e);
          border-radius: 12px;
          padding: 20px;
          max-width: 500px;
        }
        .rating-form__title {
          margin: 0 0 16px;
          font-size: 16px;
          color: var(--color-text, #f0f0f0);
        }
        .rating-form__domain {
          color: var(--color-primary, #0fab89);
          word-break: break-all;
        }
        .rating-form__section {
          margin-bottom: 16px;
        }
        .rating-form__label {
          display: block;
          margin-bottom: 8px;
          font-size: 14px;
          font-weight: 500;
          color: var(--color-text, #f0f0f0);
        }
        .rating-form__stars {
          display: flex;
          align-items: center;
          gap: 4px;
        }
        .rating-form__star {
          background: none;
          border: none;
          font-size: 28px;
          cursor: pointer;
          transition: transform 0.1s;
        }
        .rating-form__star--sm { font-size: 18px; }
        .rating-form__star:hover { transform: scale(1.2); }
        .rating-form__star--active { text-shadow: 0 0 8px currentColor; }
        .rating-form__value {
          margin-left: 12px;
          font-size: 14px;
          color: #9ca3af;
        }
        .rating-form__bias-slider {
          display: flex;
          gap: 8px;
          flex-wrap: wrap;
        }
        .rating-form__bias-btn {
          padding: 6px 12px;
          border: 2px solid;
          border-radius: 20px;
          background: transparent;
          color: var(--color-text, #f0f0f0);
          cursor: pointer;
          transition: all 0.2s;
          font-size: 13px;
        }
        .rating-form__bias-btn--active {
          color: white;
        }
        .rating-form__independence {
          display: flex;
          gap: 8px;
          flex-wrap: wrap;
        }
        .rating-form__independence-btn {
          padding: 8px 16px;
          border: 2px solid;
          border-radius: 20px;
          background: transparent;
          color: var(--color-text, #f0f0f0);
          cursor: pointer;
          transition: all 0.2s;
          font-size: 13px;
        }
        .rating-form__independence-btn--active {
          color: white;
        }
        .rating-form__hint {
          margin: 8px 0 0;
          font-size: 12px;
          color: #6b7280;
          font-style: italic;
        }
        .rating-form__disabled-notice {
          padding: 12px;
          background: rgba(239, 68, 68, 0.1);
          border: 1px solid rgba(239, 68, 68, 0.3);
          border-radius: 8px;
          color: #ef4444;
          font-size: 13px;
          margin-bottom: 16px;
        }
        .rating-form__textarea {
          width: 100%;
          padding: 10px;
          border: 1px solid #374151;
          border-radius: 8px;
          background: rgba(0, 0, 0, 0.3);
          color: var(--color-text, #f0f0f0);
          font-size: 14px;
          resize: vertical;
        }
        .rating-form__textarea:focus {
          outline: none;
          border-color: var(--color-primary, #0fab89);
        }
        .rating-form__toggle {
          background: none;
          border: none;
          color: var(--color-primary, #0fab89);
          cursor: pointer;
          font-size: 14px;
          padding: 0;
        }
        .rating-form__categories {
          margin-top: 12px;
          display: flex;
          flex-direction: column;
          gap: 8px;
        }
        .rating-form__category {
          display: flex;
          justify-content: space-between;
          align-items: center;
        }
        .rating-form__category-name {
          font-size: 13px;
          color: #9ca3af;
        }
        .rating-form__category-stars {
          display: flex;
          gap: 2px;
        }
        .rating-form__actions {
          display: flex;
          justify-content: flex-end;
          gap: 12px;
          margin-top: 20px;
        }
        .rating-form__btn {
          padding: 10px 20px;
          border: none;
          border-radius: 8px;
          font-size: 14px;
          font-weight: 500;
          cursor: pointer;
          transition: opacity 0.2s;
        }
        .rating-form__btn:disabled {
          opacity: 0.5;
          cursor: not-allowed;
        }
        .rating-form__btn--cancel {
          background: #374151;
          color: #f0f0f0;
        }
        .rating-form__btn--submit {
          background: var(--color-primary, #0fab89);
          color: white;
        }
      `}</style>
    </div>
  );
}

// ==================== Rating Display ====================

interface RatingDisplayProps {
  domainId: number;
  domainUrl: string;
  showReviews?: boolean;
  maxReviews?: number;
}

export function RatingDisplay({ domainId, domainUrl, showReviews = true, maxReviews = 5 }: RatingDisplayProps) {
  const [aggregate, setAggregate] = useState<RatingAggregate | null>(null);
  const [ratings, setRatings] = useState<DomainRating[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    loadRatingData();
  }, [domainId]);

  const loadRatingData = async () => {
    try {
      const [agg, reviews] = await Promise.all([
        invoke<RatingAggregate | null>('get_rating_aggregate', { domainId }),
        showReviews ? invoke<DomainRating[]>('get_domain_ratings', { domainId, limit: maxReviews }) : Promise.resolve([]),
      ]);
      setAggregate(agg);
      setRatings(reviews);
    } catch (err) {
      console.error('Failed to load rating data:', err);
    } finally {
      setLoading(false);
    }
  };

  const handleHelpful = async (ratingId: number) => {
    try {
      await invoke('mark_rating_helpful', { ratingId });
      // Refresh ratings
      loadRatingData();
    } catch (err) {
      console.error('Failed to mark as helpful:', err);
    }
  };

  if (loading) {
    return <div className="rating-display rating-display--loading">Loading ratings...</div>;
  }

  const getTrustLabel = (score: number) => {
    if (score < 1.5) return 'Trusted';
    if (score < 2.5) return 'Mostly Trusted';
    if (score < 3.5) return 'Mixed';
    if (score < 4.5) return 'Questionable';
    return 'Sketchy';
  };

  const getBiasLabel = (score: number) => {
    if (score < 1.5) return 'Far Left';
    if (score < 2.5) return 'Left';
    if (score < 3.5) return 'Center';
    if (score < 4.5) return 'Right';
    return 'Far Right';
  };

  const getIndependenceLabel = (score: number) => {
    if (score < 1.5) return 'Biased';
    if (score < 2.5) return 'Neutral';
    if (score < 3.5) return 'Independent';
    return 'Unbiased';
  };

  return (
    <div className="rating-display">
      <h3 className="rating-display__title">Community Ratings for {domainUrl}</h3>

      {!aggregate || aggregate.total_ratings === 0 ? (
        <p className="rating-display__empty">No community ratings yet. Be the first to rate!</p>
      ) : (
        <>
          {/* Summary */}
          <div className="rating-display__summary">
            <div className="rating-display__score-block">
              <div
                className="rating-display__main-score"
                style={{ backgroundColor: trustColors[getTrustLabel(aggregate.avg_trust)] }}
              >
                {aggregate.avg_trust.toFixed(1)}
              </div>
              <span className="rating-display__score-label">Trust: {getTrustLabel(aggregate.avg_trust)}</span>
            </div>

            <div className="rating-display__score-block">
              <div
                className="rating-display__bias-indicator"
                style={{ backgroundColor: biasColors[getBiasLabel(aggregate.avg_bias)] }}
              >
                {getBiasLabel(aggregate.avg_bias)}
              </div>
              <span className="rating-display__score-label">Political Bias</span>
            </div>

            <div className="rating-display__score-block">
              <div
                className="rating-display__independence-indicator"
                style={{ backgroundColor: independenceColors[getIndependenceLabel(aggregate.avg_independence)] }}
              >
                {getIndependenceLabel(aggregate.avg_independence)}
              </div>
              <span className="rating-display__score-label">Independence</span>
            </div>

            <div className="rating-display__count">
              {aggregate.total_ratings} rating{aggregate.total_ratings !== 1 ? 's' : ''}
            </div>
          </div>

          {/* Distribution bars */}
          <div className="rating-display__distribution">
            <h4>Trust Distribution</h4>
            <div className="rating-display__bars">
              {aggregate.trust_distribution.map((count, index) => {
                const percentage = aggregate.total_ratings > 0
                  ? (count / aggregate.total_ratings) * 100
                  : 0;
                return (
                  <div key={index} className="rating-display__bar-row">
                    <span className="rating-display__bar-label">{index + 1}★</span>
                    <div className="rating-display__bar-track">
                      <div
                        className="rating-display__bar-fill"
                        style={{
                          width: `${percentage}%`,
                          backgroundColor: trustColors[TRUST_LABELS[index]],
                        }}
                      />
                    </div>
                    <span className="rating-display__bar-count">{count}</span>
                  </div>
                );
              })}
            </div>
          </div>

          {/* Reviews */}
          {showReviews && ratings.length > 0 && (
            <div className="rating-display__reviews">
              <h4>Recent Reviews</h4>
              {ratings.map((rating) => (
                <div key={rating.id} className="rating-display__review">
                  <div className="rating-display__review-header">
                    <span className="rating-display__review-stars">
                      {'★'.repeat(rating.trust_rating)}{'☆'.repeat(5 - rating.trust_rating)}
                    </span>
                    <span className="rating-display__review-bias" style={{ color: biasColors[BIAS_LABELS[rating.bias_rating - 1]] }}>
                      {BIAS_LABELS[rating.bias_rating - 1]}
                    </span>
                  </div>
                  {rating.review_text && (
                    <p className="rating-display__review-text">{rating.review_text}</p>
                  )}
                  <div className="rating-display__review-footer">
                    <button
                      className="rating-display__helpful-btn"
                      onClick={() => rating.id && handleHelpful(rating.id)}
                    >
                      Helpful ({rating.helpful_count})
                    </button>
                    <span className="rating-display__review-date">
                      {new Date(parseInt(rating.created_at) * 1000).toLocaleDateString()}
                    </span>
                  </div>
                </div>
              ))}
            </div>
          )}
        </>
      )}

      <style>{`
        .rating-display {
          background: var(--color-card, #1a1a2e);
          border-radius: 12px;
          padding: 20px;
        }
        .rating-display--loading {
          color: #9ca3af;
          text-align: center;
          padding: 40px;
        }
        .rating-display__title {
          margin: 0 0 16px;
          font-size: 16px;
          color: var(--color-text, #f0f0f0);
        }
        .rating-display__empty {
          color: #9ca3af;
          text-align: center;
          padding: 20px;
          font-style: italic;
        }
        .rating-display__summary {
          display: flex;
          align-items: center;
          gap: 20px;
          margin-bottom: 20px;
          flex-wrap: wrap;
        }
        .rating-display__score-block {
          display: flex;
          flex-direction: column;
          align-items: center;
          gap: 4px;
        }
        .rating-display__main-score {
          font-size: 32px;
          font-weight: bold;
          color: white;
          padding: 12px 20px;
          border-radius: 12px;
        }
        .rating-display__bias-indicator {
          padding: 8px 16px;
          border-radius: 20px;
          color: white;
          font-weight: 500;
        }
        .rating-display__independence-indicator {
          padding: 8px 16px;
          border-radius: 20px;
          color: white;
          font-weight: 500;
        }
        .rating-display__score-label {
          font-size: 12px;
          color: #9ca3af;
        }
        .rating-display__count {
          margin-left: auto;
          color: #9ca3af;
          font-size: 14px;
        }
        .rating-display__distribution {
          margin-bottom: 20px;
        }
        .rating-display__distribution h4 {
          margin: 0 0 12px;
          font-size: 14px;
          color: #9ca3af;
        }
        .rating-display__bars {
          display: flex;
          flex-direction: column;
          gap: 6px;
        }
        .rating-display__bar-row {
          display: flex;
          align-items: center;
          gap: 8px;
        }
        .rating-display__bar-label {
          width: 30px;
          font-size: 12px;
          color: #9ca3af;
        }
        .rating-display__bar-track {
          flex: 1;
          height: 8px;
          background: rgba(255, 255, 255, 0.1);
          border-radius: 4px;
          overflow: hidden;
        }
        .rating-display__bar-fill {
          height: 100%;
          border-radius: 4px;
          transition: width 0.3s;
        }
        .rating-display__bar-count {
          width: 30px;
          font-size: 12px;
          color: #6b7280;
          text-align: right;
        }
        .rating-display__reviews h4 {
          margin: 0 0 12px;
          font-size: 14px;
          color: #9ca3af;
        }
        .rating-display__review {
          padding: 12px;
          background: rgba(0, 0, 0, 0.2);
          border-radius: 8px;
          margin-bottom: 12px;
        }
        .rating-display__review-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
          margin-bottom: 8px;
        }
        .rating-display__review-stars {
          color: #eab308;
          font-size: 14px;
        }
        .rating-display__review-bias {
          font-size: 12px;
          font-weight: 500;
        }
        .rating-display__review-text {
          margin: 0 0 8px;
          font-size: 14px;
          color: var(--color-text, #f0f0f0);
          line-height: 1.5;
        }
        .rating-display__review-footer {
          display: flex;
          justify-content: space-between;
          align-items: center;
        }
        .rating-display__helpful-btn {
          background: none;
          border: 1px solid #374151;
          color: #9ca3af;
          padding: 4px 8px;
          border-radius: 4px;
          font-size: 12px;
          cursor: pointer;
        }
        .rating-display__helpful-btn:hover {
          border-color: var(--color-primary, #0fab89);
          color: var(--color-primary, #0fab89);
        }
        .rating-display__review-date {
          font-size: 12px;
          color: #6b7280;
        }
      `}</style>
    </div>
  );
}

export default { RatingBadge, RatingForm, RatingDisplay };
