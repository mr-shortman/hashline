import { expect, it } from 'vitest';
import { replayFragment, sanitizeFragment } from '../src/core/content/policy';
import { decodeStrings, sectionEncoded } from '../src/core/markdown/opbuffer';
import { buildBuffer } from './opbuilder';
import type { DocumentGateway, FileDocument } from '../src/platform/gateway';

it('collects sanitized resources without skipping siblings or sanitizing generated placeholders', () => {
  const file: FileDocument = {
    id: '1',
    name: 'a.md',
    path: '/a.md',
    digest: '0',
    readMs: 0,
  };
  const gateway = { imageUrl: () => null } as unknown as DocumentGateway;
  const clean = sanitizeFragment(
    '<input type="text"><h2 id="doc-allowed">Allowed</h2><h2 id="doc-allowed">Duplicate</h2><a href="javascript:alert(1)">blocked</a><img src="https://example.com/a.png" alt="remote"><image src="local.png"><table><tr><td colspan="9000">cell</td></tr></table><pre><code class="language-js">code</code></pre><input type="checkbox">',
    [{ id: 'doc-allowed', level: 2, text: 'Allowed' }],
    file,
    gateway,
  );
  expect(clean.fragment.querySelectorAll('#doc-allowed')).toHaveLength(1);
  expect(clean.fragment.querySelectorAll('input')).toHaveLength(1);
  expect(clean.fragment.querySelector('input')?.disabled).toBe(true);
  expect(clean.fragment.querySelector('a')?.hasAttribute('href')).toBe(false);
  expect(clean.fragment.querySelector('td')?.hasAttribute('colspan')).toBe(
    false,
  );
  expect(clean.remoteImages).toBe(1);
  expect(clean.blockedImages).toBe(2);
  expect(clean.hasImages).toBe(true);
  expect(
    clean.fragment.querySelectorAll('.image-placeholder[data-search-ignore]'),
  ).toHaveLength(2);
  expect(clean.tables).toEqual(
    Array.from(clean.fragment.querySelectorAll('table')),
  );
  expect(clean.pres).toEqual(
    Array.from(clean.fragment.querySelectorAll('pre')),
  );
  expect(clean.pres[0].querySelector('code')?.className).toBe('language-js');
});

it('applies the same attribute policy when replaying the op buffer', () => {
  const file: FileDocument = {
    id: '1',
    name: 'a.md',
    path: '/a.md',
    digest: '0',
    readMs: 0,
  };
  const gateway = { imageUrl: () => null } as unknown as DocumentGateway;
  const headings = [{ id: 'doc-allowed', level: 2, text: 'Allowed' }];
  // A buffer written by hand, so the replay — not the fallback — has to enforce
  // every value rule on markup the parser would never produce.
  const buffer = buildBuffer([
    [
      { tag: 'input', attrs: [['type', 'text']] },
      { tag: 'h2', attrs: [['id', 'doc-allowed']], children: ['Allowed'] },
      { tag: 'h2', attrs: [['id', 'doc-allowed']], children: ['Duplicate'] },
      { tag: 'p', attrs: [['id', 'doc-allowed']], children: ['Absatz'] },
      {
        tag: 'a',
        attrs: [['href', 'javascript:alert(1)']],
        children: ['blocked'],
      },
      {
        tag: 'a',
        attrs: [['href', ' javascript:alert(1)']],
        children: ['spaced'],
      },
      {
        tag: 'a',
        attrs: [['href', 'https://example.com/x']],
        children: ['extern'],
      },
      {
        tag: 'img',
        attrs: [
          ['src', 'https://example.com/a.png'],
          ['alt', 'remote'],
        ],
      },
      {
        tag: 'img',
        attrs: [
          ['src', 'local.png'],
          ['alt', 'lokal'],
        ],
      },
      {
        tag: 'table',
        children: [
          {
            tag: 'tbody',
            children: [
              {
                tag: 'tr',
                children: [
                  {
                    tag: 'td',
                    attrs: [
                      ['colspan', '9000'],
                      ['rowspan', '2'],
                    ],
                    children: ['cell'],
                  },
                ],
              },
            ],
          },
        ],
      },
      {
        tag: 'pre',
        children: [
          {
            tag: 'code',
            attrs: [['class', 'language-js']],
            children: ['code'],
          },
        ],
      },
      { tag: 'p', attrs: [['class', 'evil']], children: ['Klasse'] },
      { tag: 'input', attrs: [['type', 'checkbox']] },
    ],
  ]);
  expect(sectionEncoded(buffer, 0)).toBe(true);
  const clean = replayFragment(
    buffer,
    decodeStrings(buffer),
    0,
    headings,
    file,
    gateway,
  );
  const { fragment } = clean;
  expect(fragment.querySelectorAll('#doc-allowed')).toHaveLength(1);
  expect(fragment.querySelector('h2')?.id).toBe('doc-allowed');
  expect(fragment.querySelector('p')?.hasAttribute('id')).toBe(false);
  expect(fragment.querySelectorAll('input')).toHaveLength(1);
  expect(fragment.querySelector('input')?.disabled).toBe(true);
  expect(fragment.querySelector('input')?.getAttribute('tabindex')).toBe('-1');
  const links = fragment.querySelectorAll('a');
  expect(links[0].hasAttribute('href')).toBe(false);
  expect(links[0].hasAttribute('data-link')).toBe(false);
  expect(links[1].hasAttribute('href')).toBe(false);
  expect(links[2].getAttribute('href')).toBe('#');
  expect(links[2].dataset.link).toBe('https://example.com/x');
  expect(fragment.querySelector('td')?.hasAttribute('colspan')).toBe(false);
  expect(fragment.querySelector('td')?.getAttribute('rowspan')).toBe('2');
  expect(fragment.querySelector('.evil')).toBe(null);
  expect(clean.pres[0].querySelector('code')?.className).toBe('language-js');
  expect(clean.tables).toEqual(Array.from(fragment.querySelectorAll('table')));
  expect(clean.remoteImages).toBe(1);
  expect(clean.blockedImages).toBe(2);
  expect(clean.hasImages).toBe(true);
  expect(
    fragment.querySelectorAll('.image-placeholder[data-search-ignore]'),
  ).toHaveLength(2);
});
