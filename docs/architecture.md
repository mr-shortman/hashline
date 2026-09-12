# Architecture

One native process: Rust, GTK4 (min 4.14, for `GtkAccessibleText`), Pango for
text, GSK for drawing. The document area is a custom widget, not a
`GtkTextView`.

## From file to pixels

1. **Read and parse** on a worker thread. Every load gets an increasing
   `RequestId`; only the newest may replace the document. Stale results are
   dropped.
2. **`crates/markdown`** (pulldown-cmark) turns the source into an **op
   buffer**: ops, two blobs (`strings`, `text`), blocks, sections, headings,
   anchors. No HTML is produced. The document is immutable after parsing and
   shared as an `Arc`.
3. **Block plan**: one pass over `blocks`, one estimated height per block. It
   is the only structure that spans the whole document. Heights live in a
   Fenwick tree, so total height, block top and block-at-offset are all
   O(log n).
4. **Layout** covers the viewport plus about one screen of buffer. The buffer
   is laid out in idle slices of ~6 ms, not in the same frame. A measured
   height replaces the estimate; if that block is above the reading position,
   the scroll offset moves by the same amount, so visible text stands still.
5. **Draw** as GSK nodes in one pass after measuring: decoration, then search
   matches, then selection, then text.

Syntax highlighting and image decoding also run on worker threads. Block plan,
Pango layout and drawing stay on the main thread and inside the 16 ms budget.

## The invariant everything depends on

**Every position in laid-out text is exactly one byte in the document.**

A block is laid out as several *pieces*: one per list item, one per table cell,
otherwise one. Each piece carries a `TextMap`, the document runs it took over.
Inserted decoration (bullets, numbers, the copy button) has no run and
therefore no document position.

Selection, hit testing, search highlighting, link lookup, syntax colors and the
AT-SPI text interface all go through this one mapping. Do not bypass it.
Consequences:

- A selection position is `(block index, byte offset)`; selection across blocks
  is just an ordering on those pairs.
- A soft break is drawn as a space: one byte for one byte, offsets unchanged.
- Controls (the copy button) are never selection or hit-test targets.

## Op buffer

- All offsets and lengths are **UTF-8 byte offsets**.
- `blocks`: five words each, tag, opStart, opCount, textStart, textLen. A block
  is a **top-level flow element**; a nested list or a whole table is one block.
- Blocks are separated by `\n` in the text blob. That byte belongs to no block,
  so a search match never spans blocks.
- Raw HTML becomes `pre > code.raw-html` (block) or `code.raw-html` (inline).
  `OpDocument::raw_html` says whether the document contained any.
- Heading IDs follow GitHub's rule (NFKC, lowercase, keep letters, marks,
  digits, `_` and `-`, each whitespace becomes one `-`), prefixed `doc-`,
  deduplicated with `-1`, `-2`, and so on. A fragment link resolves in this
  order: the ID as written, then with `doc-`, then the slug of the fragment.
  Footnotes get `fn-<label>`.

`crates/markdown` knows neither GTK nor the filesystem. `hashline_markdown::parse`
is the only place Markdown enters the app; the app only ever sees `OpDocument`.

## Oversized blocks

A block much taller than a screen defeats virtualization: a 70,000-line code
block was one block and took 67 s to lay out. Layout cost grows with the
**square** of an unbreakable run, so the plan splits large blocks into parts
when it is built. A part is an ordinary plan block.

| Kind | Split limit | Split at |
| --- | --- | --- |
| Text | 2 KiB | word boundary in the last quarter, if there is one |
| Code | 256 lines or 32 KiB | line boundary |
| Lists, tables | not split | - |

Limits are byte counts, not lines, so block numbers stay stable across width,
zoom and font changes. Whole-block actions (copy code block, triple click,
outline, spacing) use the part index and part count.

## Estimates and caches

- Code block height estimates are exact to 0.2 %. Prose runs ~32 % low, so the
  scrollbar is too short, never too long, and corrects while reading.
- A width, zoom or font change drops measured heights, re-estimates, and holds
  the reading anchor.
- Layout cache: 240 blocks. Highlight cache: 480 blocks. Code blocks over
  128 KiB stay uncolored.

## Modules

```text
crates/markdown/     parser and op buffer; no GTK, no filesystem
crates/hashline/src/
  app/               GApplication, window, header bar, actions, tabs, menu, CLI
  document/          loading, watching, digest, reading anchor
  layout/            block plan, estimates, op buffer to Pango, TextMap
  view/              document widget, selection, images, a11y, main-thread probe
  search/            search on the text blob
  outline/           headings, fragment resolution
  highlight/         syntect
  preferences/       GSettings, reading positions
  theme/             design tokens, chrome stylesheet
```

`layout` reads no files and knows no application state; it asks `ImageSource`
for image sizes. `view` is the only owner of the layout cache. No plugin
system, no dependency injection, no event bus, no database.

## Choices that shape the code

- **GSK renderer: cairo.** Empty-window PSS is 33 MiB against 88 MiB on Vulkan,
  and startup is about half. Vulkan scrolls slightly better; memory wins.
  Nothing sets it, though, so an installed build runs on the GTK default
  ([limitations.md](limitations.md)).
- **Tabs: `GtkNotebook`, no libadwaita.** libadwaita would add a runtime
  dependency, its own style layer and memory, for features that are out of
  scope. An inactive tab drops its layout cache, highlights and decoded images,
  and keeps op buffer, block plan and reading anchor. Zoom is per window.
- **Chrome stylesheet.** Header bar, search, outline, menu and tabs are styled
  from the same tokens as the document, through a `GtkCssProvider` at
  application priority (`theme/chrome.rs`). A system theme cannot be trusted to
  have a light variant. Window buttons, focus rings and sizes stay with the
  system theme.
- **Syntect parser only.** Scopes map straight onto three token colors
  (keyword, string, number); no syntect themes. `regex-fancy`, so no C library.
  The syntax set loads lazily on first use.
- **Search without a second copy.** Case folding happens per candidate during
  the comparison; no lowercased copy plus offset table, which would cost around
  50 MiB on a 10 MiB file. Expanding folds (`ss` for a sharp s, `fi` for the
  ligature) are therefore not supported.
- **Line height is absolute** (`size x 1.65`). Pango's line-spacing factor
  multiplies the font's natural height and would land near 2.0.
- **Allocator tuning**: one malloc arena, fixed mmap threshold, `malloc_trim`
  after each load, so parse garbage goes back to the operating system.
- **Startup**: GTK's first call to the settings portal costs ~168 ms when the
  portal is not running yet, which the first GTK4 program of a session pays.
  The application cannot avoid it.

## Parser correctness

`crates/markdown/tests` runs the 652 CommonMark 0.31.2 examples: the ops are
replayed to HTML and compared as DOM trees against the specification
(html5ever, a dev-dependency only). 580 match exactly. The 72 with raw HTML
differ on purpose and are pinned, so any further drift fails the test. The test
also checks that it can detect differences at all.

## Behavior contracts

**Files**

- UTF-8 (with or without BOM), at most 20 MiB. Invalid encoding gives a clear
  message. A document is never written to; task lists stay read-only.
- Relative paths resolve against the document's directory. CLI paths resolve
  against the caller's working directory, also when handed to a running
  instance.
- Atomic saves (write temp, rename), deletes and re-creates survive watching.
- A failed load keeps the current document visible, with a short message and a
  retry.
- Missing or broken settings never block startup. Persistence is preferences
  plus the last 100 reading positions; there is no document cache.

**Links**

- Markdown links open in the same window as a new tab, fragment links jump.
- `https:`, `http:` and `mailto:` open in the system application after a click.
- Everything else is refused with a hint. No shell command is ever built from
  document content.

**Images**

- Loaded automatically only from the document's directory and below, checked
  after canonicalization, so a symlink cannot escape.
- At most 40 megapixels, checked from the header before decoding.
- An image alone in a paragraph is shown; an image inside running text shows
  its alt text. Remote images do not load; the placeholder names the source.

**Reload**

- Watch the parent directory, filtered to the file. Settle for ~150 ms, but
  take a complete write (rename into place, close after write) immediately.
- A content digest skips reloads that changed nothing.
- The reading anchor, a heading plus the offset from the viewport top, keeps
  the text still. Never jump to the top on save.

## Security boundaries

No script runtime and no HTML parser, so there is no XSS-style surface. What
remains:

- **Paths**: canonicalization, symlinks and allowed roots are handled in one
  place and enforced at read time, not only at check time.
- **Image decoding** is the largest remaining surface, a C decoder on foreign
  data, which is why the pixel budget is checked from the header before a pixel
  is decoded.
- **No fake controls**: document content never produces clickable UI other than
  links and images, and a control is never selectable document text.
- **No telemetry.** Local logs contain no document content and no full private
  paths by default.
