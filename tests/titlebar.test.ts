import { PhysicalSize } from '@tauri-apps/api/dpi';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createTitlebar, type Titlebar } from '../src/ui/titlebar';
import { el, svg } from '../src/ui/dom';
import {
  windowPlatform,
  type DesktopWindow,
  type WindowPlatform,
} from '../src/platform/window';

let container: HTMLDivElement;
let titlebar: Titlebar | undefined;
let resized: () => void;
let focusChanged: () => void;
let maximized: boolean;
let fullscreen: boolean;
let focused: boolean;
let native: DesktopWindow;
const stopResize = vi.fn();
const stopFocus = vi.fn();
const onError = vi.fn();

beforeEach(() => {
  vi.clearAllMocks();
  maximized = false;
  fullscreen = false;
  focused = true;
  native = {
    close: vi.fn(async () => {}),
    minimize: vi.fn(async () => {}),
    toggleMaximize: vi.fn(async () => {}),
    startDragging: vi.fn(async () => {}),
    startResizeDragging: vi.fn(async () => {}),
    isMaximized: vi.fn(async () => maximized),
    isFullscreen: vi.fn(async () => fullscreen),
    isFocused: vi.fn(async () => focused),
    setFullscreen: vi.fn(async () => {}),
    onResized: vi.fn(async (handler) => {
      resized = () =>
        handler({
          event: 'tauri://resize',
          id: 1,
          payload: new PhysicalSize(800, 600),
        });
      return stopResize;
    }),
    onFocusChanged: vi.fn(async (handler) => {
      focusChanged = () =>
        handler({ event: 'tauri://focus', id: 2, payload: focused });
      return stopFocus;
    }),
  };
  container = document.createElement('div');
  document.body.append(container);
});
afterEach(() => {
  titlebar?.destroy();
  titlebar = undefined;
  container.remove();
});

// The subscriptions resolve on microtasks; settle them before asserting.
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

async function render(
  platform: WindowPlatform = 'linux',
  options: { preview?: boolean } = {},
) {
  titlebar = createTitlebar({
    nativeWindow: options.preview ? undefined : native,
    platform,
    onError,
    children: [
      el('button', {}, svg('svg', { 'data-tool-icon': 'true' })),
      el('div', { class: 'file-title' }, el('span', {}, 'reader.md')),
    ],
  });
  titlebar.mount(container);
  await settle();
}
async function click(selector: string) {
  container.querySelector<HTMLElement>(selector)!.click();
  await settle();
}
async function mouseDown(selector: string, detail = 1, button = 0) {
  container
    .querySelector(selector)!
    .dispatchEvent(
      new MouseEvent('mousedown', { bubbles: true, detail, button }),
    );
  await settle();
}

describe('custom titlebar', () => {
  it.each([
    ['Win32', 'windows'],
    ['MacIntel', 'macos'],
    ['Linux x86_64', 'linux'],
  ])('detects %s as %s', (input, expected) => {
    expect(windowPlatform(input)).toBe(expected);
  });

  it.each(['windows', 'linux'] as const)(
    'calls native controls and follows external window changes on %s',
    async (platform) => {
      await render(platform);
      expect(
        [...container.querySelectorAll('.window-control')].map((node) =>
          node.getAttribute('aria-label'),
        ),
      ).toEqual([
        'Fenster minimieren',
        'Fenster maximieren',
        'Fenster schließen',
      ]);
      await click('.window-minimize');
      await click('.window-maximize');
      await click('.window-close');
      expect(native.minimize).toHaveBeenCalledOnce();
      expect(native.toggleMaximize).toHaveBeenCalledOnce();
      expect(native.close).toHaveBeenCalledOnce();
      maximized = true;
      resized();
      await settle();
      expect(
        container.querySelector('.window-maximize')?.getAttribute('aria-label'),
      ).toBe('Fenster wiederherstellen');
      expect(container.querySelector('.window-resize-handles')).toBeNull();
      focused = false;
      focusChanged();
      await settle();
      expect(
        container.querySelector<HTMLElement>('header')?.dataset.focused,
      ).toBe('false');
      maximized = false;
      resized();
      await settle();
      expect(container.querySelectorAll('.window-resize-handle')).toHaveLength(
        8,
      );
    },
  );

  it('places macOS controls first and toggles native fullscreen', async () => {
    await render('macos');
    expect(
      container.querySelector('header')?.firstElementChild?.className,
    ).toBe('window-controls');
    expect(
      container.querySelector('.window-control')?.getAttribute('aria-label'),
    ).toBe('Fenster schließen');
    await click('.window-maximize');
    expect(native.setFullscreen).toHaveBeenLastCalledWith(true);
    fullscreen = true;
    resized();
    await settle();
    expect(
      container.querySelector('.window-maximize')?.getAttribute('aria-label'),
    ).toBe('Vollbild verlassen');
    expect(container.querySelector('.window-resize-handles')).toBeNull();
    await mouseDown('.file-title');
    expect(native.startDragging).not.toHaveBeenCalled();
    await click('.window-maximize');
    expect(native.setFullscreen).toHaveBeenLastCalledWith(false);
  });

  it('drags and double-clicks only noninteractive areas, including title children', async () => {
    await render();
    await mouseDown('.file-title span');
    expect(native.startDragging).toHaveBeenCalledOnce();
    await mouseDown('.file-title span', 2);
    expect(native.toggleMaximize).toHaveBeenCalledOnce();
    await mouseDown('[data-tool-icon]');
    await mouseDown('.window-close path');
    await mouseDown('.file-title', 1, 2);
    expect(native.startDragging).toHaveBeenCalledOnce();
    expect(native.close).not.toHaveBeenCalled();
  });

  it('resizes through native edge dragging and reports rejected actions', async () => {
    await render();
    await mouseDown('.resize-southeast');
    expect(native.startResizeDragging).toHaveBeenCalledWith('SouthEast');
    vi.mocked(native.minimize).mockRejectedValueOnce(new Error('denied'));
    await click('.window-minimize');
    expect(onError).toHaveBeenCalledWith(
      'Die Fensteraktion konnte nicht ausgeführt werden.',
    );
  });

  it('cleans up event subscriptions, including late registrations after destroy', async () => {
    let resolveResize!: (stop: () => void) => void;
    vi.mocked(native.onResized).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveResize = resolve;
        }),
    );
    await render();
    titlebar!.destroy();
    resolveResize(stopResize);
    await settle();
    expect(stopResize).toHaveBeenCalledOnce();
    expect(stopFocus).toHaveBeenCalledOnce();
  });

  it('leaves the browser preview without window controls or resize handles', async () => {
    await render('linux', { preview: true });
    expect(container.querySelector('.window-controls')).toBeNull();
    expect(container.querySelector('.window-resize-handles')).toBeNull();
    await mouseDown('header');
    expect(native.startDragging).not.toHaveBeenCalled();
  });
});
