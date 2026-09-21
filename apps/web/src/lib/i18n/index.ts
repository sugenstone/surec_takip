import { en } from './en';
import { tr } from './tr-TR';

export const locales = ['tr-TR', 'en'] as const;
export type Locale = (typeof locales)[number];
export type TranslationKey = keyof typeof tr;
export const defaultLocale: Locale = 'tr-TR';
const messages = { 'tr-TR': tr, en };

export function isLocale(value: unknown): value is Locale {
  return value === 'tr-TR' || value === 'en';
}

export function resolveLocale(user?: unknown, organization?: unknown): Locale {
  return isLocale(user) ? user : isLocale(organization) ? organization : defaultLocale;
}

// Locale stays request/component-local; SSR requests never share mutable language state.
// Interpolation replaces {placeholders} from the translation dictionary only.
export function translate(
  locale: Locale,
  key: TranslationKey,
  params?: Record<string, string>,
): string {
  const template = messages[locale][key];
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (match, name: string) =>
    name in params ? params[name] : match,
  );
}

export function formatNumber(locale: Locale, value: number, options?: Intl.NumberFormatOptions) {
  return new Intl.NumberFormat(locale, options).format(value);
}

export function formatDate(
  locale: Locale,
  value: Date,
  timeZone: string,
  options?: Intl.DateTimeFormatOptions,
) {
  return new Intl.DateTimeFormat(locale, { ...options, timeZone }).format(value);
}
