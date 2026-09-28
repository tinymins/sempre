ALTER TABLE subscription_artifacts ADD COLUMN revision_hash TEXT;
CREATE INDEX subscription_artifacts_revision_idx
    ON subscription_artifacts (subscribe_id, target, revision_hash, last_success_at DESC);
