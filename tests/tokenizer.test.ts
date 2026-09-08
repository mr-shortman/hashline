import { readFileSync } from 'node:fs';
import { expect, it } from 'vitest';
import { Marked } from 'marked';
import { BoundedTokenizer } from '../src/core/markdown/tokenizer';

const examples = JSON.parse(
  readFileSync('tests/fixtures/commonmark-0.31.2.json', 'utf8'),
) as { example: number; section: string; markdown: string }[];
const reference = new Marked({ gfm: true, breaks: false });
const optimized = new Marked({ gfm: true, breaks: false }).setOptions({
  tokenizer: new BoundedTokenizer(),
});
it.each(examples)(
  'matches the unchanged Marked parser: CommonMark $example ($section)',
  ({ markdown }) => {
    expect(optimized.parse(markdown)).toBe(reference.parse(markdown));
    const cached = new Marked({ gfm: true, breaks: false }).setOptions({
      tokenizer: new BoundedTokenizer(!markdown.includes('<')),
    });
    expect(cached.parse(markdown)).toBe(reference.parse(markdown));
    const repeated = `${markdown}\n\nSeparator.\n\n${markdown}`;
    expect(cached.parse(repeated)).toBe(reference.parse(repeated));
  },
);
it.each([
  '- first\n- second\n  - nested\n\n',
  '- first\n\n- second\n\n',
  '1. ordered\n2. list\n\n',
  '- ## Heading\n\n  Paragraph **text**.\n\n',
  '- https://example.com\n- *emphasis* and `code`\n\n',
  '- [x] task\n- [link][ref]\n\n',
])(
  'repeated list reuse matches Marked without sharing mutable outer tokens: %s',
  (list) => {
    const source =
      ('Before.\n\n' + list + 'After.\n\n').repeat(20) +
      '\n[ref]: destination.md\n';
    const cached = new Marked({ gfm: true, breaks: false }).setOptions({
      tokenizer: new BoundedTokenizer(true),
    });
    expect(cached.parse(source)).toBe(reference.parse(source));
    expect(cached.parse(source.replace('destination.md', 'other.md'))).toBe(
      reference.parse(source.replace('destination.md', 'other.md')),
    );
  },
);
it('matches Marked for the mixed GFM fixture and cross-document reference definitions', () => {
  const source =
    readFileSync('tests/fixtures/reader.md', 'utf8') +
    '\n\n[reference][later]\n\n- list\n\n# End\n\n[later]: https://example.com\n';
  expect(optimized.parse(source)).toBe(reference.parse(source));
});

it.each([
  'HTTP://example.com HTTPS://example.com FTP://example.com www.example.com',
  'a+b_c.d-e@example-domain.test **strong** _em_ plain text',
  'WWW.example.com ftp://example.com/path?a=1&b=2',
  'text@example.com Grüße_日本語 plainword **nested _text_**',
])('preserves URL and emphasis grammar for %s', (source) => {
  expect(optimized.parse(source)).toBe(reference.parse(source));
});
