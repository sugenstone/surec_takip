import { expect, test } from '@playwright/test';

const OWNER_EMAIL = 'e2e2@example.test';
const MEMBER_EMAIL = 'e2e3@example.test';
const PASSWORD = 'e2e-password-1';

test.describe.configure({ mode: 'serial' });

async function signIn(page, email: string) {
  await page.goto('/login');
  await page.waitForLoadState('networkidle');
  await page.getByLabel('E-posta').fill(email);
  await page.getByLabel('Şifre', { exact: true }).fill(PASSWORD);
  await page.getByRole('button', { name: 'Giriş yap' }).click();
}

test('projects: owner creates a project from the workspace modules and manages its lifecycle', async ({
  page,
}) => {
  await signIn(page, OWNER_EMAIL);
  await expect(page).toHaveURL(/\/app$/, { timeout: 10_000 });

  // Fresh organization + workspace owned by this spec's user.
  await page.getByLabel('Organizasyon adı').fill('Projects Org');
  await page.getByRole('button', { name: 'Organizasyon oluştur' }).click();
  await expect(page).toHaveURL(/\/app\/[a-f0-9-]+$/, { timeout: 10_000 });
  await page.getByRole('button', { name: 'Çalışma alanı oluştur' }).click();
  await page.getByRole('dialog').getByLabel('Çalışma alanı adı').fill('Projects Ws');
  await page.getByRole('dialog').getByRole('button', { name: 'Oluştur' }).click();
  await expect(page).toHaveURL(/\/app\/[a-f0-9-]+\/[a-f0-9-]+$/, { timeout: 10_000 });

  // Workspace home exposes the Projects module entry.
  await page
    .getByRole('main')
    .getByRole('link', { name: /Projeler/ })
    .click();
  await expect(page).toHaveURL(/\/projects$/, { timeout: 10_000 });
  await expect(
    page.getByRole('heading', { name: 'Bu çalışma alanında henüz proje yok' }),
  ).toBeVisible();

  // Owner (projects:create) opens the create drawer and uses it.
  await page.getByRole('button', { name: 'Proje oluştur' }).click();
  await page.getByRole('dialog').getByLabel('Proje adı').fill('İlk Proje');
  await page.getByRole('dialog').getByLabel('Açıklama (isteğe bağlı)').fill('Uçtan uca deneme');
  await page.getByRole('dialog').getByRole('button', { name: 'Oluştur' }).click();
  await expect(page).toHaveURL(/\/projects\/[a-f0-9-]+$/, { timeout: 10_000 });
  await expect(page.getByRole('heading', { name: 'İlk Proje' })).toBeVisible();
  await expect(page.getByText('Aktif')).toBeVisible();
  // Step 18 replaced the sections placeholder with the real panel; a fresh
  // project shows its empty state.
  await expect(page.getByRole('heading', { name: 'Bu projede henüz bölüm yok' })).toBeVisible();

  // A full reload re-resolves the SAME valid deep link through SSR: URL
  // context survives and the detail re-renders (URL authority, ADR 0011).
  await page.reload();
  await expect(page).toHaveURL(/\/projects\/[a-f0-9-]+$/, { timeout: 10_000 });
  await expect(page.getByRole('heading', { name: 'İlk Proje' })).toBeVisible();

  // Owner (projects:update) edits content through the action menu + drawer.
  await page.getByRole('button', { name: 'İşlemler', exact: true }).click();
  await page.getByRole('menuitem', { name: 'Projeyi düzenle' }).click();
  await page.getByRole('dialog').getByLabel('Proje adı').fill('İlk Proje v2');
  await page.getByRole('dialog').getByRole('button', { name: 'Kaydet' }).click();
  await expect(page.getByRole('heading', { name: 'İlk Proje v2' })).toBeVisible();

  // Lifecycle: complete, archive (projects:archive), reactivate — secondary
  // actions live in the overflow menu; archive stays a destructive confirm.
  await page.getByRole('button', { name: 'İşlemler', exact: true }).click();
  await page.getByRole('menuitem', { name: 'Tamamlandı olarak işaretle' }).click();
  await expect(page.getByText('Tamamlandı').first()).toBeVisible();
  await page.getByRole('button', { name: 'İşlemler', exact: true }).click();
  await page.getByRole('menuitem', { name: 'Arşivle' }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Arşivlemeyi onayla' }).click();
  await expect(page.getByText('Arşivlendi').first()).toBeVisible();
  await page.getByRole('button', { name: 'İşlemler', exact: true }).click();
  await page.getByRole('menuitem', { name: 'Yeniden etkinleştir' }).click();
  await expect(page.getByText('Aktif').first()).toBeVisible();

  // Back on the list, the project is reachable with its status.
  await page
    .getByRole('navigation', { name: 'Konum' })
    .getByRole('link', { name: 'Projeler', exact: true })
    .click();
  await expect(page).toHaveURL(/\/projects$/, { timeout: 10_000 });
  await expect(page.getByRole('link', { name: /İlk Proje v2/ })).toBeVisible();
});

// A fresh sign-in lands on the organization page (workspace selection);
// enter the serially-created workspace before asserting project routes.
async function enterFirstWorkspace(page) {
  await signIn(page, OWNER_EMAIL);
  await expect(page).toHaveURL(/\/app\/[a-f0-9-]+$/, { timeout: 10_000 });
  await page.getByRole('link', { name: 'Projects Ws' }).click();
  await expect(page).toHaveURL(/\/app\/[a-f0-9-]+\/[a-f0-9-]+$/, { timeout: 10_000 });
}

test('projects: unknown project id recovers to the workspace projects list', async ({ page }) => {
  await enterFirstWorkspace(page);
  const url = page.url();
  const staleUrl = `${url}/projects/00000000-0000-4000-8000-000000000000`;
  await page.goto(staleUrl);
  await expect(page).toHaveURL(/\/projects$/, { timeout: 10_000 });
  await expect(page.getByRole('heading', { name: 'Projeler' })).toBeVisible();
});

// Permission-aware negative: an invited Member (no workspace membership, no
// project permissions) must never reach the projects surface — neither via
// the API (uniform 404) nor via deep links (safe redirect, no leak).
test('projects: invited member without grants cannot reach the projects surface', async ({
  page,
  request,
}) => {
  await enterFirstWorkspace(page);
  const orgId = page.url().split('/app/')[1]?.split('/')[0] ?? '';
  const wsId = page.url().split('/app/')[1]?.split('/')[1] ?? '';
  expect(orgId).not.toBe('');
  expect(wsId).not.toBe('');

  // API-level, in an isolated request context: the owner authenticates and
  // invites the member (raw token is returned exactly once).
  const ownerLogin = await request.post('/api/v1/auth/login', {
    data: { email: OWNER_EMAIL, password: PASSWORD },
  });
  expect(ownerLogin.ok()).toBeTruthy();
  const invite = await request.post(`/api/v1/organizations/${orgId}/invitations`, {
    data: { email: MEMBER_EMAIL },
  });
  expect(invite.ok()).toBeTruthy();
  const token = ((await invite.json()) as { data?: { token?: string } }).data?.token ?? '';
  expect(token).not.toBe('');

  // The member takes over the context session and accepts.
  const memberLogin = await request.post('/api/v1/auth/login', {
    data: { email: MEMBER_EMAIL, password: PASSWORD },
  });
  expect(memberLogin.ok()).toBeTruthy();
  const accept = await request.post('/api/v1/invitations/accept', { data: { token } });
  expect(accept.ok()).toBeTruthy();

  // No workspace membership: the projects list is a plain 404 (never 403 —
  // existence is not leaked).
  const list = await request.get(`/api/v1/organizations/${orgId}/workspaces/${wsId}/projects`);
  expect(list.status()).toBe(404);
  expect(((await list.json()) as { error?: { code?: string } }).error?.code).toBe(
    'RESOURCE_NOT_FOUND',
  );

  // UI-level: the member's deep link recovers to the organization page and
  // the projects UI never renders. Wait for the sign-in navigation to settle
  // first, or goto would cancel the login request itself.
  await page.getByRole('button', { name: 'Çıkış yap' }).click();
  await expect(page).toHaveURL(/\/login$/);
  await signIn(page, MEMBER_EMAIL);
  await expect(page).toHaveURL(/\/app\/[a-f0-9-]+/, { timeout: 10_000 });
  await page.goto(`/app/${orgId}/${wsId}/projects`);
  await expect(page).toHaveURL(new RegExp(`/app/${orgId}$`), { timeout: 10_000 });
  await expect(page.getByRole('heading', { name: 'Projeler' })).toHaveCount(0);
});
