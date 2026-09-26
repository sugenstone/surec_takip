-- STEP 21D / ADR 0019: TIME SESSIONS — tracked labor intervals under
-- process executions.
--
-- One row = one contiguous interval during which ONE worker was actively
-- working on ONE execution attempt. The three identities stay independent:
--   worker_user_id     = who was actually working (labor truth)
--   started_by/ended_by = who performed the start/stop action (actor truth)
--   assignee columns    = responsibility (STEP 21C; untouched here)
--
-- Pause/resume is modeled WITHOUT a paused status: pausing closes the row,
-- resuming inserts a new row. Execution start does NOT create a session;
-- work start is an explicit command (approved product decision).

-- Composite FK target: lets a session prove at the DB level that its full
-- (tenant, workspace, project, section, work item, process) chain matches
-- the execution row it references — same pattern 011 used on `processes`.
ALTER TABLE process_executions
  ADD CONSTRAINT process_executions_scope_id_key
    UNIQUE (tenant_id, workspace_id, project_id, section_id, work_item_id, process_id, id);

CREATE TABLE process_execution_time_sessions (
  id uuid PRIMARY KEY,
  tenant_id uuid NOT NULL REFERENCES organizations (id),
  workspace_id uuid NOT NULL,
  project_id uuid NOT NULL,
  section_id uuid NOT NULL,
  work_item_id uuid NOT NULL,
  process_id uuid NOT NULL,
  process_execution_id uuid NOT NULL,
  -- Labor identity → users(id), NOT a membership: revoked members' history
  -- stays attributable (STEP 21C lesson). Eligibility is enforced at INSERT
  -- time only; open/closed rows survive the membership lifecycle.
  worker_user_id uuid NOT NULL REFERENCES users (id),
  -- Server-authoritative timestamps only; the client never sends times.
  started_at timestamptz NOT NULL DEFAULT now(),
  ended_at timestamptz NULL,
  started_by_user_id uuid NOT NULL REFERENCES users (id),
  ended_by_user_id uuid NULL REFERENCES users (id),
  created_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT process_execution_time_sessions_execution_fkey
    FOREIGN KEY (tenant_id, workspace_id, project_id, section_id, work_item_id, process_id, process_execution_id)
    REFERENCES process_executions (tenant_id, workspace_id, project_id, section_id, work_item_id, process_id, id),
  -- Open/closed matrix: no status column. A closed session always carries
  -- its closing actor; a closed timestamp can never precede its start.
  CONSTRAINT process_execution_time_sessions_state_check CHECK (
    (ended_at IS NULL AND ended_by_user_id IS NULL)
    OR
    (ended_at IS NOT NULL AND ended_at >= started_at AND ended_by_user_id IS NOT NULL)
  )
);

-- Per-execution interval list (the nested read surface + auto-close scan).
CREATE INDEX process_execution_time_sessions_execution_idx
  ON process_execution_time_sessions (process_execution_id, started_at, id);

-- "Who is working right now" per work item / scope — future Active
-- Processes and TV surfaces read this without touching closed history.
CREATE INDEX process_execution_time_sessions_open_scope_idx
  ON process_execution_time_sessions
    (tenant_id, workspace_id, project_id, section_id, work_item_id)
  WHERE ended_at IS NULL;

-- V1 rule: one worker may track at most ONE open session per tenant —
-- across projects, work items, processes and executions alike. The index
-- is the structural backstop; the application pre-check produces the
-- deterministic ACTIVE_SESSION_EXISTS error and this turns races into it.
CREATE UNIQUE INDEX process_execution_time_sessions_one_open_per_worker
  ON process_execution_time_sessions (tenant_id, worker_user_id)
  WHERE ended_at IS NULL;

-- Multiple workers MAY hold overlapping open sessions on one execution —
-- deliberately no partial unique on process_execution_id.

INSERT INTO permissions (id, key, description) VALUES
  (gen_random_uuid(), 'time_sessions:start', 'Start a work session on an active process execution'),
  (gen_random_uuid(), 'time_sessions:stop', 'Stop an open work session');

-- Owner and Member grants at organization scope (011 pattern): built-in
-- member assignments are organization-wide (workspace_id IS NULL), so only
-- organization-scope grants match them. Workspace membership still gates
-- eligibility; no administrative permissions are granted.
INSERT INTO role_permissions (role_id, permission_id, scope)
SELECT r.id, p.id, 'organization'
FROM roles r
JOIN permissions p
  ON p.key IN ('time_sessions:start', 'time_sessions:stop')
WHERE r.is_system AND r.name IN ('owner', 'member') AND r.deleted_at IS NULL;
