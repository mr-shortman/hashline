import { Marked, Parser } from 'marked';
import type { Heading, MarkdownSection, ParsedMarkdown } from './types';
import { BoundedTokenizer } from './tokenizer';
import { isClosedHtml } from './html-boundary';

export function slugBase(text: string): string {
  return (
    text
      .normalize('NFKC')
      .toLowerCase()
      .replace(/[^\p{L}\p{N}\s_-]/gu, '')
      .trim()
      .replace(/[\s_]+/g, '-') || 'section'
  );
}

export function parseMarkdown(
  source: string,
  sectioned = false,
): ParsedMarkdown {
  const start = performance.now();
  const headings: Heading[] = [];
  const used = new Set<string>();
  const parser = new Marked({
    gfm: true,
    breaks: false,
    async: false,
  });
  parser.setOptions({ tokenizer: new BoundedTokenizer(!source.includes('<')) });
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
  if (sectioned) {
    // Lex the entire source first: reference definitions are document-wide.
    const tokens = parser.lexer(source);
    const sections: MarkdownSection[] = [];
    let html = '';
    let headingStart = 0;
    const flush = () => {
      if (!html) return;
      sections.push({ html, headings: headings.slice(headingStart) });
      html = '';
      headingStart = headings.length;
    };
    // Raw HTML can span Markdown tokens. Keep that document in one semantic
    // fragment rather than changing the HTML tree at an artificial boundary.
    let rawHtml = false;
    if (source.includes('<'))
      parser.walkTokens(tokens, (token) => {
        if (token.type === 'html' && !isClosedHtml(token.text)) rawHtml = true;
      });
    const renderer = new Parser(parser.defaults);
    for (const token of tokens) {
      html += renderer.parse([token]);
      if (!rawHtml && html.length >= 16_384) flush();
    }
    flush();
    return { html: '', sections, headings, parseMs: performance.now() - start };
  }
  return {
    html: parser.parse(source) as string,
    headings,
    parseMs: performance.now() - start,
  };
}
