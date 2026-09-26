-- Reverse 013_process_assignments. No CASCADE: dependencies must block
-- rollback instead of being removed implicitly.
DROP INDEX process_executions_active_assignee_idx;
DROP INDEX processes_workspace_assignee_idx;

ALTER TABLE process_executions
  DROP CONSTRAINT process_executions_assignee_user_fkey,
  DROP COLUMN assignee_user_id;

ALTER TABLE processes
  DROP CONSTRAINT processes_assignee_workspace_member_fkey,
  DROP COLUMN assignee_user_id;

DELETE FROM role_permissions rp
USING permissions p
WHERE rp.permission_id = p.id
  AND p.key = 'processes:assign';

DELETE FROM permissions
WHERE key = 'processes:assign';
