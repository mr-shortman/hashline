import { afterEach, describe, expect, it, vi } from 'vitest';
import { DocumentController } from '../src/features/document/controller';
import { parseMarkdown } from './parser';
import type { DocumentGateway, ReadDocument } from '../src/platform/gateway';

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}
function read(path: string, source = '# ' + path): ReadDocument {
  return {
    file: {
      id: path + source,
      path,
      name: path,
      digest: source,
      readMs: 1,
    },
    parsed: parseMarkdown(source),
  };
}
function setup(open: DocumentGateway['read']) {
  const stop = vi.fn();
  const gateway = {
    read: vi.fn(open),
    release: vi.fn(async () => {}),
    watch: vi.fn(async () => stop),
    setTitle: vi.fn(async () => {}),
    imageUrl: () => null,
  } as unknown as DocumentGateway;
  const controller = new DocumentController(gateway);
  return { controller, gateway, stop };
}
afterEach(() => vi.useRealTimers());
describe('document lifecycle', () => {
  it('keeps B when the earlier read of A arrives late and releases A', async () => {
    const a = deferred<ReadDocument>();
    const { controller, gateway } = setup((path) =>
      path === 'A.md' ? a.promise : Promise.resolve(read(path)),
    );
    const openingA = controller.open('A.md');
    await controller.open('B.md');
    a.resolve(read('A.md'));
    await openingA;
    expect(controller.getSnapshot().document?.file.path).toBe('B.md');
    expect(gateway.release).toHaveBeenCalledWith(read('A.md').file.id);
    controller.dispose();
  });
  it('discards a late read and keeps the previous document on read errors', async () => {
    const late = deferred<ReadDocument>();
    const { controller, stop } = setup(async (path) => {
      if (path === 'missing.md') throw new Error('nicht gefunden');
      if (path === 'A.md') return late.promise;
      return read(path);
    });
    const a = controller.open('A.md');
    await Promise.resolve();
    await controller.open('B.md');
    late.resolve(read('A.md'));
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
    const { controller, gateway } = setup(async (path) => read(path));
    await controller.open('A.md');
    const original = controller.getSnapshot().document;
    await controller.reload();
    // Same digest, same document object: the viewport must not rebuild.
    expect(controller.getSnapshot().document).toBe(original);
    expect(gateway.read).toHaveBeenCalledTimes(2);
    const changed = vi.mocked(gateway.watch).mock.calls[0][1];
    changed();
    changed();
    changed();
    await vi.advanceTimersByTimeAsync(150);
    expect(gateway.read).toHaveBeenCalledTimes(3);
    controller.dispose();
  });
  it('rebuilds when the content behind the same path changes', async () => {
    let body = '# eins';
    const { controller } = setup(async (path) => read(path, body));
    await controller.open('A.md');
    const original = controller.getSnapshot().document;
    body = '# zwei';
    await controller.reload();
    expect(controller.getSnapshot().document).not.toBe(original);
    expect(controller.getSnapshot().document?.headings[0].text).toBe('zwei');
    controller.dispose();
  });
});
