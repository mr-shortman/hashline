// A real task boundary (a resolved Promise only yields to microtasks).
// Window messages avoid WebKit's slower MessagePort dispatch while doing DOM work.
const taskPrefix = `hashline-render-${Math.random().toString(36).slice(2)}-`;
let sequence = 0;
export function nextTask(signal?: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) {
      reject(new DOMException('Ersetzt', 'AbortError'));
      return;
    }
    const id = taskPrefix + ++sequence;
    const close = () => {
      window.removeEventListener('message', delivered);
      signal?.removeEventListener('abort', abort);
    };
    const abort = () => {
      close();
      reject(new DOMException('Ersetzt', 'AbortError'));
    };
    const delivered = (event: MessageEvent) => {
      if (event.source !== window || event.data !== id) return;
      close();
      resolve();
    };
    signal?.addEventListener('abort', abort, { once: true });
    window.addEventListener('message', delivered);
    window.postMessage(id, '*');
  });
}

// Remove connected sections from the end in bounded tasks. Hiding or moving
// the entire document first can itself synchronously tear down its render tree.
export async function retireContent(
  root: HTMLElement,
  signal: AbortSignal,
  keep: ReadonlySet<Element> = new Set(),
): Promise<void> {
  const retired: (Element | null)[] = Array.from(root.children).filter(
    (element) => !keep.has(element),
  );
  let maximum = 0;
  const started = performance.now();
  for (let i = retired.length - 1; i >= 0;) {
    await nextTask(signal);
    const start = performance.now();
    do {
      retired[i]?.remove();
      retired[i--] = null;
    } while (i >= 0 && performance.now() - start < 18);
    maximum = Math.max(maximum, performance.now() - start);
  }
  performance.clearMeasures('hashline.max-retire-task');
  performance.measure('hashline.max-retire-task', {
    start: 0,
    duration: maximum,
  });
  performance.measure('hashline.retire-wall', {
    start: started,
    end: performance.now(),
  });
}
