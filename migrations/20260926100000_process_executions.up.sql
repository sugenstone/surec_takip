-- STEP 21A / ADR 0016: process EXECUTION attempts — operational history for
-- process definitions. Definitions (processes) stay configuration; each
-- attempt is an immutable record of what actually happened.

-- Composite FK target: lets executions prove at the DB level that their full
-- (tenant, workspace, project, section, work item) chain matches the process
-- row they reference.
ALTER TABLE processes
  ADD CONSTRAINT processes_scope_id_key
    UNIQUE (tenant_id, workspace_id, project_id, section_id, work_item_id, id);

CREATE TABLE process_executions (
  id uuid PRIMARY KEY,
  tenant_id uuid NOT NULL REFERENCES organizations (id),
  workspace_id uuid NOT NULL,
  project_id uuid NOT NULL,
  section_id uuid NOT NULL,
  work_item_id uuid NOT NULL,
  process_id uuid NOT NULL,
  attempt_no integer NOT NULL,
  -- Execution lifecycle: active | completed | cancelled. Pending is implicit
  -- (no active attempt); terminal attempts are immutable — retry inserts a
  -- new attempt row instead of resurrecting one (ADR 0016).
  status text NOT NULL DEFAULT 'active',
  started_at timestamptz NOT NULL DEFAULT now(),
  completed_at timestamptz NULL,
  cancelled_at timestamptz NULL,
  -- Optional bounded operational notes: why this attempt started (covers
  -- retry/rework) and why it was cancelled. Completion needs no reason in V1.
  start_reason text NULL,
  cancel_reason text NULL,
  -- Actor history: the authenticated user who ran each transition. Distinct
  -- from assignee (a later assignment domain concern, STEP 21C).
  started_by_user_id uuid NOT NULL REFERENCES users (id),
  completed_by_user_id uuid NULL REFERENCES users (id),
  cancelled_by_user_id uuid NULL REFERENCES users (id),
  created_at timestamptz NOT NULL DEFAULT now(),
  -- updated_at exists only to record the single terminal write; terminal
  -- attempts never change again.
  updated_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT process_executions_process_fk
    FOREIGN KEY (tenant_id, workspace_id, project_id, section_id, work_item_id, process_id)
    REFERENCES processes (tenant_id, workspace_id, project_id, section_id, work_item_id, id),
  CONSTRAINT process_executions_attempt_key UNIQUE (process_id, attempt_no),
  CONSTRAINT process_executions_attempt_check CHECK (attempt_no >= 1),
  CONSTRAINT process_executions_status_check CHECK (status IN ('active', 'completed', 'cancelled')),
  CONSTRAINT process_executions_start_reason_check
    CHECK (start_reason IS NULL OR char_length(btrim(start_reason)) BETWEEN 1 AND 500),
  CONSTRAINT process_executions_cancel_reason_check
    CHECK (cancel_reason IS NULL OR char_length(btrim(cancel_reason)) BETWEEN 1 AND 500),
  -- Terminal-state matrix: exactly the columns of the reached state are set;
  -- terminal timestamps can never precede the attempt's start.
  CONSTRAINT process_executions_state_check CHECK (
    (status = 'active'
      AND completed_at IS NULL AND cancelled_at IS NULL
      AND completed_by_user_id IS NULL AND cancelled_by_user_id IS NULL
      AND cancel_reason IS NULL)
    OR
    (status = 'completed'
      AND completed_at IS NOT NULL AND completed_at >= started_at
      AND completed_by_user_id IS NOT NULL
      AND cancelled_at IS NULL AND cancelled_by_user_id IS NULL
      AND cancel_reason IS NULL)
    OR
    (status = 'cancelled'
      AND cancelled_at IS NOT NULL AND cancelled_at >= started_at
      AND cancelled_by_user_id IS NOT NULL
      AND completed_at IS NULL AND completed_by_user_id IS NULL)
  )
);

-- At most ONE active attempt per process — the structural double-start guard.
CREATE UNIQUE INDEX process_executions_one_active
  ON process_executions (process_id)
  WHERE status = 'active';

-- Future Active Processes surface + project roll-ups over open attempts.
CREATE INDEX process_executions_active_scope_idx
  ON process_executions (tenant_id, workspace_id, project_id, started_at, id)
  WHERE status = 'active';

-- Per-process attempt history (list ordering is position-agnostic; the UI
-- groups rows by process_id).
CREATE INDEX process_executions_process_idx
  ON process_executions (process_id, attempt_no);

-- Work-item execution list (the 21A read surface).
CREATE INDEX process_executions_work_item_idx
  ON process_executions (tenant_id, workspace_id, project_id, section_id, work_item_id, process_id, attempt_no);

INSERT INTO permissions (id, key, description) VALUES
  (gen_random_uuid(), 'process_executions:start', 'Start a process execution attempt'),
  (gen_random_uuid(), 'process_executions:complete', 'Complete an active process execution'),
  (gen_random_uuid(), 'process_executions:cancel', 'Cancel an active process execution');

-- Owner grants at organization scope (010 pattern), no membership writes.
INSERT INTO role_permissions (role_id, permission_id, scope)
SELECT r.id, p.id, 'organization'
FROM roles r
JOIN permissions p
  ON p.key IN ('process_executions:start', 'process_executions:complete', 'process_executions:cancel')
WHERE r.is_system AND r.name = 'owner' AND r.deleted_at IS NULL;

-- Member grants at organization scope (ADR 0016): built-in member
-- assignments are organization-wide (workspace_id IS NULL), so only
-- organization-scope grants match them. Workspace membership still gates
-- eligibility; no administrative permissions are granted.
INSERT INTO role_permissions (role_id, permission_id, scope)
SELECT r.id, p.id, 'organization'
FROM roles r
JOIN permissions p
  ON p.key IN ('process_executions:start', 'process_executions:complete', 'process_executions:cancel')
WHERE r.is_system AND r.name = 'member' AND r.deleted_at IS NULL;
