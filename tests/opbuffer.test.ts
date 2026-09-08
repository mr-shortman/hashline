import { expect, it } from 'vitest';
import {
  decodeHeadings,
  decodeStrings,
  encodeDocument,
  replaySection,
  sectionCount,
  sectionEncoded,
  transferables,
  type OpBuffer,
} from '../src/core/markdown/opbuffer';
import { parseMarkdown } from '../src/core/markdown/parser';

function replayAll(buffer: OpBuffer, sections: readonly { html: string }[]) {
  const text = decodeStrings(buffer);
  const root = document.createElement('div');
  for (let i = 0; i < sectionCount(buffer); i++) {
    if (sectionEncoded(buffer, i)) root.append(replaySection(buffer, text, i));
    else {
      const fallback = document.createElement('div');
      fallback.innerHTML = sections[i].html;
      root.append(...fallback.childNodes);
    }
  }
  return root;
}

function expectRoundTrip(html: string) {
  const buffer = encodeDocument([{ html }]);
  expect(sectionEncoded(buffer, 0)).toBe(true);
  const expected = document.createElement('div');
  expected.innerHTML = html;
  expected.normalize();
  const actual = replayAll(buffer, [{ html }]);
  expect(actual.isEqualNode(expected)).toBe(true);
  return buffer;
}

it('round-trips paragraphs, inline markup and attributes', () => {
  expectRoundTrip(
    '<h2 id="doc-title">Titel</h2>\n<p>Ein <em>kursiver</em> und ' +
      '<strong>fetter</strong> Text mit <code>code</code> und ' +
      '<a href="other.md" title="Titel">Link</a>.</p>\n<hr>\n',
  );
});

it('round-trips text beyond the basic multilingual plane', () => {
  // The offsets are UTF-16 code units: every astral character costs two, and a
  // decoder that counted UTF-8 bytes or code points would slice the text apart.
  const buffer = expectRoundTrip(
    '<p>Vor 𝕳𝖆𝖘𝖍 nach</p>\n<p>👩‍👩‍👧‍👦 und 🇩🇪 und ✅</p>\n' +
      '<p><code>𝔸𝔹ℂ</code> 中文 <em>日本語</em></p>\n',
  );
  const text = decodeStrings(buffer);
  expect(text).toContain('𝕳𝖆𝖘𝖍');
  expect(replayAll(buffer, []).textContent).toContain('👩‍👩‍👧‍👦');
});

it('round-trips an empty document and an empty section', () => {
  const empty = encodeDocument([]);
  expect(sectionCount(empty)).toBe(0);
  expect(empty.ops).toHaveLength(0);
  expect(empty.strings).toHaveLength(0);
  const blank = encodeDocument([{ html: '' }]);
  expect(sectionEncoded(blank, 0)).toBe(true);
  expect(replaySection(blank, decodeStrings(blank), 0).childNodes).toHaveLength(
    0,
  );
});

it('round-trips nested lists, task lists and block quotes', () => {
  expectRoundTrip(
    parseMarkdown(
      '- eins\n  - zwei\n    1. drei\n    2. vier\n- [x] erledigt\n- [ ] offen\n\n' +
        '> zitiert\n>\n> - mit Liste\n',
    ).html,
  );
});

it('round-trips tables with alignment and spans', () => {
  expectRoundTrip(
    parseMarkdown('| A | B |\n| :-- | --: |\n| 1 | 2 |\n| 3 | 4 |\n').html,
  );
  expectRoundTrip(
    '<table>\n<tbody>\n<tr>\n<td colspan="2" width="120">x</td>\n</tr>\n</tbody>\n</table>\n',
  );
});

it('round-trips fenced and indented code blocks', () => {
  expectRoundTrip(
    parseMarkdown(
      '```js\nconst a = 1 < 2 && 3 > 2;\nconst s = "a&b";\n```\n\n    indented <code>\n',
    ).html,
  );
});

it('merges adjacent text runs into one operation', () => {
  // The HTML parser merges neighbouring text; the replay does not. Merging in
  // the encoder keeps `isEqualNode` usable and saves nodes at run time.
  const buffer = encodeDocument([{ html: '<p>a &amp; b &lt; c</p>' }]);
  const fragment = replaySection(buffer, decodeStrings(buffer), 0);
  expect(fragment.firstChild!.childNodes).toHaveLength(1);
  expect(fragment.textContent).toBe('a & b < c');
});

it('encodes headings with their section and keeps section boundaries', () => {
  const sections = [
    {
      html: '<h1 id="doc-a">A</h1>\n',
      headings: [{ id: 'doc-a', text: 'A', level: 1 }],
    },
    {
      html: '<h2 id="doc-b">B</h2>\n',
      headings: [{ id: 'doc-b', text: 'B', level: 2 }],
    },
  ];
  const buffer = encodeDocument(
    sections,
    sections.flatMap((s) => s.headings),
  );
  expect(sectionCount(buffer)).toBe(2);
  expect(decodeHeadings(buffer, decodeStrings(buffer))).toEqual([
    { id: 'doc-a', level: 1, section: 0 },
    { id: 'doc-b', level: 2, section: 1 },
  ]);
  const text = decodeStrings(buffer);
  expect(replaySection(buffer, text, 1).textContent).toBe('B\n');
});

it('marks sections it cannot reproduce as fallback without losing later ones', () => {
  const sections = [
    { html: '<p>ok</p>\n' },
    { html: '<p>&ouml;</p>\n' },
    { html: '<p>x</p><script>alert(1)</script>' },
    { html: '<div><p>unbalanced</div>' },
    { html: '<p>danach</p>\n' },
    { html: '<p>roh</p>\n', raw: true },
  ];
  const buffer = encodeDocument(sections);
  expect(
    Array.from({ length: sectionCount(buffer) }, (_, i) =>
      sectionEncoded(buffer, i),
    ),
  ).toEqual([true, false, false, false, true, false]);
  const text = decodeStrings(buffer);
  expect(replaySection(buffer, text, 0).textContent).toBe('ok\n');
  expect(replaySection(buffer, text, 4).textContent).toBe('danach\n');
});

it('drops attribute names outside the table and keeps the first duplicate', () => {
  const buffer = encodeDocument([
    { html: '<p onclick="x" data-x="y" id="doc-a" id="doc-b">t</p>' },
  ]);
  const element = replaySection(buffer, decodeStrings(buffer), 0)
    .firstChild as Element;
  expect(element.getAttributeNames()).toEqual(['id']);
  expect(element.getAttribute('id')).toBe('doc-a');
});

it('exposes every buffer for a zero-copy transfer', () => {
  const buffer = encodeDocument([{ html: '<p>x</p>' }]);
  const list = transferables(buffer);
  expect(list).toHaveLength(5);
  expect(new Set(list).size).toBe(5);
  expect(list).toEqual([
    buffer.ops.buffer,
    buffer.attrs.buffer,
    buffer.strings.buffer,
    buffer.sections.buffer,
    buffer.headings.buffer,
  ]);
  expect(list.every((item) => item.constructor.name === 'ArrayBuffer')).toBe(
    true,
  );
});
