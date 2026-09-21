import type { Handle, HandleServerError } from '@sveltejs/kit';
import { resolveLocale, translate } from '$lib/i18n';
import { resolveTheme } from '$lib/theme';

export const handle: Handle = async ({ event, resolve }) => {
  // This preference cookie is presentation-only; it never selects tenant or authorization.
  event.locals.locale = resolveLocale(event.cookies.get('locale'));
  event.locals.theme = resolveTheme(event.cookies.get('theme'));
  const response = await resolve(event, {
    transformPageChunk: ({ html }) =>
      html.replace('%app.locale%', event.locals.locale).replace('%app.theme%', event.locals.theme),
  });
  response.headers.set('Content-Language', event.locals.locale);
  response.headers.set('X-Content-Type-Options', 'nosniff');
  response.headers.set('Referrer-Policy', 'strict-origin-when-cross-origin');
  response.headers.set('Cache-Control', 'private, no-store');
  return response;
};

export const handleError: HandleServerError = ({ event, status }) => {
  // Do not render framework exceptions, URLs or request data into the UI/logs.
  if (status >= 500) console.error(JSON.stringify({ event: 'web.request_failed', status }));
  return { message: translate(event.locals.locale, 'error.unexpected.description') };
};
