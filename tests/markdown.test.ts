import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { parseMarkdown } from '../src/core/markdown/parser';
import { classifyUrl, sanitizeContent } from '../src/core/content/policy';
import type { DocumentGateway, FileDocument } from '../src/platform/gateway';

const file: FileDocument = {
  id: '1',
  path: '/docs/a.md',
  name: 'a.md',
  source: '',
  readMs: 0,
};
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
  it('preserves CommonMark breaks and renders GFM as passive semantic HTML', () => {
    const parsed = parseMarkdown(
      'soft\nbreak\n\nhard  \nbreak\n\n- [x] done\n\n| a | b |\n| - | - |\n| c | d |',
    );
    expect(parsed.html).toContain('soft\nbreak');
    expect(parsed.html).toContain('hard<br>');
    expect(parsed.html).toContain('<table>');
    expect(parsed.html).toContain('disabled');
  });
  it('removes active content, app impersonation and remote resource requests', () => {
    const parsed = parseMarkdown(
      readFileSync('tests/fixtures/hostile.md', 'utf8'),
    );
    const { html } = sanitizeContent(parsed, file, gateway);
    const root = document.createElement('div');
    root.innerHTML = html;
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
    const { html, blockedImages } = sanitizeContent(
      parseMarkdown(
        '![x](https://example.com/a.png)\n![y](data:image/png;base64,aaa)\n[go](file.md#hi)',
      ),
      file,
      gateway,
    );
    expect(blockedImages).toBe(2);
    expect(html).not.toContain('src=');
    expect(html).toContain('data-link="file.md#hi"');
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
