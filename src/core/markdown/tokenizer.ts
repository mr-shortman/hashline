import { Tokenizer } from 'marked';

const starts = {
  space: /^[ \t\r\n]/,
  code: /^ {4}|^ {0,3}\t/,
  fences: /^ {0,3}[`~]/,
  heading: /^ {0,3}#/,
  blockquote: /^ {0,3}>/,
  list: /^ {0,3}(?:[-+*]|\d)/,
  html: /^ {0,3}</,
  def: /^ {0,3}\[/,
};

function paragraphPrefix(source: string): string {
  const end = source.indexOf('\n\n');
  return end < 0 ? source : source.slice(0, end + 1);
}

// JavaScriptCore spends disproportionate time rejecting anchored block regexes on
// long Unicode tails. These guards retain Marked's grammar and reference-link pass.
// Only tokens which cannot span the chosen boundary receive a shorter subject.
export class BoundedTokenizer extends Tokenizer {
  override space(source: string) {
    return starts.space.test(source.slice(0, 5))
      ? super.space(source)
      : undefined;
  }
  override code(source: string) {
    return starts.code.test(source.slice(0, 5))
      ? super.code(source)
      : undefined;
  }
  override fences(source: string) {
    return starts.fences.test(source.slice(0, 5))
      ? super.fences(source)
      : undefined;
  }
  override heading(source: string) {
    return starts.heading.test(source.slice(0, 5))
      ? super.heading(source)
      : undefined;
  }
  override blockquote(source: string) {
    return starts.blockquote.test(source.slice(0, 5))
      ? super.blockquote(source)
      : undefined;
  }
  override html(source: string) {
    return starts.html.test(source.slice(0, 5))
      ? super.html(source)
      : undefined;
  }
  override def(source: string) {
    return starts.def.test(source.slice(0, 5)) ? super.def(source) : undefined;
  }
  override hr(source: string) {
    const end = source.indexOf('\n');
    return super.hr(end < 0 ? source : source.slice(0, end + 1));
  }
  override table(source: string) {
    return super.table(paragraphPrefix(source));
  }
  override lheading(source: string) {
    return super.lheading(paragraphPrefix(source));
  }
  override list(source: string) {
    if (!starts.list.test(source.slice(0, 5))) return undefined;
    let end = source.indexOf('\n\n');
    while (end >= 0) {
      // An unindented non-list block after an empty line ends this list. Indented
      // continuations, nested blocks and subsequent list items stay in the subject.
      if (/[^\s*+\-\d]/.test(source.charAt(end + 2))) {
        source = source.slice(0, end + 2);
        break;
      }
      end = source.indexOf('\n\n', end + 2);
    }
    return super.list(source);
  }
}
