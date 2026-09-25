import { expect, request, test, type Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';

// Process EXECUTIONS (ADR 0016, STEP 21A): start/complete/cancel, immutable
// attempt history and the server-anchored display timer. Definitions are
// exercised in processes.spec.ts; this file owns the runtime surface.
test.describe.configure({ mode: 'serial' });
let orgId = '';
let itemUrl = '';
let itemPath = '';
let processId = '';

async function signIn(page: Page) {
  await page.goto('/login');
  await page.waitForLoadState('networkidle');
  await page.getByLabel('E-posta').fill('e2e8@example.test');
  await page.getByLabel('Şifre', { exact: true }).fill('e2e-password-1');
  await page.getByRole('button', { name: 'Giriş yap' }).click();
  await expect(page).toHaveURL(/\/app(?:\/|$)/);
}

async function post(page: Page, path: string, data: object): Promise<string> {
  const response = await page.request.post(path, { data });
  expect(response.status(), path).toBe(201);
  const body = await response.json();
  return body.data.workspace?.id ?? body.data.id;
}

function processRegion(page: Page) {
  return page.getByRole('region', { name: /Süreçler|Processes/ });
}

function kesimRow(page: Page) {
  return processRegion(page).getByRole('listitem').filter({ hasText: 'Kesim' });
}

async function timerSeconds(page: Page): Promise<number> {
  const text = (await kesimRow(page).getByRole('timer').textContent()) ?? '00:00:00';
  const [h, m, s] = text.trim().split(':').map(Number);
  return h * 3600 + m * 60 + s;
}

test('executions: owner starts, completes, retries and cancels with history', async ({ page }) => {
  await signIn(page);
  await page.getByLabel('Organizasyon adı').fill('Execution Org');
  await page.getByRole('button', { name: 'Organizasyon oluştur' }).click();
  await expect(page).toHaveURL(/\/app\/[a-f0-9-]+$/);
  orgId = new URL(page.url()).pathname.split('/')[2];
  const org = `/api/v1/organizations/${orgId}`;
  const ws = await post(page, `${org}/workspaces`, { name: 'Exec Ws' });
  const project = await post(page, `${org}/workspaces/${ws}/projects`, { name: 'Atölye' });
  const base = `${org}/workspaces/${ws}/projects/${project}`;
  const section = await post(page, `${base}/sections`, { name: 'Zemin' });
  const item = await post(page, `${base}/sections/${section}/work-items`, {
    name: 'Tezgah',
  });
  itemPath = `${base}/sections/${section}/work-items/${item}`;
  processId = await post(page, `${itemPath}/processes`, { name: 'Kesim' });
  itemUrl = `/app/${orgId}/${ws}/projects/${project}/sections/${section}/work-items/${item}`;

  await page.goto(itemUrl);
  const row = kesimRow(page);
  await expect(row.getByRole('button', { name: 'Başlat', exact: true })).toBeVisible();
  // Attempt history stays collapsed until work actually happens.
  await expect(row.getByText(/Deneme geçmişi/)).toHaveCount(0);

  // START → active state with a live, monotonically increasing timer.
  await row.getByRole('button', { name: 'Başlat', exact: true }).click();
  await expect(row.locator('.exec-state')).toContainText('Devam ediyor');
  await expect(row.getByRole('timer')).toHaveText(/\d{2}:\d{2}:\d{2}/);
  const first = await timerSeconds(page);
  await expect
    .poll(async () => timerSeconds(page), { message: 'timer advances' })
    .toBeGreaterThanOrEqual(first + 1);

  // Reload mid-run: the timer re-anchors to server time and stays correct.
  await page.reload();
  await expect(kesimRow(page).locator('.exec-state')).toContainText('Devam ediyor');
  const afterReload = await timerSeconds(page);
  expect(afterReload).toBeGreaterThanOrEqual(first);
  await expect
    .poll(async () => timerSeconds(page), { message: 'timer advances after reload' })
    .toBeGreaterThanOrEqual(afterReload + 1);

  // COMPLETE → terminal state with a duration and a retry affordance.
  await kesimRow(page).getByRole('button', { name: 'Tamamla', exact: true }).click();
  await expect(kesimRow(page).locator('.exec-state')).toContainText('Tamamlandı');
  await expect(kesimRow(page).getByText(/Süre: \d{2}:\d{2}:\d{2}/)).toBeVisible();
  await expect(kesimRow(page).getByRole('timer')).toHaveCount(0);

  // RETRY creates a NEW attempt — attempt history keeps the terminal record.
  await kesimRow(page).getByRole('button', { name: 'Tekrar Başlat', exact: true }).click();
  await expect(kesimRow(page).locator('.exec-state')).toContainText('Devam ediyor');
  await kesimRow(page)
    .getByText(/Deneme geçmişi/)
    .click();
  await expect(kesimRow(page).getByText('Deneme 1')).toBeVisible();
  await expect(kesimRow(page).getByText('Deneme 2')).toBeVisible();

  // CANCEL with a reason → terminal cancelled state, reason in history.
  await kesimRow(page).getByRole('button', { name: 'İptal', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Süreci iptal et' });
  await expect(dialog).toBeVisible();
  await dialog.getByLabel(/İptal nedeni/).fill('Yanlış iş emri');
  await dialog.getByRole('button', { name: 'İptali onayla' }).click();
  await expect(dialog).toBeHidden();
  await expect(kesimRow(page).locator('.exec-state')).toContainText('İptal edildi');
  // The disclosure survives invalidation (same DOM node); open it only when
  // the previous interaction left it closed.
  const history = kesimRow(page).locator('details.exec-history');
  if (!(await history.evaluate((el: HTMLDetailsElement) => el.open))) {
    await history.locator('summary').click();
  }
  await expect(kesimRow(page).getByText('Yanlış iş emri')).toBeVisible();

  // Backend state agrees: two terminal attempts, none active.
  const list = await (await page.request.get(`${itemPath}/executions`)).json();
  expect(list.data.map((e: { attempt_no: number }) => e.attempt_no)).toEqual([1, 2]);
  expect(list.data.map((e: { status: string }) => e.status)).toEqual(['completed', 'cancelled']);
  expect(list.data[1].cancel_reason).toBe('Yanlış iş emri');
  expect(typeof list.server_time).toBe('string');
});

test('executions: member executes but cannot administer definitions', async ({ page }) => {
  await signIn(page);
  // Demote this runner-owned org membership to the built-in Member role.
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
  const row = kesimRow(page);
  // Member carries the three execution grants: the runtime controls stay.
  await row.getByRole('button', { name: 'Tekrar Başlat', exact: true }).click();
  await expect(row.locator('.exec-state')).toContainText('Devam ediyor');
  await row.getByRole('button', { name: 'Tamamla', exact: true }).click();
  await expect(row.locator('.exec-state')).toContainText('Tamamlandı');
  // …but no definition administration surface appears.
  await expect(page.getByRole('button', { name: 'Süreç ekle' })).toHaveCount(0);
  await expect(page.getByRole('button', { name: /sürecini (yukarı|aşağı) taşı/ })).toHaveCount(0);
  await expect(page.getByRole('button', { name: /işlemleri$/ })).toHaveCount(0);
  // Backend authority is unaffected by the visible controls.
  const denied = await page.request.patch(`${itemPath}/processes/${processId}`, {
    data: { name: 'Denied' },
  });
  expect(denied.status()).toBe(403);
});

test('executions: non-member gets uniform 404 and no controls', async ({ page }) => {
  // A user outside the organization sees nothing — even the API is a
  // not-found, identical to a guessed id (enumeration resistance).
  const foreign = await request.newContext({ baseURL: 'http://127.0.0.1:4173' });
  const login = await foreign.post('/api/v1/auth/login', {
    data: { email: 'e2e@example.test', password: 'e2e-password-1' },
  });
  expect(login.status()).toBe(200);
  for (const [method, path] of [
    ['get', `${itemPath}/executions`],
    ['post', `${itemPath}/processes/${processId}/executions`],
  ] as const) {
    const response =
      method === 'get' ? await foreign.get(path) : await foreign.post(path, { data: {} });
    expect(response.status(), path).toBe(404);
  }
  await foreign.dispose();
  // The signed-in member's page itself never exposes admin affordances.
  await signIn(page);
  await page.goto(itemUrl);
  await expect(kesimRow(page)).toBeVisible();
});

test('executions: English, dark mode and 360px render correctly', async ({ page, context }) => {
  // Sign in first: the locale cookie would render the login form in English.
  await signIn(page);
  await context.addCookies([
    { name: 'locale', value: 'en', url: 'http://127.0.0.1:4173' },
    { name: 'theme', value: 'dark', url: 'http://127.0.0.1:4173' },
  ]);
  await page.setViewportSize({ width: 360, height: 800 });
  await page.goto(itemUrl);
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  const row = kesimRow(page);
  await expect(row.locator('.exec-state')).toContainText('Completed');
  await expect(row.getByRole('button', { name: 'Start again', exact: true })).toBeVisible();
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
    'horizontal overflow at 360px',
  ).toBeTruthy();
});
