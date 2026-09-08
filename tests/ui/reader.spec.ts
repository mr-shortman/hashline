import { expect, test, type Page } from '@playwright/test';

async function open(page: Page, path = 'tests/fixtures/reader.md') {
  const chooser = page.waitForEvent('filechooser');
  await page
    .getByRole('button', { name: 'Markdown-Datei öffnen', exact: true })
    .click();
  await (await chooser).setFiles(path);
  await expect(page.locator('article h1')).toBeVisible();
}
test('opens, searches across content, preserves DOM and supports outline navigation', async ({
  page,
}) => {
  await page.goto('/');
  await open(page);
  await expect(page.locator('.file-title')).toHaveText('reader.md');
  await page
    .locator('article h1')
    .evaluate((node) => node.setAttribute('data-sentinel', 'kept'));
  await page.keyboard.press('Control+f');
  await page
    .getByRole('textbox', { name: 'Dokument durchsuchen' })
    .fill('Nadel');
  await expect(page.locator('.search-count')).toHaveText('1 / 3');
  await page.keyboard.press('Enter');
  await expect(page.locator('.search-count')).toHaveText('2 / 3');
  await expect(page.locator('article h1')).toHaveAttribute(
    'data-sentinel',
    'kept',
  );
  await page.keyboard.press('Escape');
  await expect(
    page.getByRole('button', { name: 'Suche', exact: true }),
  ).toBeFocused();
  await page.keyboard.press('Control+Shift+o');
  await page.getByRole('button', { name: 'Grüße 日本語', exact: true }).click();
  await expect(page.locator('#doc-grüße-日本語')).toBeInViewport();
  await expect(
    page.locator('article input[type=checkbox]').first(),
  ).toBeDisabled();
});
test('the highlight fallback keeps selection intact and marks the active match', async ({
  page,
}) => {
  await page.addInitScript(() =>
    Object.defineProperty(CSS, 'highlights', {
      value: undefined,
      configurable: true,
    }),
  );
  await page.goto('/');
  await open(page);
  await page.keyboard.press('Control+f');
  await page
    .getByRole('textbox', { name: 'Dokument durchsuchen' })
    .fill('Nadel');
  await expect(page.locator('.search-count')).toHaveText('1 / 3');
  await expect(page.locator('.search-fallback span')).toHaveCount(1);
  await page.locator('.document-scroll').focus();
  await page.keyboard.press('Control+a');
  expect(await page.evaluate(() => getSelection()?.toString())).toContain(
    'Ein letzter Absatz',
  );
  await page.keyboard.press('Control+r');
  await expect(page.locator('.search-count')).toHaveText('1 / 3');
});
test('selection spans blocks and code copying excludes labels', async ({
  page,
  context,
}) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await page.goto('/');
  await open(page);
  await page.locator('.document-scroll').focus();
  await page.keyboard.press('Control+a');
  const selection = await page.evaluate(() => getSelection()?.toString());
  expect(selection).toContain('Hashline · Lesetest');
  expect(selection).toContain('Ein letzter Absatz');
  await page.evaluate(() => getSelection()?.removeAllRanges());
  await page
    .getByRole('button', { name: 'Codeblock kopieren' })
    .first()
    .click();
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(
    "const message: string = 'Nadel';\nconsole.log(message);\n",
  );
});
test('theme, zoom and narrow outline work; startup stays empty', async ({
  page,
}) => {
  await page.setViewportSize({ width: 460, height: 760 });
  await page.goto('/');
  await open(page);
  await page.getByRole('button', { name: 'Darstellung und Optionen' }).click();
  await page.getByRole('button', { name: 'Dunkel', exact: true }).click();
  for (let i = 0; i < 10; i++)
    await page.getByRole('button', { name: 'Text vergrößern' }).click();
  await expect(page.locator('article')).toHaveCSS('font-size', '34px');
  await page.keyboard.press('Escape');
  await page.keyboard.press('Control+Shift+o');
  await expect(page.locator('.outline')).toBeVisible();
  expect(
    await page.evaluate(() => document.body.scrollWidth <= innerWidth),
  ).toBe(true);
  await page.keyboard.press('Escape');
  await expect(
    page.getByRole('button', { name: 'Inhaltsverzeichnis', exact: true }),
  ).toBeFocused();
  await page.reload();
  await expect(
    page.getByRole('heading', { name: 'Raum zum Lesen.' }),
  ).toBeVisible();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
});
test('hostile HTML is passive and missing files do not replace the current document', async ({
  page,
}) => {
  await page.goto('/');
  await open(page, 'tests/fixtures/hostile.md');
  expect(await page.evaluate(() => '__injected' in window)).toBe(false);
  await expect(
    page.locator('article script, article iframe, article style, article form'),
  ).toHaveCount(0);
  const chooser = page.waitForEvent('filechooser');
  await page.getByRole('button', { name: 'Öffnen', exact: true }).click();
  await (
    await chooser
  ).setFiles({
    name: 'kaputt.md',
    mimeType: 'text/markdown',
    buffer: Buffer.from([0xff, 0xfe, 0xff]),
  });
  await expect(page.getByRole('alert')).toContainText('UTF-8');
  await expect(page.locator('.file-title')).toHaveText('hostile.md');
});
