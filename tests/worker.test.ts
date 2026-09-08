import { afterEach, expect, it, vi } from 'vitest';
import { WorkerMarkdownService } from '../src/core/markdown/service';

class FakeWorker {
  static created: FakeWorker[] = [];
  onmessage?: (event: { data: unknown }) => void;
  onerror?: () => void;
  postMessage = vi.fn();
  terminate = vi.fn();
  constructor() {
    FakeWorker.created.push(this);
  }
  respond() {
    const id = this.postMessage.mock.calls.at(-1)![0].id;
    this.onmessage?.({
      data: {
        id,
        result: { html: '', sections: [], headings: [], parseMs: 1 },
      },
    });
  }
}
afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
  FakeWorker.created = [];
});

it('terminates parsing on replacement and ignores late worker errors', async () => {
  vi.stubGlobal('Worker', FakeWorker);
  const service = new WorkerMarkdownService();
  const first = service.parse('first').catch((error) => error);
  const old = FakeWorker.created[0];
  const second = service.parse('second');
  old.onerror?.();
  old.respond();
  FakeWorker.created[1].respond();
  expect((await first).name).toBe('AbortError');
  expect(old.terminate).toHaveBeenCalledOnce();
  expect((await second).sections).toEqual([]);
  service.dispose();
});

it('reuses a worker during a burst and releases its lexer heap after idle', async () => {
  vi.useFakeTimers();
  vi.stubGlobal('Worker', FakeWorker);
  const service = new WorkerMarkdownService();
  const first = service.parse('first');
  FakeWorker.created[0].respond();
  await first;
  await vi.advanceTimersByTimeAsync(29_000);
  const second = service.parse('second');
  await vi.advanceTimersByTimeAsync(40_000);
  expect(FakeWorker.created[0].terminate).not.toHaveBeenCalled();
  FakeWorker.created[0].respond();
  await second;
  await vi.advanceTimersByTimeAsync(30_000);
  expect(FakeWorker.created[0].terminate).toHaveBeenCalledOnce();
  const third = service.parse('third');
  FakeWorker.created[1].respond();
  await third;
  service.dispose();
});
