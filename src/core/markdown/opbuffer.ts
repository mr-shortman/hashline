// Op buffer — the renderer-independent document representation from
// docs/decisions/007-performance-path.md section 4.
//
// Since P2.3 the buffer is *produced in Rust* (`src-tauri/markdown`) and
// arrives ready to replay: over IPC on the desktop, from the WebAssembly build
// of the same crate in the browser preview and the tests. This module only
// reads it. The encoder that used to live here scanned marked's HTML output and
// disappeared with marked (docs/decisions/008-parser-reference.md).
//
// OFFSET DECISION (the most common source of errors with this approach; see
// 007 section 4, "Fallstricke"):
//
//   `strings` stays a UTF-8 blob because that is compact and transferable.
//   Every offset and length stored in a TEXT op, an attribute entry or a
//   heading entry is a **UTF-16 code unit offset into the JS string that
//   `decodeStrings()` produces once per document**, not a byte offset.
//
// The Rust encoder carries the UTF-16 length forward while it appends pieces,
// which costs one addition per piece. The replay can then use
// `String.prototype.substring` without a byte-to-UTF-16 translation table.
// Byte offsets would have forced that table into the hot path — exactly the
// work this format exists to remove.
//
// Layout (all arrays are Uint32Array except `strings`):
//
//   ops       4 words per operation  [kind, a, b, c]
//               kind 0 OPEN   a = tagId      b = attrStart  c = attrCount
//               kind 1 CLOSE  a, b, c = 0
//               kind 2 TEXT   a = strOffset  b = strLen     c = 0
//   attrs     3 words per attribute  [nameId, strOffset, strLen]
//   strings   UTF-8 blob: attribute values, heading ids and texts, and the
//             HTML of fallback sections — never document text
//   text      UTF-8 blob: the document's text in document order, with a
//             separator between blocks. TEXT operations index into it, and the
//             search runs on it directly (007, P2.4)
//   sections  9 words per section    [opStart, opCount, flags,
//                                     hashLow, hashHigh, htmlOffset, htmlLen,
//                                     textStart, textLen]
//   headings  6 words per heading    [level, idOffset, idLen, sectionIndex,
//                                     textOffset, textLen]
//
// Deviations from 007, each with a reason:
//   * `sections` carries `flags`. A section that could not be encoded must be
//     distinguishable from a section that legitimately produced no operations,
//     and `[opStart, opCount]` cannot express that.
//   * `sections` carries a 64-bit content hash. The viewport recognizes
//     unchanged sections by it; the HTML string it used to compare disappeared
//     with the encoder (docs/decisions/008, section 5).
//   * `sections` carries the HTML of fallback sections, as a slice of the same
//     string blob. Raw HTML has no opcode and stays on the DOMPurify path.
//   * `headings` carries the heading text, which the outline needs and which no
//     longer travels as JSON beside the buffer.
//   * The text is a blob of its own. It is what the search reads, so it must be
//     contiguous and free of attribute values; and `sections` carries each
//     section's range in it, so a match maps back to a section in one lookup.
//
// Every OPEN has a matching CLOSE, void elements included. That keeps the
// replay a single stack without a void-element table of its own.

// Exactly the tag allowlist the sanitizer enforces; policy.ts imports it so the
// two can never drift. A tag outside this table has no id and is therefore not
// expressible in the buffer at all, rather than being removed afterwards.
// `src-tauri/markdown/src/opbuffer.rs` carries the same two tables; the round
// trip over the real parser in tests/opbuffer.test.ts is what keeps them aligned.
export const ALLOWED_TAGS = [
  'p',
  'h1',
  'h2',
  'h3',
  'h4',
  'h5',
  'h6',
  'em',
  'strong',
  'del',
  's',
  'code',
  'pre',
  'blockquote',
  'ul',
  'ol',
  'li',
  'hr',
  'br',
  'a',
  'img',
  'table',
  'thead',
  'tbody',
  'tfoot',
  'tr',
  'th',
  'td',
  'input',
  'sup',
  'sub',
  'kbd',
  'details',
  'summary',
  'div',
  'span',
  'dl',
  'dt',
  'dd',
] as const;

// Exactly the attribute allowlist the sanitizer enforces. Attribute *values*
// stay subject to inspection during replay; see policy.ts.
export const ALLOWED_ATTR = [
  'id',
  'href',
  'src',
  'alt',
  'title',
  'class',
  'start',
  'type',
  'checked',
  'disabled',
  'colspan',
  'rowspan',
  'align',
  'width',
  'height',
  'open',
] as const;

export const OP_OPEN = 0;
export const OP_CLOSE = 1;
export const OP_TEXT = 2;
export const OP_WORDS = 4;
export const ATTR_WORDS = 3;
export const SECTION_WORDS = 9;
export const HEADING_WORDS = 6;
/** The section could not be encoded; the renderer must sanitize its HTML. */
export const SECTION_FALLBACK = 1;

export interface OpBuffer {
  ops: Uint32Array;
  attrs: Uint32Array;
  strings: Uint8Array;
  text: Uint8Array;
  sections: Uint32Array;
  headings: Uint32Array;
}

/** Both blobs, decoded once per document. */
export interface OpStrings {
  /** Attribute values, heading ids and texts, fallback HTML. */
  readonly strings: string;
  /** The document's text in document order. */
  readonly text: string;
}

// The five arrays are views on one received packet; nothing copies or
// transfers them any more, because the parser no longer runs in a worker.

/** Decodes both blobs. Call once per document, not once per section. */
export function decodeStrings(buffer: OpBuffer): OpStrings {
  const decoder = new TextDecoder();
  return {
    strings: decoder.decode(buffer.strings),
    text: decoder.decode(buffer.text),
  };
}

export function sectionCount(buffer: OpBuffer): number {
  return buffer.sections.length / SECTION_WORDS;
}

/** False when the section must be sanitized through the HTML path instead. */
export function sectionEncoded(buffer: OpBuffer, index: number): boolean {
  const base = index * SECTION_WORDS;
  return (
    base < buffer.sections.length &&
    (buffer.sections[base + 2] & SECTION_FALLBACK) === 0
  );
}

/**
 * Content identity of a section, for recognizing unchanged sections across
 * reloads. A 64-bit hash from the parser, not a comparison: holding the
 * previous document's buffers for an exact comparison would cost tens of
 * megabytes at 10 MiB (docs/decisions/008, section 5).
 */
export function sectionKey(buffer: OpBuffer, index: number): string {
  const base = index * SECTION_WORDS;
  return `${buffer.sections[base + 3]}:${buffer.sections[base + 4]}`;
}

/** The HTML of a fallback section; empty for encoded sections. */
export function sectionHtml(
  buffer: OpBuffer,
  strings: OpStrings,
  index: number,
): string {
  const base = index * SECTION_WORDS;
  return strings.strings.substring(
    buffer.sections[base + 5],
    buffer.sections[base + 5] + buffer.sections[base + 6],
  );
}

/** Where the section's text lies in the document text: [start, end). */
export function sectionTextRange(
  buffer: OpBuffer,
  index: number,
): [number, number] {
  const base = index * SECTION_WORDS;
  return [
    buffer.sections[base + 7],
    buffer.sections[base + 7] + buffer.sections[base + 8],
  ];
}

/**
 * Document-text offset of the section's first text run. Offsets inside a
 * section are kept relative to it, so a section reused across reloads keeps a
 * valid text-node map even though the document around it moved.
 */
export function sectionFirstText(buffer: OpBuffer, index: number): number {
  const base = index * SECTION_WORDS;
  const start = buffer.sections[base];
  const end = start + buffer.sections[base + 1];
  for (let i = start; i < end; i++)
    if (buffer.ops[i * OP_WORDS] === OP_TEXT)
      return buffer.ops[i * OP_WORDS + 1];
  return 0;
}

/** The section a document-text offset falls into, or -1. */
export function sectionAt(buffer: OpBuffer, offset: number): number {
  let low = 0;
  let high = sectionCount(buffer) - 1;
  while (low <= high) {
    const mid = (low + high) >> 1;
    const base = mid * SECTION_WORDS;
    if (offset < buffer.sections[base + 7]) high = mid - 1;
    else if (offset >= buffer.sections[base + 7] + buffer.sections[base + 8])
      low = mid + 1;
    else return mid;
  }
  return -1;
}

export interface DecodedHeading {
  readonly id: string;
  readonly text: string;
  readonly level: number;
  readonly section: number;
}

export function decodeHeadings(
  buffer: OpBuffer,
  strings: OpStrings,
): DecodedHeading[] {
  const text = strings.strings;
  const headings: DecodedHeading[] = [];
  for (let i = 0; i < buffer.headings.length; i += HEADING_WORDS)
    headings.push({
      level: buffer.headings[i],
      id: text.substring(
        buffer.headings[i + 1],
        buffer.headings[i + 1] + buffer.headings[i + 2],
      ),
      section: buffer.headings[i + 3],
      text: text.substring(
        buffer.headings[i + 4],
        buffer.headings[i + 4] + buffer.headings[i + 5],
      ),
    });
  return headings;
}

/**
 * Where the attribute and element policy of the replay lives. The buffer
 * guarantees the tag and attribute *names*; the values stay the sink's
 * responsibility, exactly as in `sanitizeFragment`.
 */
export interface ReplaySink {
  /** Called in document order, before `opened` for the same element. */
  attribute(element: Element, name: string, value: string): void;
  /** Called once the element carries its attributes. */
  opened(element: Element): void;
  /**
   * Called for every text node with its offset in the document text. This is
   * the mapping the search uses to turn a text offset back into a DOM position
   * without ever indexing the DOM.
   */
  text?(node: Text, offset: number, length: number): void;
}

/** Structural replay without any value policy; for round-trip tests. */
export const structuralSink: ReplaySink = {
  attribute: (element, name, value) => element.setAttribute(name, value),
  opened: () => {},
};

export function replaySection(
  buffer: OpBuffer,
  strings: OpStrings,
  index: number,
  sink: ReplaySink = structuralSink,
  owner: Document = document,
): DocumentFragment {
  const fragment = owner.createDocumentFragment();
  const base = index * SECTION_WORDS;
  const start = buffer.sections[base];
  const end = start + buffer.sections[base + 1];
  const { ops, attrs } = buffer;
  const stack: (Element | DocumentFragment)[] = [fragment];
  let depth = 0;
  for (let i = start; i < end; i++) {
    const op = i * OP_WORDS;
    switch (ops[op]) {
      case OP_OPEN: {
        const element = owner.createElement(ALLOWED_TAGS[ops[op + 1]]);
        const attrEnd = ops[op + 2] + ops[op + 3];
        for (let a = ops[op + 2]; a < attrEnd; a++) {
          const word = a * ATTR_WORDS;
          sink.attribute(
            element,
            ALLOWED_ATTR[attrs[word]],
            strings.strings.substring(
              attrs[word + 1],
              attrs[word + 1] + attrs[word + 2],
            ),
          );
        }
        sink.opened(element);
        stack[depth].append(element);
        stack[++depth] = element;
        break;
      }
      case OP_CLOSE:
        depth--;
        break;
      default: {
        const node = owner.createTextNode(
          strings.text.substring(ops[op + 1], ops[op + 1] + ops[op + 2]),
        );
        stack[depth].append(node);
        sink.text?.(node, ops[op + 1], ops[op + 2]);
      }
    }
  }
  return fragment;
}
