import { expect, test } from '@playwright/test';

// Dedicated second seeded user: keeps these fixtures isolated from the
// organization spec running in parallel on the shared stack database.
const E2E_EMAIL = 'e2e2@example.test';
const E2E_PASSWORD = 'e2e-password-1';

// Workspaces require an organization first; tests run serially and create
// their own fixtures in the shared stack database.
test.describe.configure({ mode: 'serial' });

async function signIn(page) {
  await page.goto('/login');
  await page.waitForLoadState('networkidle');
  await page.getByLabel('E-posta').fill(E2E_EMAIL);
  await page.getByLabel('Şifre', { exact: true }).fill(E2E_PASSWORD);
  await page.getByRole('button', { name: 'Giriş yap' }).click();
  await expect(page).toHaveURL(/\/$/);
}

async function createWorkspace(page, name) {
  await page.getByLabel('Çalışma alanı adı').fill(name);
  await page.getByRole('button', { name: 'Çalışma alanı oluştur' }).click();
}

test('workspace creation appears in context and selection persists', async ({ page }) => {
  await signIn(page);
  // Ensure an organization exists and is selected for this user.
  await page.getByLabel('Organizasyon adı').fill('Ws Org Bir');
  await page.getByRole('button', { name: 'Organizasyon oluştur' }).click();
  await expect(page.getByRole('heading', { name: 'Çalışma Alanları' })).toBeVisible();
  await expect(page.getByText('Bu organizasyonda henüz çalışma alanı yok')).toBeVisible();

  await createWorkspace(page, 'Ws Alpha');
  await expect(
    page.locator('.org-section .org-name').filter({ hasText: 'Ws Alpha' }),
  ).toBeVisible();
  // A single workspace renders as quiet context text, not a switcher.
  await expect(page.locator('#workspace-switcher')).toHaveCount(0);

  await createWorkspace(page, 'Ws Beta');
  const switcher = page.getByLabel('Çalışma alanı', { exact: true });
  await expect(switcher).toBeVisible();
  await expect(switcher.locator('option')).toHaveCount(2, { timeout: 10_000 });
  const betaValue = await switcher.locator('option', { hasText: 'Ws Beta' }).getAttribute('value');
  await switcher.selectOption(betaValue);
  await page.reload();
  await expect(page.getByLabel('Çalışma alanı', { exact: true })).toHaveValue(betaValue);
});

test('switching organization resets the workspace context', async ({ page }) => {
  await signIn(page);
  // Second organization for the same user.
  await page.getByLabel('Organizasyon adı').fill('Ws Org İki');
  await page.getByRole('button', { name: 'Organizasyon oluştur' }).click();

  const orgSwitcher = page.getByLabel('Organizasyon', { exact: true });
  const secondOrgValue = await orgSwitcher
    .locator('option', { hasText: 'Ws Org İki' })
    .getAttribute('value');
  await orgSwitcher.selectOption(secondOrgValue);
  await expect(orgSwitcher).toHaveValue(secondOrgValue);
  // The new organization has no workspaces: the empty state appears and the
  // workspace selected under the previous organization must not leak.
  await expect(page.getByText('Bu organizasyonda henüz çalışma alanı yok')).toBeVisible({
    timeout: 10_000,
  });
  await expect(page.locator('#workspace-switcher')).toHaveCount(0);

  // Switching back re-resolves the workspace context from the permitted
  // list of that organization.
  await orgSwitcher.selectOption({ label: 'Ws Org Bir' });
  await expect(page.locator('#workspace-switcher')).toBeVisible({ timeout: 10_000 });
  await expect(page.locator('.context .single-org, #workspace-switcher')).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Çalışma Alanları' })).toBeVisible();
});
