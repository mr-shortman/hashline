import type {
  DocumentController,
  DocumentState,
} from '../features/document/controller';
import { createViewport, type Viewport } from '../features/document/viewport';
import { createOutline, type Outline } from '../features/outline/outline';
import { createSearchBar, type SearchBar } from '../features/search/searchbar';
import {
  loadPreferences,
  rememberPosition,
  savePreferences,
  type Preferences,
  type ReadingPosition,
  type Theme,
} from '../features/preferences/preferences';
import type { DocumentGateway } from '../platform/gateway';
import { el } from '../ui/dom';
import { icon } from '../ui/icon';
import { createTitlebar } from '../ui/titlebar';

export function createApp(
  controller: DocumentController,
  gateway: DocumentGateway,
): { element: HTMLElement; destroy(): void } {
  let prefs: Preferences = loadPreferences();
  let state: DocumentState = controller.getSnapshot();
  let search = false;
  let query = '';
  let menu = false;
  let loadingTimer: ReturnType<typeof setTimeout> | undefined;
  let noticeTimer: ReturnType<typeof setTimeout> | undefined;
  let pendingFragment: { path: string; fragment: string } | undefined;
  let viewport: Viewport | undefined;
  let outline: Outline | undefined;
  let searchBar: SearchBar | undefined;
  let menuPanel: HTMLElement | undefined;
  let errorBanner: HTMLElement | undefined;
  let toast: HTMLElement | undefined;
  let emptyState: HTMLElement | undefined;
  let shownDocument = state.document;
  let activeHeading = '';

  const onPosition = (position: ReadingPosition) => {
    // Reading position is deliberately not UI state: scrolling does not redraw.
    prefs = rememberPosition(prefs, position);
    savePreferences(prefs);
  };

  const fileTitle = el('div', { class: 'file-title' });
  const fileName = document.createTextNode('Hashline');
  const loadingLabel = el('span', { class: 'loading-label', role: 'status' });
  fileTitle.append(fileName);
  const outlineTrigger = el(
    'button',
    {
      class: 'icon-button',
      'aria-label': 'Inhaltsverzeichnis',
      'aria-expanded': 'false',
      title: 'Inhaltsverzeichnis (Strg+Umschalt+O)',
      onclick: () => setPrefs({ outline: !prefs.outline }),
    },
    icon('outline'),
  );
  const searchTrigger = el(
    'button',
    {
      class: 'icon-button',
      'aria-label': 'Suche',
      'aria-expanded': 'false',
      title: 'Suche (Strg+F)',
      onclick: () => (search ? closeSearch() : openSearch()),
    },
    icon('search'),
  );
  const menuTrigger = el(
    'button',
    {
      class: 'icon-button',
      'aria-label': 'Darstellung und Optionen',
      'aria-expanded': 'false',
      onclick: () => setMenu(!menu),
    },
    icon('menu'),
  );
  const openButton = el(
    'button',
    {
      class: 'open-button',
      'aria-label': 'Öffnen',
      title: 'Datei öffnen (Strg+O)',
      onclick: () => void controller.choose(),
    },
    icon('open'),
    el('span', {}, 'Öffnen'),
  );

  const element = el('div', { class: 'app' });
  const titlebar = createTitlebar({
    onError: controller.notice,
    children: [
      openButton,
      fileTitle,
      el(
        'div',
        { class: 'toolbar-actions' },
        outlineTrigger,
        searchTrigger,
        menuTrigger,
      ),
    ],
  });
  titlebar.mount(element);
  // Comment anchors keep the optional regions in their original document order
  // without adding layout-affecting wrappers.
  const menuAnchor = document.createComment('menu');
  const searchAnchor = document.createComment('search');
  const errorAnchor = document.createComment('error');
  const main = el('main', { class: 'reader-layout' });
  element.append(menuAnchor, searchAnchor, errorAnchor, main);
  const outlineAnchor = document.createComment('outline');
  main.append(outlineAnchor);

  function setPrefs(patch: Partial<Preferences>): void {
    prefs = { ...prefs, ...patch };
    applyPreferences();
  }
  function applyPreferences(): void {
    document.documentElement.dataset.theme = prefs.theme;
    // Merge positions saved outside the UI before persisting.
    prefs = { ...prefs, recent: loadPreferences().recent };
    savePreferences(prefs);
    viewport?.setZoom(prefs.zoom);
    syncOutline();
    syncMenu();
    outlineTrigger.setAttribute('aria-expanded', String(prefs.outline));
  }
  function zoom(delta: number | 'reset'): void {
    setPrefs({
      zoom:
        delta === 'reset'
          ? 100
          : Math.min(200, Math.max(80, prefs.zoom + delta)),
    });
  }

  function openSearch(): void {
    if (search) return;
    search = true;
    searchTrigger.setAttribute('aria-expanded', 'true');
    searchBar = createSearchBar({
      query,
      setQuery: (value) => {
        query = value;
        viewport?.setQuery(value);
      },
      step: (delta) => viewport?.step(delta),
      close: closeSearch,
    });
    searchAnchor.before(searchBar.element);
    searchBar.setMatches(0, 0, false);
    searchBar.focus();
  }
  function closeSearch(): void {
    if (!search) return;
    search = false;
    searchTrigger.setAttribute('aria-expanded', 'false');
    searchBar?.destroy();
    searchBar = undefined;
    query = '';
    viewport?.setQuery('');
    searchTrigger.focus();
  }

  function closeOutline(): void {
    setPrefs({ outline: false });
    // WebKit refuses focus while the narrow dialog's background is still inert.
    requestAnimationFrame(() => {
      if (document.activeElement === document.body) outlineTrigger.focus();
    });
  }
  function syncOutline(): void {
    if (prefs.outline && !outline) {
      outline = createOutline({
        headings: state.document?.headings || [],
        active: activeHeading,
        close: closeOutline,
        jump: (id) => {
          viewport?.jump(id);
          if (window.matchMedia('(max-width: 899px)').matches) closeOutline();
        },
      });
      outlineAnchor.before(...outline.nodes);
      outline.nodes[1].querySelector<HTMLButtonElement>('button')?.focus();
    } else if (!prefs.outline && outline) {
      outline.destroy();
      outline = undefined;
    }
  }

  const dismissMenu = (event: PointerEvent) => {
    if (
      !menuPanel?.contains(event.target as Node) &&
      !menuTrigger.contains(event.target as Node)
    )
      setMenu(false);
  };
  function setMenu(open: boolean): void {
    if (menu === open) return;
    menu = open;
    menuTrigger.setAttribute('aria-expanded', String(menu));
    syncMenu();
  }
  function closeMenu(): void {
    setMenu(false);
    menuTrigger.focus();
  }
  let menuParts:
    | {
        themes: Map<Theme, HTMLButtonElement>;
        smaller: HTMLButtonElement;
        level: HTMLButtonElement;
        larger: HTMLButtonElement;
        reload: HTMLButtonElement;
        copyPath: HTMLButtonElement;
      }
    | undefined;
  function syncMenu(): void {
    if (!menu) {
      if (menuPanel) {
        window.removeEventListener('pointerdown', dismissMenu);
        menuPanel.remove();
        menuPanel = undefined;
        menuParts = undefined;
      }
      return;
    }
    if (!menuPanel) buildMenu();
    const parts = menuParts!;
    for (const [theme, button] of parts.themes)
      button.setAttribute('aria-pressed', String(prefs.theme === theme));
    parts.smaller.disabled = prefs.zoom === 80;
    parts.larger.disabled = prefs.zoom === 200;
    parts.level.textContent = `${prefs.zoom} %`;
    parts.reload.disabled = !state.document;
    parts.copyPath.hidden = !state.document;
  }
  function buildMenu(): void {
    const themes: [Theme, string][] = [
      ['system', 'System'],
      ['light', 'Hell'],
      ['dark', 'Dunkel'],
    ];
    const buttons = new Map<Theme, HTMLButtonElement>(
      themes.map(([theme, label]) => [
        theme,
        el(
          'button',
          { 'aria-pressed': 'false', onclick: () => setPrefs({ theme }) },
          label,
        ),
      ]),
    );
    const smaller = el(
      'button',
      { 'aria-label': 'Text verkleinern', onclick: () => zoom(-10) },
      '−',
    );
    const level = el('button', {
      title: 'Textgröße zurücksetzen',
      onclick: () => zoom('reset'),
    });
    const larger = el(
      'button',
      { 'aria-label': 'Text vergrößern', onclick: () => zoom(10) },
      '+',
    );
    const reload = el(
      'button',
      {
        class: 'menu-row',
        onclick: () => {
          void controller.reload();
          closeMenu();
        },
      },
      'Neu laden ',
      el('kbd', {}, 'Strg R'),
    );
    const copyPath = el(
      'button',
      {
        class: 'menu-row',
        onclick: () => {
          const path = state.document?.file.path;
          if (path === undefined) return;
          void gateway
            .copy(path)
            .then(() => controller.notice('Dateipfad kopiert.'))
            .catch(() =>
              controller.notice('Dateipfad konnte nicht kopiert werden.'),
            );
          closeMenu();
        },
      },
      'Dateipfad kopieren',
    );
    const panel = el(
      'div',
      {
        class: 'settings',
        role: 'dialog',
        'aria-label': 'Darstellung und Optionen',
        onkeydown: ((event: KeyboardEvent) => {
          if (event.key !== 'Tab') return;
          const focusable = Array.from(
            panel.querySelectorAll<HTMLButtonElement>('button:not(:disabled)'),
          ).filter((button) => !button.hidden);
          const first = focusable[0];
          const last = focusable[focusable.length - 1];
          if (event.shiftKey && document.activeElement === first) {
            event.preventDefault();
            last?.focus();
          } else if (!event.shiftKey && document.activeElement === last) {
            event.preventDefault();
            first?.focus();
          }
        }) as EventListener,
      },
      el('p', { class: 'settings-label' }, 'Darstellung'),
      el('div', { class: 'theme-options' }, ...buttons.values()),
      el(
        'div',
        { class: 'zoom-options' },
        el('span', {}, 'Textgröße'),
        smaller,
        level,
        larger,
      ),
      reload,
      copyPath,
      el(
        'div',
        { class: 'app-credit' },
        'Hashline ',
        el('span', {}, 'Markdown Viewer · 0.1.0'),
      ),
    );
    menuPanel = panel;
    menuParts = {
      themes: buttons,
      smaller,
      level,
      larger,
      reload,
      copyPath,
    };
    menuAnchor.before(panel);
    window.addEventListener('pointerdown', dismissMenu);
    panel.querySelector<HTMLElement>('button')?.focus();
  }

  function syncError(): void {
    if (!state.error) {
      errorBanner?.remove();
      errorBanner = undefined;
      return;
    }
    if (!errorBanner) {
      errorBanner = el(
        'div',
        { class: 'error-banner', role: 'alert' },
        el('span', {}),
        el(
          'button',
          { onclick: () => void controller.retry() },
          'Erneut versuchen',
        ),
      );
      errorAnchor.before(errorBanner);
    }
    errorBanner.firstElementChild!.textContent = state.error;
  }

  let shownNotice: string | undefined;
  function syncToast(): void {
    if (state.notice === shownNotice) return;
    shownNotice = state.notice;
    clearTimeout(noticeTimer);
    if (!state.notice) {
      toast?.remove();
      toast = undefined;
      return;
    }
    if (!toast) {
      toast = el(
        'div',
        { class: 'toast', role: 'status' },
        el('span', {}),
        el(
          'button',
          {
            class: 'icon-button',
            'aria-label': 'Hinweis schließen',
            onclick: () => controller.notice(undefined),
          },
          icon('close'),
        ),
      );
      element.append(toast);
    }
    toast.firstElementChild!.textContent = state.notice;
    noticeTimer = setTimeout(() => controller.notice(undefined), 7000);
  }

  // Mirrors the former effect: the delay restarts only when the busy state
  // itself changes, never on unrelated updates.
  let busyState = '';
  function syncLoading(): void {
    const busy = state.status === 'loading' || state.refreshing;
    const next = busy ? (state.refreshing ? 'refreshing' : 'loading') : '';
    if (next === busyState) return;
    busyState = next;
    clearTimeout(loadingTimer);
    if (!busy) {
      loadingLabel.remove();
      return;
    }
    loadingTimer = setTimeout(() => {
      loadingLabel.textContent = state.refreshing
        ? 'Aktualisieren …'
        : 'Öffnen …';
      fileTitle.append(loadingLabel);
    }, 150);
  }

  function syncDocument(): void {
    const doc = state.document;
    fileName.data = doc?.file.name || 'Hashline';
    fileTitle.title = doc?.file.path || 'Hashline — Markdown Viewer';
    if (doc) {
      emptyState?.remove();
      emptyState = undefined;
      if (!viewport) {
        viewport = createViewport({
          gateway,
          zoom: prefs.zoom,
          position: (path) => prefs.recent.find((p) => p.path === path),
          onPosition,
          onHeading: (id) => {
            activeHeading = id;
            outline?.setActive(id);
          },
          onMatches: (count, active, pending = false) =>
            searchBar?.setMatches(count, active, pending),
          onLink: (path, fragment) => {
            pendingFragment = { path, fragment: fragment || '' };
            void controller.open(path);
          },
          onNotice: controller.notice,
        });
        main.append(viewport.element);
        viewport.setQuery(query);
      }
      viewport.setDocument(doc);
    } else {
      viewport?.destroy();
      viewport = undefined;
      if (!emptyState) {
        emptyState = el(
          'div',
          { class: 'empty-state' },
          el('div', { class: 'brand-mark', 'aria-hidden': 'true' }, '#'),
          el('h1', {}, 'Raum zum Lesen.'),
          el('p', {}, 'Dein Markdown. Klar und ungestört.'),
          el(
            'button',
            {
              class: 'primary-button',
              onclick: () => void controller.choose(),
            },
            icon('open'),
            'Markdown-Datei öffnen',
          ),
          el(
            'span',
            { class: 'drop-hint' },
            'Oder eine Datei hierher ziehen ',
            el('span', {}, '·'),
            ' ',
            el('kbd', {}, 'Strg O'),
          ),
        );
        main.append(emptyState);
      }
    }
    if (doc !== shownDocument) {
      shownDocument = doc;
      outline?.setHeadings(doc?.headings || []);
      if (pendingFragment && doc?.file.path === pendingFragment.path) {
        viewport?.jump(pendingFragment.fragment);
        pendingFragment = undefined;
      }
    }
  }

  function sync(): void {
    state = controller.getSnapshot();
    syncDocument();
    syncError();
    syncToast();
    syncLoading();
    if (menu) syncMenu();
  }

  const unsubscribe = controller.subscribe(sync);
  const keydown = (event: KeyboardEvent) => {
    const input =
      event.target instanceof HTMLElement &&
      !!event.target.closest('input,textarea,[contenteditable=true]');
    if (event.key === 'Escape') {
      if (menu) closeMenu();
      else if (prefs.outline && window.matchMedia('(max-width: 899px)').matches)
        closeOutline();
      else if (search) closeSearch();
      else if (prefs.outline) closeOutline();
      return;
    }
    if (!event.ctrlKey || event.altKey) return;
    if (
      prefs.outline &&
      window.matchMedia('(max-width: 899px)').matches &&
      ['f', 'o'].includes(event.key.toLowerCase()) &&
      !event.shiftKey
    )
      closeOutline();
    const key = event.key.toLowerCase();
    if (key === 'o') {
      event.preventDefault();
      if (event.shiftKey) setPrefs({ outline: !prefs.outline });
      else void controller.choose();
    } else if (key === 'f') {
      event.preventDefault();
      openSearch();
      searchBar?.focus();
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
      viewport?.selectAll();
    }
  };
  window.addEventListener('keydown', keydown);

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

  document.documentElement.dataset.theme = prefs.theme;
  sync();
  // The outline marks its background inert, so it may only be built once the
  // regions it reaches for exist.
  applyPreferences();

  return {
    element,
    destroy() {
      stopped = true;
      unlisten?.();
      unsubscribe();
      window.removeEventListener('keydown', keydown);
      window.removeEventListener('pointerdown', dismissMenu);
      clearTimeout(loadingTimer);
      clearTimeout(noticeTimer);
      viewport?.destroy();
      outline?.destroy();
      searchBar?.destroy();
      titlebar.destroy();
      element.remove();
    },
  };
}
