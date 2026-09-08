import { expect, test } from '@playwright/test';

for (const platform of ['windows', 'macos', 'linux']) {
  test(`${platform} titlebar fits narrow windows and keeps tools interactive`, async ({
    page,
  }, testInfo) => {
    // Replace only the native window boundary; render the real app and its toolbar.
    await page.route('**/src/platform/window.ts', (route) =>
      route.fulfill({
        contentType: 'application/javascript',
        body: `
        let maximized = false, fullscreen = false;
        let resized = () => {};
        window.windowCalls = [];
        export const windowPlatform = () => '${platform}';
        export const desktopWindow = {
          close: async () => window.windowCalls.push('close'),
          minimize: async () => window.windowCalls.push('minimize'),
          toggleMaximize: async () => { maximized = !maximized; resized(); },
          setFullscreen: async (value) => { fullscreen = value; resized(); },
          startDragging: async () => window.windowCalls.push('drag'),
          startResizeDragging: async (direction) => window.windowCalls.push(direction),
          isMaximized: async () => maximized,
          isFullscreen: async () => fullscreen,
          isFocused: async () => true,
          onResized: async (handler) => { resized = handler; return () => {}; },
          onFocusChanged: async () => () => {},
        };
      `,
      }),
    );
    await page.goto('/');
    const chooser = page.waitForEvent('filechooser');
    await page.getByRole('button', { name: 'Öffnen', exact: true }).click();
    await (
      await chooser
    ).setFiles({
      name: 'Architektur und technische Entscheidungen für Hashline.md',
      mimeType: 'text/markdown',
      buffer: Buffer.from('# Architektur\n\nEin Dokument zum Lesen.'),
    });
    await expect(page.locator('article h1')).toBeVisible();
    await expect(page.locator('.titlebar')).toHaveAttribute(
      'data-platform',
      platform,
    );
    await page
      .locator('.titlebar')
      .screenshot({ path: testInfo.outputPath(`${platform}-light.png`) });
    await page.locator('.file-title').dblclick();
    await expect(
      page.getByRole('button', {
        name:
          platform === 'macos'
            ? 'Vollbild aktivieren'
            : 'Fenster wiederherstellen',
        exact: true,
      }),
    ).toBeVisible();
    await expect(page.locator('.window-resize-handles')).toHaveCount(0);
    await page.locator('.file-title').dblclick();
    await expect(page.locator('.window-resize-handles')).toHaveCount(1);
    await page.getByRole('button', { name: 'Suche', exact: true }).click();
    await expect(
      page.getByRole('textbox', { name: 'Dokument durchsuchen' }),
    ).toBeVisible();
    await page
      .getByRole('button', { name: 'Darstellung und Optionen' })
      .click();
    await page.getByRole('button', { name: 'Dunkel', exact: true }).click();
    await page.keyboard.press('Escape');
    await page
      .locator('.titlebar')
      .screenshot({ path: testInfo.outputPath(`${platform}-dark.png`) });
    await page.setViewportSize({ width: 360, height: 640 });
    const bounds = await page
      .locator('.titlebar button, .file-title')
      .evaluateAll((nodes) =>
        nodes.map((node) => {
          const { x, y, right, bottom, width } = node.getBoundingClientRect();
          return { x, y, right, bottom, width };
        }),
      );
    for (const rect of bounds) {
      expect(rect.x).toBeGreaterThanOrEqual(0);
      expect(rect.right).toBeLessThanOrEqual(360);
      expect(rect.y).toBeGreaterThanOrEqual(0);
      expect(rect.bottom).toBeLessThanOrEqual(48);
      expect(rect.width).toBeGreaterThan(0);
    }
    await page
      .locator('.titlebar')
      .screenshot({ path: testInfo.outputPath(`${platform}-narrow.png`) });
    await page.getByRole('button', { name: 'Fenster minimieren' }).click();
    await page.getByRole('button', { name: 'Fenster schließen' }).click();
    expect(
      await page.evaluate(() =>
        (Reflect.get(window, 'windowCalls') as string[]).filter(
          (call) => call !== 'drag',
        ),
      ),
    ).toEqual(['minimize', 'close']);
  });
}
