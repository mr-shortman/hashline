import type { FileDocument } from './gateway';

// u32 little-endian metadata byte length, UTF-8 JSON metadata, UTF-8 Markdown.
export function decodeDocument(buffer: ArrayBuffer): FileDocument {
  if (buffer.byteLength < 4)
    throw new Error('Unvollständige Dokumentübertragung.');
  const length = new DataView(buffer).getUint32(0, true);
  if (length > buffer.byteLength - 4)
    throw new Error('Ungültige Dokumentmetadaten.');
  // Native file reading already strips one file BOM. Do not silently consume a
  // second U+FEFF which belongs to the transmitted document text.
  const decoder = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true });
  const metadata = JSON.parse(
    decoder.decode(new Uint8Array(buffer, 4, length)),
  ) as FileDocument;
  return {
    ...metadata,
    source: decoder.decode(new Uint8Array(buffer, 4 + length)),
  };
}
