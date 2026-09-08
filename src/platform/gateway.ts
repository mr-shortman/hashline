export interface FileDocument {
  readonly id: string;
  readonly path: string;
  readonly name: string;
  readonly source: string;
  readonly readMs: number;
  readonly processStartedAtMs?: number;
}
export interface OpenRequest {
  paths: string[];
}
export interface DocumentGateway {
  choose(): Promise<string[]>;
  read(path: string): Promise<FileDocument>;
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
