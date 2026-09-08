import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { parseMarkdown, renderClean, testFile } from './parser';
import { classifyUrl } from '../src/core/content/policy';
import type { DocumentGateway, FileDocument } from '../src/platform/gateway';

const gateway = {
  imageUrl: (_file: FileDocument, source: string) =>
    `hashline-image://localhost/1/${encodeURIComponent(source)}`,
} as DocumentGateway;

describe('Markdown contract', () => {
  it('creates unique, deterministic Unicode IDs, including suffix collisions', () => {
    const source =
      '# Grüße 日本語\n# Grüße 日本語\n# !!!\n# section\n# a\n# a\n# a-1';
    const parsed = parseMarkdown(source);
    expect(parsed.headings.map((h) => h.id)).toEqual([
      'doc-grüße-日本語',
      'doc-grüße-日本語-1',
      'doc-section',
      'doc-section-1',
      'doc-a',
      'doc-a-1',
      'doc-a-1-1',
    ]);
    expect(parseMarkdown(source).headings).toEqual(parsed.headings);
  });
  it('keeps heading text for the outline, without markup', () => {
    const parsed = parseMarkdown('## Ein **fetter** `Titel` mit [Link](a.md)');
    expect(parsed.headings).toEqual([
      {
        id: 'doc-ein-fetter-titel-mit-link',
        text: 'Ein fetter Titel mit Link',
        level: 2,
      },
    ]);
  });
  it('preserves CommonMark breaks and renders GFM as passive semantic HTML', () => {
    const root = renderClean(
      'soft\nbreak\n\nhard  \nbreak\n\n- [x] done\n\n| a | b |\n| - | - |\n| c | d |',
      gateway,
    );
    expect(root.querySelector('p')?.textContent).toBe('soft\nbreak');
    expect(root.querySelectorAll('br')).toHaveLength(1);
    expect(root.querySelector('table')).not.toBeNull();
    expect(root.querySelector('input')?.hasAttribute('disabled')).toBe(true);
  });
  it('removes active content, app impersonation and remote resource requests', () => {
    const root = renderClean(
      readFileSync('tests/fixtures/hostile.md', 'utf8'),
      gateway,
    );
    expect(
      root.querySelector('script,style,iframe,svg,form,input[type=text]'),
    ).toBeNull();
    expect(
      root.querySelector('[onerror],[onclick],[style],#root,.toolbar'),
    ).toBeNull();
    expect(root.querySelector('input')?.disabled).toBe(true);
    expect(root.querySelectorAll('#doc-passive-inhalte')).toHaveLength(1);
    expect(root.querySelectorAll('a[data-link]')).toHaveLength(1);
  });
  it('blocks remote and embedded images before any request and preserves original link targets', () => {
    const root = renderClean(
      '![x](https://example.com/a.png)\n\n![y](data:image/png;base64,aaa)\n\n[go](file.md#hi)',
      gateway,
    );
    expect(root.querySelectorAll('img[data-unavailable]')).toHaveLength(2);
    expect(root.querySelector('img[src]')).toBeNull();
    expect(root.querySelector('a')?.dataset.link).toBe('file.md#hi');
  });
  it('resolves local images through the gateway', () => {
    const root = renderClean('![x](bild.png)', gateway, testFile);
    expect(root.querySelector('img')?.getAttribute('src')).toBe(
      'hashline-image://localhost/1/bild.png',
    );
  });
  it.each([
    'javascript:alert(1)',
    'data:text/html,x',
    '//host/file',
    'file:///etc/passwd',
    'java\nscript:x',
    'foo\\bar',
  ])('blocks %s', (url) => expect(classifyUrl(url)).toBe('blocked'));
  it.each([
    'grüße%20welt.md#ziel',
    '../other.md',
    '#abc',
    'https://example.com',
    'mailto:a@example.com',
  ])('accepts intended link %s', (url) =>
    expect(classifyUrl(url)).not.toBe('blocked'),
  );
});

it('keeps locale-independent Unicode heading case mapping', () => {
  // The rule itself, applied in JavaScript, is the reference for the Rust port
  // (docs/decisions/008-parser-reference.md, section 4).
  const headings = ['I İ ı i', 'Σ ΟΣ', 'ẞ Straße', 'ＡＢＣ ﬁ', 'GRÜẞE 日本語'];
  for (const heading of headings) {
    const expected =
      heading
        .normalize('NFKC')
        .toLocaleLowerCase('und')
        .replace(/[^\p{L}\p{N}\s_-]/gu, '')
        .trim()
        .replace(/[\s_]+/g, '-') || 'section';
    expect(parseMarkdown('# ' + heading).headings[0].id).toBe(
      'doc-' + expected,
    );
  }
});
