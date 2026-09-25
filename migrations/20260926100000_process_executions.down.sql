-- Reverse 011_process_executions. No CASCADE: dependencies must block
-- rollback instead of being removed implicitly.
DROP TABLE process_executions;

DELETE FROM role_permissions rp
USING permissions p
WHERE rp.permission_id = p.id
  AND p.key IN ('process_executions:start', 'process_executions:complete', 'process_executions:cancel');

DELETE FROM permissions
WHERE key IN ('process_executions:start', 'process_executions:complete', 'process_executions:cancel');

ALTER TABLE processes DROP CONSTRAINT processes_scope_id_key;
