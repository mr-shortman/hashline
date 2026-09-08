import { expect, test } from '@playwright/test';

for (const theme of ['light', 'dark'] as const) {
  test(`${theme}: contrast, 200% text and keyboard at 360px`, async ({
    page,
  }) => {
    await page.emulateMedia({ colorScheme: theme, reducedMotion: 'reduce' });
    await page.setViewportSize({ width: 360, height: 640 });
    await page.goto('/');
    const chooser = page.waitForEvent('filechooser');
    await page
      .getByRole('button', { name: 'Markdown-Datei öffnen', exact: true })
      .click();
    await (
      await chooser
    ).setFiles({
      name: 'a11y.md',
      mimeType: 'text/markdown',
      buffer: Buffer.from(
        '# Kontrast\n\n> Zitat\n\n[Link](https://example.com)\n\n```css\nh1 { color: red; }\n```\n\n```js\nconst value = true; // Kommentar\n```\n\n| Tabelle | Inhalt |\n| --- | --- |\n| Breite | Eine lange Tabellenzeile |',
      ),
    });
    await expect(page.locator('article')).toHaveAttribute(
      'data-render-state',
      'complete',
    );
    await expect(page.locator('.hljs-keyword').first()).toBeAttached();
    const ratios = await page.evaluate(() => {
      function luminance(value: string) {
        const rgb = value
          .match(/[\d.]+/g)!
          .slice(0, 3)
          .map(Number)
          .map((v) => {
            v /= 255;
            return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
          });
        return rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
      }
      return [
        ...document.querySelectorAll(
          'article a, article blockquote, [class^=hljs-], .file-title, .open-button',
        ),
      ].map((el) => {
        let parent: Element | null = el;
        let bg = 'rgba(0, 0, 0, 0)';
        while (parent && bg === 'rgba(0, 0, 0, 0)') {
          bg = getComputedStyle(parent).backgroundColor;
          parent = parent.parentElement;
        }
        const fg = luminance(getComputedStyle(el).color);
        const back = luminance(bg);
        return {
          name: el.className || el.tagName,
          ratio: (Math.max(fg, back) + 0.05) / (Math.min(fg, back) + 0.05),
        };
      });
    });
    for (const result of ratios)
      expect(result.ratio, result.name).toBeGreaterThanOrEqual(4.5);
    for (let i = 0; i < 10; i++) await page.keyboard.press('Control+Equal');
    await expect(page.locator('article')).toHaveCSS('font-size', '34px');
    expect(
      await page.evaluate(() => document.body.scrollWidth <= innerWidth),
    ).toBe(true);
    await page.keyboard.press('Control+Shift+o');
    await expect(
      page.getByRole('dialog', { name: 'Inhaltsverzeichnis', exact: true }),
    ).toBeVisible();
    await expect(page.locator('.viewport-shell')).toHaveAttribute('inert', '');
    await page.keyboard.press('Shift+Tab');
    await expect(
      page.getByRole('button', { name: 'Kontrast', exact: true }),
    ).toBeFocused();
    await page.keyboard.press('Tab');
    await expect(page.locator('.outline .icon-button')).toBeFocused();
    await page.keyboard.press('Escape');
    await expect(
      page.getByRole('button', { name: 'Inhaltsverzeichnis', exact: true }),
    ).toBeFocused();
    await page.keyboard.press('Control+f');
    await expect(page.getByRole('textbox')).toBeFocused();
    await page.keyboard.press('Control+Shift+o');
    await expect(page.locator('.outline')).toBeVisible();
    await page.keyboard.press('Control+f');
    await expect(page.locator('.outline')).toHaveCount(0);
    await expect(page.getByRole('textbox')).toBeFocused();
    await page.keyboard.press('Escape');
    await expect(
      page.getByRole('button', { name: 'Suche', exact: true }),
    ).toBeFocused();
    await page
      .getByRole('button', { name: 'Darstellung und Optionen' })
      .click();
    await page.keyboard.press('Shift+Tab');
    await expect(
      page.getByRole('button', { name: 'Dateipfad kopieren' }),
    ).toBeFocused();
    await page.keyboard.press('Escape');
    await expect(
      page.getByRole('button', { name: 'Darstellung und Optionen' }),
    ).toBeFocused();
  });
}

test('remote placeholders cannot be forged by document attributes', async ({
  page,
}) => {
  let requests = 0;
  page.on('request', (request) => {
    if (request.url().includes('example.com')) requests++;
  });
  await page.goto('/');
  const chooser = page.waitForEvent('filechooser');
  await page
    .getByRole('button', { name: 'Markdown-Datei öffnen', exact: true })
    .click();
  await (
    await chooser
  ).setFiles({
    name: 'remote.md',
    mimeType: 'text/markdown',
    buffer: Buffer.from(
      '# Bilder\n\n![Remote](https://example.com/image.png)\n\n<img data-remote-source="file:///etc/passwd" src="data:image/svg+xml,x">',
    ),
  });
  await expect(
    page.getByRole('region', { name: 'Remote-Bilder', exact: true }),
  ).toContainText('1 Remote-Bilder blockiert');
  await expect(page.locator('img[data-remote-source]')).toHaveCount(1);
  await expect(page.locator('img[src]')).toHaveCount(0);
  expect(requests).toBe(0);
});

test('wide tables scroll without splitting column words into individual letters', async ({
  page,
}) => {
  await page.setViewportSize({ width: 360, height: 640 });
  await page.goto('/');
  const chooser = page.waitForEvent('filechooser');
  await page
    .getByRole('button', { name: 'Markdown-Datei öffnen', exact: true })
    .click();
  await (await chooser).setFiles('tests/fixtures/accessibility.md');
  await expect(page.locator('article')).toHaveAttribute(
    'data-render-state',
    'complete',
  );
  for (let i = 0; i < 10; i++) await page.keyboard.press('Control+Equal');
  const result = await page
    .locator('th')
    .last()
    .evaluate((cell) => {
      const range = document.createRange();
      range.setStart(cell.firstChild!, 0);
      range.setEnd(cell.firstChild!, 6); // The word “Spalte” stays on one line.
      const region = cell.closest('.table-scroll')!;
      return {
        lines: range.getClientRects().length,
        overflowing: region.scrollWidth > region.clientWidth,
      };
    });
  expect(result).toEqual({ lines: 1, overflowing: true });
  expect(
    await page.evaluate(() => document.body.scrollWidth <= innerWidth),
  ).toBe(true);
});

test('unavailable images expose visible text, including blocked remote images', async ({
  page,
}) => {
  await page.goto('/');
  const chooser = page.waitForEvent('filechooser');
  await page
    .getByRole('button', { name: 'Markdown-Datei öffnen', exact: true })
    .click();
  await (
    await chooser
  ).setFiles({
    name: 'images.md',
    mimeType: 'text/markdown',
    buffer: Buffer.from(
      '# Images\n\n![Remote alt](https://example.com/a.png)\n\n![Local alt](missing.png)',
    ),
  });
  await expect(page.locator('.image-placeholder').first()).toHaveText(
    'Remote alt — Bildzugriff nicht freigegeben',
  );
  await expect(page.locator('.image-placeholder').last()).toHaveText(
    'Local alt — Bildzugriff nicht freigegeben',
  );
  await expect(page.locator('.image-placeholder').first()).toBeVisible();
  await expect(page.locator('img').first()).toBeHidden();
});
