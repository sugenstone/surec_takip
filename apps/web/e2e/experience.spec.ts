import { expect, test, type Page } from '@playwright/test';
import { randomUUID } from 'node:crypto';

async function fixture(page: Page) {
  const login = await page.request.post('/api/v1/auth/login', {
    data: { email: 'e2e6@example.test', password: 'e2e-password-1' },
  });
  expect(login.status()).toBe(200);
  async function post(path: string, data: object) {
    const response = await page.request.post(`/api/v1${path}`, { data });
    expect(response.status()).toBe(201);
    return (await response.json()).data;
  }
  const { organization: org } = await post('/organizations', { name: `UX ${randomUUID()}` });
  const { workspace: ws } = await post(`/organizations/${org.id}/workspaces`, {
    name: 'Operations',
  });
  const prefix = `/organizations/${org.id}/workspaces/${ws.id}/projects`;
  const project = await post(prefix, {
    name: 'Operational project with a long name that must wrap without hiding navigation',
  });
  const api = `${prefix}/${project.id}`;
  let parent: string | null = null;
  const sections: { id: string; name: string }[] = [];
  for (let i = 0; i < 7; i++) {
    const section = await post(`${api}/sections`, {
      name: `Section ${i + 1} — a readable nested area`,
      parent_section_id: parent,
    });
    sections.push(section);
    parent = section.id;
  }
  const itemsApi = `${api}/sections/${parent}/work-items`;
  const item = await post(itemsApi, {
    name: 'Long work item name for operational review and delivery preparation',
    slug: 'delivery-preparation',
  });
  const projectUrl = `/app/${org.id}/${ws.id}/projects/${project.id}`;
  const sectionUrl = `${projectUrl}/sections/${parent}`;
  return {
    org,
    ws,
    project,
    sections,
    item,
    itemsApi,
    projectUrl,
    sectionUrl,
    itemUrl: `${sectionUrl}/work-items/${item.id}`,
  };
}

function sectionCard(page: Page, name: string) {
  return page.locator('.section-card').filter({
    has: page.getByRole('link', { name, exact: true }),
  });
}

for (const width of [360, 375, 768, 1280]) {
  for (const theme of ['light', 'dark']) {
    test(`experience: drill-down hierarchy and work at ${width}px ${theme}`, async ({
      page,
      context,
    }, testInfo) => {
      const f = await fixture(page);
      await context.addCookies([{ name: 'theme', value: theme, url: 'http://127.0.0.1:4173' }]);
      await page.setViewportSize({ width, height: 900 });
      await page.goto(`/app/${f.org.id}/${f.ws.id}/projects`);
      await expect(page.getByRole('heading', { level: 1 })).toHaveText('Projeler');
      await page.screenshot({ path: testInfo.outputPath('projects.png'), fullPage: true });
      await page.goto(f.projectUrl);
      await expect(page.getByRole('main')).toHaveCount(1);
      await expect(page.getByRole('link', { name: 'İçeriğe geç' })).toHaveCount(1);
      await expect(page.getByLabel('Çalışma alanı', { exact: true })).toHaveValue(f.ws.id);
      await expect(page.getByLabel('Tema', { exact: true })).toHaveValue(theme);
      if (width >= 768) {
        const workspace = await page.getByLabel('Çalışma alanı', { exact: true }).boundingBox();
        const account = await page.getByText('E2e UX Review', { exact: true }).boundingBox();
        if (!workspace || !account) throw new Error('Workspace and account must be visible');
        expect(
          workspace.x + workspace.width <= account.x ||
            account.x + account.width <= workspace.x ||
            workspace.y + workspace.height <= account.y ||
            account.y + account.height <= workspace.y,
        ).toBe(true);
      }
      await expect(page.getByRole('heading', { level: 1 })).toHaveText(f.project.name);
      // The project page exposes ROOT sections only — deeper levels are
      // reached by drilling in, never by expanding a tree on one page.
      await expect(sectionCard(page, f.sections[0].name)).toBeVisible();
      await expect(page.locator('.section-card')).toHaveCount(1);
      await expect(sectionCard(page, f.sections[1].name)).toHaveCount(0);
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
        true,
      );
      // Click through the first levels; each page shows only direct children.
      await sectionCard(page, f.sections[0].name)
        .getByRole('link', { name: f.sections[0].name, exact: true })
        .click();
      await expect(page).toHaveURL(`${f.projectUrl}/sections/${f.sections[0].id}`);
      await expect(page.getByRole('heading', { level: 1 })).toHaveText(f.sections[0].name);
      await expect(sectionCard(page, f.sections[1].name)).toBeVisible();
      await expect(sectionCard(page, f.sections[2].name)).toHaveCount(0);
      await sectionCard(page, f.sections[1].name)
        .getByRole('link', { name: f.sections[1].name, exact: true })
        .click();
      await expect(page.getByRole('heading', { level: 1 })).toHaveText(f.sections[1].name);
      await expect(sectionCard(page, f.sections[2].name)).toBeVisible();
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
        true,
      );
      await page.screenshot({ path: testInfo.outputPath('section.png'), fullPage: true });
      // The leaf section page carries its work item; breadcrumb keeps the
      // ancestor chain readable at every width.
      await page.goto(f.sectionUrl);
      await expect(page.getByRole('heading', { level: 1 })).toHaveText(f.sections[6].name);
      await expect(page.getByRole('navigation', { name: 'Konum' })).toContainText(
        f.sections[0].name,
      );
      await expect(page.getByRole('link', { name: f.item.name, exact: true })).toBeVisible();
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
        true,
      );
      await page.screenshot({ path: testInfo.outputPath('leaf-section.png'), fullPage: true });
      await page.getByRole('link', { name: f.item.name, exact: true }).click();
      await expect(page).toHaveURL(f.itemUrl);
      await page.reload();
      await expect(page.getByLabel('İşçilik adı')).toHaveValue(f.item.name);
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
        true,
      );
      await page.screenshot({ path: testInfo.outputPath('work-item.png'), fullPage: true });
      await page.getByLabel('Dil', { exact: true }).selectOption('en');
      await expect(page.getByLabel('Work item name')).toHaveValue(f.item.name);
      await expect(page.getByRole('navigation', { name: 'Location' })).toContainText(
        f.sections[0].name,
      );
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
        true,
      );
    });
  }
}

test('experience: failed create/edit preserves drafts and archive requires deliberate confirmation', async ({
  page,
}) => {
  const f = await fixture(page);
  await page.goto(f.sectionUrl);
  // Creation happens in the drawer: a backend failure keeps it open and the
  // typed draft intact.
  await page.getByRole('button', { name: 'İşçilik ekle', exact: true }).click();
  const drawer = page.getByRole('dialog', { name: 'İşçilik ekle' });
  await drawer.getByLabel('İşçilik adı').fill('A draft that must survive');
  await drawer.getByLabel('Kısa ad', { exact: true }).fill('delivery-preparation');
  await drawer.getByRole('button', { name: 'Oluştur' }).click();
  await expect(page.getByRole('alert')).toContainText('zaten kullanılıyor');
  await expect(drawer.getByLabel('İşçilik adı')).toHaveValue('A draft that must survive');
  await expect(drawer.getByLabel('Kısa ad', { exact: true })).toHaveValue('delivery-preparation');
  await drawer.getByRole('button', { name: 'Vazgeç' }).click();
  await expect(drawer).not.toBeVisible();
  const reserved = await page.request.post(`/api/v1${f.itemsApi}`, { data: { name: 'Reserved' } });
  expect(reserved.status()).toBe(201);
  await page.goto(f.itemUrl);
  await page.getByLabel('İşçilik adı').fill('Unsaved edit');
  await page.getByLabel('Kısa ad', { exact: true }).fill('reserved');
  await page.getByRole('button', { name: 'Kaydet', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('zaten kullanılıyor');
  await expect(page.getByLabel('İşçilik adı')).toHaveValue('Unsaved edit');
  const archive = page.getByRole('button', { name: 'Arşivle', exact: true });
  await archive.click();
  await expect(page.getByRole('dialog')).toBeVisible();
  await expect(
    page.getByRole('dialog').getByRole('button', { name: 'Vazgeç', exact: true }),
  ).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('dialog')).not.toBeVisible();
  await expect(archive).toBeFocused();
  const existing = await page.request.get(`/api/v1${f.itemsApi}/${f.item.id}`);
  expect(existing.status()).toBe(200);
  expect((await existing.json()).name).toBe(f.item.name);
  await archive.click();
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Arşivlemeyi onayla', exact: true })
    .click();
  await expect(page).toHaveURL(f.sectionUrl);
  expect((await page.request.get(`/api/v1${f.itemsApi}/${f.item.id}`)).status()).toBe(404);
});

test('experience: application shell — desktop sidebar, mobile drawer, active nav', async ({
  page,
}) => {
  const f = await fixture(page);
  // Desktop: the sidebar is persistent and marks the active module.
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.goto(`/app/${f.org.id}/${f.ws.id}/projects`);
  const nav = page.getByRole('navigation', { name: 'Ana gezinme' });
  await expect(nav).toBeVisible();
  await expect(nav.getByRole('link', { name: 'Projeler' })).toHaveAttribute('aria-current', 'page');
  await expect(page.getByRole('button', { name: 'Menüyü aç/kapat' })).not.toBeVisible();
  // Mobile: the sidebar becomes an off-canvas drawer behind the topbar toggle.
  await page.setViewportSize({ width: 375, height: 800 });
  await expect(nav).not.toBeVisible();
  const toggle = page.getByRole('button', { name: 'Menüyü aç/kapat' });
  await expect(toggle).toBeVisible();
  await toggle.click();
  await expect(nav).toBeVisible();
  await expect(toggle).toHaveAttribute('aria-expanded', 'true');
  await page.keyboard.press('Escape');
  await expect(nav).not.toBeVisible();
  // Drawer navigation still drills through the URL — opening it again and
  // picking the workspace entry lands on the org page and closes the drawer.
  await toggle.click();
  await nav.getByRole('link', { name: 'Çalışma alanları' }).click();
  await expect(page).toHaveURL(`/app/${f.org.id}`, { timeout: 10_000 });
  await expect(nav).not.toBeVisible();
});

test('experience: create drawer opens from the primary action and cancels cleanly', async ({
  page,
}) => {
  const f = await fixture(page);
  await page.goto(f.projectUrl);
  // No create form consumes page space before the user asks for it.
  await expect(page.getByLabel('Bölüm adı')).toHaveCount(0);
  await page.getByRole('button', { name: 'Bölüm ekle', exact: true }).click();
  const drawer = page.getByRole('dialog', { name: 'Bölüm ekle' });
  await expect(drawer).toBeVisible();
  await expect(drawer.getByLabel('Bölüm adı')).toBeFocused();
  await drawer.getByLabel('Bölüm adı').fill('Draft section');
  await drawer.getByRole('button', { name: 'Vazgeç' }).click();
  await expect(drawer).not.toBeVisible();
  await expect(page.getByRole('button', { name: 'Bölüm ekle', exact: true })).toBeFocused();
});

test.describe('server-rendered interaction safety', () => {
  test.use({ javaScriptEnabled: false });
  test('content remains readable while client-only controls await hydration', async ({ page }) => {
    const f = await fixture(page);
    await page.goto(f.projectUrl);
    await expect(page.getByRole('heading', { level: 1 })).toHaveText(f.project.name);
    await expect(page.getByLabel('Dil', { exact: true })).toBeDisabled();
    await expect(page.getByLabel('Tema', { exact: true })).toBeDisabled();
    await expect(page.getByLabel('Çalışma alanı', { exact: true })).toBeDisabled();
    await expect(page.getByRole('button', { name: 'Çıkış yap', exact: true })).toBeDisabled();
    await expect(page.getByRole('button', { name: 'İşlemler', exact: true })).toBeDisabled();
    // Section cards are plain links: drill-down works without JavaScript.
    await page.goto(`${f.projectUrl}/sections/${f.sections[0].id}`);
    await expect(page.getByRole('heading', { level: 1 })).toHaveText(f.sections[0].name);
    await expect(page.getByRole('link', { name: f.sections[1].name, exact: true })).toBeVisible();
    await page.goto(f.itemUrl);
    await expect(page.getByLabel('İşçilik adı')).toHaveValue(f.item.name);
    await expect(page.getByLabel('İşçilik adı')).toBeDisabled();
    await expect(page.getByLabel('Kısa ad', { exact: true })).toBeDisabled();
    await expect(page.getByRole('button', { name: 'Kaydet', exact: true })).toBeDisabled();
    await expect(page.getByRole('button', { name: 'Arşivle', exact: true })).toBeDisabled();
  });
});
