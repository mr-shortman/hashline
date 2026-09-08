interface TextPart {
  node: Text;
  start: number;
  end: number;
}
export interface TextIndex {
  text: string;
  parts: TextPart[];
}

export function indexText(root: HTMLElement): TextIndex {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode(node) {
      const parent = node.parentElement;
      if (!parent || parent.closest('button, [hidden], [data-search-ignore]'))
        return NodeFilter.FILTER_REJECT;
      const closed = parent.closest('details:not([open])');
      if (closed && !parent.closest('summary')) return NodeFilter.FILTER_REJECT;
      return NodeFilter.FILTER_ACCEPT;
    },
  });
  let text = '';
  let lastBlock: Element | null = null;
  const parts: TextPart[] = [];
  while (walker.nextNode()) {
    const node = walker.currentNode as Text;
    const block = node.parentElement!.closest(
      'p,li,pre,td,th,h1,h2,h3,h4,h5,h6,summary,dt,dd',
    );
    if (lastBlock && block !== lastBlock) text += '\n';
    lastBlock = block;
    const start = text.length;
    text += node.data;
    parts.push({ node, start, end: text.length });
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
