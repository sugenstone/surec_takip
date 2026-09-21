import type { Locale } from '$lib/i18n';
import type { Theme } from '$lib/theme';

declare global {
  namespace App {
    interface Locals {
      locale: Locale;
      theme: Theme;
    }
    interface PageData {
      locale: Locale;
      theme: Theme;
    }
  }
}

export {};
