import { expect, request, test, type Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';

// TIME SESSIONS (ADR 0019, STEP 21D): tracked labor under an execution —
// start/pause controls on the active attempt, live worker chips, and the
// one-open-session-per-worker rule surfacing as a localized conflict.
test.describe.configure({ mode: 'serial' });
let orgId = '';
let wsId = '';
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
  wsId = ws;
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

  // START WORK → the self worker chip appears with the "(sen)" marker and
  // a live labor timer in HH:MM:SS — anchored to the session, not the
  // execution clock.
  await row.getByTestId(`work-start-${processId}`).click();
  await expect(row.getByTestId(`work-pause-${processId}`)).toBeVisible();
  const workers = row.getByTestId(`workers-${processId}`);
  await expect(workers).toContainText('E2e Sessions');
  await expect(workers).toContainText('sen');
  const timer = workers.locator('[role="timer"]');
  await expect(timer).toHaveCount(1);
  await expect(timer).toHaveText(/\d{2}:\d{2}:\d{2}/);
  // The display tick advances without any API write: the rendered value
  // changes within a bounded window (one interval tick = 1s).
  const before = await timer.textContent();
  await expect
    .poll(async () => (await timer.textContent()) ?? '', { timeout: 5_000 })
    .not.toBe(before ?? '');

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

// Direct SQL against the run's isolated database — the same mechanism
// assignments.spec.ts uses for workspace membership (no product API yet).
function sql(input: string) {
  const project = process.env.E2E_COMPOSE_PROJECT;
  expect(project).toMatch(/^surec-e2e-[a-f0-9]{8}$/);
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
      'exec psql -U "$POSTGRES_USER" -d "$POSTGRES_DB" -v ON_ERROR_STOP=1 -t -A',
    ],
    { input, encoding: 'utf8' },
  );
  expect(result.status, result.stderr).toBe(0);
  return result.stdout.trim();
}

test('sessions: two workers keep independent labor timers', async ({ page }) => {
  await signIn(page);
  // Invite a second workspace member (org membership + built-in member
  // role), then seed the workspace membership at the persistence layer.
  const member = 'e2e3@example.test';
  const org = `/api/v1/organizations/${orgId}`;
  const invite = await page.request.post(`${org}/invitations`, {
    data: { email: member },
  });
  expect(invite.ok()).toBeTruthy();
  const token = ((await invite.json()) as { data?: { token?: string } }).data?.token ?? '';
  const memberContext = await request.newContext({ baseURL: 'http://127.0.0.1:4173' });
  const memberLogin = await memberContext.post('/api/v1/auth/login', {
    data: { email: member, password: 'e2e-password-1' },
  });
  expect(memberLogin.ok()).toBeTruthy();
  const accept = await memberContext.post('/api/v1/invitations/accept', { data: { token } });
  expect(accept.ok()).toBeTruthy();
  sql(
    `INSERT INTO workspace_memberships (id, tenant_id, workspace_id, user_id, status) ` +
      `SELECT gen_random_uuid(), '${orgId}', '${wsId}', id, 'active' FROM users ` +
      `WHERE email='${member}';`,
  );

  // The owner's Kesim session is still open from the previous test; find
  // the active execution and have the member start labor on it too.
  const executions = await page.request.get(`${itemPath}/executions`);
  const active = (
    (await executions.json()).data as { id: string; process_id: string; status: string }[]
  ).find((row) => row.process_id === processId && row.status === 'active');
  expect(active).toBeTruthy();
  const memberStart = await memberContext.post(
    `${itemPath}/processes/${processId}/executions/${active!.id}/time-sessions`,
    { data: {} },
  );
  expect(memberStart.status()).toBe(201);
  await memberContext.dispose();

  await page.goto(itemUrl);
  const workers = kesimRow(page).getByTestId(`workers-${processId}`);
  const ownerChip = workers.locator('.worker-chip').filter({ hasText: 'E2e Sessions' });
  const memberChip = workers.locator('.worker-chip').filter({ hasText: 'E2e User Three' });
  await expect(ownerChip).toHaveCount(1);
  await expect(memberChip).toHaveCount(1);
  // Each chip carries its own ticking labor timer — never a shared or
  // collapsed count.
  await expect(ownerChip.locator('[role="timer"]')).toHaveText(/\d{2}:\d{2}:\d{2}/);
  await expect(memberChip.locator('[role="timer"]')).toHaveText(/\d{2}:\d{2}:\d{2}/);
  await expect(ownerChip).toContainText('sen');
});
