import { expect, request, test, type Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';

// ASSIGNMENT domain (ADR 0018, STEP 21C): process responsibility is a
// separate fact from execution actors. This file owns the assignment UX —
// picker dialog, chip persistence, permission-aware controls — plus the
// snapshot invariant checked at API level (the history UI deliberately does
// not render assignees).
test.describe.configure({ mode: 'serial' });

const OWNER = 'e2e10@example.test';
const MEMBER = 'e2e3@example.test';
const MEMBER_NAME = 'E2e User Three';
const PASSWORD = 'e2e-password-1';

let orgId = '';
let wsId = '';
let itemUrl = '';
let itemPath = '';
let processId = '';
let memberId = '';

async function signIn(page: Page, email: string) {
  await page.goto('/login');
  await page.waitForLoadState('networkidle');
  await page.getByLabel('E-posta').fill(email);
  await page.getByLabel('Şifre', { exact: true }).fill(PASSWORD);
  await page.getByRole('button', { name: 'Giriş yap' }).click();
  await expect(page).toHaveURL(/\/app(?:\/|$)/);
}

async function post(page: Page, path: string, data: object): Promise<string> {
  const response = await page.request.post(path, { data });
  expect(response.status(), path).toBe(201);
  const body = await response.json();
  return body.data.workspace?.id ?? body.data.id;
}

// Direct SQL against the run's isolated database — the same mechanism
// executions.spec.ts uses for role changes. Workspace membership has no
// product API yet, so the fixture is seeded at the persistence layer.
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

// psql prints command tags ("INSERT 0 1") after result rows — extract the
// uuid line rather than trusting raw stdout.
function sqlUuid(input: string): string {
  return sql(input).match(/[0-9a-f]{8}-[0-9a-f-]{27,}/)?.[0] ?? '';
}

function montajRow(page: Page) {
  return page
    .getByRole('region', { name: /Süreçler|Processes/ })
    .getByRole('listitem')
    .filter({ hasText: 'Montaj' });
}

async function openAssignDialog(page: Page) {
  const row = montajRow(page);
  await row.getByRole('button', { name: 'Montaj işlemleri' }).click();
  await page.getByRole('menuitem', { name: 'Sorumlu değiştir' }).click();
  return page.getByRole('dialog', { name: 'Montaj sürecine ata' });
}

test('assignments: owner assigns via the picker; reload persists; unassign works', async ({
  page,
}) => {
  await signIn(page, OWNER);
  await page.getByLabel('Organizasyon adı').fill('Assignment Org');
  await page.getByRole('button', { name: 'Organizasyon oluştur' }).click();
  await expect(page).toHaveURL(/\/app\/[a-f0-9-]+$/);
  orgId = new URL(page.url()).pathname.split('/')[2];
  const org = `/api/v1/organizations/${orgId}`;
  wsId = await post(page, `${org}/workspaces`, { name: 'Assign Ws' });
  const project = await post(page, `${org}/workspaces/${wsId}/projects`, { name: 'Şantiye' });
  const base = `${org}/workspaces/${wsId}/projects/${project}`;
  const section = await post(page, `${base}/sections`, { name: 'Daire 1' });
  const item = await post(page, `${base}/sections/${section}/work-items`, { name: 'Tezgah' });
  itemPath = `${base}/sections/${section}/work-items/${item}`;
  processId = await post(page, `${itemPath}/processes`, { name: 'Montaj' });
  itemUrl = `/app/${orgId}/${wsId}/projects/${project}/sections/${section}/work-items/${item}`;

  // Invite the member (org membership + built-in member role), then seed the
  // workspace membership — no product API exists for it yet.
  const invite = await page.request.post(`${org}/invitations`, {
    data: { email: MEMBER },
  });
  expect(invite.ok()).toBeTruthy();
  const token = ((await invite.json()) as { data?: { token?: string } }).data?.token ?? '';
  expect(token).not.toBe('');
  const memberContext = await request.newContext({ baseURL: 'http://127.0.0.1:4173' });
  const memberLogin = await memberContext.post('/api/v1/auth/login', {
    data: { email: MEMBER, password: PASSWORD },
  });
  expect(memberLogin.ok()).toBeTruthy();
  const accept = await memberContext.post('/api/v1/invitations/accept', { data: { token } });
  expect(accept.ok()).toBeTruthy();
  memberId = sqlUuid(
    `INSERT INTO workspace_memberships (id, tenant_id, workspace_id, user_id, status) ` +
      `SELECT gen_random_uuid(), '${orgId}', '${wsId}', id, 'active' FROM users ` +
      `WHERE email='${MEMBER}' RETURNING user_id;`,
  );
  expect(memberId).toMatch(/^[0-9a-f-]{36}$/);

  await page.goto(itemUrl);
  const chip = montajRow(page).getByTestId(`assignee-${processId}`);
  await expect(chip).toContainText('Atanmamış');

  // Assign the member through the deliberate dialog.
  const dialog = await openAssignDialog(page);
  await expect(dialog).toBeVisible();
  await dialog.getByRole('radio', { name: MEMBER_NAME }).check();
  await dialog.getByRole('button', { name: 'Sorumluyu kaydet' }).click();
  await expect(dialog).toBeHidden();
  await expect(chip).toContainText(MEMBER_NAME);
  await expect(chip).not.toContainText('Artık üye değil');

  // Reload: assignment persists and re-renders from the server.
  await page.reload();
  await expect(montajRow(page).getByTestId(`assignee-${processId}`)).toContainText(MEMBER_NAME);

  // Unassign via the explicit "no assignee" option.
  const reopen = await openAssignDialog(page);
  await reopen.getByRole('radio', { name: 'Sorumlu yok' }).check();
  await reopen.getByRole('button', { name: 'Sorumluyu kaydet' }).click();
  await expect(reopen).toBeHidden();
  await expect(montajRow(page).getByTestId(`assignee-${processId}`)).toContainText('Atanmamış');
  await page.reload();
  await expect(montajRow(page).getByTestId(`assignee-${processId}`)).toContainText('Atanmamış');

  // Leave the member assigned for the following tests.
  const assign = await page.request.put(`${itemPath}/processes/${processId}/assignment`, {
    data: { user_id: memberId },
  });
  expect(assign.status()).toBe(200);
  await memberContext.dispose();
});

test('assignments: member sees the assignee but never the control', async ({ page }) => {
  // The member carries process_executions:* but never processes:assign.
  await signIn(page, MEMBER);
  await page.goto(itemUrl);
  const row = montajRow(page);
  await expect(row).toBeVisible();
  await expect(row.getByTestId(`assignee-${processId}`)).toContainText(MEMBER_NAME);
  // Member sees zero definition administration, including assignment.
  await expect(row.getByRole('button', { name: 'Montaj işlemleri' })).toHaveCount(0);
  await expect(page.getByRole('menuitem', { name: 'Sorumlu değiştir' })).toHaveCount(0);
  // Backend authority is unaffected by the hidden control.
  const denied = await page.request.put(`${itemPath}/processes/${processId}/assignment`, {
    data: { user_id: memberId },
  });
  expect(denied.status()).toBe(403);
});

test('assignments: execution snapshot survives reassignment', async ({ page }) => {
  await signIn(page, OWNER);
  // Start attempt 1 while the process is assigned to the member.
  const start = await page.request.post(`${itemPath}/processes/${processId}/executions`, {
    data: {},
  });
  expect(start.status()).toBe(201);
  const attempt = (await start.json()).data;
  expect(attempt.assignee_user_id).toBe(memberId);
  expect(attempt.started_by_user_id).not.toBe(memberId); // actor = owner
  // Reassign to the owner — the running attempt's snapshot must not move.
  const ownerId = sqlUuid(`SELECT id FROM users WHERE email='${OWNER}';`);
  const reassign = await page.request.put(`${itemPath}/processes/${processId}/assignment`, {
    data: { user_id: ownerId },
  });
  expect(reassign.status()).toBe(200);
  const history = await (await page.request.get(`${itemPath}/executions`)).json();
  const attempt1 = history.data.find((e: { attempt_no: number }) => e.attempt_no === 1);
  expect(attempt1.assignee_user_id).toBe(memberId);
  // Attempt 2 snapshots the new assignee.
  const complete = await page.request.post(
    `${itemPath}/processes/${processId}/executions/${attempt.id}/complete`,
    { data: {} },
  );
  expect(complete.status()).toBe(200);
  const restart = await page.request.post(`${itemPath}/processes/${processId}/executions`, {
    data: {},
  });
  expect(restart.status()).toBe(201);
  expect((await restart.json()).data.assignee_user_id).toBe(ownerId);
});

test('assignments: chip renders in dark mode at 360px without overflow', async ({
  page,
  context,
}) => {
  await signIn(page, OWNER);
  await context.addCookies([
    { name: 'theme', value: 'dark', url: 'http://127.0.0.1:4173' },
    { name: 'locale', value: 'en', url: 'http://127.0.0.1:4173' },
  ]);
  await page.setViewportSize({ width: 360, height: 800 });
  await page.goto(itemUrl);
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  const chip = montajRow(page).getByTestId(`assignee-${processId}`);
  await expect(chip).toContainText('E2e Assignments');
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
    'horizontal overflow at 360px',
  ).toBeTruthy();
});
