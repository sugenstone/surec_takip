import { expect, test } from '@playwright/test';

for (const locale of ['tr-TR', 'en']) {
  for (const width of [360, 1280]) {
    test(`${locale} at ${width}px supports keyboard and localized errors`, async ({
      page,
      context,
    }) => {
      await context.addCookies([{ name: 'locale', value: locale, url: 'http://127.0.0.1:4173' }]);
      await page.setViewportSize({ width, height: 800 });
      const errors: string[] = [];
      page.on('pageerror', (error) => errors.push(error.message));
      await page.goto('/');
      await expect(page.locator('html')).toHaveAttribute('lang', locale);
      await expect(page.getByRole('heading', { level: 1 })).toHaveText(
        locale === 'en' ? 'Workflow & Operations' : 'İş ve Operasyon Yönetimi',
      );
      await page.keyboard.press('Tab');
      await expect(page.getByRole('link').first()).toBeFocused();
      await page.keyboard.press('Enter');
      await expect(page.getByRole('main')).toBeFocused();
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
        true,
      );
      const response = await page.goto('/missing-page');
      expect(response?.status()).toBe(404);
      await expect(page.getByRole('heading', { level: 1 })).toHaveText(
        locale === 'en' ? 'Page not found' : 'Sayfa bulunamadı',
      );
      await expect(
        page.getByRole('link', { name: locale === 'en' ? 'Return to home' : 'Ana sayfaya dön' }),
      ).toBeVisible();
      expect(errors).toEqual([]);
    });
  }
}

test('theme preference survives reload and system respects dark mode', async ({
  page,
  context,
}) => {
  await page.emulateMedia({ colorScheme: 'dark', reducedMotion: 'reduce' });
  await page.goto('/');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  for (const theme of ['dark', 'system']) {
    await context.addCookies([{ name: 'theme', value: theme, url: 'http://127.0.0.1:4173' }]);
    await page.reload();
    await expect(page.locator('html')).toHaveAttribute('data-theme', theme);
    await expect(page.locator('body')).toHaveCSS('background-color', 'rgb(15, 23, 42)');
  }
});

test('parallel SSR locale requests do not contaminate each other', async ({ request }) => {
  const responses = await Promise.all(
    ['en', 'tr-TR', 'en', 'tr-TR'].map((locale) =>
      request.get('/', { headers: { Cookie: `locale=${locale}` } }),
    ),
  );
  for (const [i, response] of responses.entries()) {
    expect(response.headers()['content-language']).toBe(i % 2 === 0 ? 'en' : 'tr-TR');
  }
});
