import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { PhysicalSize } from '@tauri-apps/api/dpi';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { Titlebar } from '../src/ui/Titlebar';
import {
  windowPlatform,
  type DesktopWindow,
  type WindowPlatform,
} from '../src/platform/window';

let container: HTMLDivElement;
let root: Root;
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
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
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
  root = createRoot(container);
});
afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
  vi.unstubAllGlobals();
});

async function render(
  platform: WindowPlatform = 'linux',
  nativeWindow: DesktopWindow | undefined = native,
) {
  await act(async () => {
    root.render(
      createElement(Titlebar, {
        nativeWindow,
        platform,
        onError,
        children: [
          createElement(
            'button',
            { key: 'open' },
            createElement('svg', { 'data-tool-icon': true }),
          ),
          createElement(
            'div',
            { key: 'title', className: 'file-title' },
            createElement('span', {}, 'reader.md'),
          ),
        ],
      }),
    );
  });
}
async function click(selector: string) {
  await act(async () =>
    container.querySelector<HTMLElement>(selector)!.click(),
  );
}
async function mouseDown(selector: string, detail = 1, button = 0) {
  await act(async () => {
    container
      .querySelector(selector)!
      .dispatchEvent(
        new MouseEvent('mousedown', { bubbles: true, detail, button }),
      );
  });
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
        [...container.querySelectorAll('.window-control')].map((el) =>
          el.getAttribute('aria-label'),
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
      await act(async () => resized());
      expect(
        container.querySelector('.window-maximize')?.getAttribute('aria-label'),
      ).toBe('Fenster wiederherstellen');
      expect(container.querySelector('.window-resize-handles')).toBeNull();
      focused = false;
      await act(async () => focusChanged());
      expect(container.querySelector('header')?.dataset.focused).toBe('false');
      maximized = false;
      await act(async () => resized());
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
    await act(async () => resized());
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

  it('cleans up event subscriptions, including late registrations after unmount', async () => {
    let resolveResize!: (stop: () => void) => void;
    vi.mocked(native.onResized).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveResize = resolve;
        }),
    );
    await render();
    await act(async () => root.unmount());
    await act(async () => resolveResize(stopResize));
    expect(stopResize).toHaveBeenCalledOnce();
    expect(stopFocus).toHaveBeenCalledOnce();
  });

  it('leaves the browser preview without window controls or resize handles', async () => {
    await act(async () =>
      root.render(createElement(Titlebar, { onError, children: 'Hashline' })),
    );
    expect(container.querySelector('.window-controls')).toBeNull();
    expect(container.querySelector('.window-resize-handles')).toBeNull();
    await mouseDown('header');
    expect(native.startDragging).not.toHaveBeenCalled();
  });
});
