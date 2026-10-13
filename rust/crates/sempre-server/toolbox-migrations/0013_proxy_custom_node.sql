ALTER TABLE subscription_proxies ADD COLUMN custom_node_id UUID
    REFERENCES proxy_custom_nodes(id) ON DELETE RESTRICT;
CREATE INDEX subscription_proxies_custom_node_idx ON subscription_proxies(custom_node_id);
