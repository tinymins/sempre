CREATE TABLE profile_revisions (
    profile_id UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    revision BIGINT NOT NULL CHECK (revision > 0),
    name TEXT NOT NULL,
    document JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (profile_id, revision)
);

INSERT INTO profile_revisions (profile_id, revision, name, document, created_at)
SELECT id, revision, name, document, updated_at
FROM profiles;
