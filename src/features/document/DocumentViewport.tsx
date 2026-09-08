import { memo, useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { RenderDocument } from './controller';
import type { DocumentGateway } from '../../platform/gateway';
import type { ReadingPosition } from '../preferences/preferences';
import {
  findRanges,
  hasHighlights,
  indexText,
  paintRanges,
  type TextIndex,
} from '../search';
import { highlightCode } from './highlight';

export interface ViewportActions {
  jump(id: string): void;
  selectAll(): void;
}
interface Props {
  document: RenderDocument;
  gateway: DocumentGateway;
  zoom: number;
  query: string;
  matchStep: number;
  actions: React.RefObject<ViewportActions | null>;
  position?: ReadingPosition;
  onPosition(position: ReadingPosition): void;
  onHeading(id: string): void;
  onMatches(count: number, active: number): void;
  onLink(path: string, fragment?: string): void;
  onNotice(message: string): void;
}

export const DocumentViewport = memo(function DocumentViewport(props: Props) {
  const { document: doc, query, matchStep, zoom, actions, gateway } = props;
  const root = useRef<HTMLElement>(null);
  const scroll = useRef<HTMLDivElement>(null);
  const overlay = useRef<HTMLDivElement>(null);
  const latest = useRef(props);
  latest.current = props;
  const position = useRef<ReadingPosition | undefined>(undefined);
  const headings = useRef<{ id: string; top: number }[]>([]);
  const index = useRef<TextIndex | undefined>(undefined);
  const ranges = useRef<Range[]>([]);
  const current = useRef(0);
  const [textVersion, setTextVersion] = useState(0);
  const navigation = useRef(0);
  const cleanupPaint = useRef<() => void>(() => {});
  const resumeHighlight = useRef<() => void>(() => {});

  useLayoutEffect(() => {
    const article = root.current!;
    const viewport = scroll.current!;
    const insertionStart = performance.now();
    let cancelled = false;
    let measureFrame = 0;
    let scrollFrame = 0;
    let saveTimer: ReturnType<typeof setTimeout>;
    const desired =
      position.current?.path === doc.file.path
        ? position.current
        : latest.current.position;
    // This is the sole HTML insertion boundary. doc.html is already sanitized.
    article.innerHTML = doc.html;
    article.querySelectorAll('table').forEach((table) => {
      const wrap = document.createElement('div');
      wrap.className = 'table-scroll';
      wrap.tabIndex = 0;
      wrap.setAttribute('role', 'region');
      wrap.setAttribute('aria-label', 'Tabelle, horizontal scrollbar');
      table.replaceWith(wrap);
      wrap.append(table);
    });
    article.querySelectorAll('pre').forEach((pre) => {
      const code = pre.querySelector('code');
      if (!code) return;
      const original = code.textContent || '';
      const button = document.createElement('button');
      button.type = 'button';
      button.className = 'copy-code';
      button.textContent = 'Kopieren';
      button.setAttribute('aria-label', 'Codeblock kopieren');
      button.addEventListener('click', () => {
        void gateway
          .copy(original)
          .then(() => {
            if (!cancelled) latest.current.onNotice('Code kopiert.');
          })
          .catch(() =>
            latest.current.onNotice(
              'Kopieren ist nicht verfügbar. Bitte den Text auswählen und Strg+C verwenden.',
            ),
          );
      });
      pre.append(button);
      pre.tabIndex = 0;
    });
    const headingElements = Array.from(
      article.querySelectorAll<HTMLElement>(
        'h1[id],h2[id],h3[id],h4[id],h5[id],h6[id]',
      ),
    );
    const measure = () => {
      const base = viewport.getBoundingClientRect().top;
      headings.current = headingElements.map((h) => ({
        id: h.id,
        top: h.getBoundingClientRect().top - base + viewport.scrollTop,
      }));
    };
    const restore = (saved?: ReadingPosition) => {
      if (!saved) {
        viewport.scrollTop = 0;
        return;
      }
      const heading =
        headings.current.find((h) => h.id === saved.heading) ||
        headings.current.find((h) => h.id === saved.previous);
      viewport.scrollTop = heading
        ? heading.top - saved.offset
        : saved.progress *
          Math.max(0, viewport.scrollHeight - viewport.clientHeight);
    };
    const capture = () => {
      const list = headings.current;
      let low = 0;
      let high = list.length - 1;
      let found = -1;
      while (low <= high) {
        const mid = (low + high) >> 1;
        if (list[mid].top <= viewport.scrollTop + 96) {
          found = mid;
          low = mid + 1;
        } else high = mid - 1;
      }
      const h = list[found];
      const saved: ReadingPosition = {
        path: doc.file.path,
        heading: h?.id || '',
        previous: list[found - 1]?.id || '',
        offset: h ? h.top - viewport.scrollTop : 0,
        progress:
          viewport.scrollTop /
          Math.max(1, viewport.scrollHeight - viewport.clientHeight),
      };
      position.current = saved;
      latest.current.onHeading(h?.id || list[0]?.id || '');
      return saved;
    };
    measure();
    restore(desired);
    capture();
    const initialNavigation = navigation.current;
    const resize = new ResizeObserver(() => {
      cancelAnimationFrame(measureFrame);
      measureFrame = requestAnimationFrame(() => {
        measure();
        if (navigation.current === initialNavigation && desired)
          restore(desired);
        capture();
      });
    });
    resize.observe(article);
    const onScroll = () => {
      cancelAnimationFrame(scrollFrame);
      scrollFrame = requestAnimationFrame(() => {
        capture();
        clearTimeout(saveTimer);
        saveTimer = setTimeout(() => {
          if (position.current) latest.current.onPosition(position.current);
        }, 250);
        drawFallback();
      });
    };
    const persist = () => latest.current.onPosition(capture());
    window.addEventListener('pagehide', persist);
    const userNavigation = () => {
      navigation.current++;
    };
    viewport.addEventListener('scroll', onScroll, { passive: true });
    for (const name of ['wheel', 'touchstart', 'pointerdown', 'keydown'])
      viewport.addEventListener(name, userNavigation, { passive: true });
    const imageError = (event: Event) => {
      if (event.target instanceof HTMLImageElement) {
        event.target.removeAttribute('src');
        event.target.dataset.unavailable = '';
        event.target.alt = `${event.target.alt || 'Bild'} — nicht verfügbar`;
      }
    };
    article.addEventListener('error', imageError, true);
    const toggle = () => {
      index.current = undefined;
      setTextVersion((v) => v + 1);
    };
    article.addEventListener('toggle', toggle, true);
    const highlightObserver = new IntersectionObserver(
      (entries) => {
        for (const entry of entries)
          if (entry.isIntersecting) {
            const code = entry.target as HTMLElement;
            highlightObserver.unobserve(code);
            // Yield between blocks. Do not mutate text nodes during an active selection/search.
            queue.push(code);
            scheduleHighlight();
          }
      },
      { root: viewport, rootMargin: '250px' },
    );
    const queue: HTMLElement[] = [];
    let highlightTimer: ReturnType<typeof setTimeout> | undefined;
    const scheduleHighlight = () => {
      if (
        highlightTimer ||
        !queue.length ||
        cancelled ||
        latest.current.query ||
        !getSelection()?.isCollapsed
      )
        return;
      highlightTimer = setTimeout(() => {
        highlightTimer = undefined;
        if (cancelled) return;
        const code = queue.shift()!;
        if (latest.current.query || !getSelection()?.isCollapsed) {
          queue.unshift(code);
          return;
        }
        void highlightCode(code, () => !cancelled && !latest.current.query)
          .then((changed) => {
            if (changed && !cancelled) {
              index.current = undefined;
              setTextVersion((v) => v + 1);
            }
            scheduleHighlight();
          })
          .catch(() => {
            scheduleHighlight();
          });
      }, 16);
    };
    resumeHighlight.current = scheduleHighlight;
    document.addEventListener('selectionchange', scheduleHighlight);
    article
      .querySelectorAll<HTMLElement>('pre code')
      .forEach((code) => highlightObserver.observe(code));
    index.current = undefined;
    setTextVersion((v) => v + 1);
    const paintFrame = requestAnimationFrame(() =>
      requestAnimationFrame(() => {
        if (!cancelled)
          performance.measure('hashline.content-to-frame', {
            start: insertionStart,
            end: performance.now(),
          });
      }),
    );
    const api: ViewportActions = {
      jump(id) {
        navigation.current++;
        let decoded: string;
        try {
          decoded = decodeURIComponent(id.replace(/^#/, ''));
        } catch {
          decoded = id;
        }
        const target = headingElements.find(
          (h) => h.id === decoded || h.id === `doc-${decoded}`,
        );
        if (target) {
          viewport.scrollTop +=
            target.getBoundingClientRect().top -
            viewport.getBoundingClientRect().top -
            28;
          target.tabIndex = -1;
          target.focus({ preventScroll: true });
        } else if (!decoded) viewport.scrollTop = 0;
        else latest.current.onNotice('Dieser Abschnitt wurde nicht gefunden.');
      },
      selectAll() {
        const selection = getSelection();
        const range = document.createRange();
        range.selectNodeContents(article);
        selection?.removeAllRanges();
        selection?.addRange(range);
      },
    };
    actions.current = api;
    return () => {
      cancelled = true;
      capture();
      if (position.current) latest.current.onPosition(position.current);
      clearTimeout(saveTimer);
      clearTimeout(highlightTimer);
      cancelAnimationFrame(measureFrame);
      cancelAnimationFrame(scrollFrame);
      cancelAnimationFrame(paintFrame);
      resize.disconnect();
      highlightObserver.disconnect();
      document.removeEventListener('selectionchange', scheduleHighlight);
      if (resumeHighlight.current === scheduleHighlight)
        resumeHighlight.current = () => {};
      cleanupPaint.current();
      viewport.removeEventListener('scroll', onScroll);
      window.removeEventListener('pagehide', persist);
      for (const name of ['wheel', 'touchstart', 'pointerdown', 'keydown'])
        viewport.removeEventListener(name, userNavigation);
      article.removeEventListener('error', imageError, true);
      article.removeEventListener('toggle', toggle, true);
      if (actions.current === api) actions.current = null;
    };
  }, [doc, actions, gateway]);

  function drawFallback() {
    const layer = overlay.current;
    if (!layer) return;
    layer.replaceChildren();
    if (hasHighlights()) return;
    const range = ranges.current[current.current];
    if (!range || !scroll.current) return;
    const base = scroll.current.getBoundingClientRect();
    for (const rect of range.getClientRects()) {
      const mark = document.createElement('span');
      Object.assign(mark.style, {
        left: `${rect.left - base.left}px`,
        top: `${rect.top - base.top}px`,
        width: `${rect.width}px`,
        height: `${rect.height}px`,
      });
      layer.append(mark);
    }
  }

  useEffect(() => {
    if (!query) resumeHighlight.current();
    const timer = setTimeout(() => {
      cleanupPaint.current();
      if (query && !index.current) index.current = indexText(root.current!);
      ranges.current =
        query && index.current ? findRanges(index.current, query) : [];
      current.current = 0;
      cleanupPaint.current = paintRanges(ranges.current, 0);
      latest.current.onMatches(ranges.current.length, 0);
      const range = ranges.current[0];
      if (range && scroll.current) {
        navigation.current++;
        scroll.current.scrollTop +=
          range.getBoundingClientRect().top -
          scroll.current.getBoundingClientRect().top -
          scroll.current.clientHeight / 3;
      }
      drawFallback();
    }, 50);
    return () => clearTimeout(timer);
  }, [query, doc, textVersion]);

  const previousStep = useRef(matchStep);
  useEffect(() => {
    const delta = matchStep - previousStep.current;
    previousStep.current = matchStep;
    if (!delta || !ranges.current.length) return;
    current.current =
      (current.current +
        (delta % ranges.current.length) +
        ranges.current.length) %
      ranges.current.length;
    cleanupPaint.current();
    cleanupPaint.current = paintRanges(ranges.current, current.current);
    const range = ranges.current[current.current];
    const viewport = scroll.current!;
    navigation.current++;
    viewport.scrollTop +=
      range.getBoundingClientRect().top -
      viewport.getBoundingClientRect().top -
      viewport.clientHeight / 3;
    latest.current.onMatches(ranges.current.length, current.current);
    drawFallback();
  }, [matchStep]);

  return (
    <div className="viewport-shell">
      <div
        className="document-scroll"
        ref={scroll}
        tabIndex={0}
        aria-label="Dokument"
      >
        <article
          className="markdown"
          ref={root}
          style={{ fontSize: `${(17 * zoom) / 100}px` }}
          onClick={(event) => {
            const link = (event.target as Element).closest<HTMLAnchorElement>(
              'a[data-link]',
            );
            if (!link) return;
            event.preventDefault();
            const href = link.dataset.link!;
            if (href.startsWith('#')) {
              actions.current?.jump(href);
              return;
            }
            void gateway
              .followLink(doc.file, href)
              .then((result) => {
                if (latest.current.document === doc && result.path)
                  latest.current.onLink(result.path, result.fragment);
              })
              .catch((error: unknown) =>
                latest.current.onNotice(
                  error instanceof Error ? error.message : String(error),
                ),
              );
          }}
        />
      </div>
      <div ref={overlay} className="search-fallback" aria-hidden="true" />
    </div>
  );
});
