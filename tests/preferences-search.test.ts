import { readFileSync } from 'node:fs';
import { parseMarkdown } from '../src/core/markdown/parser';
import { describe, expect, it, vi } from 'vitest';
import { validatePreferences } from '../src/features/preferences/preferences';
import {
  findRanges,
  findMatches,
  toRange,
  indexText,
} from '../src/features/search';

describe('preferences', () => {
  it('recovers from invalid versions and validates stored values and LRU bound', () => {
    expect(validatePreferences({ version: 2, theme: 'dark' }).theme).toBe(
      'system',
    );
    expect(
      validatePreferences({ version: 1, zoom: 999, theme: 'bad' }).zoom,
    ).toBe(200);
    expect(validatePreferences({ version: 1, zoom: NaN }).zoom).toBe(100);
    expect(
      validatePreferences({ version: 1, recent: [null, {}, { path: 1 }] })
        .recent,
    ).toEqual([]);
    expect(
      validatePreferences({
        version: 1,
        recent: Array.from({ length: 150 }, (_, i) => ({
          path: String(i),
          heading: '',
          previous: '',
          offset: 0,
          progress: 0.5,
        })),
      }).recent,
    ).toHaveLength(100);
  });
});
describe('visible text search', () => {
  it('finds literal case-insensitive matches across inline nodes, code and table cells', () => {
    const root = document.createElement('article');
    root.innerHTML =
      '<p>Na<strong>del</strong> a.b</p><table><tr><td>NADEL</td></tr></table><pre><code>Nadel</code><button>Kopieren</button></pre><details><summary>Mehr</summary><p>Nadel</p></details>';
    const index = indexText(root);
    expect(findRanges(index, 'nadel').map((r) => r.toString())).toEqual([
      'Nadel',
      'NADEL',
      'Nadel',
    ]);
    expect(findRanges(index, 'a.b')).toHaveLength(1);
    expect(findRanges(index, 'Kopieren')).toHaveLength(0);
    root.querySelector('details')!.open = true;
    expect(findRanges(indexText(root), 'nadel')).toHaveLength(4);
  });
  it('preserves DOM offsets for Unicode before matches', () => {
    const root = document.createElement('article');
    root.textContent = 'İ 🐈 Nadel';
    expect(findRanges(indexText(root), 'nadel')[0].toString()).toBe('Nadel');
  });
});

describe('search traversal contract', () => {
  it.each(['hostile.md', 'reader.md', 'search-context.html'])(
    'preserves text, document order and UTF-16 part boundaries: %s',
    (fixture) => {
      const root = document.createElement('section');
      root.innerHTML = parseMarkdown(
        readFileSync(`tests/fixtures/${fixture}`, 'utf8'),
      ).html;
      const index = indexText(root);
      expect({
        text: index.text,
        parts: index.parts.map(({ node, start, end }) => ({
          text: node.data,
          start,
          end,
        })),
      }).toMatchSnapshot();
    },
  );
  it('indexes the deep-list stress fixture without call-stack recursion', () => {
    const root = document.createElement('section');
    root.innerHTML = parseMarkdown(
      readFileSync('tests/fixtures/deep-list.md', 'utf8'),
    ).html;
    const index = indexText(root);
    expect(index.parts.length).toBeGreaterThan(100);
    expect(index.parts.map((part) => part.node.data).join('')).toBe(
      root.textContent,
    );
  });
  it('honors context on the indexed root itself', () => {
    const root = document.createElement('details');
    root.innerHTML = '<summary>visible</summary><p>closed</p>';
    expect(indexText(root).text).toBe('visible');
    root.hidden = true;
    expect(indexText(root).parts).toEqual([]);
  });
});

it('keeps matches as offsets until paint, including cross-node Unicode matches', () => {
  const root = document.createElement('section');
  root.innerHTML = '<p>İ 🐈 Na<strong>del</strong> a.b NADEL</p>';
  const index = indexText(root);
  const createRange = vi.spyOn(document, 'createRange');
  const matches = findMatches(index, 'nadel');
  expect(createRange).not.toHaveBeenCalled();
  expect(matches).toHaveLength(2);
  expect(matches[0]).toEqual({
    startNode: root.querySelector('p')!.firstChild,
    startOffset: 5,
    endNode: root.querySelector('strong')!.firstChild,
    endOffset: 3,
  });
  expect(matches.map(toRange).map((range) => range.toString())).toEqual([
    'Nadel',
    'NADEL',
  ]);
  createRange.mockRestore();
  expect(findMatches(index, '')).toEqual([]);
  expect(findMatches(index, 'a.b').map(toRange)[0].toString()).toBe('a.b');
});
