import { memo, useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { RenderDocument } from './controller';
import type { DocumentGateway } from '../../platform/gateway';
import type { ReadingPosition } from '../preferences/preferences';
import {
  findMatches,
  toRange,
  type Match,
  hasHighlights,
  indexText,
  paintRanges,
  type TextIndex,
} from '../search';
import { highlightCode } from './highlight';
import { sanitizeFragment, unavailableImage } from '../../core/content/policy';
import { nextTask, retireContent } from './schedule';

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
  onMatches(count: number, active: number, pending?: boolean): void;
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
  const retainedSections = useRef<{ html: string; shell: HTMLElement }[]>([]);
  const indexes = useRef(new WeakMap<HTMLElement, TextIndex>());
  const ranges = useRef<Match[]>([]);
  const visibleSections = useRef(new Set<Element>());
  const sectionRanges = useRef(new Map<Element, Match[]>());
  const current = useRef(0);
  const forcedSection = useRef<HTMLElement | null>(null);
  const [textVersion, setTextVersion] = useState(0);
  const navigation = useRef(0);
  const [remoteState, setRemoteState] = useState<{
    id: string;
    status: 'blocked' | 'loading' | 'allowed';
    count: number;
  }>({ id: '', status: 'blocked', count: 0 });
  const remoteAccess = useRef({ id: '', allowed: false });
  const loadRemoteImages = (container: ParentNode) => {
    for (const img of container.querySelectorAll<HTMLImageElement>(
      'img[data-remote-source]',
    )) {
      const url = gateway.imageUrl(doc.file, img.dataset.remoteSource!);
      if (!url?.startsWith(`hashline-image://localhost/${doc.file.id}/`))
        continue;
      img.alt = img.dataset.remoteAlt || 'Bild';
      img.removeAttribute('data-unavailable');
      img.hidden = false;
      img.removeAttribute('aria-hidden');
      if (img.nextElementSibling?.classList.contains('image-placeholder'))
        img.nextElementSibling.remove();
      img.removeAttribute('data-remote-source');
      img.loading = 'lazy';
      img.decoding = 'async';
      img.src = url;
    }
  };
  const loadRemoteRef = useRef(loadRemoteImages);
  loadRemoteRef.current = loadRemoteImages;
  async function allowRemoteImages() {
    if (!gateway.allowRemoteImages) return;
    setRemoteState((state) => ({ ...state, status: 'loading' }));
    try {
      await gateway.allowRemoteImages(doc.file);
      if (latest.current.document !== doc) return;
      remoteAccess.current = { id: doc.file.id, allowed: true };
      loadRemoteRef.current(root.current!);
      setRemoteState((state) => ({ ...state, status: 'allowed' }));
      scroll.current?.focus({ preventScroll: true });
      latest.current.onNotice(
        'Remote-Bilder für diese Dokumentversion freigegeben.',
      );
    } catch {
      if (latest.current.document !== doc) return;
      setRemoteState((state) => ({ ...state, status: 'blocked' }));
      latest.current.onNotice(
        'Remote-Bilder konnten nicht freigegeben werden. Bitte erneut versuchen.',
      );
    }
  }
  const cleanupPaint = useRef<() => void>(() => {});
  const resumeHighlight = useRef<() => void>(() => {});

  useLayoutEffect(() => {
    const preparation = new AbortController();
    const reusable = new Map<string, HTMLElement[]>();
    for (const { html, shell } of retainedSections.current.slice().reverse()) {
      if (
        shell.parentNode !== root.current ||
        shell.dataset.populated !== 'true' ||
        shell.dataset.hasImages === 'true'
      )
        continue;
      const candidates = reusable.get(html) || [];
      candidates.push(shell);
      reusable.set(html, candidates);
    }
    const planned = doc.sections.map(({ html }) => {
      const shell = reusable.get(html)?.pop() || document.createElement('div');
      shell.className = 'markdown-section';
      return { html, shell };
    });
    const keep = new Set(planned.map(({ shell }) => shell));
    root.current!.dataset.renderState = 'retiring';
    root.current!.setAttribute('aria-busy', 'true');
    let cleanup: (() => void) | undefined;
    let pendingJump: string | undefined;
    let pendingSelection = false;
    const waiting: ViewportActions = {
      jump(id) {
        pendingJump = id;
      },
      selectAll() {
        pendingSelection = true;
      },
    };
    actions.current = waiting;
    void (async () => {
      try {
        await retireContent(root.current!, preparation.signal, keep);
        if (preparation.signal.aborted) return;
        cleanup = mount();
        if (pendingJump !== undefined) actions.current?.jump(pendingJump);
        if (pendingSelection) actions.current?.selectAll();
      } catch (error) {
        if (!preparation.signal.aborted) latest.current.onNotice(String(error));
      }
    })();
    return () => {
      preparation.abort();
      cleanup?.();
      if (actions.current === waiting) actions.current = null;
    };

    function mount() {
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
      const abort = new AbortController();
      forcedSection.current?.style.removeProperty('content-visibility');
      forcedSection.current = null;
      remoteAccess.current = { id: doc.file.id, allowed: false };
      setRemoteState({ id: doc.file.id, status: 'blocked', count: 0 });
      article.dataset.renderState = 'rendering';
      article.setAttribute('aria-busy', 'true');
      visibleSections.current.clear();
      sectionRanges.current.clear();
      const shells = planned.map(({ shell }) => shell);
      // Keep already ordered nodes connected: moving them through a fragment
      // would synchronously destroy the render trees we are trying to retain.
      let cursor: ChildNode | null = article.firstChild;
      for (const shell of shells) {
        if (shell !== cursor) article.insertBefore(shell, cursor);
        cursor = shell.nextSibling;
      }
      retainedSections.current = planned;
      const rendered = new Set<number>();
      const headingSection = new Map<string, number>();
      doc.sections.forEach((section, i) => {
        for (const heading of section.headings)
          headingSection.set(heading.id, i);
      });
      let selectWholeDocument = false;
      let sanitizeMs = 0;
      let insertionMs = 0;
      let maxSliceMs = 0;
      let completed = false;
      const selectAll = () => {
        const selection = getSelection();
        const range = document.createRange();
        if (shells.length) {
          range.setStartBefore(shells[0]);
          range.setEndAfter(shells[shells.length - 1]);
        } else {
          range.selectNodeContents(article);
          range.collapse(true);
        }
        selection?.removeAllRanges();
        selection?.addRange(range);
      };
      const insert = (i: number) => {
        if (rendered.has(i) || cancelled) return;
        const start = performance.now();
        const section = doc.sections[i];
        const {
          fragment,
          remoteImages: count,
          hasImages,
          tables,
          pres,
        } = sanitizeFragment({ ...section, parseMs: 0 }, doc.file, gateway);
        if (count) {
          setRemoteState((state) => ({ ...state, count: state.count + count }));
          if (
            remoteAccess.current.id === doc.file.id &&
            remoteAccess.current.allowed
          )
            loadRemoteRef.current(fragment);
        }
        const sanitized = performance.now();
        sanitizeMs += sanitized - start;
        tables.forEach((table) => {
          const wrap = document.createElement('div');
          wrap.className = 'table-scroll';
          wrap.tabIndex = 0;
          wrap.setAttribute('role', 'region');
          wrap.setAttribute('aria-label', 'Tabelle, horizontal scrollbar');
          table.replaceWith(wrap);
          wrap.append(table);
        });
        pres.forEach((pre) => {
          const code = pre.querySelector('code');
          if (!code) return;
          const button = document.createElement('button');
          button.type = 'button';
          button.className = 'copy-code';
          button.textContent = 'Kopieren';
          button.setAttribute('aria-label', 'Codeblock kopieren');
          pre.append(button);
          pre.tabIndex = 0;
        });
        // Inspect the actual sanitized tree: HTML's legacy <image> spelling is
        // normalized to <img> too. Resource handles belong to this document.
        shells[i].dataset.hasImages = String(hasImages);
        shells[i].append(fragment);
        shells[i].dataset.populated = 'true';
        rendered.add(i);
        highlightObserver.observe(shells[i]);
        insertionMs += performance.now() - sanitized;
        maxSliceMs = Math.max(maxSliceMs, performance.now() - start);
        if (selectWholeDocument) selectAll();
      };
      const headingTop = (id: string) => {
        const i = headingSection.get(id);
        if (i === undefined) return undefined;
        insert(i);
        const heading = document.getElementById(id);
        if (!heading) return undefined;
        reveal(heading);
        return (
          heading.getBoundingClientRect().top -
          viewport.getBoundingClientRect().top +
          viewport.scrollTop
        );
      };
      const restore = (saved?: ReadingPosition) => {
        if (!saved) {
          viewport.scrollTop = 0;
          return;
        }
        const top = headingTop(saved.heading) ?? headingTop(saved.previous);
        viewport.scrollTop =
          top !== undefined
            ? top - saved.offset
            : saved.progress *
              Math.max(0, viewport.scrollHeight - viewport.clientHeight);
      };
      const capture = () => {
        const top = viewport.scrollTop;
        const base = viewport.getBoundingClientRect().top;
        // Locate the current section without laying out every skipped subtree.
        let low = 0;
        let high = shells.length - 1;
        let found = 0;
        while (low <= high) {
          const mid = (low + high) >> 1;
          if (shells[mid].getBoundingClientRect().top - base <= 96) {
            found = mid;
            low = mid + 1;
          } else high = mid - 1;
        }
        let h: { id: string; top: number } | undefined;
        for (let i = found; i >= 0 && !h; i--) {
          if (!rendered.has(i)) continue;
          for (const element of shells[i].querySelectorAll<HTMLElement>(
            'h1[id],h2[id],h3[id],h4[id],h5[id],h6[id]',
          )) {
            const y = element.getBoundingClientRect().top - base + top;
            if (y <= top + 96) h = { id: element.id, top: y };
          }
        }
        const ordinal = doc.headings.findIndex(
          (heading) => heading.id === h?.id,
        );
        const saved: ReadingPosition = {
          path: doc.file.path,
          heading: h?.id || '',
          previous: doc.headings[ordinal - 1]?.id || '',
          offset: h ? h.top - top : 0,
          progress:
            top / Math.max(1, viewport.scrollHeight - viewport.clientHeight),
        };
        position.current = saved;
        latest.current.onHeading(h?.id || doc.headings[0]?.id || '');
        return saved;
      };
      let jumpAnchor: { target: HTMLElement; navigation: number } | undefined;
      const keepJump = () => {
        if (jumpAnchor && jumpAnchor.navigation === navigation.current) {
          viewport.scrollTop +=
            jumpAnchor.target.getBoundingClientRect().top -
            viewport.getBoundingClientRect().top -
            28;
        }
      };
      const initialNavigation = navigation.current;
      const resize = new ResizeObserver(() => {
        cancelAnimationFrame(measureFrame);
        measureFrame = requestAnimationFrame(() => {
          if (navigation.current === initialNavigation && desired)
            restore(desired);
          keepJump();
          capture();
        });
      });
      resize.observe(article);
      const onScroll = () => {
        if (scrollFrame) return;
        scrollFrame = requestAnimationFrame(() => {
          scrollFrame = 0;
          capture();
          clearTimeout(saveTimer);
          saveTimer = setTimeout(() => {
            if (position.current) latest.current.onPosition(position.current);
          }, 250);
          paintVisible();
          drawFallback();
        });
      };
      const persist = () => latest.current.onPosition(capture());
      window.addEventListener('pagehide', persist);
      const userNavigation = () => {
        navigation.current++;
        selectWholeDocument = false;
      };
      viewport.addEventListener('scroll', onScroll, { passive: true });
      for (const name of ['wheel', 'touchstart', 'pointerdown', 'keydown'])
        viewport.addEventListener(name, userNavigation, { passive: true });
      const imageError = (event: Event) => {
        if (event.target instanceof HTMLImageElement) {
          unavailableImage(event.target, 'nicht verfügbar');
        }
      };
      article.addEventListener('error', imageError, true);
      const copyCode = (event: Event) => {
        const button = (event.target as Element).closest('button.copy-code');
        const code = button?.parentElement?.querySelector('code');
        if (!code) return;
        void gateway
          .copy(code.textContent || '')
          .then(() => {
            if (!cancelled) latest.current.onNotice('Code kopiert.');
          })
          .catch(() => {
            if (!cancelled)
              latest.current.onNotice(
                'Kopieren ist nicht verfügbar. Bitte den Text auswählen und Strg+C verwenden.',
              );
          });
      };
      article.addEventListener('click', copyCode);
      const toggle = () => {
        indexes.current = new WeakMap();
        setTextVersion((v) => v + 1);
      };
      article.addEventListener('toggle', toggle, true);
      const observedCode = new WeakSet<HTMLElement>();
      const highlightObserver = new IntersectionObserver(
        (entries) => {
          for (const entry of entries) {
            const shell = entry.target as HTMLElement;
            if (entry.isIntersecting) {
              visibleSections.current.add(shell);
              // Observing code inside skipped subtrees would force offscreen layout.
              for (const code of shell.querySelectorAll<HTMLElement>(
                'pre code',
              )) {
                if (
                  !observedCode.has(code) &&
                  code.dataset.highlighted !== 'yes'
                ) {
                  queue.push(code);
                  observedCode.add(code);
                }
              }
              scheduleHighlight();
            } else visibleSections.current.delete(shell);
          }
          paintVisible();
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
                const shell = code.closest<HTMLElement>('.markdown-section');
                if (shell) indexes.current.delete(shell);
                if (latest.current.query) setTextVersion((v) => v + 1);
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
      for (let i = 0; i < shells.length; i++) {
        if (shells[i].dataset.populated === 'true') {
          rendered.add(i);
          highlightObserver.observe(shells[i]);
        }
      }
      article.dataset.reusedSections = String(rendered.size);
      ranges.current = [];
      cleanupPaint.current();
      // Paint the reading anchor (or first section) before filling the document.
      if (shells.length) insert(0);
      restore(desired);
      capture();
      performance.measure('hashline.prepare-sections', {
        start: insertionStart,
        end: performance.now(),
      });
      const paintFrame = requestAnimationFrame(() =>
        requestAnimationFrame(() => {
          if (cancelled) return;
          performance.measure('hashline.content-to-frame', {
            start: insertionStart,
            end: performance.now(),
          });
          if (doc.file.processStartedAtMs !== undefined) {
            performance.measure('hashline.native-main-to-first-frame', {
              start: 0,
              duration:
                performance.timeOrigin +
                performance.now() -
                doc.file.processStartedAtMs,
            });
            const ready = performance.getEntriesByName(
              'hashline.frontend-ready',
            )[0];
            if (ready)
              performance.measure('hashline.native-main-to-frontend', {
                start: 0,
                duration:
                  performance.timeOrigin +
                  ready.startTime -
                  doc.file.processStartedAtMs,
              });
          }
          performance.measure('hashline.open-to-first-frame', {
            start: doc.openedAt,
            end: performance.now(),
          });
          void fill();
        }),
      );
      async function fill() {
        let yieldMs = 0;
        let tasks = 0;
        let maxYieldMs = 0;
        try {
          for (let i = 0; i < shells.length;) {
            const start = performance.now();
            do {
              insert(i++);
            } while (i < shells.length && performance.now() - start < 18);
            maxSliceMs = Math.max(maxSliceMs, performance.now() - start);
            const waitingAt = performance.now();
            await nextTask(abort.signal);
            const waited = performance.now() - waitingAt;
            yieldMs += waited;
            tasks++;
            maxYieldMs = Math.max(maxYieldMs, waited);
          }
          if (cancelled) return;
          for (const [name, value] of Object.entries({
            yieldMs,
            maxYieldMs,
          }))
            performance.measure(`hashline.fill-${name}`, {
              start: 0,
              duration: value,
            });
          article.dataset.renderTasks = String(tasks);
          completed = true;
          article.dataset.renderState = 'complete';
          article.setAttribute('aria-busy', 'false');
          keepJump();
          performance.measure('hashline.sanitize', {
            start: 0,
            duration: sanitizeMs,
          });
          performance.measure('hashline.insert', {
            start: 0,
            duration: insertionMs,
          });
          performance.measure('hashline.max-render-task', {
            start: 0,
            duration: maxSliceMs,
          });
          performance.measure('hashline.open-to-complete', {
            start: doc.openedAt,
            end: performance.now(),
          });
          setTextVersion((v) => v + 1);
        } catch (error) {
          if (!cancelled) {
            article.dataset.renderState = 'error';
            article.setAttribute('aria-busy', 'false');
            latest.current.onNotice(
              error instanceof Error ? error.message : String(error),
            );
          }
        }
      }
      const api: ViewportActions = {
        jump(id) {
          navigation.current++;
          let decoded: string;
          try {
            decoded = decodeURIComponent(id.replace(/^#/, ''));
          } catch {
            decoded = id;
          }
          const key = headingSection.has(decoded) ? decoded : `doc-${decoded}`;
          const section = headingSection.get(key);
          if (section !== undefined) insert(section);
          const target = document.getElementById(key);
          if (target) {
            reveal(target);
            jumpAnchor = { target, navigation: navigation.current };
            viewport.scrollTop +=
              target.getBoundingClientRect().top -
              viewport.getBoundingClientRect().top -
              28;
            target.tabIndex = -1;
            target.focus({ preventScroll: true });
          } else if (!decoded) viewport.scrollTop = 0;
          else
            latest.current.onNotice('Dieser Abschnitt wurde nicht gefunden.');
        },
        selectAll() {
          selectWholeDocument = !completed;
          selectAll();
        },
      };
      actions.current = api;
      return () => {
        cancelled = true;
        abort.abort();
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
        article.removeEventListener('click', copyCode);
        article.removeEventListener('toggle', toggle, true);
        if (actions.current === api) actions.current = null;
      };
    }
  }, [doc, actions, gateway]);

  function reveal(element: Element | null) {
    const shell = element?.closest<HTMLElement>('.markdown-section');
    if (!shell || shell === forcedSection.current) return;
    forcedSection.current?.style.removeProperty('content-visibility');
    shell.style.contentVisibility = 'visible';
    forcedSection.current = shell;
  }

  function paintVisible() {
    cleanupPaint.current();
    if (!hasHighlights()) return;
    const visible: Match[] = [];
    for (const shell of visibleSections.current) {
      for (const range of sectionRanges.current.get(shell) || [])
        visible.push(range);
    }
    const active = ranges.current[current.current];
    if (active && !visible.includes(active)) visible.push(active);
    cleanupPaint.current = paintRanges(
      visible.map(toRange),
      active ? visible.indexOf(active) : 0,
    );
  }

  function drawFallback() {
    const layer = overlay.current;
    if (!layer || hasHighlights()) return;
    if (!layer.hasChildNodes() && !ranges.current.length) return;
    layer.replaceChildren();
    const match = ranges.current[current.current];
    if (!match || !scroll.current) return;
    const range = toRange(match);
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
    // Drop node offsets before highlighting can replace their text nodes.
    cleanupPaint.current();
    ranges.current = [];
    sectionRanges.current.clear();
    drawFallback();
    if (!query) resumeHighlight.current();
    const abort = new AbortController();
    const start = performance.now();
    latest.current.onMatches(0, 0, !!query);
    const timer = setTimeout(() => {
      void runSearch();
    }, 30);
    async function search() {
      if (query && root.current?.dataset.renderState !== 'complete') return;
      if (!root.current) return;
      const found: Match[] = [];
      const bySection = new Map<Element, Match[]>();
      let slice = performance.now();
      if (query) {
        for (const shell of root.current.querySelectorAll<HTMLElement>(
          '.markdown-section',
        )) {
          let index = indexes.current.get(shell);
          if (!index) {
            index = indexText(shell);
            indexes.current.set(shell, index);
          }
          const matches = findMatches(index, query);
          bySection.set(shell, matches);
          for (const range of matches) found.push(range);
          if (performance.now() - slice >= 6) {
            await nextTask(abort.signal);
            slice = performance.now();
          }
        }
      }
      if (abort.signal.aborted) return;
      cleanupPaint.current();
      ranges.current = found;
      sectionRanges.current = bySection;
      current.current = 0;
      paintVisible();
      latest.current.onMatches(found.length, 0);
      const match = found[0];
      if (match && scroll.current) {
        reveal(match.startNode.parentElement);
        const range = toRange(match);
        navigation.current++;
        scroll.current.scrollTop +=
          range.getBoundingClientRect().top -
          scroll.current.getBoundingClientRect().top -
          scroll.current.clientHeight / 3;
      }
      drawFallback();
      performance.measure('hashline.search', { start, end: performance.now() });
    }
    // Cleanup aborts task-boundary waits, including a document replaced mid-search.
    // Catch at the promise boundary so cancellations never become unhandled errors.
    async function runSearch() {
      try {
        await search();
      } catch (error) {
        if (!abort.signal.aborted) latest.current.onNotice(String(error));
      }
    }
    return () => {
      clearTimeout(timer);
      abort.abort();
    };
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
    paintVisible();
    const match = ranges.current[current.current];
    const viewport = scroll.current!;
    reveal(match.startNode.parentElement);
    const range = toRange(match);
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
      {remoteState.id === doc.file.id &&
        remoteState.count > 0 &&
        remoteState.status !== 'allowed' && (
          <div
            className="remote-images"
            role="region"
            aria-label="Remote-Bilder"
          >
            <span>
              {remoteState.count} Remote-Bilder blockiert. Beim Laden wird deine
              IP-Adresse an die Bildanbieter übertragen.
            </span>
            {gateway.allowRemoteImages ? (
              <button
                disabled={remoteState.status === 'loading'}
                onClick={() => void allowRemoteImages()}
              >
                {remoteState.status === 'loading'
                  ? 'Freigeben …'
                  : 'Remote-Bilder für dieses Dokument laden'}
              </button>
            ) : (
              <span>Freigabe in der Desktop-App verfügbar.</span>
            )}
          </div>
        )}
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
            // Connected old sections must not resolve links against the new file.
            if (event.currentTarget.dataset.renderState === 'retiring') return;
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
