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
  },
);
it('matches Marked for the mixed GFM fixture and cross-document reference definitions', () => {
  const source =
    readFileSync('tests/fixtures/reader.md', 'utf8') +
    '\n\n[reference][later]\n\n- list\n\n# End\n\n[later]: https://example.com\n';
  expect(optimized.parse(source)).toBe(reference.parse(source));
});
