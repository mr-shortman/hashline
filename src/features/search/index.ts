interface TextPart {
  node: Text;
  start: number;
  end: number;
}
export interface TextIndex {
  text: string;
  parts: TextPart[];
}

const BLOCKS = new Set([
  'P',
  'LI',
  'PRE',
  'TD',
  'TH',
  'H1',
  'H2',
  'H3',
  'H4',
  'H5',
  'H6',
  'SUMMARY',
  'DT',
  'DD',
]);

export function indexText(root: HTMLElement): TextIndex {
  let text = '';
  let lastBlock: Element | null = null;
  const parts: TextPart[] = [];
  interface Context {
    next: ChildNode | null;
    block: Element | null;
    closed: boolean;
    summary: boolean;
  }
  // Callers index a markdown-section inside article.markdown: ancestors outside
  // root have no block, hidden, button, details or search-ignore semantics.
  // Carry context in an explicit stack so even untrusted deep DOM cannot exhaust
  // the JS call stack. Ignored subtrees need not be visited at all.
  const ignored = (el: Element) =>
    el.tagName === 'BUTTON' ||
    el.hasAttribute('hidden') ||
    el.hasAttribute('data-search-ignore');
  if (ignored(root)) return { text, parts };
  const stack: Context[] = [
    {
      next: root.firstChild,
      block: BLOCKS.has(root.tagName) ? root : null,
      closed: root.tagName === 'DETAILS' && !root.hasAttribute('open'),
      summary: root.tagName === 'SUMMARY',
    },
  ];
  while (stack.length) {
    const context = stack[stack.length - 1];
    const child = context.next;
    if (!child) {
      stack.pop();
      continue;
    }
    context.next = child.nextSibling;
    if (child.nodeType === Node.TEXT_NODE) {
      if (context.closed && !context.summary) continue;
      const node = child as Text;
      if (lastBlock && context.block !== lastBlock) text += '\n';
      lastBlock = context.block;
      const start = text.length;
      text += node.data;
      parts.push({ node, start, end: text.length });
    } else if (child.nodeType === Node.ELEMENT_NODE) {
      const el = child as Element;
      if (ignored(el)) continue;
      stack.push({
        next: el.firstChild,
        block: BLOCKS.has(el.tagName) ? el : context.block,
        closed:
          context.closed ||
          (el.tagName === 'DETAILS' && !el.hasAttribute('open')),
        summary: context.summary || el.tagName === 'SUMMARY',
      });
    }
  }
  return { text, parts };
}

export function findRanges(index: TextIndex, query: string): Range[] {
  if (!query) return [];
  // Escaped literal regex preserves original UTF-16 offsets, including case mappings.
  const pattern = new RegExp(
    query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'),
    'giu',
  );
  const ranges: Range[] = [];
  let partIndex = 0;
  for (const match of index.text.matchAll(pattern)) {
    const start = match.index;
    const end = start + match[0].length;
    while (
      partIndex < index.parts.length &&
      index.parts[partIndex].end <= start
    )
      partIndex++;
    const first = index.parts[partIndex];
    let endIndex = partIndex;
    while (endIndex < index.parts.length && index.parts[endIndex].end < end)
      endIndex++;
    const last = index.parts[endIndex];
    if (!first || !last) continue;
    const range = document.createRange();
    range.setStart(first.node, Math.max(0, start - first.start));
    range.setEnd(last.node, end - last.start);
    ranges.push(range);
  }
  return ranges;
}

interface NativeHighlight {
  add(range: Range): void;
}
type HighlightConstructor = new (...ranges: Range[]) => NativeHighlight;
function highlightRegistry(): {
  registry: Map<string, unknown>;
  Highlight: HighlightConstructor;
} | null {
  const css = globalThis.CSS as typeof CSS & {
    highlights?: Map<string, unknown>;
  };
  const ctor = (globalThis as unknown as { Highlight?: HighlightConstructor })
    .Highlight;
  return css?.highlights && ctor
    ? { registry: css.highlights, Highlight: ctor }
    : null;
}

export function paintRanges(ranges: Range[], active: number): () => void {
  const api = highlightRegistry();
  if (!api) return () => {}; // Viewport draws a pointer-transparent overlay for older WebKit.
  const all = new api.Highlight();
  for (const range of ranges) all.add(range);
  api.registry.set('search', all);
  api.registry.set(
    'search-current',
    new api.Highlight(...(ranges[active] ? [ranges[active]] : [])),
  );
  return () => {
    api.registry.delete('search');
    api.registry.delete('search-current');
  };
}
export function hasHighlights(): boolean {
  return highlightRegistry() !== null;
}
