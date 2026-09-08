import { invoke, isTauri } from '@tauri-apps/api/core';
import { decodePacket } from './document-packet';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { readDocument } from '../core/markdown/parser';
import { loadParser } from '../core/markdown/wasm';
import type {
  DocumentGateway,
  FileDocument,
  OpenRequest,
  ReadDocument,
} from './gateway';

class TauriGateway implements DocumentGateway {
  private restoredDevFile = false;
  choose = (): Promise<string[]> => invoke('choose_file');
  read = async (path: string): Promise<ReadDocument> => {
    const packet = decodePacket(
      await invoke<ArrayBuffer>('read_document', { path }),
    );
    // The header carries the file metadata beside the buffer layout and the
    // native parse time; neither belongs on the FileDocument.
    const { layout, parseMs, ...file } = packet.header;
    void layout;
    return {
      file: file as unknown as FileDocument,
      parsed: readDocument(packet.ops, Number(parseMs) || 0),
    };
  };
  release = (id: string): Promise<void> => invoke('release_document', { id });
  allowRemoteImages = (file: FileDocument): Promise<void> =>
    invoke('allow_remote_images', { id: file.id });
  imageUrl = (file: FileDocument, source: string): string =>
    `hashline-image://localhost/${file.id}/${encodeURIComponent(source)}`;
  followLink = (
    file: FileDocument,
    href: string,
  ): Promise<{ path?: string; fragment?: string }> =>
    invoke('follow_link', { id: file.id, href });
  copy = (text: string): Promise<void> => invoke('copy_text', { text });
  setTitle = (title: string): Promise<void> =>
    getCurrentWindow().setTitle(title);
  async watch(
    file: FileDocument,
    changed: () => void,
    failed: () => void,
  ): Promise<() => void> {
    const unlisten = await listen<{ id: string; failed: boolean }>(
      'document-changed',
      ({ payload }) => {
        if (payload.id === file.id) {
          if (payload.failed) failed();
          else changed();
        }
      },
    );
    try {
      await invoke('watch_document', { id: file.id });
    } catch (error) {
      unlisten();
      throw error;
    }
    return () => {
      unlisten();
      void invoke('unwatch_document', { id: file.id }).catch(() => {});
    };
  }
  async subscribeOpen(
    callback: (request: OpenRequest) => void,
  ): Promise<() => void> {
    const drain = async () => {
      const paths = await invoke<string[]>('take_open_requests');
      if (paths.length) callback({ paths });
    };
    const stop = await listen('open-request', () => {
      void drain();
    });
    // A full Vite reload keeps the native process but consumes no new CLI request.
    // Native authorization still validates this path; no access is granted here.
    if (import.meta.env.DEV && !this.restoredDevFile) {
      this.restoredDevFile = true;
      const paths = await invoke<string[]>('take_open_requests');
      if (paths.length) callback({ paths });
      else {
        try {
          const path = sessionStorage.getItem('hashline.dev.file');
          if (path) callback({ paths: [path] });
        } catch {
          /* Optional storage. */
        }
      }
    } else await drain();
    return stop;
  }
}

// Browser preview exercises the same pipeline. Native resources remain desktop-only.
class BrowserGateway implements DocumentGateway {
  private files = new Map<string, File>();
  private sequence = 0;
  private register(files: File[]): string[] {
    this.files.clear();
    const paths = files.map((file) => {
      this.files.set(file.name, file);
      return file.name;
    });
    return paths;
  }
  choose(): Promise<string[]> {
    return new Promise((resolve) => {
      const input = document.createElement('input');
      input.type = 'file';
      input.accept = '.md,.markdown,.mdown,.mkd,.mkdn,.mdwn';
      input.multiple = true;
      input.addEventListener(
        'change',
        () => resolve(this.register(Array.from(input.files || []))),
        { once: true },
      );
      input.addEventListener('cancel', () => resolve([]), { once: true });
      input.click();
    });
  }
  async read(path: string): Promise<ReadDocument> {
    const start = performance.now();
    const file = this.files.get(path);
    if (!file)
      throw new Error(
        'Diese Datei ist in der Browser-Vorschau nicht verfügbar. Bitte über den Dialog öffnen.',
      );
    if (file.size > 20 * 1024 * 1024)
      throw new Error('Die Datei überschreitet das Limit von 20 MiB.');
    let source: string;
    try {
      source = new TextDecoder('utf-8', { fatal: true }).decode(
        await file.arrayBuffer(),
      );
    } catch {
      throw new Error('Die Datei ist nicht gültig UTF-8-kodiert.');
    }
    // The same parser the desktop runs natively, as WebAssembly. Loading it
    // here keeps it out of the desktop bundle's startup path entirely.
    const parser = await loadParser();
    const readMs = performance.now() - start;
    return {
      file: {
        id: String(++this.sequence),
        path,
        name: file.name,
        digest: `${file.size}:${file.lastModified}:${source.length}`,
        readMs,
      },
      parsed: parser.parse(source),
    };
  }
  async watch(): Promise<() => void> {
    return () => {};
  }
  async release(): Promise<void> {}
  imageUrl(): null {
    return null;
  }
  async followLink(
    _file: FileDocument,
    href: string,
  ): Promise<{ path?: string; fragment?: string }> {
    if (/^(https?:|mailto:)/i.test(href)) {
      window.open(href, '_blank', 'noopener,noreferrer');
      return {};
    }
    throw new Error('Relative Dateien sind in der Desktop-App verfügbar.');
  }
  copy = (text: string): Promise<void> => navigator.clipboard.writeText(text);
  async setTitle(title: string): Promise<void> {
    document.title = title;
  }
  async subscribeOpen(
    callback: (request: OpenRequest) => void,
  ): Promise<() => void> {
    const over = (event: DragEvent) => event.preventDefault();
    const drop = (event: DragEvent) => {
      event.preventDefault();
      callback({
        paths: this.register(Array.from(event.dataTransfer?.files || [])),
      });
    };
    window.addEventListener('dragover', over);
    window.addEventListener('drop', drop);
    return () => {
      window.removeEventListener('dragover', over);
      window.removeEventListener('drop', drop);
    };
  }
}
export const desktop = isTauri();
export function createGateway(): DocumentGateway {
  return desktop ? new TauriGateway() : new BrowserGateway();
}
