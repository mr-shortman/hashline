import type { OpBuffer } from './opbuffer';

export interface Heading {
  readonly id: string;
  readonly text: string;
  readonly level: number;
}
export interface MarkdownSection {
  readonly html: string;
  readonly headings: readonly Heading[];
}
export interface ParsedMarkdown {
  readonly sections?: readonly MarkdownSection[];
  // The same sections as replayable operations; absent outside sectioned
  // parses. Sections the encoder could not reproduce are marked inside the
  // buffer and keep the HTML path (docs/decisions/007, P2.1).
  readonly ops?: OpBuffer;
  readonly html: string;
  readonly headings: readonly Heading[];
  readonly parseMs: number;
}
export interface ParseRequest {
  id: number;
  source: string;
}
export type ParseResponse =
  { id: number; result: ParsedMarkdown } | { id: number; error: string };
