# Roadmap

## v1 — closed

Hashline v1 is a read-only Markdown viewer for Linux: virtualized layout, tabs,
search, outline, syntax highlighting, live reload, themes, AT-SPI, and a `.deb`
for Ubuntu 26.04 amd64 with desktop entry, MIME registration and an isolated
install test.

It closes with the current round of startup and interaction work. What it does
not do and where it misses its own targets is written down once, in
[limitations.md](limitations.md), and is accepted rather than carried forward as
a task list. Targets and measured numbers stay in [metrics.md](metrics.md); they
remain the baseline v2 must not regress against.

## v2 — the mandate

v2 is not a bigger viewer. It is a different program, and these points are
given, not up for discussion in the planning:

### 1. A reader for text-shaped files, not only Markdown

Markdown becomes one producer among several. The cut already exists in the
code: the application only ever sees `OpDocument`, and `hashline_markdown::parse`
is the single place Markdown enters. `OpDocument` becomes the document
interface, and every new format inherits virtualization, whole-document search,
cross-block selection, live reload, reading positions and themes without doing
anything for them.

**Text-shaped yes, paginated never. This is permanent.** PDF, EPUB, DjVu,
Office formats and images are out and stay out. They need a second rendering
pipeline and a library the size of poppler, mupdf or a WebView, which hands
back exactly what the native build bought: the memory floor and the cold start.
A page is not a block, and page navigation is not block virtualization.

The advantage worth having — opening time and memory independent of document
size — is worth least on Markdown and most on the files where every graphical
editor on Linux falls over: a log after a long run, a CSV export, a generated
JSON dump, a large generated source file. Today that is `less` and nothing
graphical.

### 2. An editor, not a viewer

WYSIWYG editing, with the resource profile of the viewer intact. Not a
Markdown source pane with a preview beside it: the laid-out document is what
the caret moves through and what typing changes.

The two halves of that sentence are in tension, and the tension is the project.
An editor that is slower or heavier than the viewer has missed the point; a
viewer with a caret bolted on is not an editor.

### 3. Smart, efficient state management

Editing turns the document from a value into a state. What today is an
immutable `Arc` replaced wholesale by the newest `RequestId` becomes a thing
that changes under the layout, the block plan, the search index, the outline
and the file watcher at once. Getting that model right is the core work of v2,
not the visible editing surface.

### 4. One package with everything

Format producers are Cargo features so a build without one is possible, but
exactly one package ships and it contains all of them. No install-time
selection: the whole binary is 3.8 MiB against a 33 MiB memory floor, so there
is no number a user could decide on. Every producer loads on the first document
of its kind, never at startup, and the empty window's memory floor must not
move.

### 5. The name

Undecided on purpose. A permanent name is chosen when v2 is done and the
program's shape is known.

## What v2 has to answer

The mandate above is settled; these are the questions the plan has to answer,
and they are roughly in the order the work depends on them.

**The document model**
- Does editing change the source text and reparse incrementally, or does it
  change a tree that is serialized back? The first keeps one source of truth
  and needs incremental parsing; the second needs round-trip fidelity for every
  format.
- Today the mapping runs one way, document byte to laid-out position. A caret
  needs the inverse, and an edit needs to move every offset after it: block
  plan, measured heights, search matches, outline, selection, reading anchor.
- What is the unit of re-layout after a keystroke? A block, a part, a paragraph?
  The 16 ms budget has to hold on the 10 MiB fixture, not just on a README.

**Editing behavior**
- Undo and redo: scope, granularity, memory, and whether it survives a reload.
- Save: explicit or automatic, and how an atomic write interacts with the
  watcher that currently reloads whatever appears on disk.
- External change while the buffer is dirty. Today reload always wins; that is
  no longer acceptable once typing exists.
- IME, and an AT-SPI text interface that also accepts insertion.

**Formats**
- Which formats are editable at all. Markdown yes. A log file is a read-only
  view of something a program writes; CSV and JSON are editable in principle
  but with very different semantics.
- Order of work. Plain text and source code are nearly free as a reader: syntect
  is already paid for, a source file is one code block, and code blocks already
  split into virtualized parts. CSV stays blocked until the table gaps in
  [limitations.md](limitations.md) are fixed, because a CSV file is the worst
  case for exactly those two bugs.
- Each producer brings its own fixtures, small and large, states its own
  outline (functions, top-level keys, nothing), and detects by extension first
  and content second, falling back to plain text so no file is ever refused.

**Budgets**
- New ones are needed: keystroke to frame, time to first edit on a large file,
  memory with undo history, save time. The existing table in
  [metrics.md](metrics.md) stays valid and is the floor: v2 does not get to be
  slower at opening and reading because it can now write.

**Security**
- The program has never written a file. Writing changes the threat model:
  atomic saves, permissions, symlinks, and no path to data loss on a crash or a
  full disk.
