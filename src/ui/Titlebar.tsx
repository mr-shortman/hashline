import { useEffect, useState, type MouseEvent, type ReactNode } from 'react';
import {
  desktopWindow,
  windowPlatform,
  type DesktopWindow,
  type WindowPlatform,
} from '../platform/window';
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

export function Titlebar({
  children,
  onError,
  nativeWindow = desktopWindow,
  platform = windowPlatform(),
}: {
  children: ReactNode;
  onError: (message: string) => void;
  nativeWindow?: DesktopWindow;
  platform?: WindowPlatform;
}) {
  const [maximized, setMaximized] = useState(false);
  const [fullscreen, setFullscreen] = useState(false);
  const [focused, setFocused] = useState(true);

  useEffect(() => {
    if (!nativeWindow) return;
    let stopped = false;
    let revision = 0;
    const stops: (() => void)[] = [];
    const refresh = async () => {
      const current = ++revision;
      const [max, full, focus] = await Promise.all([
        nativeWindow.isMaximized(),
        nativeWindow.isFullscreen(),
        nativeWindow.isFocused(),
      ]);
      if (!stopped && current === revision) {
        setMaximized(max);
        setFullscreen(full);
        setFocused(focus);
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
    return () => {
      stopped = true;
      stops.forEach((stop) => stop());
    };
  }, [nativeWindow, onError]);

  const run = (action: () => Promise<unknown>) => {
    void action().catch(() =>
      onError('Die Fensteraktion konnte nicht ausgeführt werden.'),
    );
  };
  const drag = (event: MouseEvent<HTMLElement>) => {
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
  const controls = nativeWindow && (
    <div className="window-controls" role="group" aria-label="Fenstersteuerung">
      {(platform === 'macos'
        ? (['close', 'minimize', 'maximize'] as const)
        : (['minimize', 'maximize', 'close'] as const)
      ).map((action) => {
        const label =
          action === 'close'
            ? 'Fenster schließen'
            : action === 'minimize'
              ? 'Fenster minimieren'
              : platform === 'macos' || fullscreen
                ? fullscreen
                  ? 'Vollbild verlassen'
                  : 'Vollbild aktivieren'
                : maximized
                  ? 'Fenster wiederherstellen'
                  : 'Fenster maximieren';
        const glyph =
          action !== 'maximize'
            ? action
            : fullscreen
              ? 'exitFullscreen'
              : platform === 'macos'
                ? 'fullscreen'
                : maximized
                  ? 'restore'
                  : 'maximize';
        return (
          <button
            key={action}
            type="button"
            className={`window-control window-${action}`}
            aria-label={label}
            title={label}
            onClick={() =>
              run(async () => {
                if (action === 'close') await nativeWindow.close();
                else if (action === 'minimize') await nativeWindow.minimize();
                else if (platform === 'macos' || fullscreen)
                  await nativeWindow.setFullscreen(
                    !(await nativeWindow.isFullscreen()),
                  );
                else await nativeWindow.toggleMaximize();
              })
            }
          >
            <span className="window-control-disc">
              <svg
                width="16"
                height="16"
                viewBox="0 0 16 16"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.2"
                aria-hidden="true"
              >
                <path d={glyphs[glyph]} />
              </svg>
            </span>
          </button>
        );
      })}
    </div>
  );

  return (
    <>
      <header
        className={`toolbar${nativeWindow ? ' titlebar' : ''}`}
        data-platform={nativeWindow ? platform : undefined}
        data-focused={focused}
        onMouseDown={drag}
      >
        {platform === 'macos' && controls}
        {children}
        {platform !== 'macos' && controls}
      </header>
      {nativeWindow && !maximized && !fullscreen && (
        <div className="window-resize-handles" aria-hidden="true">
          {resizeDirections.map((direction) => (
            <div
              key={direction}
              className={`window-resize-handle resize-${direction.toLowerCase()}`}
              onMouseDown={(event) => {
                if (event.button !== 0) return;
                event.preventDefault();
                run(() => nativeWindow.startResizeDragging(direction));
              }}
            />
          ))}
        </div>
      )}
    </>
  );
}
