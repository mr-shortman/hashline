import {
  ALLOWED_ATTR,
  ALLOWED_TAGS,
  OP_CLOSE,
  OP_OPEN,
  OP_TEXT,
  SECTION_FALLBACK,
  type OpBuffer,
} from '../src/core/markdown/opbuffer';

export type Node =
  | string
  | {
      tag: (typeof ALLOWED_TAGS)[number];
      attrs?: [(typeof ALLOWED_ATTR)[number], string][];
      children?: Node[];
    };

/**
 * Builds a buffer by hand, so the replay's attribute policy can be tested
 * against values the parser would never emit. The parser has no encoder in
 * JavaScript any more; this is deliberately a test fixture, not a second
 * implementation of the format. Callers keep their content ASCII: the fixture
 * stores UTF-16 offsets into a UTF-8 blob, which only coincide there.
 */
export function buildBuffer(sections: (Node[] | 'fallback')[]): OpBuffer {
  const ops: number[] = [];
  const attrs: number[] = [];
  const sectionWords: number[] = [];
  let text = '';
  const intern = (value: string): [number, number] => {
    const offset = text.length;
    text += value;
    return [offset, value.length];
  };
  const emit = (node: Node) => {
    if (typeof node === 'string') {
      ops.push(OP_TEXT, ...intern(node), 0);
      return;
    }
    const attrStart = attrs.length / 3;
    for (const [name, value] of node.attrs || [])
      attrs.push(ALLOWED_ATTR.indexOf(name), ...intern(value));
    ops.push(
      OP_OPEN,
      ALLOWED_TAGS.indexOf(node.tag),
      attrStart,
      attrs.length / 3 - attrStart,
    );
    for (const child of node.children || []) emit(child);
    ops.push(OP_CLOSE, 0, 0, 0);
  };
  for (const section of sections) {
    const start = ops.length / 4;
    if (section === 'fallback') {
      sectionWords.push(start, 0, SECTION_FALLBACK, 0, 0, 0, 0);
      continue;
    }
    for (const node of section) emit(node);
    sectionWords.push(start, ops.length / 4 - start, 0, 0, 0, 0, 0);
  }
  return {
    ops: new Uint32Array(ops),
    attrs: new Uint32Array(attrs),
    strings: new TextEncoder().encode(text),
    sections: new Uint32Array(sectionWords),
    headings: new Uint32Array(),
  };
}
