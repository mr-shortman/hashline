import { expect, test, type Page } from '@playwright/test';

const source =
  '# Begin\n\n[jump](#last)\n\n' +
  Array.from(
    { length: 3000 },
    (_, i) =>
      `## Section ${i}\n\nPrefix **needle** suffix [reference][end].\n\n- list\n- text\n\n`,
  ).join('') +
  '# Last\n\nTHE-END\n\n[end]: destination.md\n';
async function open(page: Page, name: string, content: string) {
  const chooser = page.waitForEvent('filechooser');
  await page.getByRole('button', { name: 'Öffnen', exact: true }).click();
  await (
    await chooser
  ).setFiles({ name, mimeType: 'text/markdown', buffer: Buffer.from(content) });
  await expect(page.locator('.file-title')).toHaveText(name);
}

test('jumps to an unrendered section and searches every section with stable selected text', async ({
  page,
}) => {
  await page.goto('/');
  await open(page, 'sections.md', source);
  await page.locator('article a').first().click();
  await expect(page.locator('#doc-last')).toBeInViewport();
  await expect(page.locator('article')).toHaveAttribute(
    'data-render-state',
    'complete',
  );
  await page.locator('article').evaluate((root) => {
    const first = root.querySelector('p strong')!.previousSibling!;
    const last = root.querySelector('p strong')!.nextSibling!;
    const range = document.createRange();
    range.setStart(first, 0);
    range.setEnd(last, 7);
    getSelection()!.removeAllRanges();
    getSelection()!.addRange(range);
  });
  await page.keyboard.press('Control+f');
  await page
    .getByRole('textbox', { name: 'Dokument durchsuchen' })
    .fill('needle');
  await expect(page.locator('.search-count')).toHaveText('1 / 3000');
  await page.keyboard.press('Enter');
  await expect(page.locator('.search-count')).toHaveText('2 / 3000');
  await page.keyboard.press('Escape');
  await page.locator('.document-scroll').focus();
  await page.keyboard.press('Control+a');
  const selectedAll = await page.evaluate(() => getSelection()!.toString());
  expect(selectedAll).toContain('THE-END');
  expect(
    new Set(
      Array.from(selectedAll.matchAll(/Section (\d+)/g), (match) => match[1]),
    ).size,
  ).toBe(3000);
  expect(await page.evaluate(() => getSelection()!.toString())).toContain(
    'Prefix needle suffix',
  );
});

test('opening another file cancels pending insertions and search work', async ({
  page,
}) => {
  await page.goto('/');
  await open(page, 'large.md', source.repeat(8));
  await page.locator('article a').first().waitFor();
  await page.locator('article').evaluate((article) => {
    const observer = new MutationObserver(() => {
      if (article.dataset.renderState !== 'retiring') return;
      const link = article.querySelector<HTMLAnchorElement>('a');
      article.dataset.retiredLinkClicked = String(!!link);
      link?.click();
      observer.disconnect();
    });
    observer.observe(article, {
      attributes: true,
      attributeFilter: ['data-render-state'],
    });
  });
  await open(page, 'replacement.md', '# Replacement\n\nOnly this text.');
  await expect(page.locator('article')).toHaveAttribute(
    'data-render-state',
    'complete',
  );
  await expect(page.locator('article h1')).toHaveText('Replacement');
  await expect(page.locator('article')).not.toContainText('THE-END');
  await expect(page.locator('article')).toHaveAttribute(
    'data-retired-link-clicked',
    'true',
  );
  await expect(
    page.getByText('Dieser Abschnitt wurde nicht gefunden.', { exact: true }),
  ).not.toBeVisible();
  await page.keyboard.press('Control+f');
  await page
    .getByRole('textbox', { name: 'Dokument durchsuchen' })
    .fill('needle');
  await expect(page.locator('.search-count')).toHaveText('Keine Treffer');
});

test('keeps an inline selection made while later sections are inserted', async ({
  page,
}) => {
  await page.goto('/');
  await open(page, 'progressive.md', source.repeat(4));
  const selected = await page.locator('article').evaluate((root) => {
    const strong = root.querySelector('p strong')!;
    const range = document.createRange();
    range.setStart(strong.previousSibling!, 0);
    range.setEnd(strong.nextSibling!, 7);
    getSelection()!.removeAllRanges();
    getSelection()!.addRange(range);
    return getSelection()!.toString();
  });
  await expect(page.locator('article')).toHaveAttribute(
    'data-render-state',
    'complete',
  );
  expect(await page.evaluate(() => getSelection()!.toString())).toBe(selected);
});

test('reuses unchanged sections and updates document navigation without duplicating nodes', async ({
  page,
}) => {
  await page.goto('/');
  await open(page, 'first.md', source);
  await expect(page.locator('article')).toHaveAttribute(
    'data-render-state',
    'complete',
  );
  await page.locator('article').evaluate((article) => {
    (window as unknown as { firstSection: Element | null }).firstSection =
      article.firstElementChild;
  });
  await open(page, 'second.md', source);
  await expect(page.locator('article')).toHaveAttribute(
    'data-render-state',
    'complete',
  );
  expect(
    await page
      .locator('article')
      .evaluate(
        (article) =>
          article.firstElementChild ===
          (window as unknown as { firstSection: Element | null }).firstSection,
      ),
  ).toBe(true);
  expect(
    await page.locator('article').getAttribute('data-reused-sections'),
  ).not.toBe('0');
  await expect(page.locator('#doc-last')).toHaveCount(1);
  await page.locator('article a').first().click();
  await expect(page.locator('#doc-last')).toBeInViewport();
  await page.keyboard.press('Control+f');
  await page
    .getByRole('textbox', { name: 'Dokument durchsuchen' })
    .fill('needle');
  await expect(page.locator('.search-count')).toHaveText('1 / 3000');
});

test('rebuilds resource-bearing sections including the legacy HTML image spelling', async ({
  page,
}) => {
  await page.goto('/');
  const content = '# Images\n\n<image src="picture.png" alt="Local image">\n';
  await open(page, 'first.md', content);
  await expect(page.locator('article')).toHaveAttribute(
    'data-render-state',
    'complete',
  );
  await expect(page.locator('article img')).toHaveCount(1);
  await page
    .locator('article img')
    .evaluate((img) => img.setAttribute('data-old-resource', 'true'));
  await open(page, 'second.md', content);
  await expect(page.locator('article')).toHaveAttribute(
    'data-render-state',
    'complete',
  );
  await expect(page.locator('article img')).not.toHaveAttribute(
    'data-old-resource',
    'true',
  );
  await expect(page.locator('article')).toHaveAttribute(
    'data-reused-sections',
    '0',
  );
});
