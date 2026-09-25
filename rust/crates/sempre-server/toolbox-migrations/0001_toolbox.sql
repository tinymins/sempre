CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    settings JSONB,
    role TEXT NOT NULL DEFAULT 'user',
    last_login_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX sessions_user_id_idx ON sessions(user_id);

CREATE TABLE system_settings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    allow_registration BOOLEAN NOT NULL DEFAULT TRUE,
    single_workspace_mode BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE proxy_subscribes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id),
    url TEXT NOT NULL UNIQUE,
    remark TEXT,
    subscribe_url TEXT,
    subscribe_items JSONB,
    rule_list TEXT,
    use_system_rule_list BOOLEAN NOT NULL DEFAULT TRUE,
    "group" TEXT,
    use_system_group BOOLEAN NOT NULL DEFAULT TRUE,
    filter TEXT,
    use_system_filter BOOLEAN NOT NULL DEFAULT TRUE,
    servers TEXT,
    custom_config TEXT,
    use_system_custom_config BOOLEAN NOT NULL DEFAULT TRUE,
    dns_config TEXT,
    use_system_dns_config BOOLEAN NOT NULL DEFAULT TRUE,
    authorized_user_ids JSONB DEFAULT '[]'::jsonb,
    cache_ttl_minutes INTEGER,
    cached_node_count INTEGER DEFAULT 0,
    last_access_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    private_access_config TEXT,
    log_level TEXT NOT NULL DEFAULT 'info'
);
CREATE INDEX proxy_subscribes_user_id_idx ON proxy_subscribes(user_id);
CREATE INDEX proxy_subscribes_authorized_user_ids_idx ON proxy_subscribes USING GIN(authorized_user_ids);

CREATE TABLE proxy_custom_nodes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id),
    content TEXT NOT NULL,
    authorized_user_ids JSONB,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE proxy_subscribe_custom_nodes (
    subscribe_id UUID NOT NULL REFERENCES proxy_subscribes(id) ON DELETE CASCADE,
    custom_node_id UUID NOT NULL REFERENCES proxy_custom_nodes(id) ON DELETE CASCADE,
    enabled BOOLEAN NOT NULL DEFAULT FALSE,
    position INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    PRIMARY KEY (subscribe_id, custom_node_id)
);

CREATE TABLE proxy_access_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    subscribe_id UUID NOT NULL REFERENCES proxy_subscribes(id) ON DELETE CASCADE,
    access_type TEXT NOT NULL,
    ip TEXT,
    user_agent TEXT,
    node_count INTEGER DEFAULT 0,
    created_at TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX proxy_access_logs_subscribe_created_idx ON proxy_access_logs(subscribe_id, created_at DESC);
