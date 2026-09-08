export interface Heading {
  readonly id: string;
  readonly text: string;
  readonly level: number;
}
export interface ParsedMarkdown {
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
