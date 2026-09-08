import { expect, it } from 'vitest';
import { decodeDocument } from '../src/platform/document-packet';

it('decodes byte-counted metadata and preserves multiline Unicode Markdown exactly', () => {
  const metadata = {
    id: '1',
    path: '/Grüße 日本語.md',
    name: '日本語.md',
    readMs: 1,
    source: '',
  };
  const source = '\uFEFF# Grüße 日本語\n\n"quoted" \\ \\n\n';
  const header = new TextEncoder().encode(JSON.stringify(metadata));
  const body = new TextEncoder().encode(source);
  const buffer = new ArrayBuffer(4 + header.length + body.length);
  new DataView(buffer).setUint32(0, header.length, true);
  new Uint8Array(buffer, 4).set(header);
  new Uint8Array(buffer, 4 + header.length).set(body);
  expect(decodeDocument(buffer)).toEqual({ ...metadata, source });
});
it('rejects truncated document packets', () => {
  expect(() => decodeDocument(new ArrayBuffer(3))).toThrow();
  const buffer = new ArrayBuffer(8);
  new DataView(buffer).setUint32(0, 20, true);
  expect(() => decodeDocument(buffer)).toThrow();
});
