import { readFileSync } from 'node:fs';
import { expect, it } from 'vitest';
import { parseMarkdown, renderStructural as render } from './parser';
import { sectionCount, sectionEncoded } from '../src/core/markdown/opbuffer';

const examples = JSON.parse(
  readFileSync('tests/fixtures/commonmark-0.31.2.json', 'utf8'),
) as { example: number; markdown: string; html: string }[];

function fromHtml(html: string): HTMLDivElement {
  const root = document.createElement('div');
  root.innerHTML = html;
  return root;
}

/**
 * The normalizations decision 008 allows, and nothing else: the generated
 * heading ids, which the specification does not know, and whitespace at block
 * boundaries, which the html renderer writes for legibility and the operations
 * do not carry. Whitespace inside `pre` and `code`, and between inline
 * elements, is content and stays.
 */
const BLOCK = new Set([
  'P',
  'DIV',
  'UL',
  'OL',
  'LI',
  'BLOCKQUOTE',
  'PRE',
  'TABLE',
  'THEAD',
  'TBODY',
  'TFOOT',
  'TR',
  'TH',
  'TD',
  'H1',
  'H2',
  'H3',
  'H4',
  'H5',
  'H6',
  'HR',
  'DL',
  'DT',
  'DD',
  'DETAILS',
  'SUMMARY',
]);
const isBoundary = (node: Node | null): boolean =>
  !node ||
  (node.nodeType === Node.ELEMENT_NODE && BLOCK.has((node as Element).tagName));

function normalize(root: HTMLDivElement): HTMLDivElement {
  for (const heading of root.querySelectorAll('h1,h2,h3,h4,h5,h6'))
    if (heading.id.startsWith('doc-')) heading.removeAttribute('id');
  const empty: Text[] = [];
  const walk = (node: Node, verbatim: boolean) => {
    for (let child = node.firstChild; child; child = child.nextSibling) {
      if (child.nodeType === Node.TEXT_NODE) {
        if (verbatim) continue;
        const text = child as Text;
        if (isBoundary(text.previousSibling))
          text.data = text.data.replace(/^\s+/, '');
        if (isBoundary(text.nextSibling))
          text.data = text.data.replace(/\s+$/, '');
        if (!text.data) empty.push(text);
      } else if (child.nodeType === Node.ELEMENT_NODE) {
        const tag = (child as Element).tagName;
        walk(child, verbatim || tag === 'PRE' || tag === 'CODE');
      }
    }
  };
  walk(root, false);
  for (const node of empty) node.remove();
  root.normalize();
  return root;
}

it.each(examples)(
  'parser output matches CommonMark $example',
  ({ markdown, html }) => {
    expect(
      normalize(render(markdown)).isEqualNode(normalize(fromHtml(html))),
    ).toBe(true);
  },
);

it('renders the same content whichever section boundary falls where', () => {
  const block =
    '## Abschnitt **eins**\n\n' +
    'Ein Absatz mit [Link](other.md), `code`, *kursiv* und ![Bild](a.png).\n\n' +
    '- Punkt eins\n  - verschachtelt\n- [x] erledigt\n- [ ] offen\n\n' +
    '> Zitat mit &amp; und <https://example.com/>\n\n' +
    '| A | B |\n| :-- | --: |\n| 1 | 2 |\n\n' +
    '```js\nconst a = 1 < 2 && "x";\n```\n\n' +
    '1. eins\n2. zwei\n\n---\n\n';
  const repetitions = 40;
  const source = block.repeat(repetitions);
  expect(parseMarkdown(source).sections.length).toBeGreaterThan(1);
  const single = document.createElement('div');
  for (let i = 0; i < repetitions; i++)
    single.append(...Array.from(render(block).childNodes));
  // Heading ids differ by design across the repetitions; the structure must not.
  expect(normalize(render(source)).isEqualNode(normalize(single))).toBe(true);
});

it('keeps raw HTML sections on the sanitized HTML path', () => {
  // The reader fixture carries <details>/<summary> and a literal angle bracket.
  const parsed = parseMarkdown(
    readFileSync('tests/fixtures/reader.md', 'utf8'),
  );
  expect(sectionCount(parsed.ops)).toBeGreaterThan(0);
  for (let i = 0; i < sectionCount(parsed.ops); i++)
    expect(sectionEncoded(parsed.ops, i)).toBe(false);
});

it('does not split raw HTML containers spanning Markdown tokens', () => {
  const source =
    '<details>\n<summary>Summary</summary>\n\n' +
    'A **paragraph**.\n\n'.repeat(2000) +
    '</details>\n\n# End';
  const parsed = parseMarkdown(source);
  expect(sectionCount(parsed.ops)).toBe(1);
  expect(sectionEncoded(parsed.ops, 0)).toBe(false);
  expect(render(source).querySelectorAll('p')).toHaveLength(2000);
});

it('keeps closed raw badges sectioned', () => {
  const source =
    '<div><img src="badge.png" alt="a > b"></div>\n\n' +
    'A **paragraph**.\n\n'.repeat(2000) +
    '<hr>\n';
  expect(sectionCount(parseMarkdown(source).ops)).toBeGreaterThan(1);
  expect(render(source).querySelectorAll('p')).toHaveLength(2000);
});

it('replays the great majority of CommonMark sections from the op buffer', () => {
  // Guards the fallback rate: raw HTML stays on the DOMPurify path by design,
  // everything else must not silently drift back onto it.
  let encoded = 0;
  let fallback = 0;
  for (const { markdown } of examples) {
    const parsed = parseMarkdown(markdown);
    for (let i = 0; i < sectionCount(parsed.ops); i++)
      if (sectionEncoded(parsed.ops, i)) encoded++;
      else fallback++;
  }
  expect(fallback).toBeLessThan(encoded / 6);
  expect(encoded).toBeGreaterThan(550);
});
