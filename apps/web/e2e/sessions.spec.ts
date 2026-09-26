import { expect, test, type Page } from '@playwright/test';

// TIME SESSIONS (ADR 0019, STEP 21D): tracked labor under an execution —
// start/pause controls on the active attempt, live worker chips, and the
// one-open-session-per-worker rule surfacing as a localized conflict.
test.describe.configure({ mode: 'serial' });
let orgId = '';
let itemUrl = '';
let itemPath = '';
let processId = '';

async function signIn(page: Page) {
  await page.goto('/login');
  await page.waitForLoadState('networkidle');
  await page.getByLabel('E-posta').fill('e2e11@example.test');
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

test('sessions: worker starts, pauses and resumes tracked labor', async ({ page }) => {
  await signIn(page);
  await page.getByLabel('Organizasyon adı').fill('Session Org');
  await page.getByRole('button', { name: 'Organizasyon oluştur' }).click();
  await expect(page).toHaveURL(/\/app\/[a-f0-9-]+$/);
  orgId = new URL(page.url()).pathname.split('/')[2];
  const org = `/api/v1/organizations/${orgId}`;
  const ws = await post(page, `${org}/workspaces`, { name: 'Session Ws' });
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
  // No active execution → no work control, no worker chips.
  await expect(row.getByTestId(`work-start-${processId}`)).toHaveCount(0);
  await expect(row.getByTestId(`workers-${processId}`)).toHaveCount(0);

  await row.getByRole('button', { name: 'Başlat', exact: true }).click();
  await expect(row.locator('.exec-state')).toContainText('Devam ediyor');

  // START WORK → the self worker chip appears with the "(sen)" marker.
  await row.getByTestId(`work-start-${processId}`).click();
  await expect(row.getByTestId(`work-pause-${processId}`)).toBeVisible();
  const workers = row.getByTestId(`workers-${processId}`);
  await expect(workers).toContainText('E2e Sessions');
  await expect(workers).toContainText('sen');

  // Pause → the chip clears and the start control returns.
  await row.getByTestId(`work-pause-${processId}`).click();
  await expect(row.getByTestId(`workers-${processId}`)).toHaveCount(0);
  await expect(row.getByTestId(`work-start-${processId}`)).toBeVisible();

  // Resume → a fresh interval opens; history stays server-side.
  await row.getByTestId(`work-start-${processId}`).click();
  await expect(row.getByTestId(`work-pause-${processId}`)).toBeVisible();
});

test('sessions: completing the execution auto-closes open work', async ({ page }) => {
  await signIn(page);
  await page.goto(itemUrl);
  const row = kesimRow(page);
  await expect(row.getByTestId(`workers-${processId}`)).toContainText('E2e Sessions');

  // COMPLETE the execution → the open session closes with it and the
  // exec bar collapses to the terminal state.
  await row.getByRole('button', { name: 'Tamamla' }).click();
  await expect(row.locator('.exec-state')).toContainText('Tamamlandı');
  await expect(row.getByTestId(`workers-${processId}`)).toHaveCount(0);
  await expect(row.getByTestId(`work-start-${processId}`)).toHaveCount(0);
});

test('sessions: second active execution on another process conflicts', async ({ page }) => {
  await signIn(page);
  // Seed a second process + active execution + open session via the API.
  const montajId = await post(page, `${itemPath}/processes`, { name: 'Montaj' });
  const first = await page.request.post(`${itemPath}/processes/${processId}/executions`, {
    data: {},
  });
  expect(first.status()).toBe(201);
  const firstExec = (await first.json()).data.id;
  const secondResponse = await page.request.post(`${itemPath}/processes/${montajId}/executions`, {
    data: {},
  });
  expect(secondResponse.status()).toBe(201);
  const secondExec = (await secondResponse.json()).data;
  // Open a session on the FIRST process through the API.
  const sessionResponse = await page.request.post(
    `${itemPath}/processes/${processId}/executions/${firstExec}/time-sessions`,
    { data: {} },
  );
  expect(sessionResponse.status()).toBe(201);

  await page.goto(itemUrl);
  // Kesim shows the live worker; Montaj offers a start-work control.
  await expect(kesimRow(page).getByTestId(`workers-${processId}`)).toContainText('E2e Sessions');
  const montajRow = processRegion(page).getByRole('listitem').filter({ hasText: 'Montaj' });
  await montajRow.getByTestId(`work-start-${montajId}`).click();
  // ACTIVE_SESSION_EXISTS surfaces as the localized conflict message.
  await expect(
    page.getByText('Zaten açık bir çalışma oturumunuz var', { exact: false }),
  ).toBeVisible();
  // The Kesim session stays open — never a silent auto-stop.
  await expect(kesimRow(page).getByTestId(`workers-${processId}`)).toContainText('E2e Sessions');
  const secondSessions = await page.request.get(
    `${itemPath}/processes/${montajId}/executions/${secondExec.id}/time-sessions`,
  );
  expect((await secondSessions.json()).data).toHaveLength(0);
});
