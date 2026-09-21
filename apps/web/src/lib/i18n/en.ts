import type { tr } from './tr-TR';

export const en: Record<keyof typeof tr, string> = {
  'app.name': 'Workflow & Operations',
  'navigation.skip': 'Skip to content',
  'home.welcome': 'Hello, {name}',
  'auth.title': 'Sign in',
  'auth.description': 'Sign in to your account to continue.',
  'auth.email': 'Email',
  'auth.password': 'Password',
  'auth.submit': 'Sign in',
  'auth.pending': 'Signing in…',
  'auth.logout': 'Sign out',
  'auth.signedIn': 'Signed in:',
  'auth.error.invalidCredentials': 'Email or password is incorrect.',
  'auth.error.network': 'Could not reach the server. Check your connection and try again.',
  'auth.error.unexpected': 'Sign-in failed. Please try again.',
  'error.notFound.title': 'Page not found',
  'error.notFound.description': 'There is no page at this address.',
  'error.unexpected.title': 'Something went wrong',
  'error.unexpected.description': 'The page could not be loaded. Please try again.',
  'error.home': 'Return to home',
};
