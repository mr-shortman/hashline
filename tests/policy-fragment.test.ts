import { expect, it } from 'vitest';
import { replayFragment, sanitizeFragment } from '../src/core/content/policy';
import {
  decodeStrings,
  encodeDocument,
  sectionEncoded,
} from '../src/core/markdown/opbuffer';
import type { DocumentGateway, FileDocument } from '../src/platform/gateway';

it('collects sanitized resources without skipping siblings or sanitizing generated placeholders', () => {
  const file: FileDocument = {
    id: '1',
    name: 'a.md',
    path: '/a.md',
    source: '',
    readMs: 0,
  };
  const gateway = { imageUrl: () => null } as unknown as DocumentGateway;
  const clean = sanitizeFragment(
    {
      html: '<input type="text"><h2 id="doc-allowed">Allowed</h2><h2 id="doc-allowed">Duplicate</h2><a href="javascript:alert(1)">blocked</a><img src="https://example.com/a.png" alt="remote"><image src="local.png"><table><tr><td colspan="9000">cell</td></tr></table><pre><code class="language-js">code</code></pre><input type="checkbox">',
      headings: [{ id: 'doc-allowed', level: 2, text: 'Allowed' }],
      parseMs: 0,
    },
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
    source: '',
    readMs: 0,
  };
  const gateway = { imageUrl: () => null } as unknown as DocumentGateway;
  const headings = [{ id: 'doc-allowed', level: 2, text: 'Allowed' }];
  // Structurally sound markup inside the encodable tables, so the replay — not
  // the fallback — has to enforce every value rule DOMPurify and the walk did.
  const html =
    '<input type="text"><h2 id="doc-allowed">Allowed</h2>' +
    '<h2 id="doc-allowed">Duplicate</h2><p id="doc-allowed">Absatz</p>' +
    '<a href="javascript:alert(1)">blocked</a>' +
    '<a href=" javascript:alert(1)">spaced</a>' +
    '<a href="https://example.com/x">extern</a>' +
    '<img src="https://example.com/a.png" alt="remote">' +
    '<img src="local.png" alt="lokal">' +
    '<table><tbody><tr><td colspan="9000" rowspan="2">cell</td></tr></tbody></table>' +
    '<pre><code class="language-js">code</code></pre>' +
    '<p class="evil">Klasse</p><input type="checkbox">';
  const buffer = encodeDocument([{ html, headings }], headings);
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
