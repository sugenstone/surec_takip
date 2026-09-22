import { expect, test } from '@playwright/test';

const E2E_EMAIL = 'e2e@example.test';
const E2E_PASSWORD = 'e2e-password-1';

// The stack database is shared by this file's tests; run them in order so the
// second test can rely on the organization created by the first.
test.describe.configure({ mode: 'serial' });

async function signIn(page) {
  await page.goto('/login');
  await page.waitForLoadState('networkidle');
  await page.getByLabel('E-posta').fill(E2E_EMAIL);
  await page.getByLabel('Şifre', { exact: true }).fill(E2E_PASSWORD);
  await page.getByRole('button', { name: 'Giriş yap' }).click();
  await expect(page).toHaveURL(/\/$/);
}

test('empty state offers creation; created organization appears as quiet context', async ({
  page,
}) => {
  await signIn(page);
  await expect(page.getByRole('heading', { name: 'Organizasyonlar' })).toBeVisible();
  await expect(page.getByText('Henüz bir organizasyonunuz yok')).toBeVisible();

  await page.getByLabel('Organizasyon adı').fill('E2e İlk Organizasyon');
  await page.getByRole('button', { name: 'Organizasyon oluştur' }).click();
  // The organization shows in the list and as single-org header context.
  await expect(page.locator('section[aria-labelledby="org-title"] .org-name')).toHaveText(
    'E2e İlk Organizasyon',
  );
  await expect(page.locator('.single-org')).toHaveText('E2e İlk Organizasyon');
  await expect(page.locator('#organization-switcher')).toHaveCount(0);
});

test('second organization enables the switcher and selection persists', async ({ page }) => {
  await signIn(page);
  await page.getByLabel('Organizasyon adı').fill('E2e İkinci Organizasyon');
  await page.getByRole('button', { name: 'Organizasyon oluştur' }).click();

  const switcher = page.getByLabel('Organizasyon', { exact: true });
  await expect(switcher).toBeVisible();
  await expect(switcher.locator('option')).toHaveCount(2, { timeout: 10_000 });
  const secondValue = await switcher
    .locator('option', { hasText: 'E2e İkinci Organizasyon' })
    .getAttribute('value');
  await switcher.selectOption(secondValue);
  await page.reload();
  // Selection is presentation context stored in a cookie, so it survives.
  await expect(page.getByLabel('Organizasyon', { exact: true })).toHaveValue(secondValue);
  await expect(page.getByRole('heading', { name: 'Organizasyonlar' })).toBeVisible();
});
