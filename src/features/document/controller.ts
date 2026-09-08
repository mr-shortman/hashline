import type { DocumentGateway, FileDocument } from '../../platform/gateway';
import type { MarkdownService } from '../../core/markdown/service';
import type { Heading, MarkdownSection } from '../../core/markdown/types';

export interface RenderDocument {
  readonly file: FileDocument;
  readonly sections: readonly MarkdownSection[];
  readonly openedAt: number;
  readonly headings: readonly Heading[];
  readonly revision: number;
}
export interface DocumentState {
  status: 'empty' | 'loading' | 'ready' | 'error';
  document?: RenderDocument;
  refreshing: boolean;
  error?: string;
  notice?: string;
}
export class DocumentController {
  private state: DocumentState = { status: 'empty', refreshing: false };
  private listeners = new Set<() => void>();
  private requestId = 0;
  private unwatch?: () => void;
  private reloadTimer?: ReturnType<typeof setTimeout>;
  private retryPath?: string;
  private dirtyPath?: string;
  constructor(
    private gateway: DocumentGateway,
    private markdown: MarkdownService,
  ) {}
  getSnapshot = (): DocumentState => this.state;
  subscribe = (callback: () => void): (() => void) => {
    this.listeners.add(callback);
    return () => this.listeners.delete(callback);
  };
  private update(state: DocumentState): void {
    this.state = state;
    this.listeners.forEach((f) => f());
  }
  notice = (notice?: string): void => this.update({ ...this.state, notice });
  async openPaths(paths: string[]): Promise<void> {
    if (!paths.length) return;
    if (paths.length > 1)
      this.notice(
        'Hashline zeigt eine Datei an. Die erste Datei wird geöffnet.',
      );
    await this.open(paths[0]);
  }
  async choose(): Promise<void> {
    try {
      await this.openPaths(await this.gateway.choose());
    } catch {
      this.notice('Der Dateidialog konnte nicht geöffnet werden.');
    }
  }
  reload = async (): Promise<void> => {
    const path = this.state.document?.file.path;
    if (path) await this.open(path);
  };
  retry = async (): Promise<void> => {
    if (this.retryPath) await this.open(this.retryPath);
  };
  async open(path: string): Promise<void> {
    const request = ++this.requestId;
    this.markdown.cancel();
    this.retryPath = path;
    this.dirtyPath = undefined;
    clearTimeout(this.reloadTimer);
    const previous = this.state.document;
    const start = performance.now();
    this.update({
      ...this.state,
      status: previous ? 'ready' : 'loading',
      refreshing: !!previous,
      error: undefined,
    });
    let file: FileDocument | undefined;
    try {
      file = await this.gateway.read(path);
      if (request !== this.requestId) {
        await this.gateway.release(file.id);
        return;
      }
      if (
        previous?.file.path === file.path &&
        previous.file.source === file.source
      ) {
        await this.gateway.release(file.id);
        this.update({ ...this.state, status: 'ready', refreshing: false });
        return;
      }
      const parsed = await this.markdown.parse(file.source);
      if (request !== this.requestId) {
        await this.gateway.release(file.id);
        return;
      }
      performance.measure('hashline.read', { start: 0, duration: file.readMs });
      performance.measure('hashline.parse', {
        start: 0,
        duration: parsed.parseMs,
      });
      performance.measure('hashline.open-to-content', {
        start,
        end: performance.now(),
      });
      this.unwatch?.();
      this.unwatch = undefined;
      const doc: RenderDocument = Object.freeze({
        file,
        sections: parsed.sections || [
          { html: parsed.html, headings: parsed.headings },
        ],
        openedAt: start,
        headings: parsed.headings,
        revision: request,
      });
      this.update({
        status: 'ready',
        document: doc,
        refreshing: false,
        notice: this.state.notice,
      });
      void this.gateway.setTitle(`${file.name} — Hashline`).catch(() => {});
      if (previous) void this.gateway.release(previous.file.id).catch(() => {});
      try {
        const stop = await this.gateway.watch(
          file,
          () => {
            if (this.state.document !== doc) return;
            this.dirtyPath = doc.file.path;
            clearTimeout(this.reloadTimer);
            this.reloadTimer = setTimeout(() => {
              if (this.state.document === doc && !this.state.refreshing)
                void this.reload();
            }, 150);
          },
          () =>
            this.notice(
              'Automatisches Nachladen ist nicht verfügbar. Mit Strg+R manuell nachladen.',
            ),
        );
        if (this.state.document === doc) this.unwatch = stop;
        else stop();
      } catch {
        this.notice(
          'Automatisches Nachladen ist nicht verfügbar. Mit Strg+R manuell nachladen.',
        );
      }
    } catch (error) {
      if (file && file.id !== this.state.document?.file.id)
        void this.gateway.release(file.id).catch(() => {});
      if (request !== this.requestId) return;
      const detail = error instanceof Error ? error.message : String(error);
      this.update({
        ...this.state,
        status: previous ? 'ready' : 'error',
        refreshing: false,
        error: `„${path}“: ${detail}`,
      });
    } finally {
      if (
        request === this.requestId &&
        this.dirtyPath === this.state.document?.file.path &&
        this.dirtyPath
      ) {
        this.dirtyPath = undefined;
        clearTimeout(this.reloadTimer);
        this.reloadTimer = setTimeout(() => {
          void this.reload();
        }, 150);
      }
    }
  }
  dispose(): void {
    this.requestId++;
    clearTimeout(this.reloadTimer);
    this.unwatch?.();
    this.markdown.dispose();
    if (this.state.document)
      void this.gateway.release(this.state.document.file.id).catch(() => {});
    this.listeners.clear();
  }
}
