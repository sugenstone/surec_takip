import { expect, test } from '@playwright/test';

const OWNER_EMAIL = 'e2e4@example.test';
const PASSWORD = 'e2e-password-1';

test.describe.configure({ mode: 'serial' });

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
  await page.getByLabel('Çalışma alanı adı').fill('Sections Ws');
  await page.getByRole('button', { name: 'Çalışma alanı oluştur' }).click();
  await expect(page).toHaveURL(/\/app\/[a-f0-9-]+\/[a-f0-9-]+$/, { timeout: 10_000 });
  await page.getByRole('link', { name: /Projeler/ }).click();
  await expect(page).toHaveURL(/\/projects$/, { timeout: 10_000 });
  await page.getByLabel('Proje adı').fill('Tower');
  await page.getByRole('button', { name: 'Proje oluştur' }).click();
  await expect(page).toHaveURL(/\/projects\/[a-f0-9-]+$/, { timeout: 10_000 });
}

// A fresh sign-in lands on the ORG page; walk into the serially-created
// workspace and project before asserting section routes.
async function openTower(page) {
  await signIn(page, OWNER_EMAIL);
  await expect(page).toHaveURL(/\/app\/[a-f0-9-]+$/, { timeout: 10_000 });
  await page.getByRole('link', { name: 'Sections Ws' }).click();
  await expect(page).toHaveURL(/\/app\/[a-f0-9-]+\/[a-f0-9-]+$/, { timeout: 10_000 });
  await page.getByRole('link', { name: /Projeler/ }).click();
  await expect(page).toHaveURL(/\/projects$/, { timeout: 10_000 });
  await page.getByRole('link', { name: /Tower/ }).click();
  await expect(page).toHaveURL(/\/projects\/[a-f0-9-]+$/, { timeout: 10_000 });
}

test('sections: owner builds a hierarchy that renders and survives reload', async ({ page }) => {
  await enterProjectDetail(page);

  await expect(page.getByRole('heading', { name: 'Bu projede henüz bölüm yok' })).toBeVisible();

  // Root section.
  await page.getByLabel('Bölüm adı').fill('A Blok');
  await page.getByRole('button', { name: 'Bölüm ekle' }).click();
  await expect(page.getByRole('heading', { name: 'Bölümler' })).toBeVisible();
  await expect(page.locator('.section-name', { hasText: 'A Blok' })).toBeVisible();

  // Child of A Blok.
  await page
    .locator('.section-row', { hasText: 'A Blok' })
    .getByRole('button', { name: 'Alt bölüm ekle' })
    .click();
  await page
    .locator('.section-row', { hasText: 'A Blok' })
    .locator('.child-form input')
    .fill('Kat 1');
  await page.locator('.section-row', { hasText: 'A Blok' }).locator('.child-form button').click();
  await expect(page.locator('.section-name', { hasText: 'Kat 1' })).toBeVisible();

  // Grandchild of A Blok (depth 2 under Kat 1).
  await page
    .locator('.section-row', { hasText: 'Kat 1' })
    .getByRole('button', { name: 'Alt bölüm ekle' })
    .click();
  await page
    .locator('.section-row', { hasText: 'Kat 1' })
    .locator('.child-form input')
    .fill('Daire 1');
  await page.locator('.section-row', { hasText: 'Kat 1' }).locator('.child-form button').click();
  await expect(page.locator('.section-name', { hasText: 'Daire 1' })).toBeVisible();

  // Hierarchy indentation: each deeper row carries a larger inline padding.
  const depths = await page
    .locator('.section-row')
    .evaluateAll((rows) =>
      rows.map((row) => Number((row as HTMLElement).style.paddingInlineStart.replace(/\D/g, ''))),
    );
  expect(depths.length).toBe(3);
  expect(depths[0]).toBeLessThan(depths[1]);
  expect(depths[1]).toBeLessThan(depths[2]);

  // A full reload re-resolves the same deep link through SSR (URL authority).
  await page.reload();
  await expect(page).toHaveURL(/\/projects\/[a-f0-9-]+$/, { timeout: 10_000 });
  for (const name of ['A Blok', 'Kat 1', 'Daire 1']) {
    await expect(page.locator('.section-name', { hasText: name })).toBeVisible();
  }
});

test('sections: rename and reparent persist after reload', async ({ page }) => {
  await openTower(page);

  // Rename Kat 1. The inline rename form REPLACES the row's name text, so
  // scope by the form class instead of the (now hidden) row text.
  await page
    .locator('.section-row', { hasText: 'Kat 1' })
    .getByRole('button', { name: 'Yeniden adlandır' })
    .click();
  const renameForm = page.locator('.section-list .inline-form:not(.child-form):not(.move-form)');
  await renameForm.locator('input').fill('Kat 1A');
  await renameForm.locator('button[type="submit"]').click();
  await expect(page.locator('.section-name', { hasText: 'Kat 1A' })).toBeVisible();

  // Move Daire 1 to root via the reparent select.
  await page
    .locator('.section-row', { hasText: 'Daire 1' })
    .getByRole('button', { name: 'Taşı', exact: true })
    .click();
  await page
    .locator('.section-row', { hasText: 'Daire 1' })
    .locator('.move-form select')
    .selectOption({ label: '— Kök —' });
  await page.locator('.section-row', { hasText: 'Daire 1' }).locator('.move-form button').click();

  // Archive A Blok (owner holds sections:archive).
  await page
    .locator('.section-row', { hasText: 'A Blok' })
    .getByRole('button', { name: 'Arşivle' })
    .click();

  await page.reload();
  await expect(page).toHaveURL(/\/projects\/[a-f0-9-]+$/, { timeout: 10_000 });
  // Renamed node persisted.
  await expect(page.locator('.section-name', { hasText: 'Kat 1A' })).toBeVisible();
  // Daire 1 became a ROOT (appended last in depth-first order), while
  // Kat 1A stayed a child of the archived A Blok: roots never show the
  // "move to root" button, non-roots always do.
  const rows = page.locator('.section-row');
  await expect(rows.filter({ hasText: 'Daire 1' })).toBeVisible();
  await expect(
    rows.filter({ hasText: 'Daire 1' }).getByRole('button', { name: 'Köke taşı' }),
  ).toHaveCount(0);
  await expect(
    rows.filter({ hasText: 'Kat 1A' }).getByRole('button', { name: 'Köke taşı' }),
  ).toHaveCount(1);
  const names = await rows.locator('.section-name').allTextContents();
  expect(names).toEqual(['A Blok', 'Kat 1A', 'Daire 1']);
  // Archived state persisted.
  await expect(
    page.locator('.section-row', { hasText: 'A Blok' }).locator('.section-status'),
  ).toHaveText('Arşivlendi');
});

test('sections: invalid section id safely recovers to the project page', async ({ page }) => {
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
  // The project page itself still renders the tree.
  await expect(page.locator('.section-name', { hasText: 'Kat 1A' })).toBeVisible();
});
