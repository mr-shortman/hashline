import { Marked } from 'marked';
import type { Heading, ParsedMarkdown } from './types';
import { BoundedTokenizer } from './tokenizer';

export function slugBase(text: string): string {
  return (
    text
      .normalize('NFKC')
      .toLocaleLowerCase('und')
      .replace(/[^\p{L}\p{N}\s_-]/gu, '')
      .trim()
      .replace(/[\s_]+/g, '-') || 'section'
  );
}

export function parseMarkdown(source: string): ParsedMarkdown {
  const start = performance.now();
  const headings: Heading[] = [];
  const used = new Set<string>();
  const parser = new Marked({
    gfm: true,
    breaks: false,
    async: false,
  });
  parser.setOptions({ tokenizer: new BoundedTokenizer() });
  parser.use({
    renderer: {
      html({ text }) {
        // Raw HTML cannot claim the IDs reserved for parsed Markdown headings.
        return text.replace(/\bid\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]+)/gi, '');
      },
      heading({ tokens, depth }) {
        const html = this.parser.parseInline(tokens);
        const text = this.parser
          .parseInline(tokens, this.parser.textRenderer)
          .replace(/<[^>]*>/g, '')
          .replace(
            /&(?:amp|lt|gt|quot|#39);/g,
            (entity) =>
              ({
                '&amp;': '&',
                '&lt;': '<',
                '&gt;': '>',
                '&quot;': '"',
                '&#39;': "'",
              })[entity]!,
          );
        const base = slugBase(text);
        let slug = base;
        let suffix = 1;
        while (used.has(slug)) slug = `${base}-${suffix++}`;
        used.add(slug);
        const id = `doc-${slug}`;
        headings.push({ id, text, level: depth });
        return `<h${depth} id="${id}">${html}</h${depth}>\n`;
      },
    },
  });
  return {
    html: parser.parse(source) as string,
    headings,
    parseMs: performance.now() - start,
  };
}
