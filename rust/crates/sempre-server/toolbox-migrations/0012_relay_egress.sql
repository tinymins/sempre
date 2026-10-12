CREATE TABLE relay_egress_policy (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    revision BIGINT NOT NULL DEFAULT 1,
    default_outbound JSONB NOT NULL DEFAULT '{"type":"system"}'::jsonb,
    host_rules JSONB NOT NULL DEFAULT '[]'::jsonb
);
INSERT INTO relay_egress_policy (singleton) VALUES (TRUE);

CREATE TABLE relay_egress_proxy_references (
    reference_key TEXT PRIMARY KEY,
    proxy_id UUID NOT NULL REFERENCES subscription_proxies(id) ON DELETE RESTRICT
);
CREATE INDEX relay_egress_proxy_references_proxy_idx ON relay_egress_proxy_references(proxy_id);

CREATE FUNCTION protect_relay_egress_owner() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.role NOT IN ('admin', 'superadmin') AND EXISTS (
        SELECT 1 FROM subscription_proxies p
        JOIN relay_egress_proxy_references r ON r.proxy_id = p.id
        WHERE p.user_id = OLD.id
    ) THEN
        RAISE EXCEPTION 'user owns a proxy referenced by relay egress policy; choose a replacement first'
            USING ERRCODE = '23514', CONSTRAINT = 'relay_egress_admin_owner';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER relay_egress_admin_owner BEFORE UPDATE OF role ON users
    FOR EACH ROW EXECUTE FUNCTION protect_relay_egress_owner();
