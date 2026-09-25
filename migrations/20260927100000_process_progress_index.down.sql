-- Reverse 012_process_progress_index. The index has no dependents, so a
-- plain DROP is complete and safe.
DROP INDEX process_executions_completed_idx;
