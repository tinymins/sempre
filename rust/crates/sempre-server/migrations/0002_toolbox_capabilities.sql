CREATE TABLE custom_nodes (
    id UUID PRIMARY KEY,
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    proxy JSONB NOT NULL,
    authorized_user_ids UUID[] NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX custom_nodes_owner_id_idx ON custom_nodes(owner_id);
CREATE INDEX custom_nodes_authorized_user_ids_idx ON custom_nodes USING GIN(authorized_user_ids);

CREATE TABLE access_logs (
    id BIGSERIAL PRIMARY KEY,
    share_id UUID NOT NULL REFERENCES shares(id) ON DELETE CASCADE,
    profile_id UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    target TEXT NOT NULL,
    user_agent TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX access_logs_profile_created_idx ON access_logs(profile_id, created_at DESC);
CREATE INDEX access_logs_share_created_idx ON access_logs(share_id, created_at DESC);
