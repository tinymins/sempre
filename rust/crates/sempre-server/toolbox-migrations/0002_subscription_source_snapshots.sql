CREATE TABLE subscription_source_snapshots (
    subscribe_id UUID NOT NULL REFERENCES proxy_subscribes(id) ON DELETE CASCADE,
    source_id TEXT NOT NULL,
    content TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    fetched_at TIMESTAMPTZ NOT NULL,
    last_status TEXT NOT NULL,
    last_error TEXT,
    PRIMARY KEY (subscribe_id, source_id)
);
