import { expect, test } from '@playwright/test';

const E2E_EMAIL = 'e2e@example.test';
const E2E_PASSWORD = 'e2e-password-1';
const E2E_NAME = 'E2e User';

async function signIn(page, email = E2E_EMAIL, password = E2E_PASSWORD) {
  await page.goto('/login');
  // Dev-server hydration can trail first paint; interacting earlier triggers
  // a native form POST that bypasses the JS handler.
  await page.waitForLoadState('networkidle');
  await page.getByLabel('E-posta').fill(email);
  await page.getByLabel('Şifre', { exact: true }).fill(password);
  await page.getByRole('button', { name: 'Giriş yap' }).click();
}

test('unauthenticated visitors are redirected to the Turkish login screen', async ({ page }) => {
  await page.goto('/');
  await expect(page).toHaveURL(/\/login$/);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Giriş yap');
  await expect(page.locator('html')).toHaveAttribute('lang', 'tr-TR');
});

test('wrong password shows the generic localized error and stays on the form', async ({ page }) => {
  await signIn(page, E2E_EMAIL, 'definitely-wrong');
  await expect(page.getByRole('alert')).toHaveText('E-posta veya şifre hatalı.');
  await expect(page).toHaveURL(/\/login$/);
});

test('login succeeds, session cookie is HttpOnly, logout revokes the session', async ({ page }) => {
  await signIn(page);
  await expect(page).toHaveURL(/\/$/);
  await expect(page.getByText(`Merhaba, ${E2E_NAME}`)).toBeVisible();
  await expect(page.getByRole('button', { name: 'Çıkış yap' })).toBeVisible();
  // HttpOnly cookies must never be readable from page scripts.
  const readable = await page.evaluate(() => document.cookie);
  expect(readable).not.toContain('platform_session');

  await page.getByRole('button', { name: 'Çıkış yap' }).click();
  await expect(page).toHaveURL(/\/login$/);
  // The cleared/revoked session must not grant access to the protected page.
  await page.goto('/');
  await expect(page).toHaveURL(/\/login$/);
});

test('english locale renders the localized login experience', async ({ page, context }) => {
  await context.addCookies([{ name: 'locale', value: 'en', url: 'http://127.0.0.1:4173' }]);
  await page.goto('/login');
  await page.waitForLoadState('networkidle');
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Sign in');
  await page.getByLabel('Email').fill(E2E_EMAIL);
  await page.getByLabel('Password', { exact: true }).fill('nope');
  await page.getByRole('button', { name: 'Sign in' }).click();
  await expect(page.getByRole('alert')).toHaveText('Email or password is incorrect.');
});
