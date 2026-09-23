import { expect, test, type Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';

test.describe.configure({ mode: 'serial' });
let listUrl = '';
let itemUrl = '';
let apiPath = '';
let orgId = '';
async function signIn(page: Page) {
  await page.goto('/login');
  await page.waitForLoadState('networkidle');
  await page.getByLabel('E-posta').fill('e2e5@example.test');
  await page.getByLabel('Şifre', { exact: true }).fill('e2e-password-1');
  await page.getByRole('button', { name: 'Giriş yap' }).click();
  await expect(page).toHaveURL(/\/app(?:\/|$)/);
}

test('work items: owner creates through the hierarchy and list persists', async ({ page }) => {
  await signIn(page);
  await page.getByLabel('Organizasyon adı').fill('Work Items Org');
  await page.getByRole('button', { name: 'Organizasyon oluştur' }).click();
  await expect(page).toHaveURL(/\/app\/[a-f0-9-]+$/);
  await page.getByLabel('Çalışma alanı adı').fill('Work Items Ws');
  await page.getByRole('button', { name: 'Çalışma alanı oluştur' }).click();
  await expect(page).toHaveURL(/\/app\/[a-f0-9-]+\/[a-f0-9-]+$/);
  await page.getByRole('link', { name: /Projeler/ }).click();
  await page.getByLabel('Proje adı').fill('Work Project');
  await page.getByRole('button', { name: 'Proje oluştur' }).click();
  await expect(page).toHaveURL(/\/projects\/[a-f0-9-]+$/);
  await page.getByLabel('Bölüm adı').fill('Work Section');
  await page.getByRole('button', { name: 'Bölüm ekle' }).click();
  await page.getByRole('link', { name: 'İşçilik', exact: true }).click();
  await expect(page.getByText('Bu bölümde henüz işçilik yok')).toBeVisible();
  listUrl = page.url();
  const parts = new URL(listUrl).pathname.split('/');
  orgId = parts[2];
  apiPath = `/api/v1/organizations/${parts[2]}/workspaces/${parts[3]}/projects/${parts[5]}/sections/${parts[7]}/work-items`;
  await page.getByLabel('İşçilik adı').fill('Inspection');
  await page.getByRole('button', { name: 'İşçilik ekle', exact: true }).click();
  await expect(page.getByRole('link', { name: 'Inspection', exact: true })).toBeVisible();
  await page.reload();
  await page.getByRole('link', { name: 'Inspection', exact: true }).click();
  await expect(page).toHaveURL(/\/work-items\/[a-f0-9-]+$/);
  itemUrl = page.url();
  await expect(page.getByRole('heading', { name: 'Inspection', exact: true })).toBeVisible();
  await page.reload();
  await expect(page).toHaveURL(itemUrl);
  await expect(page.getByLabel('İşçilik adı')).toHaveValue('Inspection');
});

test('work items: edits, validation draft, English mobile dark mode, archive', async ({
  page,
  context,
}, testInfo) => {
  await signIn(page);
  await page.goto(itemUrl);
  // Wait for hydration before typing: fill() racing an in-flight bind:value
  // reset appends instead of replacing (observed as "InspectionUpdated
  // Inspection"). networkidle is the repo-wide settle convention.
  await page.waitForLoadState('networkidle');
  await page.getByLabel('İşçilik adı').fill('Updated Inspection');
  await page.getByLabel('Sıra', { exact: true }).fill('4');
  await page.getByLabel('Durum', { exact: true }).selectOption('completed');
  await page.getByRole('button', { name: 'Kaydet', exact: true }).click();
  await expect(page.getByRole('status')).toHaveText('Kaydedildi.');
  await page.reload();
  await expect(page.getByLabel('İşçilik adı')).toHaveValue('Updated Inspection');
  await expect(page.getByLabel('Sıra', { exact: true })).toHaveValue('4');
  await expect(page.getByLabel('Durum', { exact: true })).toHaveValue('completed');
  const duplicate = await page.request.post(apiPath, { data: { name: 'Reserved' } });
  expect(duplicate.status()).toBe(201);
  await page.getByLabel('Kısa ad', { exact: true }).fill('reserved');
  await page.getByRole('button', { name: 'Kaydet', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('zaten kullanılıyor');
  await expect(page.getByLabel('Kısa ad', { exact: true })).toHaveValue('reserved');
  await page.getByLabel('Kısa ad', { exact: true }).fill('inspection');
  await page.setViewportSize({ width: 360, height: 800 });
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
  ).toBeTruthy();
  await page.screenshot({ path: testInfo.outputPath('work-item-tr-mobile.png'), fullPage: true });
  await context.addCookies([
    { name: 'locale', value: 'en', url: 'http://127.0.0.1:4173' },
    { name: 'theme', value: 'dark', url: 'http://127.0.0.1:4173' },
  ]);
  await page.reload();
  await expect(page.getByLabel('Work item name')).toHaveValue('Updated Inspection');
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
  ).toBeTruthy();
  await page.getByLabel('Work item name').focus();
  await expect(page.getByLabel('Work item name')).toBeFocused();
  await page.screenshot({
    path: testInfo.outputPath('work-item-en-dark-mobile.png'),
    fullPage: true,
  });
  await page.getByRole('button', { name: 'Archive', exact: true }).click();
  await page.getByRole('button', { name: 'Confirm archive', exact: true }).click();
  await expect(page).toHaveURL(listUrl);
  await expect(page.getByRole('link', { name: 'Updated Inspection', exact: true })).toHaveCount(0);
  const id = new URL(itemUrl).pathname.split('/').at(-1);
  expect((await page.request.get(`${apiPath}/${id}`)).status()).toBe(404);
});

test('work items: eligible member reads but cannot create edit or archive', async ({ page }) => {
  await signIn(page);
  // Fixture mutation is restricted to this runner-owned disposable database.
  // There is no production role-management API to construct this state.
  const project = process.env.E2E_COMPOSE_PROJECT;
  expect(project).toMatch(/^surec-e2e-[a-f0-9]{8}$/);
  expect(orgId).toMatch(/^[a-f0-9-]{36}$/);
  const result = spawnSync(
    'docker',
    [
      'compose',
      '-p',
      project ?? 'invalid',
      'exec',
      '-T',
      'db',
      'sh',
      '-c',
      'exec psql -U "$POSTGRES_USER" -d "$POSTGRES_DB" -v ON_ERROR_STOP=1',
    ],
    {
      input: `UPDATE membership_roles SET role_id=(SELECT id FROM roles WHERE tenant_id='${orgId}' AND name='member' AND is_system) WHERE tenant_id='${orgId}';`,
      encoding: 'utf8',
    },
  );
  expect(result.status, result.stderr).toBe(0);
  await page.goto(listUrl);
  await expect(page.getByRole('link', { name: 'Reserved', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'İşçilik ekle', exact: true })).toHaveCount(0);
  const denied = await page.request.post(apiPath, { data: { name: 'Denied' } });
  expect(denied.status()).toBe(403);
  expect((await denied.json()).error.code).toBe('PERMISSION_DENIED');
  await page.getByRole('link', { name: 'Reserved', exact: true }).click();
  await expect(page).toHaveURL(/\/work-items\/[a-f0-9-]+$/);
  await expect(page.getByRole('heading', { name: 'Reserved', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Kaydet', exact: true })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Arşivle', exact: true })).toHaveCount(0);
  const id = new URL(page.url()).pathname.split('/').at(-1);
  expect(
    (await page.request.patch(`${apiPath}/${id}`, { data: { status: 'archived' } })).status(),
  ).toBe(403);
  expect((await page.request.get(`${apiPath}/${id}`)).status()).toBe(200);
});
