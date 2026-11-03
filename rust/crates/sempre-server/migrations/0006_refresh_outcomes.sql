ALTER TABLE profiles DROP CONSTRAINT profiles_last_refresh_status_check;
ALTER TABLE profiles
    ADD CONSTRAINT profiles_last_refresh_status_check
    CHECK (last_refresh_status IN ('never', 'running', 'success', 'partial', 'stale', 'failed'));
