import { describe, expect, it } from 'vitest';
import { en } from './en';
import { tr } from './tr-TR';
import { formatDate, formatNumber, resolveLocale, translate } from './index';
import { resolveTheme } from '../theme';

describe('language foundation', () => {
  it('resolves user, organization, then Turkish without accepting arbitrary input', () => {
    expect(resolveLocale('en', 'tr-TR')).toBe('en');
    expect(resolveLocale(undefined, 'en')).toBe('en');
    expect(resolveLocale('invalid', 'invalid')).toBe('tr-TR');
    expect(resolveLocale('<script>')).toBe('tr-TR');
  });
  it('keeps complete nonempty dictionaries and independent locale calls', () => {
    expect(Object.keys(en).sort()).toEqual(Object.keys(tr).sort());
    for (const value of [...Object.values(en), ...Object.values(tr)])
      expect(value.trim()).not.toBe('');
    expect(translate('en', 'navigation.skip')).toBe('Skip to content');
    expect(translate('tr-TR', 'navigation.skip')).toBe('İçeriğe geç');
  });
  it('uses locale and explicit time zone formatting', () => {
    expect(formatNumber('tr-TR', 1250.5)).toBe('1.250,5');
    expect(formatNumber('en', 1250.5)).toBe('1,250.5');
    expect(
      formatDate('en', new Date('2026-01-01T22:00:00Z'), 'Europe/Istanbul', { day: 'numeric' }),
    ).toBe('2');
  });
  it('defaults to light without confusing system and dark preferences', () => {
    expect(resolveTheme(undefined)).toBe('light');
    expect(resolveTheme('dark')).toBe('dark');
    expect(resolveTheme('system')).toBe('system');
    expect(resolveTheme('invalid')).toBe('light');
  });
});
