export type Theme = 'system' | 'light' | 'dark';
export interface ReadingPosition {
  path: string;
  heading: string;
  previous: string;
  offset: number;
  progress: number;
}
export interface Preferences {
  version: 1;
  theme: Theme;
  zoom: number;
  outline: boolean;
  recent: ReadingPosition[];
}
const KEY = 'hashline.preferences';
const defaults: Preferences = {
  version: 1,
  theme: 'system',
  zoom: 100,
  outline: false,
  recent: [],
};

export function validatePreferences(value: unknown): Preferences {
  if (
    !value ||
    typeof value !== 'object' ||
    !('version' in value) ||
    value.version !== 1
  )
    return { ...defaults, recent: [] };
  const v = value as Partial<Preferences>;
  const recent = Array.isArray(v.recent)
    ? v.recent
        .filter(
          (p): p is ReadingPosition =>
            !!p &&
            typeof p.path === 'string' &&
            p.path.length < 8192 &&
            typeof p.heading === 'string' &&
            typeof p.previous === 'string' &&
            Number.isFinite(p.offset) &&
            Number.isFinite(p.progress) &&
            p.progress >= 0 &&
            p.progress <= 1,
        )
        .slice(0, 100)
    : [];
  return {
    version: 1,
    theme: v.theme === 'dark' || v.theme === 'light' ? v.theme : 'system',
    zoom:
      typeof v.zoom === 'number' && Number.isFinite(v.zoom)
        ? Math.round(Math.max(80, Math.min(200, v.zoom)) / 10) * 10
        : 100,
    outline: v.outline === true,
    recent,
  };
}
export function loadPreferences(): Preferences {
  try {
    return validatePreferences(JSON.parse(localStorage.getItem(KEY) || 'null'));
  } catch {
    return { ...defaults, recent: [] };
  }
}
export function savePreferences(preferences: Preferences): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(validatePreferences(preferences)));
  } catch {
    /* Storage is optional. */
  }
}
export function rememberPosition(
  preferences: Preferences,
  position: ReadingPosition,
): Preferences {
  return {
    ...preferences,
    recent: [
      position,
      ...preferences.recent.filter((p) => p.path !== position.path),
    ].slice(0, 100),
  };
}
