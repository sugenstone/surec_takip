import { describe, expect, it } from 'vitest';
import { cn } from './utils';

// shadcn-svelte class-merge helper (STEP 21D.5B): Tailwind class conflicts
// resolve last-wins so component defaults can be overridden at call sites.
describe('cn', () => {
  it('merges conflicting Tailwind classes with last value winning', () => {
    expect(cn('px-4', 'px-2')).toBe('px-2');
    expect(cn('bg-primary', 'bg-muted')).toBe('bg-muted');
  });

  it('keeps non-conflicting classes and drops falsy input', () => {
    const missing = undefined;
    expect(cn('flex', missing, 'gap-2', missing && 'hidden', 'items-center')).toBe(
      'flex gap-2 items-center',
    );
  });

  it('supports conditional object input', () => {
    expect(cn('rounded-md', { 'bg-destructive': true, 'bg-primary': false })).toBe(
      'rounded-md bg-destructive',
    );
  });

  it('does not merge across different variants', () => {
    expect(cn('dark:bg-input/30', 'bg-background')).toBe('dark:bg-input/30 bg-background');
  });
});
