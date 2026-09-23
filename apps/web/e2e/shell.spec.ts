import { expect, test } from '@playwright/test';

const E2E_EMAIL = 'e2e@example.test';
const E2E_PASSWORD = 'e2e-password-1';

test.describe.configure({ mode: 'serial' });

async function signIn(page) {
  await page.goto('/login');
  await page.waitForLoadState('networkidle');
  await page.getByLabel('E-posta').fill(E2E_EMAIL);
  await page.getByLabel('Şifre', { exact: true }).fill(E2E_PASSWORD);
  await page.getByRole('button', { name: 'Giriş yap' }).click();
}

test('shell onboarding: no-org state, create org, create workspace, land in shell', async ({
  page,
}) => {
  await signIn(page);
  await expect(page).toHaveURL(/\/app$/);
  await expect(page.getByRole('heading', { name: 'Henüz bir organizasyonunuz yok' })).toBeVisible();

  await page.getByLabel('Organizasyon adı').fill('Shell Test Org');
  await page.getByRole('button', { name: 'Organizasyon oluştur' }).click();
  await expect(page).toHaveURL(/\/app\/[a-f0-9-]+$/, { timeout: 10_000 });
  await expect(
    page.getByRole('heading', { name: 'Bu organizasyonda henüz çalışma alanı yok' }),
  ).toBeVisible();

  await page.getByLabel('Çalışma alanı adı').fill('Shell Test Ws');
  await page.getByRole('button', { name: 'Çalışma alanı oluştur' }).click();
  await expect(page).toHaveURL(/\/app\/[a-f0-9-]+\/[a-f0-9-]+$/, { timeout: 10_000 });
  await expect(page.getByRole('banner')).toBeVisible();
  await expect(page.getByLabel('Organizasyon', { exact: true })).toBeVisible();
  await expect(page.getByLabel('Çalışma alanı', { exact: true })).toBeVisible();
});

test('shell: session cookie is HttpOnly', async ({ page }) => {
  await signIn(page);
  await expect(page).toHaveURL(/\/app\/[a-f0-9-]+/, { timeout: 10_000 });
  const readable = await page.evaluate(() => document.cookie);
  expect(readable).not.toContain('platform_session');
});

test('shell: logout revokes session and returns to login', async ({ page }) => {
  await signIn(page);
  await expect(page).toHaveURL(/\/app\/[a-f0-9-]+/, { timeout: 10_000 });
  await page.getByRole('button', { name: 'Çıkış yap' }).click();
  await expect(page).toHaveURL(/\/login$/);
  await page.goto('/');
  await expect(page).toHaveURL(/\/login$/);
});

test('shell: refresh preserves deep-link URL context', async ({ page }) => {
  await signIn(page);
  await expect(page).toHaveURL(/\/app\/[a-f0-9-]+/, { timeout: 10_000 });
  const url = page.url();
  await page.reload();
  await expect(page).toHaveURL(url);
  await expect(page.getByRole('banner')).toBeVisible();
});

test('shell: stale workspace URL safely redirects to org page', async ({ page }) => {
  await signIn(page);
  await expect(page).toHaveURL(/\/app\/[a-f0-9-]+/, { timeout: 10_000 });
  const orgId = page.url().split('/app/')[1]?.split('/')[0] ?? '';
  const staleUrl = `/app/${orgId}/00000000-0000-4000-8000-000000000000`;
  await page.goto(staleUrl);
  await expect(page).toHaveURL(new RegExp(`/app/${orgId}$`));
});

test('shell: mobile — no overflow, logout reachable', async ({ page }) => {
  await page.setViewportSize({ width: 375, height: 812 });
  await signIn(page);
  await expect(page).toHaveURL(/\/app\/[a-f0-9-]+/, { timeout: 10_000 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await expect(page.getByRole('button', { name: 'Çıkış yap' })).toBeVisible();
});
