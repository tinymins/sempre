CREATE TABLE subscription_artifacts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    subscribe_id UUID NOT NULL REFERENCES proxy_subscribes(id) ON DELETE CASCADE,
    target TEXT NOT NULL,
    input_hash TEXT NOT NULL,
    content TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    node_count INTEGER NOT NULL,
    profile_name TEXT NOT NULL,
    profile_revision BIGINT NOT NULL,
    profile_updated_at TIMESTAMPTZ NOT NULL,
    runtime JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (subscribe_id, target, input_hash, content_hash)
);
CREATE INDEX subscription_artifacts_subscribe_created_idx
    ON subscription_artifacts (subscribe_id, created_at DESC);
