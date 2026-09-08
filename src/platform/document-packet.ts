import type { OpBuffer } from '../core/markdown/opbuffer';

export interface PacketLayout {
  ops: number;
  attrs: number;
  sections: number;
  headings: number;
  strings: number;
}
export interface PacketHeader {
  layout: PacketLayout;
  [field: string]: unknown;
}
export interface DocumentPacket {
  header: PacketHeader;
  ops: OpBuffer;
}

const fail = (message: string): never => {
  throw new Error(message);
};

/**
 * u32 little-endian header byte length, UTF-8 JSON header padded to a multiple
 * of four, then the op buffer: ops, attrs, sections and headings as u32 arrays
 * followed by the UTF-8 string blob. The padding is what makes the typed-array
 * views possible without copying — see `src-tauri/markdown/src/packet.rs`.
 */
export function decodePacket(buffer: ArrayBuffer): DocumentPacket {
  if (buffer.byteLength < 4) fail('Unvollständige Dokumentübertragung.');
  const headerLength = new DataView(buffer).getUint32(0, true);
  if (headerLength % 4 !== 0 || headerLength > buffer.byteLength - 4)
    fail('Ungültige Dokumentmetadaten.');
  // Native file reading already strips one file BOM. Do not silently consume a
  // second U+FEFF which belongs to the transmitted document text.
  const decoder = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true });
  const header = JSON.parse(
    decoder.decode(new Uint8Array(buffer, 4, headerLength)),
  ) as PacketHeader;
  const layout = header.layout;
  if (
    !layout ||
    ['ops', 'attrs', 'sections', 'headings', 'strings'].some(
      (field) => !Number.isInteger(layout[field as keyof PacketLayout]),
    )
  )
    fail('Ungültige Dokumentmetadaten.');
  let offset = 4 + headerLength;
  const words = (count: number): Uint32Array => {
    if (offset + count * 4 > buffer.byteLength)
      fail('Unvollständige Dokumentübertragung.');
    const view = new Uint32Array(buffer, offset, count);
    offset += count * 4;
    return view;
  };
  const ops = words(layout.ops);
  const attrs = words(layout.attrs);
  const sections = words(layout.sections);
  const headings = words(layout.headings);
  if (offset + layout.strings > buffer.byteLength)
    fail('Unvollständige Dokumentübertragung.');
  const strings = new Uint8Array(buffer, offset, layout.strings);
  return { header, ops: { ops, attrs, strings, sections, headings } };
}
