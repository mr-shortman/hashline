import { readFileSync } from 'node:fs';
import { expect, it } from 'vitest';
import { parseMarkdown } from '../src/core/markdown/parser';
import { sanitizeContent, sanitizeFragment } from '../src/core/content/policy';
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
