import { describe, expect, it } from 'vitest';
import { sessionElapsedMs } from './sessions';

const STARTED = '2026-09-29T10:00:00Z';
const SERVER = '2026-09-29T10:30:00Z';

describe('sessionElapsedMs', () => {
  it('derives open-session labor from serverTime plus the local tick', () => {
    const session = { started_at: STARTED, ended_at: null };
    // At anchor: exactly serverTime - started_at (30 min).
    expect(sessionElapsedMs(session, SERVER, 1_000, 1_000)).toBe(30 * 60 * 1000);
    // Local tick advances the display only.
    expect(sessionElapsedMs(session, SERVER, 6_000, 1_000)).toBe(30 * 60 * 1000 + 5_000);
  });

  it('freezes a closed session at ended_at - started_at regardless of tick', () => {
    const session = { started_at: STARTED, ended_at: '2026-09-29T10:10:00Z' };
    expect(sessionElapsedMs(session, SERVER, 99_000, 1_000)).toBe(10 * 60 * 1000);
  });

  it('clamps skewed clocks to zero instead of going negative', () => {
    const session = { started_at: '2026-09-29T11:00:00Z', ended_at: null };
    expect(sessionElapsedMs(session, SERVER, 1_000, 1_000)).toBe(0);
    const closed = { started_at: '2026-09-29T11:00:00Z', ended_at: STARTED };
    expect(sessionElapsedMs(closed, SERVER, 0, 0)).toBe(0);
  });
});
