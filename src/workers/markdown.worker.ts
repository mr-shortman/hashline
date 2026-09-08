import { parseMarkdown } from '../core/markdown/parser';
import { transferables } from '../core/markdown/opbuffer';
import type { ParseRequest, ParseResponse } from '../core/markdown/types';

// The worker global, which the DOM lib types as a Window here.
const scope = self as unknown as {
  postMessage(message: ParseResponse, transfer: Transferable[]): void;
};

self.onmessage = ({ data }: MessageEvent<ParseRequest>) => {
  let response: ParseResponse;
  try {
    response = { id: data.id, result: parseMarkdown(data.source, true) };
  } catch {
    response = {
      id: data.id,
      error: 'Das Markdown-Dokument konnte nicht verarbeitet werden.',
    };
  }
  // Hand the op buffers over instead of copying them. They are detached here
  // afterwards, which is safe because nothing in this worker reads them again.
  scope.postMessage(
    response,
    'result' in response && response.result.ops
      ? transferables(response.result.ops)
      : [],
  );
};
