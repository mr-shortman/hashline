import type { OpBuffer } from './opbuffer';

export interface Heading {
  readonly id: string;
  readonly text: string;
  readonly level: number;
}
export interface MarkdownSection {
  /** Content identity, for recognizing unchanged sections across reloads. */
  readonly key: string;
  /** Only fallback sections carry HTML; encoded sections replay operations. */
  readonly html: string;
  readonly headings: readonly Heading[];
}
export interface ParsedDocument {
  readonly ops: OpBuffer;
  /** The string blob, decoded once per document. */
  readonly text: string;
  readonly sections: readonly MarkdownSection[];
  readonly headings: readonly Heading[];
  readonly parseMs: number;
}
