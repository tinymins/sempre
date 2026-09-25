ALTER TABLE subscription_artifacts ADD COLUMN last_success_at TIMESTAMPTZ;
UPDATE subscription_artifacts SET last_success_at = created_at;
ALTER TABLE subscription_artifacts ALTER COLUMN last_success_at SET NOT NULL;
ALTER TABLE subscription_artifacts ALTER COLUMN last_success_at SET DEFAULT NOW();
CREATE INDEX subscription_artifacts_last_success_idx
    ON subscription_artifacts (subscribe_id, target, last_success_at DESC);
