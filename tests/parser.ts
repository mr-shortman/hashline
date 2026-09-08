import { readFileSync } from 'node:fs';
import { createParser } from '../src/core/markdown/wasm';

// The parser under test is the real one: the WebAssembly build of
// src-tauri/markdown, the same crate the desktop links natively
// (docs/decisions/008-parser-reference.md, section 3).
export const parser = createParser(
  new WebAssembly.Module(readFileSync('src/generated/hashline-markdown.wasm')),
);
export const parseMarkdown = (source: string) => parser.parse(source);

import { replayFragment, sanitizeFragment } from '../src/core/content/policy';
import {
  replaySection,
  sectionCount,
  sectionEncoded,
  structuralSink,
} from '../src/core/markdown/opbuffer';
import type { DocumentGateway, FileDocument } from '../src/platform/gateway';

export const testFile: FileDocument = {
  id: '1',
  path: '/docs/a.md',
  name: 'a.md',
  digest: '0',
  readMs: 0,
};

/** The product path: replay where the parser encoded, DOMPurify where it did not. */
export function renderClean(
  source: string,
  gateway: DocumentGateway,
  file: FileDocument = testFile,
): HTMLDivElement {
  const parsed = parseMarkdown(source);
  const root = document.createElement('div');
  for (let i = 0; i < sectionCount(parsed.ops); i++) {
    const section = parsed.sections[i];
    const clean = sectionEncoded(parsed.ops, i)
      ? replayFragment(
          parsed.ops,
          parsed.text,
          i,
          section.headings,
          file,
          gateway,
        )
      : sanitizeFragment(section.html, section.headings, file, gateway);
    root.append(clean.fragment);
  }
  return root;
}

/** The parser's output as a DOM, before any content policy. */
export function renderStructural(source: string): HTMLDivElement {
  const parsed = parseMarkdown(source);
  const root = document.createElement('div');
  for (let i = 0; i < sectionCount(parsed.ops); i++) {
    if (sectionEncoded(parsed.ops, i)) {
      root.append(replaySection(parsed.ops, parsed.text, i, structuralSink));
    } else {
      const holder = document.createElement('div');
      holder.innerHTML = parsed.sections[i].html;
      root.append(...Array.from(holder.childNodes));
    }
  }
  return root;
}
