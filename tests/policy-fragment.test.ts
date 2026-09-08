import { expect, it } from 'vitest';
import { sanitizeFragment } from '../src/core/content/policy';
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
