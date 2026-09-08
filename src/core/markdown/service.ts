import type { ParsedMarkdown, ParseResponse } from './types';

export interface MarkdownService {
  parse(source: string): Promise<ParsedMarkdown>;
  cancel(): void;
  dispose(): void;
}

export class WorkerMarkdownService implements MarkdownService {
  private worker?: Worker;
  private sequence = 0;
  private pending?: {
    resolve: (value: ParsedMarkdown) => void;
    reject: (error: Error) => void;
  };

  parse(source: string): Promise<ParsedMarkdown> {
    this.cancel();
    if (!this.worker) {
      this.worker = new Worker(
        new URL('../../workers/markdown.worker.ts', import.meta.url),
        { type: 'module' },
      );
      this.worker.onmessage = ({ data }: MessageEvent<ParseResponse>) => {
        if (data.id !== this.sequence) return;
        const pending = this.pending;
        this.pending = undefined;
        if ('error' in data) pending?.reject(new Error(data.error));
        else pending?.resolve(data.result);
      };
      this.worker.onerror = () => {
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
    this.cancel();
    this.worker?.terminate();
    this.worker = undefined;
  }
}
