CREATE TABLE subscription_proxies (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 80),
    engine TEXT NOT NULL,
    protocol TEXT NOT NULL,
    configuration TEXT NOT NULL,
    active_configuration TEXT,
    revision BIGINT NOT NULL DEFAULT 1,
    active_revision BIGINT,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    authorized_user_ids JSONB NOT NULL DEFAULT '[]'::jsonb,
    status TEXT NOT NULL DEFAULT 'starting',
    last_error TEXT,
    checked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX subscription_proxies_user_idx ON subscription_proxies(user_id);
