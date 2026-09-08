import { expect, it } from 'vitest';
import { decodePacket } from '../src/platform/document-packet';

function packet(
  header: object,
  words: number[][],
  strings: string,
): ArrayBuffer {
  const json = new TextEncoder().encode(JSON.stringify(header));
  const padding = (4 - (json.length % 4)) % 4;
  const body = new TextEncoder().encode(strings);
  const total =
    4 + json.length + padding + 4 * words.flat().length + body.length;
  const buffer = new ArrayBuffer(total);
  const bytes = new Uint8Array(buffer);
  new DataView(buffer).setUint32(0, json.length + padding, true);
  bytes.set(json, 4);
  bytes.fill(0x20, 4 + json.length, 4 + json.length + padding);
  let offset = 4 + json.length + padding;
  for (const group of words) {
    new Uint32Array(buffer, offset, group.length).set(group);
    offset += group.length * 4;
  }
  bytes.set(body, offset);
  return buffer;
}

it('decodes the header and views every buffer without copying', () => {
  const header = {
    id: '1',
    path: '/Grüße 日本語.md',
    name: '日本語.md',
    digest: 'abc',
    readMs: 1,
    parseMs: 2,
    layout: {
      ops: 4,
      attrs: 3,
      sections: 9,
      headings: 6,
      strings: 3,
      text: 5,
    },
  };
  const decoded = decodePacket(
    packet(
      header,
      [
        [2, 0, 5, 0],
        [1, 0, 5],
        [0, 1, 0, 0, 0, 0, 0, 0, 5],
        [1, 0, 3, 0, 0, 3],
      ],
      'ID.Hallo',
    ),
  );
  expect(decoded.header.path).toBe('/Grüße 日本語.md');
  expect(Array.from(decoded.ops.ops)).toEqual([2, 0, 5, 0]);
  expect(Array.from(decoded.ops.attrs)).toEqual([1, 0, 5]);
  expect(Array.from(decoded.ops.sections)).toEqual([0, 1, 0, 0, 0, 0, 0, 0, 5]);
  expect(Array.from(decoded.ops.headings)).toEqual([1, 0, 3, 0, 0, 3]);
  expect(new TextDecoder().decode(decoded.ops.strings)).toBe('ID.');
  expect(new TextDecoder().decode(decoded.ops.text)).toBe('Hallo');
});

it('pads the header so the word arrays stay four-byte aligned', () => {
  // A header whose JSON length is not a multiple of four still has to decode.
  const decoded = decodePacket(
    packet(
      {
        pad: 'x',
        layout: {
          ops: 4,
          attrs: 0,
          sections: 0,
          headings: 0,
          strings: 0,
          text: 0,
        },
      },
      [[2, 0, 0, 0]],
      '',
    ),
  );
  expect(Array.from(decoded.ops.ops)).toEqual([2, 0, 0, 0]);
});

it('rejects truncated and misdeclared packets', () => {
  expect(() => decodePacket(new ArrayBuffer(3))).toThrow();
  const short = new ArrayBuffer(8);
  new DataView(short).setUint32(0, 20, true);
  expect(() => decodePacket(short)).toThrow();
  expect(() =>
    decodePacket(
      packet(
        {
          layout: { ops: 400, attrs: 0, sections: 0, headings: 0, strings: 0 },
        },
        [[2, 0, 0, 0]],
        '',
      ),
    ),
  ).toThrow();
  expect(() => decodePacket(packet({ nothing: true }, [], ''))).toThrow();
});
