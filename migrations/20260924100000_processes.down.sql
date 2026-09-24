-- Reverse 010_processes. No CASCADE: dependencies must block rollback
-- instead of being removed implicitly.
DROP TABLE processes;

DELETE FROM role_permissions rp
USING permissions p
WHERE rp.permission_id = p.id
  AND p.key IN ('processes:create', 'processes:update', 'processes:archive', 'processes:reorder');

DELETE FROM permissions
WHERE key IN ('processes:create', 'processes:update', 'processes:archive', 'processes:reorder');

ALTER TABLE work_items DROP CONSTRAINT work_items_scope_id_key;
