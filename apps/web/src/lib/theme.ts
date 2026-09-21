export type Theme = 'light' | 'dark' | 'system';

export function resolveTheme(value: unknown): Theme {
  return value === 'dark' || value === 'system' ? value : 'light';
}
