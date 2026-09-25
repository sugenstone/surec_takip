import { expect, test, type Page } from '@playwright/test';

// Derived PROGRESS (ADR 0017, STEP 21B): the same leaf-weighted aggregates
// surface on project/section/work-item cards and detail strips. The math is
// locked by the integration suite; this file proves the real pipeline —
// backend-derived percent reaching the UI, retry regression and the
// zero-work "—" state.
test.describe.configure({ mode: 'serial' });

let orgId = '';
let wsId = '';
let projectUrl = '';
let sectionUrl = '';
let childUrl = '';
let itemUrl = '';
let processPathA = '';
let itemPath = '';

async function signIn(page: Page) {
  await page.goto('/login');
  await page.waitForLoadState('networkidle');
  await page.getByLabel('E-posta').fill('e2e9@example.test');
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

test('progress: backend-derived aggregates render on every hierarchy surface', async ({ page }) => {
  await signIn(page);
  await page.getByLabel('Organizasyon adı').fill('Progress Org');
  await page.getByRole('button', { name: 'Organizasyon oluştur' }).click();
  await expect(page).toHaveURL(/\/app\/[a-f0-9-]+$/);
  orgId = new URL(page.url()).pathname.split('/')[2];

  const org = `/api/v1/organizations/${orgId}`;
  wsId = await post(page, `${org}/workspaces`, { name: 'Progress Ws' });
  const project = await post(page, `${org}/workspaces/${wsId}/projects`, { name: 'Site' });
  const base = `${org}/workspaces/${wsId}/projects/${project}`;
  const section = await post(page, `${base}/sections`, { name: 'A Blok' });
  const child = await post(page, `${base}/sections`, {
    name: 'Kat 1',
    parent_section_id: section,
  });
  // A sibling section with no work at all exercises the "—" zero state.
  const empty = await post(page, `${base}/sections`, { name: 'B Blok' });
  const item = await post(page, `${base}/sections/${child}/work-items`, { name: 'Tezgah' });
  itemPath = `${base}/sections/${child}/work-items/${item}`;
  const processA = await post(page, `${itemPath}/processes`, { name: 'Kesim' });
  await post(page, `${itemPath}/processes`, { name: 'Montaj' });
  processPathA = `${itemPath}/processes/${processA}`;

  projectUrl = `/app/${orgId}/${wsId}/projects/${project}`;
  sectionUrl = `${projectUrl}/sections/${section}`;
  childUrl = `${projectUrl}/sections/${child}`;
  itemUrl = `${childUrl}/work-items/${item}`;
  const emptyUrl = `${projectUrl}/sections/${empty}`;
  const listUrl = `/app/${orgId}/${wsId}/projects`;

  // One of two processes completed → 50% everywhere the item's work rolls up.
  const attempt = await post(page, `${processPathA}/executions`, {});
  await page.request.post(`${processPathA}/executions/${attempt}/complete`, { data: {} });

  await page.goto(itemUrl);
  const itemStrip = page.locator('.progress-strip');
  await expect(itemStrip).toContainText('50%');
  await expect(itemStrip).toContainText('1/2 süreç');
  await expect(itemStrip.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '50');

  await page.goto(sectionUrl);
  await expect(page.locator('.progress-strip')).toContainText('50%');
  const childCard = page.locator('article.section-card', { hasText: 'Kat 1' });
  await expect(childCard).toContainText('50%');
  // B Blok lives at the project level: its card shows "—", never a fake
  // 0% bar.
  await page.goto(childUrl);
  await expect(page.locator('.progress-strip')).toContainText('50%');
  await expect(page.locator('article.work-item-card', { hasText: 'Tezgah' })).toContainText('50%');

  await page.goto(projectUrl);
  await expect(page.locator('.progress-strip')).toContainText('50%');
  const emptyCard = page.locator('article.section-card', { hasText: 'B Blok' });
  await expect(emptyCard).toContainText('—');
  await expect(emptyCard.getByRole('progressbar')).toHaveCount(0);
  const aBlok = page.locator('article.section-card', { hasText: 'A Blok' });
  await expect(aBlok).toContainText('50%');

  await page.goto(listUrl);
  const projectCard = page.getByRole('listitem').filter({ hasText: 'Site' });
  await expect(projectCard).toContainText('50%');
  await expect(projectCard).toContainText('1/2 süreç');

  // Zero-work detail strip: "—" plus the localized empty sentence.
  await page.goto(emptyUrl);
  const emptyStrip = page.locator('.progress-strip');
  await expect(emptyStrip).toContainText('—');
  await expect(emptyStrip).toContainText('Henüz süreç tanımlanmadı');
});

test('progress: active retry regresses the aggregate truthfully', async ({ page }) => {
  await signIn(page);
  expect(processPathA).not.toBe('');
  // GET work-item detail returns a bare WorkItemPublic (no data wrapper).
  const itemProgress = async () => (await (await page.request.get(itemPath)).json()).progress;

  // Completing the second process → 2/2 = 100%.
  const list = await (await page.request.get(`${itemPath}/processes`)).json();
  const remaining = list.data.find((p: { name: string }) => p.name === 'Montaj');
  const attempt = await post(page, `${itemPath}/processes/${remaining.id}/executions`, {});
  await page.request.post(`${itemPath}/processes/${remaining.id}/executions/${attempt}/complete`, {
    data: {},
  });

  await page.goto(itemUrl);
  await expect(page.locator('.progress-strip')).toContainText('100%');
  expect((await itemProgress()).percent).toBe(100);

  // RETRY on the completed process → it stops being DONE: 100% → 50%.
  await post(page, `${processPathA}/executions`, { start_reason: 'Yeniden iş' });
  await page.goto(itemUrl);
  const strip = page.locator('.progress-strip');
  await expect(strip).toContainText('50%');
  await expect(strip).toContainText('1/2 süreç');
  await expect(strip).toContainText('1 devam ediyor');
  expect((await itemProgress()).active).toBe(1);

  await page.goto(`/app/${orgId}/${wsId}/projects`);
  await expect(page.getByRole('listitem').filter({ hasText: 'Site' })).toContainText('50%');
});

test('progress: narrow viewport and dark theme keep the meter readable', async ({ page }) => {
  await page.setViewportSize({ width: 375, height: 720 });
  await signIn(page);
  await page.emulateMedia({ colorScheme: 'dark' });
  await page.goto(itemUrl);
  const strip = page.locator('.progress-strip');
  await expect(strip.getByRole('progressbar')).toBeVisible();
  await expect(strip).toContainText('50%');
  // No horizontal overflow at 375px.
  const overflowing = await page.evaluate(
    () => document.documentElement.scrollWidth > document.documentElement.clientWidth,
  );
  expect(overflowing).toBe(false);
});
