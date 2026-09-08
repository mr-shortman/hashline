import { decodePacket } from '../../platform/document-packet';
import { readDocument } from './parser';
import type { ParsedDocument } from './types';

// The WebAssembly build of `src-tauri/markdown`, used by the browser preview
// and the test suite. The desktop never loads it; there the same crate is
// linked natively and delivers its packet over IPC
// (docs/decisions/008-parser-reference.md, section 3).
//
// The interface is bytes in, bytes out. No wasm-bindgen, no generated glue:
// `alloc` a buffer, write UTF-8 into the module's memory, `parse`, copy the
// packet out, `free`.
interface ParserExports {
  memory: WebAssembly.Memory;
  alloc(length: number): number;
  free(pointer: number, length: number): void;
  parse(pointer: number, length: number): number;
  result_ptr(): number;
}

export interface MarkdownParser {
  parse(source: string): ParsedDocument;
}

export function createParser(module: WebAssembly.Module): MarkdownParser {
  const instance = new WebAssembly.Instance(module, {});
  const exports = instance.exports as unknown as ParserExports;
  const encoder = new TextEncoder();
  return {
    parse(source) {
      const start = performance.now();
      const bytes = encoder.encode(source);
      const input = exports.alloc(bytes.length);
      try {
        new Uint8Array(exports.memory.buffer, input, bytes.length).set(bytes);
        const length = exports.parse(input, bytes.length);
        if (!length && bytes.length)
          throw new Error('Das Markdown-Dokument konnte nicht gelesen werden.');
        // The packet must leave the module's memory: growing it during the next
        // parse would detach every view the renderer holds.
        const packet = new Uint8Array(
          exports.memory.buffer,
          exports.result_ptr(),
          length,
        ).slice();
        return readDocument(
          decodePacket(packet.buffer as ArrayBuffer).ops,
          performance.now() - start,
        );
      } finally {
        exports.free(input, bytes.length);
      }
    },
  };
}

let pending: Promise<MarkdownParser> | undefined;

/** Compiles the module once per page and reuses it for every document. */
export function loadParser(): Promise<MarkdownParser> {
  pending ??= (async () => {
    const url = new URL(
      '../../generated/hashline-markdown.wasm',
      import.meta.url,
    );
    const response = await fetch(url);
    return createParser(
      await WebAssembly.compile(await response.arrayBuffer()),
    );
  })();
  return pending;
}
