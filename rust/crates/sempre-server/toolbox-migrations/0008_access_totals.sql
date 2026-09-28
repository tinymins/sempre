ALTER TABLE proxy_subscribes ADD COLUMN access_total BIGINT NOT NULL DEFAULT 0;

UPDATE proxy_subscribes s
SET access_total = l.total
FROM (
    SELECT subscribe_id, COUNT(*) AS total
    FROM proxy_access_logs
    GROUP BY subscribe_id
) l
WHERE s.id = l.subscribe_id;
