import { expect, it } from 'vitest';
import { parseMarkdown } from './parser';
import {
  decodeHeadings,
  decodeStrings,
  replaySection,
  sectionAt,
  sectionCount,
  sectionEncoded,
  sectionFirstText,
  sectionHtml,
  sectionKey,
  sectionTextRange,
} from '../src/core/markdown/opbuffer';

function replay(source: string): HTMLDivElement {
  const parsed = parseMarkdown(source);
  const root = document.createElement('div');
  for (let i = 0; i < sectionCount(parsed.ops); i++)
    if (sectionEncoded(parsed.ops, i))
      root.append(replaySection(parsed.ops, parsed.strings, i));
  return root;
}

it('carries characters beyond the BMP through the UTF-16 offsets', () => {
  // The blob is UTF-8, the offsets count UTF-16 units. An emoji is two units
  // and four bytes, so any confusion of the two shows up here.
  const root = replay('# 😀 Ende\n\nText 😀 mit ü und 𝄞 dahinter.\n');
  expect(root.querySelector('h1')?.textContent).toBe('😀 Ende');
  expect(root.querySelector('p')?.textContent).toBe(
    'Text 😀 mit ü und 𝄞 dahinter.',
  );
  // The slug rule keeps only letters, numbers and whitespace; the emoji is a
  // symbol and disappears, exactly as `slugBase` did in JavaScript.
  expect(root.querySelector('h1')?.id).toBe('doc-ende');
});

it('encodes an empty document without operations or sections', () => {
  const parsed = parseMarkdown('');
  expect(sectionCount(parsed.ops)).toBe(0);
  expect(parsed.ops.ops).toHaveLength(0);
  expect(parsed.sections).toEqual([]);
  expect(parsed.headings).toEqual([]);
});

it('reports headings with level, text and their section', () => {
  const parsed = parseMarkdown(
    '# Eins\n\n' + 'Absatz.\n\n'.repeat(4000) + '## Zwei\n',
  );
  const headings = decodeHeadings(parsed.ops, decodeStrings(parsed.ops));
  expect(headings.map((h) => [h.level, h.id, h.text])).toEqual([
    [1, 'doc-eins', 'Eins'],
    [2, 'doc-zwei', 'Zwei'],
  ]);
  expect(headings[1].section).toBeGreaterThan(0);
  expect(parsed.sections[headings[1].section].headings).toEqual([
    { id: 'doc-zwei', text: 'Zwei', level: 2 },
  ]);
});

it('gives identical content the same section key and different content another', () => {
  const block = 'Ein Absatz mit **Inhalt**.\n\n'.repeat(400);
  const a = parseMarkdown(block);
  const b = parseMarkdown(block);
  const c = parseMarkdown(block.replace('Inhalt', 'Anderes'));
  expect(sectionCount(a.ops)).toBeGreaterThan(1);
  expect(sectionKey(a.ops, 0)).toBe(sectionKey(b.ops, 0));
  expect(sectionKey(a.ops, 0)).not.toBe(sectionKey(c.ops, 0));
});

it('keeps the HTML of fallback sections and nothing else', () => {
  const parsed = parseMarkdown('<div>roh</div>\n');
  expect(sectionEncoded(parsed.ops, 0)).toBe(false);
  expect(sectionHtml(parsed.ops, parsed.strings, 0)).toContain(
    '<div>roh</div>',
  );
  const encoded = parseMarkdown('Nur **Text**.\n');
  expect(sectionEncoded(encoded.ops, 0)).toBe(true);
  expect(sectionHtml(encoded.ops, encoded.strings, 0)).toBe('');
});

it('replays tables, task lists, code languages and links', () => {
  const root = replay(
    '| A | B |\n| :-- | --: |\n| 1 | 2 |\n\n' +
      '- [x] erledigt\n- [ ] offen\n\n' +
      '```js\nconst a = 1;\n```\n\n' +
      '[Text](ziel.md "Titel")\n',
  );
  expect(root.querySelector('th')?.getAttribute('align')).toBe('left');
  expect(root.querySelectorAll('th')[1].getAttribute('align')).toBe('right');
  expect(root.querySelector('tbody td')?.textContent).toBe('1');
  expect(root.querySelectorAll('input[type=checkbox]')).toHaveLength(2);
  expect(root.querySelector('input')?.hasAttribute('checked')).toBe(true);
  expect(root.querySelector('pre code')?.className).toBe('language-js');
  expect(root.querySelector('a')?.getAttribute('title')).toBe('Titel');
});

it('keeps the document text contiguous, in order and separated by block', () => {
  const parsed = parseMarkdown(
    '# Titel\n\nfoo\n\nbar\n\n- eins\n- zwei\n\n| a | b |\n| - | - |\n| c | d |\n',
  );
  // A separator between blocks is what keeps a match from running from the end
  // of one block into the start of the next.
  expect(parsed.strings.text).toBe('Titel\nfoo\nbar\neins\nzwei\na\nb\nc\nd');
  expect(parsed.strings.text).not.toContain('foobar');
});

it('maps every text offset to the node the replay creates', () => {
  const parsed = parseMarkdown(
    '## Kapitel\n\nEin *kursiver* Absatz mit `code` und 😀.\n\n> Zitat\n',
  );
  const seen: { node: Text; offset: number; length: number }[] = [];
  const root = document.createElement('div');
  root.append(
    replaySection(parsed.ops, parsed.strings, 0, {
      attribute: (element, name, value) => element.setAttribute(name, value),
      opened: () => {},
      text: (node, offset, length) => seen.push({ node, offset, length }),
    }),
  );
  expect(seen.length).toBeGreaterThan(3);
  for (const { node, offset, length } of seen) {
    expect(parsed.strings.text.substring(offset, offset + length)).toBe(
      node.data,
    );
    expect(length).toBe(node.data.length);
  }
  // Ascending, so a binary search over the offsets is valid.
  expect(seen.map((entry) => entry.offset)).toEqual(
    [...seen.map((entry) => entry.offset)].sort((a, b) => a - b),
  );
});

it('assigns every text offset to its section', () => {
  const parsed = parseMarkdown(
    ('## Abschnitt\n\nEin Absatz mit Text.\n\n'.repeat(60) + '\n').repeat(6),
  );
  expect(sectionCount(parsed.ops)).toBeGreaterThan(1);
  for (let i = 0; i < sectionCount(parsed.ops); i++) {
    const [start, end] = sectionTextRange(parsed.ops, i);
    expect(sectionAt(parsed.ops, start)).toBe(i);
    expect(sectionAt(parsed.ops, end - 1)).toBe(i);
    expect(sectionFirstText(parsed.ops, i)).toBeGreaterThanOrEqual(start);
    expect(sectionFirstText(parsed.ops, i)).toBeLessThan(end);
  }
  const [, last] = sectionTextRange(parsed.ops, sectionCount(parsed.ops) - 1);
  expect(last).toBe(parsed.strings.text.length);
});

it('gives a raw section a separator of its own so neighbours cannot join', () => {
  const parsed = parseMarkdown('Ende\n\n<div>roh</div>\n\nAnfang\n');
  const raw = parsed.sections.findIndex(
    (_, i) => !sectionEncoded(parsed.ops, i),
  );
  expect(raw).toBeGreaterThanOrEqual(0);
  const [start, end] = sectionTextRange(parsed.ops, raw);
  expect(parsed.strings.text.substring(start, end)).toBe('\n');
  expect(parsed.strings.text).not.toContain('EndeAnfang');
});
