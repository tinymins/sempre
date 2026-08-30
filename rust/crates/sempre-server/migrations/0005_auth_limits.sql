CREATE TABLE auth_limits (
    key_hash BYTEA PRIMARY KEY,
    failed_count INTEGER NOT NULL DEFAULT 0 CHECK (failed_count >= 0),
    window_started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    blocked_until TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX auth_limits_updated_at_idx ON auth_limits(updated_at);
