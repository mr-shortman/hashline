import {
  desktopWindow,
  windowPlatform,
  type DesktopWindow,
  type WindowPlatform,
} from '../platform/window';
import { el, svg } from './dom';
import './titlebar.css';

const resizeDirections: Parameters<DesktopWindow['startResizeDragging']>[0][] =
  [
    'North',
    'South',
    'East',
    'West',
    'NorthEast',
    'NorthWest',
    'SouthEast',
    'SouthWest',
  ];

const glyphs = {
  close: 'm4 4 8 8M12 4l-8 8',
  minimize: 'M3 8h10',
  maximize: 'M3.5 3.5h9v9h-9z',
  restore: 'M5.5 5.5h7v7h-7zM3.5 10.5h-1v-8h8v1',
  fullscreen: 'M3 7V3h4M9 13h4V9M3 3l4 4M13 13l-4-4',
  exitFullscreen: 'M7 3v4H3M13 9H9v4M7 7 3 3M9 9l4 4',
};

export interface TitlebarOptions {
  children: (Node | string)[];
  onError(message: string): void;
  nativeWindow?: DesktopWindow;
  platform?: WindowPlatform;
}
export interface Titlebar {
  header: HTMLElement;
  // The resize handles are siblings of the header, so they can only be placed
  // once the header itself is connected.
  mount(parent: ParentNode): void;
  destroy(): void;
}

export function createTitlebar({
  children,
  onError,
  nativeWindow = desktopWindow,
  platform = windowPlatform(),
}: TitlebarOptions): Titlebar {
  let maximized = false;
  let fullscreen = false;
  let focused = true;
  let handles: HTMLElement | undefined;
  let stopped = false;
  let revision = 0;
  const stops: (() => void)[] = [];

  const run = (action: () => Promise<unknown>) => {
    void action().catch(() =>
      onError('Die Fensteraktion konnte nicht ausgeführt werden.'),
    );
  };
  const maximizeLabel = () =>
    platform === 'macos' || fullscreen
      ? fullscreen
        ? 'Vollbild verlassen'
        : 'Vollbild aktivieren'
      : maximized
        ? 'Fenster wiederherstellen'
        : 'Fenster maximieren';
  const maximizeGlyph = () =>
    fullscreen
      ? 'exitFullscreen'
      : platform === 'macos'
        ? 'fullscreen'
        : maximized
          ? 'restore'
          : 'maximize';

  let maximizeButton: HTMLButtonElement | undefined;
  let maximizePath: SVGElement | undefined;
  const control = (action: 'close' | 'minimize' | 'maximize') => {
    const label =
      action === 'close'
        ? 'Fenster schließen'
        : action === 'minimize'
          ? 'Fenster minimieren'
          : maximizeLabel();
    const path = svg('path', {
      d: glyphs[action === 'maximize' ? maximizeGlyph() : action],
    });
    const button = el(
      'button',
      {
        type: 'button',
        class: `window-control window-${action}`,
        'aria-label': label,
        title: label,
        onclick: () =>
          run(async () => {
            if (action === 'close') await nativeWindow!.close();
            else if (action === 'minimize') await nativeWindow!.minimize();
            else if (platform === 'macos' || fullscreen)
              await nativeWindow!.setFullscreen(
                !(await nativeWindow!.isFullscreen()),
              );
            else await nativeWindow!.toggleMaximize();
          }),
      },
      el(
        'span',
        { class: 'window-control-disc' },
        svg(
          'svg',
          {
            width: '16',
            height: '16',
            viewBox: '0 0 16 16',
            fill: 'none',
            stroke: 'currentColor',
            'stroke-width': '1.2',
            'aria-hidden': 'true',
          },
          path,
        ),
      ),
    );
    if (action === 'maximize') {
      maximizeButton = button;
      maximizePath = path;
    }
    return button;
  };
  const controls =
    nativeWindow &&
    el(
      'div',
      {
        class: 'window-controls',
        role: 'group',
        'aria-label': 'Fenstersteuerung',
      },
      ...(platform === 'macos'
        ? (['close', 'minimize', 'maximize'] as const)
        : (['minimize', 'maximize', 'close'] as const)
      ).map(control),
    );

  const drag = (event: MouseEvent) => {
    if (!nativeWindow || event.button !== 0 || fullscreen) return;
    // SVGs and labels inside buttons must never initiate a window drag.
    if (
      (event.target as Element).closest(
        'button, a, input, [data-no-window-drag]',
      )
    )
      return;
    event.preventDefault();
    run(() =>
      event.detail === 2
        ? nativeWindow.toggleMaximize()
        : nativeWindow.startDragging(),
    );
  };
  const header = el(
    'header',
    {
      class: `toolbar${nativeWindow ? ' titlebar' : ''}`,
      'data-platform': nativeWindow ? platform : undefined,
      'data-focused': String(focused),
      onmousedown: drag as EventListener,
    },
    platform === 'macos' && controls,
    ...children,
    platform !== 'macos' && controls,
  );

  const buildHandles = () =>
    el(
      'div',
      { class: 'window-resize-handles', 'aria-hidden': 'true' },
      ...resizeDirections.map((direction) =>
        el('div', {
          class: `window-resize-handle resize-${direction.toLowerCase()}`,
          onmousedown: ((event: MouseEvent) => {
            if (event.button !== 0) return;
            event.preventDefault();
            run(() => nativeWindow!.startResizeDragging(direction));
          }) as EventListener,
        }),
      ),
    );

  const sync = () => {
    header.dataset.focused = String(focused);
    if (maximizeButton && maximizePath) {
      const label = maximizeLabel();
      maximizeButton.setAttribute('aria-label', label);
      maximizeButton.title = label;
      maximizePath.setAttribute('d', glyphs[maximizeGlyph()]);
    }
    const wanted = !!nativeWindow && !maximized && !fullscreen;
    if (wanted && !handles && header.parentNode) {
      handles = buildHandles();
      header.after(handles);
    } else if (!wanted && handles) {
      handles.remove();
      handles = undefined;
    }
  };

  if (nativeWindow) {
    const refresh = async () => {
      const current = ++revision;
      const [max, full, focus] = await Promise.all([
        nativeWindow.isMaximized(),
        nativeWindow.isFullscreen(),
        nativeWindow.isFocused(),
      ]);
      if (!stopped && current === revision) {
        maximized = max;
        fullscreen = full;
        focused = focus;
        sync();
      }
    };
    const report = () => {
      if (!stopped) onError('Der Fensterstatus konnte nicht gelesen werden.');
    };
    const subscribe = async () => {
      // Register both independently so each listener is cleaned up on failure/unmount.
      await Promise.all(
        [
          nativeWindow.onResized(() => {
            void refresh().catch(report);
          }),
          nativeWindow.onFocusChanged(() => {
            void refresh().catch(report);
          }),
        ].map(async (subscription) => {
          const stop = await subscription;
          if (stopped) stop();
          else stops.push(stop);
        }),
      );
      if (!stopped) await refresh();
    };
    void subscribe().catch(report);
  }
  sync();

  return {
    header,
    mount(parent) {
      parent.append(header);
      sync();
    },
    destroy() {
      stopped = true;
      stops.forEach((stop) => stop());
      handles?.remove();
      handles = undefined;
      header.remove();
    },
  };
}
