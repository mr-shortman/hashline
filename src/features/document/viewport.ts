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
import {
  replayFragment,
  sanitizeFragment,
  unavailableImage,
} from '../../core/content/policy';
import {
  sectionAt,
  sectionCount,
  sectionEncoded,
  sectionFirstText,
} from '../../core/markdown/opbuffer';
import { nextTask, retireContent } from './schedule';
import { el } from '../../ui/dom';

export interface ViewportActions {
  jump(id: string): void;
  selectAll(): void;
}
export interface ViewportOptions {
  gateway: DocumentGateway;
  zoom: number;
  position(path: string): ReadingPosition | undefined;
  onPosition(position: ReadingPosition): void;
  onHeading(id: string): void;
  onMatches(count: number, active: number, pending?: boolean): void;
  onLink(path: string, fragment?: string): void;
  onNotice(message: string): void;
}
/**
 * A search result. Hits found in the op buffer carry text offsets and are
 * resolved to DOM positions on demand; hits from a section indexed through the
 * DOM already carry theirs.
 */
interface Hit {
  section: number;
  start: number;
  end: number;
  match?: Match;
}
/** What the search needs from a mounted document. */
interface SearchSurface {
  count: number;
  shell(section: number): HTMLElement | undefined;
  section(shell: Element): number | undefined;
  /** True when the buffer cannot map this section's text back to nodes. */
  usesDom(section: number): boolean;
  /** Builds the section if needed and indexes its DOM. */
  domIndex(section: number): TextIndex;
  /** Text-node map of the section, building the section if needed. */
  nodes(
    section: number,
  ):
    | { base: number; starts: number[]; lengths: number[]; nodes: Text[] }
    | undefined;
}

export interface Viewport extends ViewportActions {
  element: HTMLElement;
  setDocument(document: RenderDocument): void;
  setQuery(query: string): void;
  setZoom(zoom: number): void;
  step(delta: number): void;
  destroy(): void;
}

// The former React component was already a single layout effect over imperative
// DOM work. The effects survive as explicit lifecycle calls: setDocument replays
// the mount effect, setQuery and the internal text version replay the search
// effect, and step replaces the match-step effect.
export function createViewport(options: ViewportOptions): Viewport {
  const { gateway } = options;
  let zoom = options.zoom;
  let doc: RenderDocument | undefined;
  let query = '';
  let mountCleanup: (() => void) | undefined;
  let searchCleanup: (() => void) | undefined;
  let actions: ViewportActions | null = null;
  let position: ReadingPosition | undefined;
  let retainedSections: { key: string; shell: HTMLElement }[] = [];
  // Search reads the document text of the op buffer directly; the DOM is only
  // consulted for sections the buffer cannot answer for — those the parser left
  // as raw HTML, and those whose text nodes syntax highlighting has replaced
  // (docs/decisions/007, P2.4).
  let indexes = new WeakMap<HTMLElement, TextIndex>();
  let hits: Hit[] = [];
  const visibleSections = new Set<Element>();
  let sectionHits = new Map<Element, Hit[]>();
  let current = 0;
  let surface: SearchSurface | undefined;
  // Keyed by shell so a section reused across reloads keeps its map.
  const textMaps = new WeakMap<
    HTMLElement,
    { starts: number[]; lengths: number[]; nodes: Text[] }
  >();
  // Sections the buffer cannot map back to nodes: raw HTML, and sections whose
  // text nodes syntax highlighting has replaced.
  const domSections = new WeakSet<HTMLElement>();
  let forcedSection: HTMLElement | null = null;
  let navigation = 0;
  let cleanupPaint: () => void = () => {};
  let resumeHighlight: () => void = () => {};
  let remoteState = { id: '', status: 'blocked' as const, count: 0 } as {
    id: string;
    status: 'blocked' | 'loading' | 'allowed';
    count: number;
  };
  let remoteAccess = { id: '', allowed: false };

  const root = el('article', { class: 'markdown' });
  root.style.fontSize = `${(17 * zoom) / 100}px`;
  const scroll = el(
    'div',
    { class: 'document-scroll', tabindex: '0', 'aria-label': 'Dokument' },
    root,
  );
  const overlay = el('div', {
    class: 'search-fallback',
    'aria-hidden': 'true',
  });
  const element = el('div', { class: 'viewport-shell' }, scroll, overlay);

  root.addEventListener('click', (event) => {
    const link = (event.target as Element).closest<HTMLAnchorElement>(
      'a[data-link]',
    );
    if (!link || !doc) return;
    event.preventDefault();
    // Connected old sections must not resolve links against the new file.
    if (root.dataset.renderState === 'retiring') return;
    const active = doc;
    const href = link.dataset.link!;
    if (href.startsWith('#')) {
      actions?.jump(href);
      return;
    }
    void gateway
      .followLink(active.file, href)
      .then((result) => {
        if (doc === active && result.path)
          options.onLink(result.path, result.fragment);
      })
      .catch((error: unknown) =>
        options.onNotice(
          error instanceof Error ? error.message : String(error),
        ),
      );
  });

  let banner: HTMLElement | undefined;
  let bannerButton: HTMLButtonElement | undefined;
  let bannerText: HTMLElement | undefined;
  function renderRemote() {
    const visible =
      !!doc &&
      remoteState.id === doc.file.id &&
      remoteState.count > 0 &&
      remoteState.status !== 'allowed';
    if (!visible) {
      banner?.remove();
      banner = undefined;
      bannerButton = undefined;
      bannerText = undefined;
      return;
    }
    if (!banner) {
      bannerText = el('span', {});
      banner = el('div', {
        class: 'remote-images',
        role: 'region',
        'aria-label': 'Remote-Bilder',
      });
      banner.append(bannerText);
      if (gateway.allowRemoteImages) {
        bannerButton = el('button', {
          onclick: () => void allowRemoteImages(),
        });
        banner.append(bannerButton);
      } else {
        banner.append(el('span', {}, 'Freigabe in der Desktop-App verfügbar.'));
      }
      element.prepend(banner);
    }
    bannerText!.textContent = `${remoteState.count} Remote-Bilder blockiert. Beim Laden wird deine IP-Adresse an die Bildanbieter übertragen.`;
    if (bannerButton) {
      bannerButton.disabled = remoteState.status === 'loading';
      bannerButton.textContent =
        remoteState.status === 'loading'
          ? 'Freigeben …'
          : 'Remote-Bilder für dieses Dokument laden';
    }
  }

  function loadRemoteImages(container: ParentNode) {
    if (!doc) return;
    const file = doc.file;
    for (const img of container.querySelectorAll<HTMLImageElement>(
      'img[data-remote-source]',
    )) {
      const url = gateway.imageUrl(file, img.dataset.remoteSource!);
      if (!url?.startsWith(`hashline-image://localhost/${file.id}/`)) continue;
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
  }
  async function allowRemoteImages() {
    if (!gateway.allowRemoteImages || !doc) return;
    const active = doc;
    remoteState = { ...remoteState, status: 'loading' };
    renderRemote();
    try {
      await gateway.allowRemoteImages(active.file);
      if (doc !== active) return;
      remoteAccess = { id: active.file.id, allowed: true };
      loadRemoteImages(root);
      remoteState = { ...remoteState, status: 'allowed' };
      renderRemote();
      scroll.focus({ preventScroll: true });
      options.onNotice('Remote-Bilder für diese Dokumentversion freigegeben.');
    } catch {
      if (doc !== active) return;
      remoteState = { ...remoteState, status: 'blocked' };
      renderRemote();
      options.onNotice(
        'Remote-Bilder konnten nicht freigegeben werden. Bitte erneut versuchen.',
      );
    }
  }

  function reveal(target: Element | null) {
    const shell = target?.closest<HTMLElement>('.markdown-section');
    if (!shell || shell === forcedSection) return;
    forcedSection?.style.removeProperty('content-visibility');
    shell.style.contentVisibility = 'visible';
    forcedSection = shell;
  }

  /**
   * Turns a text offset back into a DOM position: offset → text operation →
   * the node that operation created. The section is materialized if the reader
   * has not scrolled to it yet, which is why a match in an unbuilt section can
   * still be counted, navigated to and painted.
   */
  function resolve(hit: Hit): Match | undefined {
    if (hit.match) return hit.match;
    const map = surface?.nodes(hit.section);
    if (!map || !map.nodes.length) return undefined;
    const start = hit.start - map.base;
    const end = hit.end - map.base;
    const before = (offset: number) => {
      let low = 0;
      let high = map.starts.length - 1;
      let found = -1;
      while (low <= high) {
        const mid = (low + high) >> 1;
        if (map.starts[mid] <= offset) {
          found = mid;
          low = mid + 1;
        } else high = mid - 1;
      }
      return found;
    };
    const first = before(start);
    const last = before(end - 1);
    if (first < 0 || last < 0) return undefined;
    // A match that reached across a block separator has no DOM counterpart.
    if (end - map.starts[last] > map.lengths[last]) return undefined;
    return {
      startNode: map.nodes[first],
      startOffset: start - map.starts[first],
      endNode: map.nodes[last],
      endOffset: end - map.starts[last],
    };
  }

  function paintVisible() {
    cleanupPaint();
    if (!hasHighlights()) return;
    const visible: Range[] = [];
    const active = hits[current];
    let activeIndex = 0;
    let paintedActive = false;
    for (const shell of visibleSections) {
      for (const hit of sectionHits.get(shell) || []) {
        const match = resolve(hit);
        if (!match) continue;
        if (hit === active) {
          activeIndex = visible.length;
          paintedActive = true;
        }
        visible.push(toRange(match));
      }
    }
    if (active && !paintedActive) {
      const match = resolve(active);
      if (match) {
        activeIndex = visible.length;
        visible.push(toRange(match));
      }
    }
    cleanupPaint = paintRanges(visible, activeIndex);
  }

  function drawFallback() {
    if (hasHighlights()) return;
    if (!overlay.hasChildNodes() && !hits.length) return;
    overlay.replaceChildren();
    const hit = hits[current];
    const match = hit && resolve(hit);
    if (!match) return;
    const range = toRange(match);
    const base = scroll.getBoundingClientRect();
    for (const rect of range.getClientRects()) {
      const mark = document.createElement('span');
      Object.assign(mark.style, {
        left: `${rect.left - base.left}px`,
        top: `${rect.top - base.top}px`,
        width: `${rect.width}px`,
        height: `${rect.height}px`,
      });
      overlay.append(mark);
    }
  }

  function mountDocument() {
    const active = doc!;
    const preparation = new AbortController();
    const reusable = new Map<string, HTMLElement[]>();
    for (const { key, shell } of retainedSections.slice().reverse()) {
      if (
        shell.parentNode !== root ||
        shell.dataset.populated !== 'true' ||
        shell.dataset.hasImages === 'true'
      )
        continue;
      const candidates = reusable.get(key) || [];
      candidates.push(shell);
      reusable.set(key, candidates);
    }
    const planned = active.sections.map(({ key }) => {
      const shell = reusable.get(key)?.pop() || document.createElement('div');
      shell.className = 'markdown-section';
      return { key, shell };
    });
    const keep = new Set(planned.map(({ shell }) => shell));
    root.dataset.renderState = 'retiring';
    root.setAttribute('aria-busy', 'true');
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
    actions = waiting;
    void (async () => {
      try {
        await retireContent(root, preparation.signal, keep);
        if (preparation.signal.aborted) return;
        cleanup = mount();
        if (pendingJump !== undefined) actions?.jump(pendingJump);
        if (pendingSelection) actions?.selectAll();
      } catch (error) {
        if (!preparation.signal.aborted) options.onNotice(String(error));
      }
    })();
    mountCleanup = () => {
      preparation.abort();
      cleanup?.();
      if (actions === waiting) actions = null;
    };

    function mount() {
      const article = root;
      const viewport = scroll;
      const insertionStart = performance.now();
      let cancelled = false;
      let measureFrame = 0;
      let scrollFrame = 0;
      let saveTimer: ReturnType<typeof setTimeout>;
      const desired =
        position?.path === active.file.path
          ? position
          : options.position(active.file.path);
      const abort = new AbortController();
      forcedSection?.style.removeProperty('content-visibility');
      forcedSection = null;
      remoteAccess = { id: active.file.id, allowed: false };
      remoteState = { id: active.file.id, status: 'blocked', count: 0 };
      renderRemote();
      article.dataset.renderState = 'rendering';
      article.setAttribute('aria-busy', 'true');
      visibleSections.clear();
      sectionHits.clear();
      const shells = planned.map(({ shell }) => shell);
      const shellIndex = new Map<Element, number>(
        shells.map((shell, i) => [shell, i]),
      );
      // Keep already ordered nodes connected: moving them through a fragment
      // would synchronously destroy the render trees we are trying to retain.
      let cursor: ChildNode | null = article.firstChild;
      for (const shell of shells) {
        if (shell !== cursor) article.insertBefore(shell, cursor);
        cursor = shell.nextSibling;
      }
      retainedSections = planned;
      const rendered = new Set<number>();
      const headingSection = new Map<string, number>();
      active.sections.forEach((section, i) => {
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
      let remotePending = 0;
      let remoteFrame = 0;
      const insert = (i: number) => {
        if (rendered.has(i) || cancelled) return;
        const start = performance.now();
        const section = active.sections[i];
        // Replay creates the nodes directly in this document: no HTML parse,
        // no foreign document, no adoption, no sanitizing walk. Sections the
        // encoder refused keep the DOMPurify path (docs/decisions/007, P2.2).
        const replayed = sectionEncoded(active.ops, i)
          ? replayFragment(
              active.ops,
              active.strings,
              i,
              section.headings,
              active.file,
              gateway,
            )
          : undefined;
        const clean =
          replayed ??
          sanitizeFragment(
            section.html,
            section.headings,
            active.file,
            gateway,
          );
        const {
          fragment,
          remoteImages: count,
          hasImages,
          tables,
          pres,
        } = clean;
        if (replayed?.textOffsets.length) {
          // Offsets are stored relative to the section's first text run so a
          // section reused across reloads keeps a valid map: its content is
          // identical, its absolute position in the document text need not be.
          const base = replayed.textOffsets[0];
          textMaps.set(shells[i], {
            starts: replayed.textOffsets.map((offset) => offset - base),
            lengths: replayed.textNodes.map((node) => node.data.length),
            nodes: replayed.textNodes,
          });
        }
        if (count) {
          // Coalesced like React batched the former state updates: the banner
          // must never lay out once per filled section.
          remotePending += count;
          if (!remoteFrame)
            remoteFrame = requestAnimationFrame(() => {
              remoteFrame = 0;
              if (cancelled) return;
              remoteState = {
                ...remoteState,
                count: remoteState.count + remotePending,
              };
              remotePending = 0;
              renderRemote();
            });
          if (remoteAccess.id === active.file.id && remoteAccess.allowed)
            loadRemoteImages(fragment);
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
          for (const heading of shells[i].querySelectorAll<HTMLElement>(
            'h1[id],h2[id],h3[id],h4[id],h5[id],h6[id]',
          )) {
            const y = heading.getBoundingClientRect().top - base + top;
            if (y <= top + 96) h = { id: heading.id, top: y };
          }
        }
        const ordinal = active.headings.findIndex(
          (heading) => heading.id === h?.id,
        );
        const saved: ReadingPosition = {
          path: active.file.path,
          heading: h?.id || '',
          previous: active.headings[ordinal - 1]?.id || '',
          offset: h ? h.top - top : 0,
          progress:
            top / Math.max(1, viewport.scrollHeight - viewport.clientHeight),
        };
        position = saved;
        options.onHeading(h?.id || active.headings[0]?.id || '');
        return saved;
      };
      let jumpAnchor: { target: HTMLElement; navigation: number } | undefined;
      const keepJump = () => {
        if (jumpAnchor && jumpAnchor.navigation === navigation) {
          viewport.scrollTop +=
            jumpAnchor.target.getBoundingClientRect().top -
            viewport.getBoundingClientRect().top -
            28;
        }
      };
      const initialNavigation = navigation;
      const resize = new ResizeObserver(() => {
        cancelAnimationFrame(measureFrame);
        measureFrame = requestAnimationFrame(() => {
          if (navigation === initialNavigation && desired) restore(desired);
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
            if (position) options.onPosition(position);
          }, 250);
          paintVisible();
          drawFallback();
        });
      };
      const persist = () => options.onPosition(capture());
      window.addEventListener('pagehide', persist);
      const userNavigation = () => {
        navigation++;
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
            if (!cancelled) options.onNotice('Code kopiert.');
          })
          .catch(() => {
            if (!cancelled)
              options.onNotice(
                'Kopieren ist nicht verfügbar. Bitte den Text auswählen und Strg+C verwenden.',
              );
          });
      };
      article.addEventListener('click', copyCode);
      const toggle = () => {
        indexes = new WeakMap();
        startSearch();
      };
      article.addEventListener('toggle', toggle, true);
      const observedCode = new WeakSet<HTMLElement>();
      const highlightObserver = new IntersectionObserver(
        (entries) => {
          for (const entry of entries) {
            const shell = entry.target as HTMLElement;
            if (entry.isIntersecting) {
              visibleSections.add(shell);
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
            } else visibleSections.delete(shell);
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
          query ||
          !getSelection()?.isCollapsed
        )
          return;
        highlightTimer = setTimeout(() => {
          highlightTimer = undefined;
          if (cancelled) return;
          const code = queue.shift()!;
          if (query || !getSelection()?.isCollapsed) {
            queue.unshift(code);
            return;
          }
          void highlightCode(code, () => !cancelled && !query)
            .then((changed) => {
              if (changed && !cancelled) {
                // Highlighting replaced this section's text nodes, so the
                // buffer can no longer map its offsets; it falls back to a DOM
                // index for search from here on.
                const shell = code.closest<HTMLElement>('.markdown-section');
                if (shell) {
                  domSections.add(shell);
                  indexes.delete(shell);
                }
                if (query) startSearch();
              }
              scheduleHighlight();
            })
            .catch(() => {
              scheduleHighlight();
            });
        }, 16);
      };
      resumeHighlight = scheduleHighlight;
      document.addEventListener('selectionchange', scheduleHighlight);
      for (let i = 0; i < shells.length; i++) {
        if (shells[i].dataset.populated === 'true') {
          rendered.add(i);
          highlightObserver.observe(shells[i]);
        }
      }
      article.dataset.reusedSections = String(rendered.size);
      surface = {
        count: sectionCount(active.ops),
        shell: (index) => shells[index],
        section: (shell) => shellIndex.get(shell),
        usesDom: (index) =>
          !sectionEncoded(active.ops, index) || domSections.has(shells[index]),
        domIndex(index) {
          insert(index);
          let index_ = indexes.get(shells[index]);
          if (!index_) {
            index_ = indexText(shells[index]);
            indexes.set(shells[index], index_);
          }
          return index_;
        },
        nodes(index) {
          insert(index);
          const map = textMaps.get(shells[index]);
          return map && { base: sectionFirstText(active.ops, index), ...map };
        },
      };
      hits = [];
      cleanupPaint();
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
          if (active.file.processStartedAtMs !== undefined) {
            performance.measure('hashline.native-main-to-first-frame', {
              start: 0,
              duration:
                performance.timeOrigin +
                performance.now() -
                active.file.processStartedAtMs,
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
                  active.file.processStartedAtMs,
              });
          }
          performance.measure('hashline.open-to-first-frame', {
            start: active.openedAt,
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
          for (const [name, value] of Object.entries({ yieldMs, maxYieldMs }))
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
            start: active.openedAt,
            end: performance.now(),
          });
        } catch (error) {
          if (!cancelled) {
            article.dataset.renderState = 'error';
            article.setAttribute('aria-busy', 'false');
            options.onNotice(
              error instanceof Error ? error.message : String(error),
            );
          }
        }
      }
      const mounted = surface;
      const api: ViewportActions = {
        jump(id) {
          navigation++;
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
            jumpAnchor = { target, navigation };
            viewport.scrollTop +=
              target.getBoundingClientRect().top -
              viewport.getBoundingClientRect().top -
              28;
            target.tabIndex = -1;
            target.focus({ preventScroll: true });
          } else if (!decoded) viewport.scrollTop = 0;
          else options.onNotice('Dieser Abschnitt wurde nicht gefunden.');
        },
        selectAll() {
          selectWholeDocument = !completed;
          selectAll();
        },
      };
      actions = api;
      return () => {
        cancelled = true;
        abort.abort();
        capture();
        if (position) options.onPosition(position);
        clearTimeout(saveTimer);
        clearTimeout(highlightTimer);
        cancelAnimationFrame(measureFrame);
        cancelAnimationFrame(scrollFrame);
        cancelAnimationFrame(paintFrame);
        cancelAnimationFrame(remoteFrame);
        resize.disconnect();
        highlightObserver.disconnect();
        document.removeEventListener('selectionchange', scheduleHighlight);
        if (resumeHighlight === scheduleHighlight) resumeHighlight = () => {};
        if (surface === mounted) surface = undefined;
        cleanupPaint();
        viewport.removeEventListener('scroll', onScroll);
        window.removeEventListener('pagehide', persist);
        for (const name of ['wheel', 'touchstart', 'pointerdown', 'keydown'])
          viewport.removeEventListener(name, userNavigation);
        article.removeEventListener('error', imageError, true);
        article.removeEventListener('click', copyCode);
        article.removeEventListener('toggle', toggle, true);
        if (actions === api) actions = null;
      };
    }
  }

  function startSearch() {
    searchCleanup?.();
    // Drop node offsets before highlighting can replace their text nodes.
    cleanupPaint();
    hits = [];
    sectionHits.clear();
    drawFallback();
    if (!query) resumeHighlight();
    const abort = new AbortController();
    const start = performance.now();
    options.onMatches(0, 0, !!query);
    const timer = setTimeout(() => {
      void runSearch();
    }, 30);
    searchCleanup = () => {
      clearTimeout(timer);
      abort.abort();
      searchCleanup = undefined;
    };
    async function search() {
      const document_ = doc;
      if (!document_ || !surface) return;
      const found: Hit[][] = Array.from({ length: surface.count }, () => []);
      let slice = performance.now();
      if (query) {
        // One pass over the document text of the whole buffer. No DOM is
        // touched here, so the full result is available from the first frame
        // rather than after the last section has been inserted.
        const pattern = new RegExp(
          query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'),
          'giu',
        );
        for (const match of document_.strings.text.matchAll(pattern)) {
          const section = sectionAt(document_.ops, match.index);
          if (section >= 0)
            found[section].push({
              section,
              start: match.index,
              end: match.index + match[0].length,
            });
          if (performance.now() - slice >= 6) {
            await nextTask(abort.signal);
            slice = performance.now();
          }
        }
        // The exceptions: raw HTML the parser could not encode, and sections
        // whose nodes highlighting replaced. Those are indexed from their DOM,
        // which means building them — for raw HTML that is unavoidable, and it
        // is the only case that still waits for insertion.
        for (let i = 0; i < surface.count; i++) {
          if (!surface.usesDom(i)) continue;
          found[i] = findMatches(surface.domIndex(i), query).map((match) => ({
            section: i,
            start: 0,
            end: 0,
            match,
          }));
          if (performance.now() - slice >= 6) {
            await nextTask(abort.signal);
            slice = performance.now();
          }
        }
      }
      if (abort.signal.aborted) return;
      cleanupPaint();
      hits = found.flat();
      sectionHits = new Map(
        found.flatMap((sectionMatches, i) => {
          const shell = surface?.shell(i);
          return shell ? [[shell, sectionMatches] as [Element, Hit[]]] : [];
        }),
      );
      current = 0;
      paintVisible();
      options.onMatches(hits.length, 0);
      const match = hits[0] && resolve(hits[0]);
      if (match) {
        reveal(match.startNode.parentElement);
        const range = toRange(match);
        navigation++;
        scroll.scrollTop +=
          range.getBoundingClientRect().top -
          scroll.getBoundingClientRect().top -
          scroll.clientHeight / 3;
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
        if (!abort.signal.aborted) options.onNotice(String(error));
      }
    }
  }

  return {
    element,
    setDocument(next) {
      if (next === doc) return;
      mountCleanup?.();
      mountCleanup = undefined;
      doc = next;
      mountDocument();
      startSearch();
    },
    setQuery(next) {
      if (next === query) return;
      query = next;
      startSearch();
    },
    setZoom(next) {
      if (next === zoom) return;
      zoom = next;
      root.style.fontSize = `${(17 * zoom) / 100}px`;
    },
    step(delta) {
      if (!delta || !hits.length) return;
      current = (current + (delta % hits.length) + hits.length) % hits.length;
      cleanupPaint();
      const match = resolve(hits[current]);
      paintVisible();
      options.onMatches(hits.length, current);
      if (match) {
        reveal(match.startNode.parentElement);
        const range = toRange(match);
        navigation++;
        scroll.scrollTop +=
          range.getBoundingClientRect().top -
          scroll.getBoundingClientRect().top -
          scroll.clientHeight / 3;
      }
      drawFallback();
    },
    jump(id) {
      actions?.jump(id);
    },
    selectAll() {
      actions?.selectAll();
    },
    destroy() {
      searchCleanup?.();
      mountCleanup?.();
      mountCleanup = undefined;
      doc = undefined;
      element.remove();
    },
  };
}
