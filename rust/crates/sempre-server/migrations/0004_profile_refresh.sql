ALTER TABLE profiles
    ADD COLUMN refresh_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN refresh_interval_minutes INTEGER NOT NULL DEFAULT 1440
        CHECK (refresh_interval_minutes BETWEEN 5 AND 43200),
    ADD COLUMN publish_targets TEXT[] NOT NULL DEFAULT ARRAY['sing-box-v13'],
    ADD COLUMN next_refresh_at TIMESTAMPTZ,
    ADD COLUMN last_refresh_at TIMESTAMPTZ,
    ADD COLUMN last_refresh_status TEXT NOT NULL DEFAULT 'never'
        CHECK (last_refresh_status IN ('never', 'running', 'success', 'failed')),
    ADD COLUMN last_refresh_error TEXT;

CREATE INDEX profiles_refresh_due_idx
    ON profiles(next_refresh_at)
    WHERE refresh_enabled = TRUE;
