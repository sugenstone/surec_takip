import { expect, test, type Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';

// Process DEFINITIONS (ADR 0015): configuration only. These specs assert
// ordering, required/optional, reorder, archive and permissions — and that
// no execution UI (progress, timers, assignees) is rendered.
test.describe.configure({ mode: 'serial' });
let orgId = '';
let itemUrl = '';
let apiPath = '';

async function signIn(page: Page) {
  await page.goto('/login');
  await page.waitForLoadState('networkidle');
  await page.getByLabel('E-posta').fill('e2e7@example.test');
  await page.getByLabel('Şifre', { exact: true }).fill('e2e-password-1');
  await page.getByRole('button', { name: 'Giriş yap' }).click();
  await expect(page).toHaveURL(/\/app(?:\/|$)/);
}

async function post(page: Page, path: string, name: string): Promise<string> {
  const response = await page.request.post(path, { data: { name } });
  expect(response.status(), path).toBe(201);
  const body = await response.json();
  return body.data.workspace?.id ?? body.data.id;
}

function processRegion(page: Page) {
  return page.getByRole('region', { name: 'Süreçler' });
}

async function processNames(page: Page): Promise<string[]> {
  return processRegion(page).getByRole('heading', { level: 3 }).allTextContents();
}

async function addProcess(page: Page, name: string, required = true) {
  await processRegion(page).getByRole('button', { name: 'Süreç ekle', exact: true }).click();
  const drawer = page.getByRole('dialog', { name: 'Süreç ekle' });
  await drawer.getByLabel('Süreç adı').fill(name);
  if (!required) await drawer.getByLabel('Zorunlu süreç').uncheck();
  await drawer.getByRole('button', { name: 'Oluştur', exact: true }).click();
  await expect(drawer).toBeHidden();
  await expect(processRegion(page).getByRole('heading', { name, exact: true })).toBeVisible();
}

test('processes: owner defines, edits, reorders and archives ordered processes', async ({
  page,
}) => {
  await signIn(page);
  await page.getByLabel('Organizasyon adı').fill('Process Org');
  await page.getByRole('button', { name: 'Organizasyon oluştur' }).click();
  await expect(page).toHaveURL(/\/app\/[a-f0-9-]+$/);
  orgId = new URL(page.url()).pathname.split('/')[2];
  // The hierarchy is prepared through the same authenticated API the UI
  // uses; the process surface itself is exercised through the browser.
  const org = `/api/v1/organizations/${orgId}`;
  const ws = await post(page, `${org}/workspaces`, 'Process Ws');
  const project = await post(page, `${org}/workspaces/${ws}/projects`, 'Mutfak');
  const base = `${org}/workspaces/${ws}/projects/${project}`;
  const section = await post(page, `${base}/sections`, 'Daire 1');
  const item = await post(page, `${base}/sections/${section}/work-items`, 'Mutfak Tezgahı');
  apiPath = `${base}/sections/${section}/work-items/${item}/processes`;
  itemUrl = `/app/${orgId}/${ws}/projects/${project}/sections/${section}/work-items/${item}`;

  await page.goto(itemUrl);
  await expect(page.getByRole('heading', { name: 'Mutfak Tezgahı', level: 1 })).toBeVisible();
  await expect(
    processRegion(page).getByText('Bu işçilikte henüz süreç tanımlı değil.'),
  ).toBeVisible();
  for (const [name, required] of [
    ['Taş Alımı', true],
    ['Kesim', true],
    ['İmalat', true],
    ['Nakliye', false],
    ['Montaj', true],
  ] as const) {
    await addProcess(page, name, required);
  }
  const expected = ['Taş Alımı', 'Kesim', 'İmalat', 'Nakliye', 'Montaj'];
  expect(await processNames(page)).toEqual(expected);
  const nakliye = processRegion(page).getByRole('listitem').filter({ hasText: 'Nakliye' });
  await expect(nakliye).toContainText('Opsiyonel');
  await expect(processRegion(page).getByRole('listitem').first()).toContainText('01');
  await expect(processRegion(page).getByRole('listitem').first()).toContainText('Zorunlu');
  // Execution (STEP 21A) adds Start controls, but no progress percentages,
  // assignees or completion shortcuts exist yet.
  await expect(processRegion(page).getByText(/%|Atanan|Assignee/i)).toHaveCount(0);

  // Deep link + reload keeps the persisted order.
  await page.reload();
  expect(await processNames(page)).toEqual(expected);

  // A duplicate short name keeps the drawer open with the draft intact.
  await processRegion(page).getByRole('button', { name: 'Süreç ekle', exact: true }).click();
  const drawer = page.getByRole('dialog', { name: 'Süreç ekle' });
  await drawer.getByLabel('Süreç adı').fill('Taş Alımı');
  await drawer.getByRole('button', { name: 'Oluştur', exact: true }).click();
  await expect(drawer.getByRole('alert')).toContainText('zaten kullanılıyor');
  await expect(drawer.getByLabel('Süreç adı')).toHaveValue('Taş Alımı');
  await drawer.getByRole('button', { name: 'Vazgeç', exact: true }).click();
  await expect(drawer).toBeHidden();

  // Edit through the row action menu.
  await page.getByRole('button', { name: 'Kesim işlemleri' }).click();
  await page.getByRole('menuitem', { name: 'Süreci düzenle' }).click();
  const edit = page.getByRole('dialog', { name: 'Süreci düzenle' });
  await expect(edit.getByLabel('Süreç adı')).toHaveValue('Kesim');
  await edit.getByLabel('Süreç adı').fill('CNC Kesim');
  await edit.getByLabel('Açıklama').fill('Ölçüye göre kesim');
  await edit.getByLabel('Zorunlu süreç').uncheck();
  await edit.getByRole('button', { name: 'Süreci kaydet' }).click();
  await expect(edit).toBeHidden();
  const cnc = processRegion(page).getByRole('listitem').filter({ hasText: 'CNC Kesim' });
  await expect(cnc).toContainText('Ölçüye göre kesim');
  await expect(cnc).toContainText('Opsiyonel');

  // Keyboard-accessible reorder: move Montaj up twice; focus follows it.
  const up = page.getByRole('button', { name: 'Montaj sürecini yukarı taşı' });
  await up.click();
  await expect(page.getByTestId('process-announcement')).toHaveText('Montaj 4. sıraya taşındı.');
  await expect(up).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(async () =>
    expect(await processNames(page)).toEqual([
      'Taş Alımı',
      'CNC Kesim',
      'Montaj',
      'İmalat',
      'Nakliye',
    ]),
  ).toPass();
  await expect(page.getByRole('button', { name: 'Taş Alımı sürecini yukarı taşı' })).toBeDisabled();
  await expect(page.getByRole('button', { name: 'Nakliye sürecini aşağı taşı' })).toBeDisabled();
  await page.reload();
  expect(await processNames(page)).toEqual([
    'Taş Alımı',
    'CNC Kesim',
    'Montaj',
    'İmalat',
    'Nakliye',
  ]);

  // Archive requires deliberate confirmation; cancel keeps the row.
  await page.getByRole('button', { name: 'Nakliye işlemleri' }).click();
  await page.getByRole('menuitem', { name: 'Süreci arşivle' }).click();
  const confirm = page.getByRole('dialog', { name: 'Süreci arşivle' });
  await expect(confirm).toContainText('Nakliye');
  await confirm.getByRole('button', { name: 'Vazgeç', exact: true }).click();
  await expect(confirm).toBeHidden();
  await expect(processRegion(page).getByRole('heading', { name: 'Nakliye' })).toBeVisible();
  await page.getByRole('button', { name: 'Nakliye işlemleri' }).click();
  await page.getByRole('menuitem', { name: 'Süreci arşivle' }).click();
  await confirm.getByRole('button', { name: 'Arşivlemeyi onayla' }).click();
  await expect(confirm).toBeHidden();
  await expect(processRegion(page).getByRole('heading', { name: 'Nakliye' })).toHaveCount(0);
  const list = await (await page.request.get(apiPath)).json();
  expect(list.data.map((p: { name: string }) => p.name)).toEqual([
    'Taş Alımı',
    'CNC Kesim',
    'Montaj',
    'İmalat',
  ]);
});

test('processes: mobile light/dark, English, no overflow', async ({ page, context }) => {
  await signIn(page);
  for (const width of [360, 375, 768]) {
    await page.setViewportSize({ width, height: 800 });
    await page.goto(itemUrl);
    await expect(processRegion(page).getByRole('heading', { name: 'Montaj' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Montaj sürecini yukarı taşı' })).toBeEnabled();
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
      `overflow at ${width}`,
    ).toBeTruthy();
  }
  await context.addCookies([
    { name: 'locale', value: 'en', url: 'http://127.0.0.1:4173' },
    { name: 'theme', value: 'dark', url: 'http://127.0.0.1:4173' },
  ]);
  await page.setViewportSize({ width: 360, height: 800 });
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  const region = page.getByRole('region', { name: 'Processes' });
  await expect(region.getByRole('listitem').filter({ hasText: 'CNC Kesim' })).toContainText(
    'Optional',
  );
  await expect(region.getByRole('button', { name: 'Add process', exact: true })).toBeVisible();
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
  ).toBeTruthy();
  // Reorder controls are enabled only after hydration.
  await expect(page.getByRole('button', { name: 'Move Montaj down' })).toBeEnabled();
  await page.getByRole('button', { name: 'Move Montaj down' }).focus();
  await expect(page.getByRole('button', { name: 'Move Montaj down' })).toBeFocused();
});

test('processes: eligible member reads but receives no mutation controls', async ({ page }) => {
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
  await page.goto(itemUrl);
  expect(await processNames(page)).toEqual(['Taş Alımı', 'CNC Kesim', 'Montaj', 'İmalat']);
  await expect(page.getByRole('button', { name: 'Süreç ekle' })).toHaveCount(0);
  await expect(page.getByRole('button', { name: /sürecini (yukarı|aşağı) taşı/ })).toHaveCount(0);
  await expect(page.getByRole('button', { name: /işlemleri$/ })).toHaveCount(0);
  // The backend stays the authority regardless of hidden controls.
  const list = await (await page.request.get(apiPath)).json();
  const ids = list.data.map((p: { id: string }) => p.id);
  const denied = await page.request.post(apiPath, { data: { name: 'Denied' } });
  expect(denied.status()).toBe(403);
  expect((await denied.json()).error.code).toBe('PERMISSION_DENIED');
  expect(
    (
      await page.request.patch(`${apiPath}/reorder`, { data: { process_ids: ids.reverse() } })
    ).status(),
  ).toBe(403);
  expect(
    (await page.request.patch(`${apiPath}/${ids[0]}`, { data: { status: 'archived' } })).status(),
  ).toBe(403);
  const after = await (await page.request.get(apiPath)).json();
  expect(after.data.map((p: { name: string }) => p.name)).toEqual([
    'Taş Alımı',
    'CNC Kesim',
    'Montaj',
    'İmalat',
  ]);
});
