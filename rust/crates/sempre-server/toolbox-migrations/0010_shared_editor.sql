-- Existing settings are materialized by the one-time Rust migration, which can
-- parse JSONC without adding a second configuration parser to PostgreSQL.
ALTER TABLE proxy_subscribes ADD COLUMN editor_version SMALLINT NOT NULL DEFAULT 0;
ALTER TABLE proxy_subscribes ALTER COLUMN editor_version SET DEFAULT 1;
