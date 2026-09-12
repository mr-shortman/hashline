# Known limitations

The state of v1 as shipped. These are accepted, not open work items: v1 is
closed with them. Anything here that v2 touches is named in
[roadmap.md](roadmap.md).

## Numbers that miss their target

See [metrics.md](metrics.md) for the full table and the method.

- **Start to readable text at 100 KiB: 172 ms p95 against 120 ms.** What is
  left is the ~28 ms of font warm-up before the first frame. 1 MiB and 10 MiB
  hold their targets.
- **PSS at 100 KiB: 40.8 MiB against 40 MiB.** About 1.5 MiB of that is the CJK
  font the fixture itself asks for; without that one word it is 38.9 MiB.
- **Scroll frames inside the refresh budget: 93.2 % against 99 %**, from a
  single sample. Vulkan measured 98.3 % in the same sample and lost on memory.
  The n = 30 series at 60 and 120 Hz was never run.
- **The renderer choice is not enforced.** Every number assumes
  `GSK_RENDERER=cairo`, but nothing in the code, the desktop entry or the
  package sets it, so an installed Hashline runs on the GTK default: roughly an
  88 MiB floor instead of 33 MiB.

## Never measured

The reference machine with integrated graphics was never chosen, so no series
is an acceptance in the strict sense. Everything below has tooling but no run:

- Opening a file in the running instance, search and menu opening, tab
  switching, live reload to visible text.
- The comparison run against all four competitors.
- Orca operation, the full visual pass across themes, widths and zoom levels,
  and a real Wayland desktop session with file manager, drag and drop and
  clipboard.

## Rendering

- **No horizontal scrolling inside blocks.** Over-wide tables and code blocks
  are clipped. Clipped table cells are not laid out and therefore not
  clickable; their text is still searched and copied.
- **A paragraph over 2 KiB is split**, and the last line of each part ends
  where the part ends instead of at the column.
- **Prose height estimates run about 32 % low**, so the scrollbar is too short,
  never too long, and corrects while reading. Code block estimates are exact to
  0.2 %.
- Images inside running text show their alt text instead of the image.
- Footnotes and definition lists get no styling of their own.
- Raw HTML stays source text, `details` and `summary` included. That is by
  design; the price is that HTML-heavy documents look different from GitHub.
  72 of the 652 CommonMark examples differ for this reason.
- Remote images do not load.

## Behavior

- Search has no expanding Unicode folding, so a sharp s does not find "ss" and
  a ligature does not find its letters. Adding it would require the offset
  table that the search deliberately avoids.
- Image decoding uses gdk-pixbuf. A sandboxed decoder (glycin) was never
  decided; the pixel budget and the directory limit sit in one place, so a
  swap does not touch them.
- Tabs do not restore after a restart, cannot be grouped, and cannot be dragged
  between windows.
- A saved reading position is a heading plus an offset, so it is lost when that
  heading's text changes.
