import { parseMarkdown } from '../core/markdown/parser';
import type { ParseRequest, ParseResponse } from '../core/markdown/types';

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
  self.postMessage(response);
};
