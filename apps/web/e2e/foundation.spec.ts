import { expect, test } from '@playwright/test';

// The root page requires authentication; unauthenticated visitors see the
// localized login screen, which now carries the foundation checks.
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
      await expect(page).toHaveURL(/\/login$/);
      await expect(page.locator('html')).toHaveAttribute('lang', locale);
      await expect(page.getByRole('heading', { level: 1 })).toHaveText(
        locale === 'en' ? 'Sign in' : 'Giriş yap',
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
  await page.goto('/login');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  for (const theme of ['dark', 'system']) {
    await context.addCookies([{ name: 'theme', value: theme, url: 'http://127.0.0.1:4173' }]);
    await page.reload();
    await expect(page.locator('html')).toHaveAttribute('data-theme', theme);
    // The dark theme must produce a dark page background, whatever the exact
    // token value is; the light token must not leak through.
    const [darkBg, lightBg] = await page.evaluate(async () => {
      const root = document.documentElement;
      const original = root.getAttribute('data-theme') ?? '';
      const dark = getComputedStyle(document.body).backgroundColor;
      root.setAttribute('data-theme', 'light');
      const light = getComputedStyle(document.body).backgroundColor;
      root.setAttribute('data-theme', original);
      return [dark, light];
    });
    expect(darkBg).not.toBe(lightBg);
    const channels = darkBg.match(/\d+/g)?.map(Number) ?? [255];
    expect(Math.max(...channels)).toBeLessThan(64);
  }
});

test('parallel SSR locale requests do not contaminate each other', async ({ request }) => {
  const responses = await Promise.all(
    ['en', 'tr-TR', 'en', 'tr-TR'].map((locale) =>
      request.get('/login', { headers: { Cookie: `locale=${locale}` } }),
    ),
  );
  for (const [i, response] of responses.entries()) {
    expect(response.headers()['content-language']).toBe(i % 2 === 0 ? 'en' : 'tr-TR');
  }
});
