import type { ParsedMarkdown, ParseResponse } from './types';

export interface MarkdownService {
  parse(source: string): Promise<ParsedMarkdown>;
  cancel(): void;
  dispose(): void;
}

export class WorkerMarkdownService implements MarkdownService {
  private worker?: Worker;
  private sequence = 0;
  private idleTimer?: ReturnType<typeof setTimeout>;
  private pending?: {
    resolve: (value: ParsedMarkdown) => void;
    reject: (error: Error) => void;
  };

  parse(source: string): Promise<ParsedMarkdown> {
    this.cancel();
    clearTimeout(this.idleTimer);
    if (!this.worker) {
      const worker = new Worker(
        new URL('../../workers/markdown.worker.ts', import.meta.url),
        { type: 'module' },
      );
      this.worker = worker;
      this.worker.onmessage = ({ data }: MessageEvent<ParseResponse>) => {
        if (
          data.id !== this.sequence ||
          !this.pending ||
          this.worker !== worker
        )
          return;
        const pending = this.pending;
        this.pending = undefined;
        if ('error' in data) pending?.reject(new Error(data.error));
        else pending?.resolve(data.result);
        // Keep the worker warm across complete large-document builds and reading
        // pauses. A one-second timeout expires during the DOM build itself.
        this.idleTimer = setTimeout(() => {
          this.worker?.terminate();
          this.worker = undefined;
        }, 30_000);
      };
      this.worker.onerror = () => {
        if (this.worker !== worker) return;
        this.pending?.reject(
          new Error(
            'Die Markdown-Verarbeitung wurde unterbrochen. Bitte erneut öffnen.',
          ),
        );
        this.pending = undefined;
        this.worker?.terminate();
        this.worker = undefined;
      };
    }
    return new Promise((resolve, reject) => {
      this.pending = { resolve, reject };
      this.worker!.postMessage({ id: ++this.sequence, source });
    });
  }

  cancel(): void {
    if (!this.pending) return;
    this.pending.reject(new DOMException('Ersetzt', 'AbortError'));
    this.pending = undefined;
    this.worker?.terminate();
    this.worker = undefined;
    this.sequence++;
  }

  dispose(): void {
    clearTimeout(this.idleTimer);
    this.cancel();
    this.worker?.terminate();
    this.worker = undefined;
  }
}
