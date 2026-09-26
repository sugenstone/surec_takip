-- STEP 21C — ASSIGNMENT DOMAIN (ADR 0018)
--
-- Two new facts, one semantic each:
--
--   processes.assignee_user_id          = who is CURRENTLY responsible for
--     this concrete process definition. Nullable, mutable, never history.
--
--   process_executions.assignee_user_id = who the process was assigned to
--     when THIS attempt began. Snapshot written once at INSERT; never
--     updated afterwards. Distinct from the existing actor fields
--     (started_by / completed_by / cancelled_by_user_id), which record who
--     performed each transition — both are kept, neither derives the other.
--
-- Scope safety on the process side: the composite key
--   (workspace_id, assignee_user_id) REFERENCES
--   workspace_memberships (workspace_id, user_id)
-- makes a cross-workspace/cross-tenant assignee physically unstorable —
-- the same composite-FK philosophy the schema already uses for parents.
-- MATCH SIMPLE is the default, so a NULL assignee skips the check.
--
-- The FK targets membership ROW EXISTENCE, not current eligibility:
-- revocation is soft (status/deleted_at), so the row survives and the
-- stored assignee remains attributable ("stale") instead of breaking.
-- Eligibility (active org + workspace membership, active user) is enforced
-- by application writes, never by this FK.
--
-- The execution snapshot deliberately references users(id) only: history
-- must outlive every membership lifecycle change.

ALTER TABLE processes
  ADD COLUMN assignee_user_id uuid NULL;

ALTER TABLE processes
  ADD CONSTRAINT processes_assignee_workspace_member_fkey
  FOREIGN KEY (workspace_id, assignee_user_id)
  REFERENCES workspace_memberships (workspace_id, user_id);

ALTER TABLE process_executions
  ADD COLUMN assignee_user_id uuid NULL;

ALTER TABLE process_executions
  ADD CONSTRAINT process_executions_assignee_user_fkey
  FOREIGN KEY (assignee_user_id)
  REFERENCES users (id);

-- "My assigned processes" / assignee-scoped lists inside a workspace.
-- Partial: only live, currently-assigned rows are indexed.
CREATE INDEX processes_workspace_assignee_idx
  ON processes (workspace_id, assignee_user_id)
  WHERE deleted_at IS NULL
    AND status = 'active'
    AND assignee_user_id IS NOT NULL;

-- Future Active-Processes-by-assignee / personal queue (STEP 21E):
-- all currently-running executions for one responsible user inside a
-- tenant/workspace, time-ordered.
CREATE INDEX process_executions_active_assignee_idx
  ON process_executions (tenant_id, workspace_id, assignee_user_id, started_at)
  WHERE status = 'active'
    AND assignee_user_id IS NOT NULL;

-- RBAC: assignment management is deliberately separate from both
-- processes:update (definition editing) and process_executions:*
-- (runtime actions). Execution permission does not grant reassignment.
INSERT INTO permissions (id, key, description) VALUES
  (gen_random_uuid(), 'processes:assign', 'Assign, reassign, or clear the responsible user of a process definition');

-- Owner grants at organization scope (010/011 pattern), no membership
-- writes. Member intentionally receives NO assignment grant — ordinary
-- members may execute but may not administer responsibility.
INSERT INTO role_permissions (role_id, permission_id, scope)
SELECT r.id, p.id, 'organization'
FROM roles r
JOIN permissions p ON p.key = 'processes:assign'
WHERE r.is_system AND r.name = 'owner' AND r.deleted_at IS NULL;
