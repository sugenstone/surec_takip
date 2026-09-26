import type { TimeSessionPublic } from '../api/client';

// Labor-interval view-model (STEP 21D, ADR 0019). The session timer is a
// DISTINCT concept from the execution timer: it measures one worker's
// tracked interval, not the attempt's wall clock. Open sessions anchor to
// the server-provided `serverTime` plus the shared local display tick;
// closed sessions freeze at ended_at - started_at. Pure and display-only —
// it never writes and never mutates the session.
export function sessionElapsedMs(
  session: Pick<TimeSessionPublic, 'started_at' | 'ended_at'>,
  serverTime: string,
  now: number,
  anchoredAt: number,
): number {
  const start = Date.parse(session.started_at);
  const end = session.ended_at
    ? Date.parse(session.ended_at)
    : Date.parse(serverTime) + (now - anchoredAt);
  return Math.max(0, end - start);
}
