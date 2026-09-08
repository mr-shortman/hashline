import type { Heading } from './types';

// Op buffer — the renderer-independent document representation from
// docs/decisions/007-performance-path.md section 4.
//
// The HTML string is a detour that is paid for three times: the worker
// serializes a tree to text, DOMPurify parses that text back into a tree inside
// a foreign document, and the nodes are adopted afterwards. The op buffer keeps
// the parser output in four transferable typed arrays instead, so the renderer
// only creates nodes.
//
// OFFSET DECISION (the most common source of errors with this approach; see
// 007 section 4, "Fallstricke"):
//
//   `strings` stays a UTF-8 blob because that is compact and transferable.
//   Every offset and length stored in a TEXT op, an attribute entry or a
//   heading entry is a **UTF-16 code unit offset into the JS string that
//   `decodeStrings()` produces once per document**, not a byte offset.
//
// The encoder carries the UTF-16 length forward incrementally while it appends
// pieces, which costs one addition per piece. The replay can then use
// `String.prototype.substring` without a byte-to-UTF-16 translation table, and
// a later Rust encoder (P2.3) can count UTF-16 units during the pass it has to
// make anyway. Byte offsets would have forced a translation table into the hot
// path — exactly the work this format exists to remove.
//
// Layout (all arrays are Uint32Array except `strings`):
//
//   ops       4 words per operation  [kind, a, b, c]
//               kind 0 OPEN   a = tagId      b = attrStart  c = attrCount
//               kind 1 CLOSE  a, b, c = 0
//               kind 2 TEXT   a = strOffset  b = strLen     c = 0
//   attrs     3 words per attribute  [nameId, strOffset, strLen]
//   strings   UTF-8 blob
//   sections  3 words per section    [opStart, opCount, flags]
//   headings  4 words per heading    [level, idOffset, idLen, sectionIndex]
//
// Deviation from 007: `sections` carries a third word. A section that could not
// be encoded needs to be distinguishable from a section that legitimately
// produced no operations, and `[0, 0]` cannot express that. `flags` carries
// SECTION_FALLBACK for sections the renderer must sanitize the old way.
//
// Every OPEN has a matching CLOSE, void elements included. That keeps the
// replay a single stack without a void-element table of its own.

// Exactly the tag allowlist the sanitizer enforces; policy.ts imports it so the
// two can never drift. A tag outside this table has no id and is therefore not
// expressible in the buffer at all, rather than being removed afterwards.
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

const TAG_ID = new Map(
  ALLOWED_TAGS.map((tag, index) => [tag as string, index]),
);
const ATTR_ID = new Map(
  ALLOWED_ATTR.map((name, index) => [name as string, index]),
);
const VOID_TAGS = new Set(['br', 'hr', 'img', 'input']);

export const OP_OPEN = 0;
export const OP_CLOSE = 1;
export const OP_TEXT = 2;
export const OP_WORDS = 4;
export const ATTR_WORDS = 3;
export const SECTION_WORDS = 3;
export const HEADING_WORDS = 4;
/** The section could not be encoded; the renderer must sanitize its HTML. */
export const SECTION_FALLBACK = 1;

export interface OpBuffer {
  ops: Uint32Array;
  attrs: Uint32Array;
  strings: Uint8Array;
  sections: Uint32Array;
  headings: Uint32Array;
}

export interface EncodableSection {
  readonly html: string;
  readonly headings?: readonly Heading[];
  /** The section contains raw HTML; see `encodeDocument`. */
  readonly raw?: boolean;
}

/** The buffers of `buffer`, for a `postMessage` transfer list. */
export function transferables(buffer: OpBuffer): ArrayBuffer[] {
  return [
    buffer.ops.buffer as ArrayBuffer,
    buffer.attrs.buffer as ArrayBuffer,
    buffer.strings.buffer as ArrayBuffer,
    buffer.sections.buffer as ArrayBuffer,
    buffer.headings.buffer as ArrayBuffer,
  ];
}

class Words {
  private data = new Uint32Array(1024);
  length = 0;
  private room(words: number): void {
    if (this.length + words <= this.data.length) return;
    let size = this.data.length * 2;
    while (size < this.length + words) size *= 2;
    const grown = new Uint32Array(size);
    grown.set(this.data);
    this.data = grown;
  }
  push3(a: number, b: number, c: number): void {
    this.room(3);
    this.data[this.length++] = a;
    this.data[this.length++] = b;
    this.data[this.length++] = c;
  }
  push4(a: number, b: number, c: number, d: number): void {
    this.room(4);
    this.data[this.length++] = a;
    this.data[this.length++] = b;
    this.data[this.length++] = c;
    this.data[this.length++] = d;
  }
  take(): Uint32Array {
    return this.data.slice(0, this.length);
  }
}

// Only references marked emits for its own escaping, plus the numeric forms.
// Anything else — `&ouml;`, `&MadeUpEntity;` — makes the section fall back
// rather than shipping a partial entity table that silently produces different
// text than the HTML parser would.
const NAMED_ENTITIES: Record<string, string> = {
  amp: '&',
  lt: '<',
  gt: '>',
  quot: '"',
  apos: "'",
  nbsp: '\u00a0',
};

function decodeCodePoint(digits: string, hex: boolean): string | null {
  const code = Number.parseInt(digits, hex ? 16 : 10);
  if (!Number.isFinite(code)) return null;
  // The values the HTML parser passes through unchanged. Everything else — NUL,
  // C1 controls, surrogates, out-of-range — it rewrites through tables this
  // encoder deliberately does not reproduce.
  const plain =
    code === 0x09 ||
    code === 0x0a ||
    code === 0x0c ||
    code === 0x0d ||
    (code >= 0x20 && code <= 0x7e) ||
    (code >= 0xa0 && code <= 0xd7ff) ||
    (code >= 0xe000 && code <= 0x10ffff);
  return plain ? String.fromCodePoint(code) : null;
}

/** Decodes HTML text; null when a reference is outside the supported set. */
function decodeEntities(text: string): string | null {
  if (!text.includes('&')) return text;
  let out = '';
  let i = 0;
  for (;;) {
    const amp = text.indexOf('&', i);
    if (amp < 0) return out + text.slice(i);
    out += text.slice(i, amp);
    const end = text.indexOf(';', amp + 1);
    if (end < 0 || end - amp > 32) return null;
    const body = text.slice(amp + 1, end);
    let decoded: string | null;
    if (body.charCodeAt(0) === 35 /* # */) {
      const hex = body.charCodeAt(1) === 120 || body.charCodeAt(1) === 88;
      const digits = body.slice(hex ? 2 : 1);
      decoded = /^[\da-f]+$/i.test(digits)
        ? decodeCodePoint(digits, hex)
        : null;
    } else decoded = NAMED_ENTITIES[body] ?? null;
    if (decoded === null) return null;
    out += decoded;
    i = end + 1;
  }
}

const isSpace = (code: number): boolean =>
  code === 32 || code === 9 || code === 10 || code === 12 || code === 13;
const isNameChar = (code: number): boolean =>
  (code >= 97 && code <= 122) ||
  (code >= 65 && code <= 90) ||
  (code >= 48 && code <= 57);

class Encoder {
  readonly ops = new Words();
  readonly attrs = new Words();
  private pieces: string[] = [];
  /** UTF-16 length of everything appended so far — see the offset decision. */
  private units = 0;
  attrCount = 0;

  private intern(value: string): number {
    const offset = this.units;
    this.pieces.push(value);
    this.units += value.length;
    return offset;
  }
  text(value: string): void {
    this.ops.push4(OP_TEXT, this.intern(value), value.length, 0);
  }
  open(tagId: number, attrStart: number, attrCount: number): void {
    this.ops.push4(OP_OPEN, tagId, attrStart, attrCount);
  }
  close(): void {
    this.ops.push4(OP_CLOSE, 0, 0, 0);
  }
  attribute(nameId: number, value: string): void {
    this.attrs.push3(nameId, this.intern(value), value.length);
    this.attrCount++;
  }
  strings(): Uint8Array {
    return new TextEncoder().encode(this.pieces.join(''));
  }
  /** Offset and length of `value` after appending it; for heading ids. */
  reference(value: string): [number, number] {
    return [this.intern(value), value.length];
  }
}

/**
 * Encodes one section of marked output. Returns false when the section must
 * keep the DOMPurify path; the caller then discards whatever was appended.
 *
 * The scanner deliberately refuses everything it cannot reproduce exactly:
 * unknown tags, unbalanced or mismatched end tags, comments, doctypes and
 * unsupported character references. It never guesses where the HTML parser
 * would insert or close an element implicitly.
 */
function encodeSection(html: string, out: Encoder): boolean {
  const length = html.length;
  const stack: number[] = [];
  let index = 0;
  let pending = '';
  const flush = () => {
    if (!pending) return;
    out.text(pending);
    pending = '';
  };
  while (index < length) {
    const lt = html.indexOf('<', index);
    const chunk = decodeEntities(html.slice(index, lt < 0 ? length : lt));
    if (chunk === null) return false;
    pending += chunk;
    if (lt < 0) break;
    index = lt + 1;
    const closing = html.charCodeAt(index) === 47; /* / */
    if (closing) index++;
    const nameStart = index;
    while (index < length && isNameChar(html.charCodeAt(index))) index++;
    if (index === nameStart) return false;
    const tag = html.slice(nameStart, index).toLowerCase();
    const tagId = TAG_ID.get(tag);
    if (tagId === undefined || (closing && VOID_TAGS.has(tag))) return false;
    if (closing) {
      while (index < length && isSpace(html.charCodeAt(index))) index++;
      if (html.charCodeAt(index) !== 62 /* > */) return false;
      index++;
      flush();
      if (stack.pop() !== tagId) return false;
      out.close();
      continue;
    }
    flush();
    const attrStart = out.attrCount;
    let attrCount = 0;
    const seen: string[] = [];
    for (;;) {
      while (index < length && isSpace(html.charCodeAt(index))) index++;
      if (index >= length) return false;
      const code = html.charCodeAt(index);
      if (code === 62 /* > */) {
        index++;
        break;
      }
      if (code === 47 /* / */) {
        // A stray solidus is ignored by the HTML parser, self-closing included.
        index++;
        continue;
      }
      const start = index;
      while (index < length) {
        const c = html.charCodeAt(index);
        if (isSpace(c) || c === 62 || c === 61 || c === 47) break;
        index++;
      }
      if (index === start) return false;
      const name = html.slice(start, index).toLowerCase();
      let raw = '';
      while (index < length && isSpace(html.charCodeAt(index))) index++;
      if (html.charCodeAt(index) === 61 /* = */) {
        index++;
        while (index < length && isSpace(html.charCodeAt(index))) index++;
        const quote = html.charCodeAt(index);
        if (quote === 34 || quote === 39) {
          const end = html.indexOf(quote === 34 ? '"' : "'", index + 1);
          if (end < 0) return false;
          raw = html.slice(index + 1, end);
          index = end + 1;
        } else {
          const valueStart = index;
          while (index < length) {
            const c = html.charCodeAt(index);
            if (isSpace(c) || c === 62) break;
            index++;
          }
          raw = html.slice(valueStart, index);
        }
      }
      const value = decodeEntities(raw);
      if (value === null) return false;
      const nameId = ATTR_ID.get(name);
      // A repeated attribute keeps its first value in the HTML parser; setting
      // both during replay would keep the last one.
      if (nameId === undefined || seen.includes(name)) continue;
      seen.push(name);
      out.attribute(nameId, value);
      attrCount++;
    }
    out.open(tagId, attrStart, attrCount);
    if (VOID_TAGS.has(tag)) out.close();
    else stack.push(tagId);
  }
  flush();
  return stack.length === 0;
}

/**
 * Encodes the parsed sections. A section marked `raw` keeps the DOMPurify path
 * unconditionally: raw HTML can rely on implied end tags, foster parenting and
 * `<tbody>` insertion, none of which a linear scanner may reproduce by guessing.
 */
export function encodeDocument(
  sections: readonly EncodableSection[],
  headings: readonly Heading[] = [],
): OpBuffer {
  const out = new Encoder();
  const sectionWords = new Words();
  const headingWords = new Words();
  const sectionOf = new Map<string, number>();
  sections.forEach((section, index) => {
    for (const heading of section.headings || [])
      if (!sectionOf.has(heading.id)) sectionOf.set(heading.id, index);
  });
  for (const section of sections) {
    const opStart = out.ops.length / OP_WORDS;
    const attrStart = out.attrCount;
    const encoded = !section.raw && encodeSection(section.html, out);
    if (encoded) {
      sectionWords.push3(opStart, out.ops.length / OP_WORDS - opStart, 0);
      continue;
    }
    // Discard the partial section: the strings it interned stay in the blob,
    // which costs a few bytes and keeps every earlier offset valid.
    out.ops.length = opStart * OP_WORDS;
    out.attrs.length = attrStart * ATTR_WORDS;
    out.attrCount = attrStart;
    sectionWords.push3(opStart, 0, SECTION_FALLBACK);
  }
  for (const heading of headings) {
    const [offset, length] = out.reference(heading.id);
    headingWords.push4(
      heading.level,
      offset,
      length,
      sectionOf.get(heading.id) ?? 0,
    );
  }
  return {
    ops: out.ops.take(),
    attrs: out.attrs.take(),
    strings: out.strings(),
    sections: sectionWords.take(),
    headings: headingWords.take(),
  };
}

/** Decodes the string blob. Call once per document, not once per section. */
export function decodeStrings(buffer: OpBuffer): string {
  return new TextDecoder().decode(buffer.strings);
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

export interface DecodedHeading {
  readonly id: string;
  readonly level: number;
  readonly section: number;
}

export function decodeHeadings(
  buffer: OpBuffer,
  text: string,
): DecodedHeading[] {
  const headings: DecodedHeading[] = [];
  for (let i = 0; i < buffer.headings.length; i += HEADING_WORDS)
    headings.push({
      level: buffer.headings[i],
      id: text.substring(
        buffer.headings[i + 1],
        buffer.headings[i + 1] + buffer.headings[i + 2],
      ),
      section: buffer.headings[i + 3],
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
}

/** Structural replay without any value policy; for round-trip tests. */
export const structuralSink: ReplaySink = {
  attribute: (element, name, value) => element.setAttribute(name, value),
  opened: () => {},
};

export function replaySection(
  buffer: OpBuffer,
  text: string,
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
            text.substring(attrs[word + 1], attrs[word + 1] + attrs[word + 2]),
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
      default:
        stack[depth].append(
          owner.createTextNode(
            text.substring(ops[op + 1], ops[op + 1] + ops[op + 2]),
          ),
        );
    }
  }
  return fragment;
}
