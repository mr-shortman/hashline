import {
  useCallback,
  useEffect,
  useRef,
  useState,
  useSyncExternalStore,
} from 'react';
import { DocumentController } from '../features/document/controller';
import {
  DocumentViewport,
  type ViewportActions,
} from '../features/document/DocumentViewport';
import { Outline } from '../features/outline/Outline';
import { SearchBar } from '../features/search/SearchBar';
import {
  loadPreferences,
  rememberPosition,
  savePreferences,
  type ReadingPosition,
  type Theme,
} from '../features/preferences/preferences';
import type { DocumentGateway } from '../platform/gateway';
import { Icon } from '../ui/Icon';

export function App({
  controller,
  gateway,
}: {
  controller: DocumentController;
  gateway: DocumentGateway;
}) {
  const state = useSyncExternalStore(
    controller.subscribe,
    controller.getSnapshot,
  );
  const [prefs, setPrefs] = useState(loadPreferences);
  const prefsRef = useRef(prefs);
  prefsRef.current = { ...prefs, recent: prefsRef.current.recent };
  const [search, setSearch] = useState(false);
  const [query, setQuery] = useState('');
  const [matchStep, setMatchStep] = useState(0);
  const [matches, setMatches] = useState({ count: 0, active: 0 });
  const [activeHeading, setActiveHeading] = useState('');
  const [menu, setMenu] = useState(false);
  const [loading, setLoading] = useState(false);
  const viewport = useRef<ViewportActions | null>(null);
  const searchTrigger = useRef<HTMLButtonElement>(null);
  const outlineTrigger = useRef<HTMLButtonElement>(null);
  const menuTrigger = useRef<HTMLButtonElement>(null);
  const menuRoot = useRef<HTMLDivElement>(null);
  const pendingFragment = useRef<
    { path: string; fragment: string } | undefined
  >(undefined);
  const onPosition = useCallback((position: ReadingPosition) => {
    const next = rememberPosition(prefsRef.current, position);
    prefsRef.current = next;
    savePreferences(next);
    // Reading position is deliberately not React state: scrolling does not rerender UI.
  }, []);
  const onMatches = useCallback(
    (count: number, active: number) => setMatches({ count, active }),
    [],
  );
  const onLink = useCallback(
    (path: string, fragment?: string) => {
      pendingFragment.current = { path, fragment: fragment || '' };
      void controller.open(path);
    },
    [controller],
  );
  const closeSearch = useCallback(() => {
    setSearch(false);
    setQuery('');
    searchTrigger.current?.focus();
  }, []);
  const closeOutline = useCallback(() => {
    setPrefs((p) => ({ ...p, outline: false }));
    outlineTrigger.current?.focus();
  }, []);
  const closeMenu = useCallback(() => {
    setMenu(false);
    menuTrigger.current?.focus();
  }, []);
  const zoom = useCallback(
    (delta: number | 'reset') =>
      setPrefs((p) => ({
        ...p,
        zoom:
          delta === 'reset' ? 100 : Math.min(200, Math.max(80, p.zoom + delta)),
      })),
    [],
  );

  useEffect(() => {
    let stopped = false;
    let unlisten: (() => void) | undefined;
    void gateway
      .subscribeOpen(({ paths }) => {
        void controller.openPaths(paths);
      })
      .then((stop) => {
        if (stopped) stop();
        else unlisten = stop;
      })
      .catch(() =>
        controller.notice(
          'Dateiübergabe ist nicht verfügbar. Bitte den Dateidialog verwenden.',
        ),
      );
    return () => {
      stopped = true;
      unlisten?.();
    };
  }, [controller, gateway]);
  useEffect(() => {
    document.documentElement.dataset.theme = prefs.theme;
    // Merge positions saved outside React before persisting UI preferences.
    const next = { ...prefs, recent: loadPreferences().recent };
    prefsRef.current = next;
    savePreferences(next);
  }, [prefs]);
  useEffect(() => {
    if (state.status !== 'loading' && !state.refreshing) {
      setLoading(false);
      return;
    }
    const timer = setTimeout(() => setLoading(true), 150);
    return () => clearTimeout(timer);
  }, [state.status, state.refreshing]);
  useEffect(() => {
    if (!state.notice) return;
    const timer = setTimeout(() => controller.notice(undefined), 7000);
    return () => clearTimeout(timer);
  }, [state.notice, controller]);
  useEffect(() => {
    const pending = pendingFragment.current;
    if (pending && state.document?.file.path === pending.path) {
      viewport.current?.jump(pending.fragment);
      pendingFragment.current = undefined;
    }
  }, [state.document]);
  useEffect(() => {
    if (!menu) return;
    menuRoot.current?.querySelector<HTMLElement>('button')?.focus();
    const dismiss = (e: PointerEvent) => {
      if (
        !menuRoot.current?.contains(e.target as Node) &&
        !menuTrigger.current?.contains(e.target as Node)
      )
        setMenu(false);
    };
    window.addEventListener('pointerdown', dismiss);
    return () => window.removeEventListener('pointerdown', dismiss);
  }, [menu]);
  useEffect(() => {
    const keydown = (event: KeyboardEvent) => {
      const input =
        event.target instanceof HTMLElement &&
        !!event.target.closest('input,textarea,[contenteditable=true]');
      if (event.key === 'Escape') {
        if (menu) closeMenu();
        else if (
          prefs.outline &&
          window.matchMedia('(max-width: 899px)').matches
        )
          closeOutline();
        else if (search) closeSearch();
        else if (prefs.outline) closeOutline();
        return;
      }
      if (!event.ctrlKey || event.altKey) return;
      const key = event.key.toLowerCase();
      if (key === 'o') {
        event.preventDefault();
        if (event.shiftKey) setPrefs((p) => ({ ...p, outline: !p.outline }));
        else void controller.choose();
      } else if (key === 'f') {
        event.preventDefault();
        setSearch(true);
      } else if (key === 'r') {
        event.preventDefault();
        void controller.reload();
      } else if (key === '+' || key === '=') {
        event.preventDefault();
        zoom(10);
      } else if (key === '-') {
        event.preventDefault();
        zoom(-10);
      } else if (key === '0') {
        event.preventDefault();
        zoom('reset');
      } else if (key === 'a' && !input) {
        event.preventDefault();
        viewport.current?.selectAll();
      }
    };
    window.addEventListener('keydown', keydown);
    return () => window.removeEventListener('keydown', keydown);
  }, [
    controller,
    search,
    menu,
    prefs.outline,
    closeSearch,
    closeOutline,
    closeMenu,
    zoom,
  ]);

  return (
    <div className="app">
      <header className="toolbar">
        <button
          className="open-button"
          title="Datei öffnen (Strg+O)"
          onClick={() => {
            void controller.choose();
          }}
        >
          <Icon name="open" />
          <span>Öffnen</span>
        </button>
        <div
          className="file-title"
          title={state.document?.file.path || 'Hashline — Markdown Viewer'}
        >
          {state.document?.file.name || 'Hashline'}
          {loading && (
            <span className="loading-label" role="status">
              {state.refreshing ? 'Aktualisieren …' : 'Öffnen …'}
            </span>
          )}
        </div>
        <div className="toolbar-actions">
          <button
            ref={outlineTrigger}
            className="icon-button"
            aria-label="Inhaltsverzeichnis"
            aria-expanded={prefs.outline}
            title="Inhaltsverzeichnis (Strg+Umschalt+O)"
            onClick={() => setPrefs((p) => ({ ...p, outline: !p.outline }))}
          >
            <Icon name="outline" />
          </button>
          <button
            ref={searchTrigger}
            className="icon-button"
            aria-label="Suche"
            aria-expanded={search}
            title="Suche (Strg+F)"
            onClick={() => (search ? closeSearch() : setSearch(true))}
          >
            <Icon name="search" />
          </button>
          <button
            ref={menuTrigger}
            className="icon-button"
            aria-label="Darstellung und Optionen"
            aria-expanded={menu}
            onClick={() => setMenu((v) => !v)}
          >
            <Icon name="menu" />
          </button>
        </div>
      </header>
      {menu && (
        <div
          className="settings"
          ref={menuRoot}
          role="dialog"
          aria-label="Darstellung und Optionen"
        >
          <p className="settings-label">Darstellung</p>
          <div className="theme-options">
            {(
              [
                ['system', 'System'],
                ['light', 'Hell'],
                ['dark', 'Dunkel'],
              ] as [Theme, string][]
            ).map(([theme, label]) => (
              <button
                key={theme}
                aria-pressed={prefs.theme === theme}
                onClick={() => setPrefs((p) => ({ ...p, theme }))}
              >
                {label}
              </button>
            ))}
          </div>
          <div className="zoom-options">
            <span>Textgröße</span>
            <button
              aria-label="Text verkleinern"
              disabled={prefs.zoom === 80}
              onClick={() => zoom(-10)}
            >
              −
            </button>
            <button
              title="Textgröße zurücksetzen"
              onClick={() => zoom('reset')}
            >
              {prefs.zoom} %
            </button>
            <button
              aria-label="Text vergrößern"
              disabled={prefs.zoom === 200}
              onClick={() => zoom(10)}
            >
              +
            </button>
          </div>
          <button
            className="menu-row"
            disabled={!state.document}
            onClick={() => {
              void controller.reload();
              closeMenu();
            }}
          >
            Neu laden <kbd>Strg R</kbd>
          </button>
          {state.document && (
            <button
              className="menu-row"
              onClick={() => {
                void gateway
                  .copy(state.document!.file.path)
                  .then(() => controller.notice('Dateipfad kopiert.'))
                  .catch(() =>
                    controller.notice('Dateipfad konnte nicht kopiert werden.'),
                  );
                closeMenu();
              }}
            >
              Dateipfad kopieren
            </button>
          )}
          <div className="app-credit">
            Hashline <span>Markdown Viewer · 0.1.0</span>
          </div>
        </div>
      )}
      {search && (
        <SearchBar
          query={query}
          setQuery={setQuery}
          count={matches.count}
          active={matches.active}
          step={(delta) => setMatchStep((v) => v + delta)}
          close={closeSearch}
        />
      )}
      {state.error && (
        <div className="error-banner" role="alert">
          <span>{state.error}</span>
          <button
            onClick={() => {
              void controller.retry();
            }}
          >
            Erneut versuchen
          </button>
        </div>
      )}
      <main className="reader-layout">
        {prefs.outline && (
          <Outline
            headings={state.document?.headings || []}
            active={activeHeading}
            close={closeOutline}
            jump={(id) => {
              viewport.current?.jump(id);
              if (window.matchMedia('(max-width: 899px)').matches)
                closeOutline();
            }}
          />
        )}
        {state.document ? (
          <DocumentViewport
            document={state.document}
            gateway={gateway}
            zoom={prefs.zoom}
            query={query}
            matchStep={matchStep}
            actions={viewport}
            position={prefsRef.current.recent.find(
              (p) => p.path === state.document!.file.path,
            )}
            onPosition={onPosition}
            onHeading={setActiveHeading}
            onMatches={onMatches}
            onLink={onLink}
            onNotice={controller.notice}
          />
        ) : (
          <div className="empty-state">
            <div className="brand-mark" aria-hidden="true">
              #
            </div>
            <h1>Raum zum Lesen.</h1>
            <p>Dein Markdown. Klar und ungestört.</p>
            <button
              className="primary-button"
              onClick={() => {
                void controller.choose();
              }}
            >
              <Icon name="open" />
              Markdown-Datei öffnen
            </button>
            <span className="drop-hint">
              Oder eine Datei hierher ziehen <span>·</span> <kbd>Strg O</kbd>
            </span>
          </div>
        )}
      </main>
      {state.notice && (
        <div className="toast" role="status">
          <span>{state.notice}</span>
          <button
            className="icon-button"
            aria-label="Hinweis schließen"
            onClick={() => controller.notice(undefined)}
          >
            <Icon name="close" />
          </button>
        </div>
      )}
    </div>
  );
}
