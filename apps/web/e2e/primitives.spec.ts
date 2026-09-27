import { expect, test, type Page } from '@playwright/test';
import { randomUUID } from 'node:crypto';

// Phase 21D.5C: shadcn-svelte primitive layer adoption. The existing specs
// cover click-driven dialog/menu flows; this file covers the primitive
// behaviors the migration relies on: keyboard operation, focus
// restoration and portal theming under data-theme.

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
  const { organization: org } = await post('/organizations', { name: `PR ${randomUUID()}` });
  const { workspace: ws } = await post(`/organizations/${org.id}/workspaces`, {
    name: 'Primitives',
  });
  const prefix = `/organizations/${org.id}/workspaces/${ws.id}/projects`;
  const project = await post(prefix, { name: 'Primitive project' });
  return { org, ws, projectUrl: `/app/${org.id}/${ws.id}/projects/${project.id}` };
}

test('primitives: action menu keyboard navigation and focus restore', async ({ page }) => {
  const f = await fixture(page);
  await page.goto(f.projectUrl);
  const trigger = page.getByRole('button', { name: 'İşlemler', exact: true });
  // Controls render disabled until hydration; focusing a still-disabled
  // trigger is silently ignored, so the keypress must wait for it.
  await expect(trigger).toBeEnabled();
  await trigger.focus();
  await page.keyboard.press('ArrowDown');
  const menu = page.getByRole('menu');
  await expect(menu).toBeVisible();
  // Bits UI roving focus: the first enabled item owns keyboard focus.
  const firstItem = menu.getByRole('menuitem').first();
  await expect(firstItem).toBeFocused();
  await page.keyboard.press('ArrowDown');
  await expect(menu.getByRole('menuitem').nth(1)).toBeFocused();
  await page.keyboard.press('ArrowUp');
  await expect(firstItem).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(menu).not.toBeVisible();
  await expect(trigger).toBeFocused();
});

test('primitives: portal content inherits dark theme tokens', async ({ page, context }) => {
  const f = await fixture(page);
  await context.addCookies([{ name: 'theme', value: 'dark', url: 'http://127.0.0.1:4173' }]);
  await page.goto(f.projectUrl);
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await page.getByRole('button', { name: 'İşlemler', exact: true }).click();
  const menu = page.getByRole('menu');
  await expect(menu).toBeVisible();
  // The menu renders in a portal under <body>; data-theme lives on <html>,
  // so it must still resolve the dark surface token (#1d2a44).
  await expect(menu).toHaveCSS('background-color', 'rgb(29, 42, 68)');
});

test('primitives: form drawer autofocuses the first field and Escape closes', async ({ page }) => {
  const f = await fixture(page);
  await page.goto(f.projectUrl);
  const openButton = page.getByRole('button', { name: 'Bölüm ekle', exact: true });
  await openButton.click();
  const drawer = page.getByRole('dialog', { name: 'Bölüm ekle' });
  await expect(drawer).toBeVisible();
  await expect(drawer.getByLabel('Bölüm adı')).toBeFocused();
  // Outside interaction must not dismiss the drawer (draft protection).
  await page.mouse.click(12, 12);
  await expect(drawer).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(drawer).not.toBeVisible();
  await expect(openButton).toBeFocused();
});
