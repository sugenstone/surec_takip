import { describe, expect, it } from 'vitest';
import { progressView } from './progress';
import type { Progress } from '$lib/api/client';

// View-model contract (ADR 0017): the backend is the single authority for
// percent; the frontend passes it through verbatim and derives only
// presentation flags. Recomputing a ratio here would be a defect.
function progress(completed: number, active: number, total: number, percent: number | null) {
  return { completed, active, total, percent } satisfies Progress;
}

describe('progressView', () => {
  it('undefined work (total = 0) has no bar and no fabricated percent', () => {
    const view = progressView(progress(0, 0, 0, null));
    expect(view.hasWork).toBe(false);
    expect(view.percent).toBeNull();
    expect(view.hasActive).toBe(false);
  });

  it('zero percent is real work — not empty state', () => {
    const view = progressView(progress(0, 0, 5, 0));
    expect(view.hasWork).toBe(true);
    expect(view.percent).toBe(0);
  });

  it.each([
    [1, 3, 33],
    [2, 3, 67],
    [10, 10, 100],
  ])('renders the supplied percent verbatim: %d/%d → %d', (completed, total, percent) => {
    const view = progressView(progress(completed, 0, total, percent));
    expect(view.percent).toBe(percent);
    expect(view.completed).toBe(completed);
    expect(view.total).toBe(total);
  });

  it('exposes the in-flight count for the running hint', () => {
    const idle = progressView(progress(3, 0, 8, 38));
    expect(idle.hasActive).toBe(false);
    const running = progressView(progress(3, 2, 8, 38));
    expect(running.hasActive).toBe(true);
    expect(running.active).toBe(2);
  });

  it('never recomputes — even a "wrong" percent is shown as supplied', () => {
    // If this fails, someone moved rounding into the frontend.
    const view = progressView(progress(1, 0, 6, 17));
    expect(view.percent).toBe(17);
  });
});
