import type { Handle, HandleServerError } from '@sveltejs/kit';
import { resolveLocale, translate } from '$lib/i18n';
import { resolveTheme } from '$lib/theme';
import type { MeData } from '$lib/api/client';

// Server-side API base; in dev the browser uses the Vite /api proxy while
// SSR calls the backend directly. Production routes both through one origin.
const apiOrigin = process.env.API_ORIGIN ?? 'http://127.0.0.1:8080';

// This preference cookie is presentation-only; it never selects tenant or authorization.
export const handle: Handle = async ({ event, resolve }) => {
  event.locals.locale = resolveLocale(event.cookies.get('locale'));
  event.locals.theme = resolveTheme(event.cookies.get('theme'));
  const session = await loadSession(event);
  event.locals.user = session?.user ?? null;
  // Only organizations the backend reports as visible memberships; the
  // organization cookie below is UX context, never an access grant.
  event.locals.organizations = session?.organizations ?? [];
  const requested = event.cookies.get('organization') ?? null;
  event.locals.currentOrganizationId = event.locals.organizations.some(
    (organization) => organization.id === requested,
  )
    ? requested
    : (event.locals.organizations[0]?.id ?? null);
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

// The session cookie is forwarded verbatim to the authoritative /auth/me
// endpoint; SSR never decides authentication or organization visibility.
async function loadSession(event: {
  cookies: { get(name: string): string | undefined };
  fetch: typeof fetch;
}): Promise<MeData | null> {
  const token = event.cookies.get('platform_session');
  if (!token) return null;
  try {
    const response = await event.fetch(`${apiOrigin}/api/v1/auth/me`, {
      headers: { cookie: `platform_session=${token}` },
    });
    if (!response.ok) return null;
    return ((await response.json()) as { data?: MeData }).data ?? null;
  } catch {
    // Backend unreachable during SSR behaves as an unauthenticated request;
    // the login form surfaces connectivity problems on submit.
    return null;
  }
}

export const handleError: HandleServerError = ({ event, status }) => {
  // Do not render framework exceptions, URLs or request data into the UI/logs.
  if (status >= 500) console.error(JSON.stringify({ event: 'web.request_failed', status }));
  return { message: translate(event.locals.locale, 'error.unexpected.description') };
};
