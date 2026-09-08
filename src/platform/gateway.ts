import type { ParsedDocument } from '../core/markdown/types';

export interface FileDocument {
  readonly id: string;
  readonly path: string;
  readonly name: string;
  /** Content fingerprint; replaces comparing the whole source for changes. */
  readonly digest: string;
  readonly readMs: number;
  readonly processStartedAtMs?: number;
}
/** A file and its parsed document; reading and parsing happen together. */
export interface ReadDocument {
  readonly file: FileDocument;
  readonly parsed: ParsedDocument;
}
export interface OpenRequest {
  paths: string[];
}
export interface DocumentGateway {
  choose(): Promise<string[]>;
  read(path: string): Promise<ReadDocument>;
  watch(
    file: FileDocument,
    changed: () => void,
    failed: () => void,
  ): Promise<() => void>;
  release(id: string): Promise<void>;
  allowRemoteImages?(file: FileDocument): Promise<void>;
  imageUrl(file: FileDocument, source: string): string | null;
  followLink(
    file: FileDocument,
    href: string,
  ): Promise<{ path?: string; fragment?: string }>;
  copy(text: string): Promise<void>;
  subscribeOpen(callback: (request: OpenRequest) => void): Promise<() => void>;
  setTitle(title: string): Promise<void>;
}
