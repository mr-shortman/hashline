import {
  decodeHeadings,
  decodeStrings,
  sectionCount,
  sectionHtml,
  sectionKey,
  type OpBuffer,
} from './opbuffer';
import type { Heading, MarkdownSection, ParsedDocument } from './types';

/**
 * Derives the small per-section and per-heading views the renderer works with
 * from the buffer. Everything expensive stays inside the buffer; this walks
 * only sections and headings, both of which are counted in hundreds, not
 * millions.
 */
export function readDocument(ops: OpBuffer, parseMs: number): ParsedDocument {
  const text = decodeStrings(ops);
  const count = sectionCount(ops);
  const bySection: Heading[][] = Array.from({ length: count }, () => []);
  const headings: Heading[] = [];
  for (const heading of decodeHeadings(ops, text)) {
    const entry = { id: heading.id, text: heading.text, level: heading.level };
    headings.push(entry);
    bySection[Math.min(heading.section, count - 1)]?.push(entry);
  }
  const sections: MarkdownSection[] = [];
  for (let i = 0; i < count; i++)
    sections.push({
      key: sectionKey(ops, i),
      html: sectionHtml(ops, text, i),
      headings: bySection[i],
    });
  return { ops, text, sections, headings, parseMs };
}
