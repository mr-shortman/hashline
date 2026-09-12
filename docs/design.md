# Design

The source of truth is code: `crates/hashline/src/theme/mod.rs` for tokens and
typography, `theme/chrome.rs` for the UI chrome. This page is the readable
copy; change both together.

Visual checks run through `examples/render`, which lays out a document with the
widget's own layout code into a PNG (see [development.md](development.md)).

## Tokens

Light and dark come from one definition. No value is invented at the point of
use.

| Token | Light | Dark |
| --- | --- | --- |
| `bg` | `#fbfaf8` | `#1c201e` |
| `surface` | `#ffffff` | `#222724` |
| `subtle` | `#f1f0ed` | `#2a302c` |
| `text` | `#242826` | `#e4e7e2` |
| `muted` | `#636b65` | `#a5aea7` |
| `border` | `#dedfd9` | `#3b433d` |
| `accent` | `#37684d` | `#9ac6a6` |
| `accent-soft` | `#e6eee7` | `#2d4133` |
| `code` | `#f0f1ee` | `#242b26` |
| `error` | `#963b30` | `#f0a89b` |
| `syntax-keyword` | `#88458f` | `#d8a2de` |
| `syntax-string` | `#326943` | `#abd2a3` |
| `syntax-number` | `#895519` | `#e7bf85` |

Radius 7 px, base spacing 8 px, motion 120 ms (respects
`gtk-enable-animations`). UI root size 14 px. Normal text keeps at least 4.5:1
contrast.

## Fonts

System font from `gtk-font-name`; code uses the fontconfig alias `monospace`.
No bundled or downloaded fonts.

## Document typesetting

- **Column**: 76 characters of the body font, measured rather than a fixed
  pixel width, centered in the viewport. Padding 42 px top, 32 px sides, 100 px
  bottom. A narrower window uses the available width minus the side padding.
- **Body**: 17 px at 100 % zoom, line height 1.65. Zoom scales this base size
  and every em-based value follows.
- **Headings**: H1 2.1 em, H2 1.5, H3 1.2, H4 1.05, H5 and H6 1.0. Weight 650,
  tracking -0.025 em, line height 1.3. Space 1.8 em before and 0.7 em after, so
  a heading belongs to the text below it. H2 carries a `border` rule 0.35 em
  below.
- **Blocks** (paragraphs, lists, quotes): 1.15 em below. List items 0.25 em,
  paragraphs inside items 0.4 em. Rules 2 em. The first block of a document has
  no space above it.
- **Code**: monospace on the `code` surface; inline code 0.85 em.
- **Tables**: 0.92 em with a visible header row.
- **Images**: fit the reading column and never exceed it. A missing or refused
  image gets a quiet placeholder with its alt text, at the height the layout
  already reserved.
- The document itself never scrolls horizontally.
- No flash of the wrong color scheme: the theme is resolved before the window
  shows content for the first time.

## Window

A `GtkApplicationWindow` with a native `GtkHeaderBar`: open, file name as the
title (full path as tooltip), outline, search, menu. The menu holds open,
reload, the color scheme as a segmented switch (system, light, dark) and text
size. No permanent status bar. Without a file: a calm empty view with "Open
Markdown file" and a drag-and-drop hint.
