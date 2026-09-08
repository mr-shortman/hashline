import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow, type Window } from '@tauri-apps/api/window';

export type WindowPlatform = 'windows' | 'macos' | 'linux';

// Desktop webviews expose the host OS here, including in direct Vite previews.
export function windowPlatform(platform = navigator.platform): WindowPlatform {
  if (/mac/i.test(platform)) return 'macos';
  if (/win/i.test(platform)) return 'windows';
  return 'linux';
}

export type DesktopWindow = Pick<
  Window,
  | 'close'
  | 'minimize'
  | 'toggleMaximize'
  | 'startDragging'
  | 'startResizeDragging'
  | 'isMaximized'
  | 'isFullscreen'
  | 'isFocused'
  | 'setFullscreen'
  | 'onResized'
  | 'onFocusChanged'
>;

export const desktopWindow: DesktopWindow | undefined = isTauri()
  ? getCurrentWindow()
  : undefined;
