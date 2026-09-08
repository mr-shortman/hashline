import { afterEach, describe, expect, it, vi } from 'vitest';
import { DocumentController } from '../src/features/document/controller';
import { parseMarkdown } from '../src/core/markdown/parser';
import type { DocumentGateway, FileDocument } from '../src/platform/gateway';
import type { MarkdownService } from '../src/core/markdown/service';

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}
function file(path: string, source = '# ' + path): FileDocument {
  return { id: path + source, path, name: path, source, readMs: 1 };
}
function setup(read: DocumentGateway['read']) {
  const stop = vi.fn();
  const gateway = {
    read,
    release: vi.fn(async () => {}),
    watch: vi.fn(async () => stop),
    setTitle: vi.fn(async () => {}),
    imageUrl: () => null,
  } as unknown as DocumentGateway;
  const service: MarkdownService = {
    parse: async (source) => parseMarkdown(source),
    cancel: vi.fn(),
    dispose: vi.fn(),
  };
  const controller = new DocumentController(gateway, service);
  return { controller, gateway, stop, service };
}
afterEach(() => vi.useRealTimers());
describe('document lifecycle', () => {
  it('keeps B when the earlier read of A arrives late and releases A', async () => {
    const a = deferred<FileDocument>();
    const { controller, gateway } = setup((path) =>
      path === 'A.md' ? a.promise : Promise.resolve(file(path)),
    );
    const openingA = controller.open('A.md');
    await controller.open('B.md');
    a.resolve(file('A.md'));
    await openingA;
    expect(controller.getSnapshot().document?.file.path).toBe('B.md');
    expect(gateway.release).toHaveBeenCalledWith(file('A.md').id);
    controller.dispose();
  });
  it('discards a late parser result and keeps the previous document on read errors', async () => {
    const { controller, service, stop } = setup(async (path) => {
      if (path === 'missing.md') throw new Error('nicht gefunden');
      return file(path);
    });
    const parsed = deferred<ReturnType<typeof parseMarkdown>>();
    service.parse = vi
      .fn()
      .mockImplementationOnce(() => parsed.promise)
      .mockImplementation(async (source: string) => parseMarkdown(source));
    const a = controller.open('A.md');
    await Promise.resolve();
    await controller.open('B.md');
    parsed.resolve(parseMarkdown('# A'));
    await a;
    await controller.open('missing.md');
    expect(controller.getSnapshot().document?.file.path).toBe('B.md');
    expect(controller.getSnapshot().error).toContain('missing.md');
    expect(stop).not.toHaveBeenCalled();
    controller.dispose();
    expect(stop).toHaveBeenCalledOnce();
  });
  it('does not replace an unchanged revision and batches watcher events', async () => {
    vi.useFakeTimers();
    const { controller, gateway, service } = setup(async (path) => file(path));
    const parse = vi.spyOn(service, 'parse');
    await controller.open('A.md');
    const original = controller.getSnapshot().document;
    await controller.reload();
    expect(controller.getSnapshot().document).toBe(original);
    expect(parse).toHaveBeenCalledOnce();
    const changed = vi.mocked(gateway.watch).mock.calls[0][1];
    changed();
    changed();
    changed();
    await vi.advanceTimersByTimeAsync(150);
    expect(parse).toHaveBeenCalledOnce();
    controller.dispose();
  });
});
