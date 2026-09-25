// View model for derived progress (ADR 0017). The API ships counts plus the
// authoritative integer percent; this maps them 1:1 to what the UI renders —
// the frontend NEVER recomputes a ratio or fabricates a percent.
import type { Progress } from '$lib/api/client';

export interface ProgressView {
  /** total > 0 — bar and counts are meaningful. */
  hasWork: boolean;
  /** Supplied verbatim; null exactly when there is no counted work. */
  percent: number | null;
  completed: number;
  active: number;
  total: number;
  /** active > 0 — the "in progress" hint is shown. */
  hasActive: boolean;
}

export function progressView(progress: Progress): ProgressView {
  return {
    hasWork: progress.total > 0,
    percent: progress.total > 0 ? (progress.percent ?? null) : null,
    completed: progress.completed,
    active: progress.active,
    total: progress.total,
    hasActive: progress.active > 0,
  };
}
