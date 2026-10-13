WITH missing AS (
    SELECT jsonb_agg(jsonb_build_object('host', host, 'outbound', p.default_outbound) ORDER BY position) AS rules
    FROM relay_egress_policy p
    CROSS JOIN (VALUES (1, 'raw.githubusercontent.com'), (2, 'cdn.jsdelivr.net')) AS defaults(position, host)
    WHERE p.singleton AND NOT EXISTS (
        SELECT 1 FROM jsonb_array_elements(p.host_rules) rule
        WHERE lower(rule->>'host') = host
    )
)
UPDATE relay_egress_policy p
SET host_rules = p.host_rules || missing.rules, revision = p.revision + 1
FROM missing
WHERE p.singleton AND missing.rules IS NOT NULL;

INSERT INTO relay_egress_proxy_references (reference_key, proxy_id)
SELECT rule->>'host', (rule->'outbound'->>'proxyId')::uuid
FROM relay_egress_policy p, jsonb_array_elements(p.host_rules) rule
WHERE p.singleton AND rule->'outbound'->>'type' = 'proxy'
ON CONFLICT (reference_key) DO NOTHING;
