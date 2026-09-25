//! Derived progress aggregates (STEP 21B, ADR 0017).
//!
//! Progress is computed, never stored. Every number on every surface comes
//! from ONE aggregate shape here — three scope variants (work item, section
//! subtree, project) differ only in their scope predicate, so project cards,
//! section cards, work-item cards, dashboards and TV can never disagree.
//!
//! Locked semantics (ADR 0016 D9–D12, extended by ADR 0017):
//! - counted(p) = live definition under reachable parents:
//!   p.deleted_at IS NULL AND p.status = 'active', its work item is
//!   visible (deleted_at IS NULL AND status <> 'archived'), and its owning
//!   section is reachable (deleted_at IS NULL AND status = 'active').
//!   `is_required` does NOT narrow the V1 denominator.
//! - done(p) = EXISTS(completed attempt) AND NOT EXISTS(active attempt);
//!   an active rework attempt regresses the definition to not-done.
//! - Execution attempts are boolean EXISTS predicates, never JOINed, so
//!   attempt count can never inflate the denominator.
//! - total = 0 ⇒ percent = null (the UI renders "—", never a fake 0%).
//! - Roll-up is process-weighted: Section/Project = done/total over the
//!   whole subtree, never an average of child percentages — which makes
//!   progress structurally invariant under re-organization.
//! - Every aggregate is a single SQL statement: numerator and denominator
//!   always share one snapshot.
//!
//! Cycle-safety: the recursive walks mirror `sections::is_self_or_descendant`
//! (UNION deduplicates visited nodes), and each process counts exactly once
//! per root because a process has exactly one owning section.

use serde::Serialize;
use sqlx::PgPool;
use std::collections::HashMap;
use utoipa::ToSchema;
use uuid::Uuid;

/// Derived progress for one scope — never persisted (ADR 0017).
#[derive(Clone, Copy, Default, Serialize, sqlx::FromRow, ToSchema)]
pub struct Progress {
    /// Counted definitions satisfying DONE (∃ completed ∧ ¬∃ active).
    pub completed: i64,
    /// Counted definitions with an execution currently in flight.
    pub active: i64,
    /// All counted definitions in the scope.
    pub total: i64,
    /// Backend-authoritative integer percent (round-half-up, integer math).
    /// NULL exactly when total = 0 — never a fake 0 or 100.
    pub percent: Option<i32>,
}

#[derive(sqlx::FromRow)]
struct ProgressRow {
    completed: i64,
    active: i64,
    total: i64,
}

impl From<ProgressRow> for Progress {
    fn from(row: ProgressRow) -> Self {
        // Round-half-away-from-zero in integer math: deterministic on every
        // surface (1/3→33, 2/3→67, 1/6→17, 5/6→83); the frontend renders
        // this value verbatim and never recomputes a ratio.
        let percent = if row.total == 0 {
            None
        } else {
            let value = (100 * row.completed + row.total / 2) / row.total;
            Some(i32::try_from(value).unwrap_or(100))
        };
        Self {
            completed: row.completed,
            active: row.active,
            total: row.total,
            percent,
        }
    }
}

#[derive(sqlx::FromRow)]
struct ScopedProgressRow {
    parent_id: Uuid,
    completed: i64,
    active: i64,
    total: i64,
}

fn grouped(rows: Vec<ScopedProgressRow>) -> HashMap<Uuid, Progress> {
    rows.into_iter()
        .map(|row| {
            (
                row.parent_id,
                Progress::from(ProgressRow {
                    completed: row.completed,
                    active: row.active,
                    total: row.total,
                }),
            )
        })
        .collect()
}

/// The DONE predicate on `p.id`: a completed attempt exists and none is in
/// flight. Served by the partial indexes `process_executions_completed_idx`
/// (012) and `process_executions_one_active` (011).
const DONE: &str = "\
    EXISTS (SELECT 1 FROM process_executions e \
             WHERE e.process_id = p.id AND e.status = 'completed') \
    AND NOT EXISTS (SELECT 1 FROM process_executions e \
                     WHERE e.process_id = p.id AND e.status = 'active')";

const RUNNING: &str = "\
    EXISTS (SELECT 1 FROM process_executions e \
             WHERE e.process_id = p.id AND e.status = 'active')";

fn aggregate_columns() -> String {
    format!(
        "COUNT(*) FILTER (WHERE {DONE}) AS completed, \
         COUNT(*) FILTER (WHERE {RUNNING}) AS active, \
         COUNT(*) AS total"
    )
}

/// Reachable-parent joins shared by every scope: a process only counts while
/// its work item is visible and its OWNING section is active (an archived
/// section hides its own items, while active child sections below it remain
/// independently reachable — matching the route contexts).
const COUNTED: &str = "\
    JOIN work_items wi ON wi.tenant_id = p.tenant_id \
       AND wi.workspace_id = p.workspace_id AND wi.project_id = p.project_id \
       AND wi.section_id = p.section_id AND wi.id = p.work_item_id \
       AND wi.deleted_at IS NULL AND wi.status <> 'archived' \
    JOIN sections s ON s.tenant_id = p.tenant_id \
       AND s.workspace_id = p.workspace_id AND s.project_id = p.project_id \
       AND s.id = p.section_id \
       AND s.deleted_at IS NULL AND s.status = 'active'";

const LIVE_PROCESS: &str = "p.deleted_at IS NULL AND p.status = 'active'";

/// Cycle-safe descendant walk identical to the archive guard's: UNION
/// deduplicates visited ids, so traversal cannot loop even on corrupt data.
/// Traversal covers ALL non-deleted sections — an archived node hides only
/// its own work items, not the still-reachable sections beneath it.
const SUBTREE_CTE: &str = "\
    WITH RECURSIVE subtree AS ( \
      SELECT id FROM sections \
      WHERE id = $4 AND tenant_id = $1 AND workspace_id = $2 AND project_id = $3 \
        AND deleted_at IS NULL \
      UNION \
      SELECT s2.id FROM sections s2 \
      JOIN subtree st ON s2.parent_section_id = st.id \
      WHERE s2.tenant_id = $1 AND s2.workspace_id = $2 AND s2.project_id = $3 \
        AND s2.deleted_at IS NULL \
    )";

/// Progress of one work item: all counted definitions it owns.
pub async fn for_work_item(
    pool: &PgPool,
    organization_id: Uuid,
    workspace_id: Uuid,
    project_id: Uuid,
    section_id: Uuid,
    work_item_id: Uuid,
) -> Result<Progress, sqlx::Error> {
    let sql = format!(
        "SELECT {} FROM processes p {COUNTED} \
         WHERE p.tenant_id = $1 AND p.workspace_id = $2 AND p.project_id = $3 \
           AND p.section_id = $4 AND p.work_item_id = $5 AND {LIVE_PROCESS}",
        aggregate_columns()
    );
    sqlx::query_as::<_, ProgressRow>(&sql)
        .bind(organization_id)
        .bind(workspace_id)
        .bind(project_id)
        .bind(section_id)
        .bind(work_item_id)
        .fetch_one(pool)
        .await
        .map(Progress::from)
}

/// Progress of one section's ENTIRE recursive subtree (leaf-weighted).
pub async fn for_section(
    pool: &PgPool,
    organization_id: Uuid,
    workspace_id: Uuid,
    project_id: Uuid,
    section_id: Uuid,
) -> Result<Progress, sqlx::Error> {
    let sql = format!(
        "{SUBTREE_CTE} \
         SELECT {} FROM subtree st \
         JOIN processes p ON p.section_id = st.id \
           AND p.tenant_id = $1 AND p.workspace_id = $2 AND p.project_id = $3 \
           AND {LIVE_PROCESS} {COUNTED}",
        aggregate_columns()
    );
    sqlx::query_as::<_, ProgressRow>(&sql)
        .bind(organization_id)
        .bind(workspace_id)
        .bind(project_id)
        .bind(section_id)
        .fetch_one(pool)
        .await
        .map(Progress::from)
}

/// Progress of a whole project: every counted definition under it. The
/// result is independent of how sections organize the work (structural
/// invariance — the aggregate is over the process set, not the tree shape).
pub async fn for_project(
    pool: &PgPool,
    organization_id: Uuid,
    workspace_id: Uuid,
    project_id: Uuid,
) -> Result<Progress, sqlx::Error> {
    let sql = format!(
        "SELECT {} FROM processes p {COUNTED} \
         WHERE p.tenant_id = $1 AND p.workspace_id = $2 AND p.project_id = $3 \
           AND {LIVE_PROCESS}",
        aggregate_columns()
    );
    sqlx::query_as::<_, ProgressRow>(&sql)
        .bind(organization_id)
        .bind(workspace_id)
        .bind(project_id)
        .fetch_one(pool)
        .await
        .map(Progress::from)
}

/// Per-work-item progress for every item under one section — ONE grouped
/// statement, never a query per row (list surfaces must stay N+1-free).
/// Items without counted processes are absent; callers default to empty.
pub async fn for_work_items(
    pool: &PgPool,
    organization_id: Uuid,
    workspace_id: Uuid,
    project_id: Uuid,
    section_id: Uuid,
) -> Result<HashMap<Uuid, Progress>, sqlx::Error> {
    let sql = format!(
        "SELECT p.work_item_id AS parent_id, {} FROM processes p {COUNTED} \
         WHERE p.tenant_id = $1 AND p.workspace_id = $2 AND p.project_id = $3 \
           AND p.section_id = $4 AND {LIVE_PROCESS} \
         GROUP BY p.work_item_id",
        aggregate_columns()
    );
    sqlx::query_as::<_, ScopedProgressRow>(&sql)
        .bind(organization_id)
        .bind(workspace_id)
        .bind(project_id)
        .bind(section_id)
        .fetch_all(pool)
        .await
        .map(grouped)
}

/// Per-section subtree progress for EVERY section of a project in one
/// statement: the CTE carries (root, node) pairs seeded from each section
/// and deduplicated by UNION, then aggregates GROUP BY the seed — each
/// process contributes to each of its ancestors exactly once.
pub async fn for_sections(
    pool: &PgPool,
    organization_id: Uuid,
    workspace_id: Uuid,
    project_id: Uuid,
) -> Result<HashMap<Uuid, Progress>, sqlx::Error> {
    let sql = format!(
        "WITH RECURSIVE subtree AS ( \
           SELECT id AS root, id AS node FROM sections \
           WHERE tenant_id = $1 AND workspace_id = $2 AND project_id = $3 \
             AND deleted_at IS NULL \
           UNION \
           SELECT st.root, s2.id FROM sections s2 \
           JOIN subtree st ON s2.parent_section_id = st.node \
           WHERE s2.tenant_id = $1 AND s2.workspace_id = $2 AND s2.project_id = $3 \
             AND s2.deleted_at IS NULL \
         ) \
         SELECT st.root AS parent_id, {} FROM subtree st \
         JOIN processes p ON p.section_id = st.node \
           AND p.tenant_id = $1 AND p.workspace_id = $2 AND p.project_id = $3 \
           AND {LIVE_PROCESS} {COUNTED} \
         GROUP BY st.root",
        aggregate_columns()
    );
    sqlx::query_as::<_, ScopedProgressRow>(&sql)
        .bind(organization_id)
        .bind(workspace_id)
        .bind(project_id)
        .fetch_all(pool)
        .await
        .map(grouped)
}

/// Per-project progress for every project of a workspace — one grouped
/// statement for the project list, scoped to the resolved tenant pair.
pub async fn for_projects(
    pool: &PgPool,
    organization_id: Uuid,
    workspace_id: Uuid,
) -> Result<HashMap<Uuid, Progress>, sqlx::Error> {
    let sql = format!(
        "SELECT p.project_id AS parent_id, {} FROM processes p {COUNTED} \
         WHERE p.tenant_id = $1 AND p.workspace_id = $2 AND {LIVE_PROCESS} \
         GROUP BY p.project_id",
        aggregate_columns()
    );
    sqlx::query_as::<_, ScopedProgressRow>(&sql)
        .bind(organization_id)
        .bind(workspace_id)
        .fetch_all(pool)
        .await
        .map(grouped)
}
