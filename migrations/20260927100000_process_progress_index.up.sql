-- 012 · STEP 21B supporting index only. Progress is always derived from
-- process definitions + execution history; nothing is stored here.
--
-- Every progress aggregate probes "has this counted definition ever
-- completed?" once per counted process. Execution history grows much faster
-- than definitions, so a small partial index keeps that EXISTS probe
-- index-only regardless of history size (the active-side probe is already
-- served by process_executions_one_active).
CREATE INDEX process_executions_completed_idx
  ON process_executions (process_id)
  WHERE status = 'completed';
