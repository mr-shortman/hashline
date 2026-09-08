import { expect, it } from 'vitest';
import { parseMarkdown } from './parser';
import {
  decodeHeadings,
  decodeStrings,
  replaySection,
  sectionCount,
  sectionEncoded,
  sectionHtml,
  sectionKey,
} from '../src/core/markdown/opbuffer';

function replay(source: string): HTMLDivElement {
  const parsed = parseMarkdown(source);
  const root = document.createElement('div');
  for (let i = 0; i < sectionCount(parsed.ops); i++)
    if (sectionEncoded(parsed.ops, i))
      root.append(replaySection(parsed.ops, parsed.text, i));
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
  expect(sectionHtml(parsed.ops, parsed.text, 0)).toContain('<div>roh</div>');
  const encoded = parseMarkdown('Nur **Text**.\n');
  expect(sectionEncoded(encoded.ops, 0)).toBe(true);
  expect(sectionHtml(encoded.ops, encoded.text, 0)).toBe('');
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
