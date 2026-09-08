import { Tokenizer, type Tokens } from 'marked';

const starts = {
  space: /^[ \t\r\n]/,
  code: /^ {4}|^ {0,3}\t/,
  fences: /^ {0,3}[`~]/,
  heading: /^ {0,3}#/,
  blockquote: /^ {0,3}>/,
  list: /^ {0,3}(?:[-+*]|\d)/,
  html: /^ {0,3}</,
  def: /^ {0,3}\[/,
  hr: /^ {0,3}[-_*]/,
};

function paragraphPrefix(source: string): string {
  const end = source.indexOf('\n\n');
  return end < 0 ? source : source.slice(0, end + 1);
}

// JavaScriptCore spends disproportionate time rejecting anchored block regexes on
// long Unicode tails. These guards retain Marked's grammar and reference-link pass.
// Only tokens which cannot span the chosen boundary receive a shorter subject.
export class BoundedTokenizer extends Tokenizer {
  private listRules?: Tokenizer['rules'];
  private listDepth = 0;
  private listLexer?: Tokenizer['lexer'];
  private lists = new Map<string, { token: Tokens.List; top: boolean }>();

  constructor(private readonly reuseLists = false) {
    super();
  }

  override link(source: string) {
    return source[0] === '[' || source.startsWith('![')
      ? super.link(source)
      : undefined;
  }
  override emStrong(source: string, maskedSource: string, previous = '') {
    if (source[0] !== '*' && source[0] !== '_') return undefined;
    return super.emStrong(source, maskedSource, previous);
  }
  override url(source: string) {
    // Marked's GFM alternatives are HTTP(S), FTP, www. and a restricted email.
    // Reject ordinary words before evaluating the long URL/email expression.
    if (
      !/^(?:https?:\/\/|ftp:\/\/|www\.)/i.test(source.slice(0, 8)) &&
      !/^[A-Za-z0-9._+-]+@/.test(source)
    )
      return undefined;
    return super.url(source);
  }
  override paragraph(source: string) {
    return super.paragraph(paragraphPrefix(source));
  }
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
    if (!starts.hr.test(source.slice(0, 4))) return undefined;
    const end = source.indexOf('\n');
    return super.hr(end < 0 ? source : source.slice(0, end + 1));
  }
  override table(source: string) {
    const prefix = paragraphPrefix(source);
    if (!prefix.includes('|')) return undefined;
    return super.table(prefix);
  }
  override lheading(source: string) {
    const prefix = paragraphPrefix(source);
    // A Setext heading needs an underline line. Reject ordinary paragraphs
    // before JavaScriptCore runs Marked's expensive multiline lookahead.
    if (!/\n {0,3}(?:=+|-+)[ \t]*(?:\n|$)/.test(prefix)) return undefined;
    return super.lheading(prefix);
  }
  override list(source: string) {
    if (!starts.list.test(source.slice(0, 5))) return undefined;
    if (this.listRules !== this.rules) {
      const create = this.rules.other.listItemRegex;
      const cache = new Map<string, RegExp>();
      // Copy the rules rather than mutating Marked's shared grammar. Bullet
      // expressions have no global/sticky state and only a few grammar forms.
      this.rules = {
        ...this.rules,
        other: {
          ...this.rules.other,
          listItemRegex(bullet) {
            const existing = cache.get(bullet);
            if (existing) return existing;
            const regex = create(bullet);
            if (cache.size < 8) cache.set(bullet, regex);
            return regex;
          },
        },
      };
      this.listRules = this.rules;
    }
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
    // No HTML anywhere in the document and no reference/task syntax in this
    // list: its queued inline trees have no context-dependent HTML/link state.
    // Only outer lists are cached, after all nested block parsing has finished.
    const reusable =
      this.reuseLists &&
      !this.listDepth &&
      source.length <= 8192 &&
      !source.includes('[');
    if (this.listLexer !== this.lexer) {
      this.lists.clear();
      this.listLexer = this.lexer;
    }
    const cached = reusable ? this.lists.get(source) : undefined;
    if (cached) {
      this.lexer.state.top = cached.top;
      // Marked can append following whitespace to a returned token's raw text.
      return { ...cached.token };
    }
    this.listDepth++;
    try {
      const token = super.list(source);
      if (reusable && token) {
        if (this.lists.size >= 64)
          this.lists.delete(this.lists.keys().next().value!);
        this.lists.set(source, {
          token: { ...token },
          top: this.lexer.state.top,
        });
      }
      return token;
    } finally {
      this.listDepth--;
    }
  }
}
