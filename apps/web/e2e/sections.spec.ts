import { expect, test, type Page } from '@playwright/test';

const OWNER_EMAIL = 'e2e4@example.test';
const PASSWORD = 'e2e-password-1';

test.describe.configure({ mode: 'serial' });

// Section cards are navigation objects: the card's title link enters that
// section's own page.
function sectionCard(page: Page, name: string) {
  return page
    .locator('.section-card')
    .filter({ has: page.getByRole('link', { name, exact: true }) });
}

async function signIn(page, email: string) {
  await page.goto('/login');
  await page.waitForLoadState('networkidle');
  await page.getByLabel('E-posta').fill(email);
  await page.getByLabel('Şifre', { exact: true }).fill(PASSWORD);
  await page.getByRole('button', { name: 'Giriş yap' }).click();
}

// Own org + workspace + project, ending on the project detail page.
async function enterProjectDetail(page) {
  await signIn(page, OWNER_EMAIL);
  await expect(page).toHaveURL(/\/app$/, { timeout: 10_000 });
  await page.getByLabel('Organizasyon adı').fill('Sections Org');
  await page.getByRole('button', { name: 'Organizasyon oluştur' }).click();
  await expect(page).toHaveURL(/\/app\/[a-f0-9-]+$/, { timeout: 10_000 });
  await page.getByRole('button', { name: 'Çalışma alanı oluştur' }).click();
  await page.getByRole('dialog').getByLabel('Çalışma alanı adı').fill('Sections Ws');
  await page.getByRole('dialog').getByRole('button', { name: 'Oluştur' }).click();
  await expect(page).toHaveURL(/\/app\/[a-f0-9-]+\/[a-f0-9-]+$/, { timeout: 10_000 });
  await page
    .getByRole('main')
    .getByRole('link', { name: /Projeler/ })
    .click();
  await expect(page).toHaveURL(/\/projects$/, { timeout: 10_000 });
  await page.getByRole('button', { name: 'Proje oluştur' }).click();
  await page.getByRole('dialog').getByLabel('Proje adı').fill('Tower');
  await page.getByRole('dialog').getByRole('button', { name: 'Oluştur' }).click();
  await expect(page).toHaveURL(/\/projects\/[a-f0-9-]+$/, { timeout: 10_000 });
}

// A fresh sign-in lands on the ORG page; walk into the serially-created
// workspace and project before asserting section routes.
async function openTower(page) {
  await signIn(page, OWNER_EMAIL);
  await expect(page).toHaveURL(/\/app\/[a-f0-9-]+$/, { timeout: 10_000 });
  await page.getByRole('link', { name: 'Sections Ws' }).click();
  await expect(page).toHaveURL(/\/app\/[a-f0-9-]+\/[a-f0-9-]+$/, { timeout: 10_000 });
  await page
    .getByRole('main')
    .getByRole('link', { name: /Projeler/ })
    .click();
  await expect(page).toHaveURL(/\/projects$/, { timeout: 10_000 });
  await page.getByRole('link', { name: /Tower/ }).click();
  await expect(page).toHaveURL(/\/projects\/[a-f0-9-]+$/, { timeout: 10_000 });
}

test('sections: drill-down navigation, mixed content, reload and history', async ({ page }) => {
  await enterProjectDetail(page);

  await expect(page.getByRole('heading', { name: 'Bu projede henüz bölüm yok' })).toBeVisible();

  // Root section: created from the project context via the create drawer.
  await page.getByRole('button', { name: 'Bölüm ekle', exact: true }).click();
  await page.getByRole('dialog').getByLabel('Bölüm adı').fill('A Blok');
  await page.getByRole('dialog').getByRole('button', { name: 'Oluştur' }).click();
  await expect(page.getByRole('heading', { name: 'Bölümler' })).toBeVisible();
  await expect(sectionCard(page, 'A Blok')).toBeVisible();

  // Clicking the card navigates into the section's own URL — one level only.
  await sectionCard(page, 'A Blok').getByRole('link', { name: 'A Blok' }).click();
  await expect(page).toHaveURL(/\/sections\/[a-f0-9-]+$/, { timeout: 10_000 });
  await expect(page.getByRole('heading', { level: 1, name: 'A Blok' })).toBeVisible();
  const blockUrl = page.url();
  await expect(page.getByRole('navigation', { name: 'Konum' })).toContainText('Tower');
  await expect(page.getByRole('navigation', { name: 'Konum' })).toContainText('A Blok');
  await expect(page.getByRole('heading', { name: 'Bu bölüm henüz boş' })).toBeVisible();

  // A child section created here lands under the CURRENT context (A Blok).
  await page.getByRole('button', { name: 'Alt bölüm ekle', exact: true }).click();
  await page.getByRole('dialog').getByLabel('Bölüm adı').fill('Kat 1');
  await page.getByRole('dialog').getByRole('button', { name: 'Oluştur' }).click();
  await expect(sectionCard(page, 'Kat 1')).toBeVisible();

  // Drill into Kat 1 and create a grandchild plus a work item: mixed content.
  await sectionCard(page, 'Kat 1').getByRole('link', { name: 'Kat 1' }).click();
  await expect(page.getByRole('heading', { level: 1, name: 'Kat 1' })).toBeVisible();
  const floorUrl = page.url();
  await page.getByRole('button', { name: 'Alt bölüm ekle', exact: true }).click();
  await page.getByRole('dialog').getByLabel('Bölüm adı').fill('Daire 1');
  await page.getByRole('dialog').getByRole('button', { name: 'Oluştur' }).click();
  await page.getByRole('button', { name: 'İşçilik ekle', exact: true }).click();
  await page.getByRole('dialog').getByLabel('İşçilik adı').fill('Sıva');
  await page.getByRole('dialog').getByRole('button', { name: 'Oluştur' }).click();

  // Kat 1 shows BOTH its direct child and its work item on one page.
  await expect(sectionCard(page, 'Daire 1')).toBeVisible();
  await expect(page.getByRole('link', { name: 'Sıva', exact: true })).toBeVisible();
  const trail = page.getByRole('navigation', { name: 'Konum' });
  await expect(trail).toContainText('Tower');
  await expect(trail).toContainText('A Blok');
  await expect(trail).toContainText('Kat 1');

  // Daire 1 never renders on the parent level as a descendant — only as a
  // direct-child card; deeper levels require navigation.
  await page.goto(blockUrl);
  await expect(page.getByRole('heading', { level: 1, name: 'A Blok' })).toBeVisible();
  await expect(sectionCard(page, 'Kat 1')).toBeVisible();
  await expect(sectionCard(page, 'Daire 1')).toHaveCount(0);
  await expect(page.getByRole('link', { name: 'Sıva', exact: true })).toHaveCount(0);

  // Leaf section page shows its work item; the detail URL is unchanged.
  await page.goto(floorUrl);
  await sectionCard(page, 'Daire 1').getByRole('link', { name: 'Daire 1' }).click();
  await expect(page.getByRole('heading', { level: 1, name: 'Daire 1' })).toBeVisible();
  const apartmentUrl = page.url();
  await page.getByRole('button', { name: 'İşçilik ekle', exact: true }).click();
  await page.getByRole('dialog').getByLabel('İşçilik adı').fill('Tezgah');
  await page.getByRole('dialog').getByRole('button', { name: 'Oluştur' }).click();
  await expect(page.getByRole('link', { name: 'Tezgah', exact: true })).toBeVisible();

  // Reload keeps the exact deep context (URL authority, no local state).
  await page.reload();
  await expect(page).toHaveURL(apartmentUrl);
  await expect(page.getByRole('heading', { level: 1, name: 'Daire 1' })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Tezgah', exact: true })).toBeVisible();
  // History traversal must wait until hydration attaches the router's
  // popstate listener — hydration-gated controls signal readiness.
  await expect(page.getByRole('button', { name: 'İşlemler', exact: true })).toBeEnabled();

  // Browser Back walks back up one level at a time; Forward returns.
  await page.goBack();
  await expect(page).toHaveURL(floorUrl, { timeout: 10_000 });
  await expect(page.getByRole('heading', { level: 1, name: 'Kat 1' })).toBeVisible();
  await page.goBack();
  await expect(page).toHaveURL(blockUrl, { timeout: 10_000 });
  await expect(page.getByRole('heading', { level: 1, name: 'A Blok' })).toBeVisible();
  await page.goForward();
  await expect(page).toHaveURL(floorUrl, { timeout: 10_000 });
  await expect(page.getByRole('heading', { level: 1, name: 'Kat 1' })).toBeVisible();
});

test('sections: rename, reparent and archive persist after reload', async ({ page }) => {
  await openTower(page);

  // The project page lists ROOT sections only.
  await expect(sectionCard(page, 'A Blok')).toBeVisible();
  await expect(sectionCard(page, 'Kat 1')).toHaveCount(0);
  await expect(sectionCard(page, 'Daire 1')).toHaveCount(0);

  // Rename Kat 1 on its own page. Card clicks navigate client-side, so the
  // target heading must be confirmed before page actions attach to context.
  await sectionCard(page, 'A Blok').getByRole('link', { name: 'A Blok' }).click();
  await expect(page.getByRole('heading', { level: 1, name: 'A Blok' })).toBeVisible();
  await sectionCard(page, 'Kat 1').getByRole('link', { name: 'Kat 1' }).click();
  await expect(page.getByRole('heading', { level: 1, name: 'Kat 1' })).toBeVisible();
  await page.getByRole('button', { name: 'İşlemler', exact: true }).click();
  await page.getByRole('menuitem', { name: 'Bölümü düzenle' }).click();
  await page.getByRole('dialog').locator('#section-edit-name').fill('Kat 1A');
  await page.getByRole('dialog').getByRole('button', { name: 'Kaydet' }).click();
  await expect(page.getByRole('heading', { level: 1, name: 'Kat 1A' })).toBeVisible();

  // Reparent Daire 1 to the project root from its own page.
  await sectionCard(page, 'Daire 1').getByRole('link', { name: 'Daire 1' }).click();
  await expect(page.getByRole('heading', { level: 1, name: 'Daire 1' })).toBeVisible();
  await page.getByRole('button', { name: 'İşlemler', exact: true }).click();
  await page.getByRole('menuitem', { name: 'Bölümü düzenle' }).click();
  await page.getByRole('dialog').locator('#section-edit-parent').selectOption({ label: '— Kök —' });
  await page.getByRole('dialog').getByRole('button', { name: 'Kaydet' }).click();
  await expect(page.getByRole('heading', { level: 1, name: 'Daire 1' })).toBeVisible();
  // The breadcrumb reflects the new ancestry: Daire 1 is now a root.
  const trail = page.getByRole('navigation', { name: 'Konum' });
  await expect(trail).toContainText('Daire 1');
  await expect(trail.getByRole('link', { name: 'Kat 1A' })).toHaveCount(0);

  // Archive A Blok from its own page (owner holds sections:archive). The
  // breadcrumb ancestor chain is the way back up the hierarchy.
  await trail.getByRole('link', { name: 'Tower' }).click();
  await expect(page).toHaveURL(/\/projects\/[a-f0-9-]+$/, { timeout: 10_000 });
  await sectionCard(page, 'A Blok').getByRole('link', { name: 'A Blok' }).click();
  await expect(page.getByRole('heading', { level: 1, name: 'A Blok' })).toBeVisible();
  await page.getByRole('button', { name: 'İşlemler', exact: true }).click();
  await page.getByRole('menuitem', { name: 'Arşivle' }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Arşivlemeyi onayla' }).click();
  await expect(page.getByRole('dialog')).not.toBeVisible();
  await expect(page.getByText('Arşivlendi').first()).toBeVisible();

  // Archived sections stay reachable: the page renders read-only so the
  // record can be reactivated; work items stay hidden as before.
  await page.getByRole('button', { name: 'İşlemler', exact: true }).click();
  await expect(page.getByRole('menuitem', { name: 'Yeniden etkinleştir' })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('heading', { name: 'İşçilik' })).toHaveCount(0);
  await expect(sectionCard(page, 'Kat 1A')).toBeVisible();

  await page.reload();
  await expect(page.getByRole('heading', { level: 1, name: 'A Blok' })).toBeVisible();
  await expect(page.getByText('Arşivlendi').first()).toBeVisible();

  // Reactivate keeps the record recoverable.
  await page.getByRole('button', { name: 'İşlemler', exact: true }).click();
  await page.getByRole('menuitem', { name: 'Yeniden etkinleştir' }).click();
  await page.getByRole('button', { name: 'İşlemler', exact: true }).click();
  await expect(page.getByRole('menuitem', { name: 'Arşivle' })).toBeVisible();

  // Root ordering on the project page is deterministic: A Blok first
  // (position 0), then Daire 1 (reparented, appended). Navigate back up via
  // the breadcrumb rather than re-entering the fixture.
  await page
    .getByRole('navigation', { name: 'Konum' })
    .getByRole('link', { name: 'Tower' })
    .click();
  await expect(page).toHaveURL(/\/projects\/[a-f0-9-]+$/, { timeout: 10_000 });
  await expect(sectionCard(page, 'A Blok')).toBeVisible();
  await expect(sectionCard(page, 'Daire 1')).toBeVisible();
  const names = await page.locator('.section-card a.card-link').allTextContents();
  expect(names).toEqual(['A Blok', 'Daire 1']);
});

test('sections: invalid section id safely shows not-found without leaking', async ({ page }) => {
  await openTower(page);
  const parts = page.url().split('/app/')[1]?.split('/') ?? [];
  const [orgId, wsId, , projectId] = parts;
  expect(orgId && wsId && projectId).toBeTruthy();
  // A section API deep link with an unknown id returns the uniform 404 and
  // never leaks existence.
  const response = await page.request.get(
    `/api/v1/organizations/${orgId}/workspaces/${wsId}/projects/${projectId}/sections/00000000-0000-4000-8000-000000000000`,
  );
  expect(response.status()).toBe(404);
  expect((await response.json()).error.code).toBe('RESOURCE_NOT_FOUND');
  // The unknown section URL renders the not-found page, not a broken tree.
  await page.goto(
    `/app/${orgId}/${wsId}/projects/${projectId}/sections/00000000-0000-4000-8000-000000000000`,
  );
  await expect(page.getByRole('heading', { name: 'Sayfa bulunamadı' })).toBeVisible();
  // The project page itself still renders its root sections.
  await page.goto(`/app/${orgId}/${wsId}/projects/${projectId}`);
  await expect(sectionCard(page, 'A Blok')).toBeVisible();
});
