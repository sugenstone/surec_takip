-- Reverse 014_time_sessions. No CASCADE: dependencies must block rollback
-- instead of being removed implicitly. The sessions table holds the FK into
-- process_executions' scope key, so it must drop before that key does.
DROP TABLE process_execution_time_sessions;

ALTER TABLE process_executions
  DROP CONSTRAINT process_executions_scope_id_key;

DELETE FROM role_permissions rp
USING permissions p
WHERE rp.permission_id = p.id
  AND p.key IN ('time_sessions:start', 'time_sessions:stop');

DELETE FROM permissions
WHERE key IN ('time_sessions:start', 'time_sessions:stop');
