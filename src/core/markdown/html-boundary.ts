// This only proves that a raw HTML token cannot leave an open container across
// section boundaries. It does not parse or sanitize HTML. Uncertain/ill-formed
// syntax takes the conservative whole-document path; DOMPurify remains mandatory.
export function isClosedHtml(html: string): boolean {
  const stack: string[] = [];
  const tag =
    /<\/?([a-z][a-z\d:-]*)(?:\s+[^\s"'<>/=]+(?:\s*=\s*(?:"[^"]*"|'[^']*'|[^\s"'=<>`]+))?)*\s*\/?>/iy;
  const voidTag =
    /^(?:area|base|br|col|embed|hr|img|input|link|meta|param|source|track|wbr)$/;
  let offset = 0;
  while ((offset = html.indexOf('<', offset)) !== -1) {
    if (html.startsWith('<!--', offset)) {
      const end = html.indexOf('-->', offset + 4);
      if (end < 0) return false;
      offset = end + 3;
      continue;
    }
    tag.lastIndex = offset;
    const match = tag.exec(html);
    if (!match) return false;
    const name = match[1].toLowerCase();
    // Raw-text and template insertion modes need a real HTML parser to prove
    // isolation, including apparent tags inside their text. Do not guess.
    if (
      /^(?:script|style|textarea|title|template|plaintext|xmp|iframe|noembed|noscript)$/.test(
        name,
      )
    )
      return false;
    if (html[offset + 1] === '/') {
      if (stack.pop() !== name) return false;
    } else if (!voidTag.test(name)) stack.push(name);
    offset = tag.lastIndex;
  }
  return stack.length === 0;
}
