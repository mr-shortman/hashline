import { readFileSync } from 'node:fs';
import { expect, it } from 'vitest';
import { parseMarkdown } from '../src/core/markdown/parser';
import {
  replayFragment,
  sanitizeContent,
  sanitizeFragment,
} from '../src/core/content/policy';
import {
  decodeStrings,
  sectionCount,
  sectionEncoded,
} from '../src/core/markdown/opbuffer';
import type { ParsedMarkdown } from '../src/core/markdown/types';
import type { DocumentGateway, FileDocument } from '../src/platform/gateway';

const examples = JSON.parse(
  readFileSync('tests/fixtures/commonmark-0.31.2.json', 'utf8'),
) as { example: number; markdown: string }[];
const file = {
  id: 'test',
  source: '',
  name: 'test.md',
  path: '/test.md',
  readMs: 0,
} satisfies FileDocument;
const gateway = { imageUrl: () => null } as unknown as DocumentGateway;
it.each([
  'Text **strong _nested_** and [reference][end].',
  '- [x] completed **task**\n- [ ] pending [reference][end]',
  '| Column | Value |\n| --- | --- |\n| **same** | [reference][end] |',
  '[![image][end]](other.md) and [nested [link](inside.md)](outside.md)',
  '<a href="other.md">\n\nwww.example.com **text**\n\n</a>\n\nwww.example.com **text**',
  '<pre>\n\n&amp; **literal**\n\n</pre>\n\n&amp; **literal**',
])('repeated inline content preserves context: %s', (body) => {
  const source = `${body}\n\n`.repeat(12) + '\n[end]: target.md "title"\n';
  expect(
    parseMarkdown(source, true)
      .sections!.map((s) => s.html)
      .join(''),
  ).toBe(parseMarkdown(source).html);
});
it('does not share reference definitions between document parses', () => {
  for (const target of ['first.md', 'second.md']) {
    const source = '[shared][end]\n\n'.repeat(10) + `\n[end]: ${target}\n`;
    expect(
      parseMarkdown(source, true)
        .sections!.map((s) => s.html)
        .join(''),
    ).toBe(parseMarkdown(source).html);
  }
});
// Both section paths, section by section: the op buffer replay against the
// sanitized HTML it replaces. Sections the encoder refused use the HTML path on
// both sides, which is exactly what the renderer does for them.
function compareSectionPaths(parsed: ParsedMarkdown): {
  encoded: number;
  fallback: number;
} {
  const sections = parsed.sections!;
  const buffer = parsed.ops!;
  const text = decodeStrings(buffer);
  let encoded = 0;
  let fallback = 0;
  expect(sectionCount(buffer)).toBe(sections.length);
  sections.forEach((section, i) => {
    const expected = document.createElement('div');
    expected.append(
      sanitizeFragment({ ...section, parseMs: 0 }, file, gateway).fragment,
    );
    expected.normalize();
    const actual = document.createElement('div');
    if (sectionEncoded(buffer, i)) {
      encoded++;
      actual.append(
        replayFragment(buffer, text, i, section.headings, file, gateway)
          .fragment,
      );
    } else {
      fallback++;
      actual.append(
        sanitizeFragment({ ...section, parseMs: 0 }, file, gateway).fragment,
      );
    }
    actual.normalize();
    expect(actual.isEqualNode(expected)).toBe(true);
  });
  return { encoded, fallback };
}

it.each(examples)(
  'op buffer replay matches the sanitized section path for CommonMark $example',
  ({ markdown }) => {
    compareSectionPaths(parseMarkdown(markdown, true));
  },
);

it('replays the great majority of CommonMark sections from the op buffer', () => {
  // Guards the fallback rate: raw HTML and character references outside the
  // supported set stay on the DOMPurify path by design, everything else must
  // not silently drift back onto it.
  let encoded = 0;
  let fallback = 0;
  for (const { markdown } of examples) {
    const counts = compareSectionPaths(parseMarkdown(markdown, true));
    encoded += counts.encoded;
    fallback += counts.fallback;
  }
  expect(fallback).toBeLessThan(encoded / 6);
  expect(encoded).toBeGreaterThan(550);
});

it('replays a multi-section Markdown document without falling back', () => {
  const parsed = parseMarkdown(
    (
      '## Abschnitt **eins**\n\n' +
      'Ein Absatz mit [Link](other.md), `code`, *kursiv* und ![Bild](a.png).\n\n' +
      '- Punkt eins\n  - verschachtelt\n- [x] erledigt\n- [ ] offen\n\n' +
      '> Zitat mit &amp; und <https://example.com/>\n\n' +
      '| A | B |\n| :-- | --: |\n| 1 | 2 |\n\n' +
      '```js\nconst a = 1 < 2 && "x";\n```\n\n' +
      '1. eins\n2. zwei\n\n---\n\n'
    ).repeat(40),
    true,
  );
  const { encoded, fallback } = compareSectionPaths(parsed);
  expect(parsed.sections!.length).toBeGreaterThan(1);
  expect(encoded).toBe(parsed.sections!.length);
  expect(fallback).toBe(0);
});

it('keeps raw HTML sections on the sanitized HTML path', () => {
  // The reader fixture carries <details>/<summary> and a literal angle bracket.
  const parsed = parseMarkdown(
    readFileSync('tests/fixtures/reader.md', 'utf8'),
    true,
  );
  const { encoded, fallback } = compareSectionPaths(parsed);
  expect(encoded).toBe(0);
  expect(fallback).toBe(parsed.sections!.length);
});

it.each(examples)(
  'section rendering preserves CommonMark $example',
  ({ markdown }) => {
    const whole = parseMarkdown(markdown);
    const sections = parseMarkdown(markdown, true);
    const root = document.createElement('div');
    for (const section of sections.sections!)
      root.append(
        sanitizeFragment({ ...section, parseMs: 0 }, file, gateway).fragment,
      );
    expect(root.innerHTML).toBe(sanitizeContent(whole, file, gateway).html);
    expect(sections.headings).toEqual(whole.headings);
  },
);
it('resolves references and duplicate heading IDs across section boundaries', () => {
  const source =
    '## Same\n\n[link][later] with **inline text**.\n\n'.repeat(500) +
    '\n[later]: other.md#target\n';
  const whole = parseMarkdown(source);
  const parsed = parseMarkdown(source, true);
  expect(parsed.sections!.length).toBeGreaterThan(1);
  expect(parsed.sections!.map((s) => s.html).join('')).toBe(whole.html);
  expect(parsed.headings).toEqual(whole.headings);
  expect(parsed.sections!.flatMap((s) => s.headings)).toEqual(whole.headings);
});
it('does not split raw HTML containers spanning Markdown tokens', () => {
  const source =
    '<details>\n<summary>Summary</summary>\n\n' +
    'A **paragraph**.\n\n'.repeat(2000) +
    '</details>\n\n# End';
  const parsed = parseMarkdown(source, true);
  const root = document.createElement('div');
  for (const section of parsed.sections!)
    root.append(
      sanitizeFragment({ ...section, parseMs: 0 }, file, gateway).fragment,
    );
  expect(root.innerHTML).toBe(
    sanitizeContent(parseMarkdown(source), file, gateway).html,
  );
});

it('keeps closed raw badges sectioned and preserves quoted angle brackets', () => {
  const source =
    '<div><img src="badge.png" alt="a > b"></div>\n\n' +
    'A **paragraph**.\n\n'.repeat(2000) +
    '<hr>\n';
  const parsed = parseMarkdown(source, true);
  expect(parsed.sections!.length).toBeGreaterThan(1);
  const root = document.createElement('div');
  for (const section of parsed.sections!)
    root.append(
      sanitizeFragment({ ...section, parseMs: 0 }, file, gateway).fragment,
    );
  expect(root.innerHTML).toBe(
    sanitizeContent(parseMarkdown(source), file, gateway).html,
  );
});

it('keeps unmatched inline formatting from leaking across artificial boundaries', () => {
  const source =
    'Start <strong>bold\n\n' +
    'Still formatted.\n\n'.repeat(2000) +
    '</strong>\n';
  const parsed = parseMarkdown(source, true);
  const root = document.createElement('div');
  for (const section of parsed.sections!)
    root.append(
      sanitizeFragment({ ...section, parseMs: 0 }, file, gateway).fragment,
    );
  expect(root.innerHTML).toBe(
    sanitizeContent(parseMarkdown(source), file, gateway).html,
  );
});
