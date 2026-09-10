//! The block plan (SPEC.md, section 5, "Das zentrale Layoutproblem").
//!
//! A virtualized viewer has to know a total height before it has set every
//! block. The plan resolves that by estimating every block up front and
//! replacing an estimate with a measurement once the block is actually laid
//! out. Two properties make that safe, and both are tested:
//!
//! * the total height and the position of any block are always consistent with
//!   the heights currently held, estimate or measurement alike, and
//! * replacing an estimate above the reading position reports how far the
//!   content below it moved, so the view can hold the visible text still.

mod offsets;
mod text;
mod textmap;

pub use offsets::Offsets;
pub use text::{
    code_language, faces, load_face, set_block, BlockLayout, Decoration, Face, ImageSource, Link,
    NoImages, Piece, Style,
};
pub use textmap::TextMap;

use hashline_markdown::{OpDocument, BLOCK_WORDS};

use crate::theme::document;

/// What a top-level block is, as far as measuring and drawing is concerned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockKind {
    Paragraph,
    /// Level 1..=6.
    Heading(u8),
    Code,
    Quote,
    List,
    Rule,
    Table,
    Other,
}

impl BlockKind {
    /// Tag ids are indices into `ALLOWED_TAGS`; see the parser's op vocabulary.
    fn from_tag(tag: u32) -> Self {
        match tag {
            0 => BlockKind::Paragraph,
            1..=6 => BlockKind::Heading(tag as u8),
            12 => BlockKind::Code,
            13 => BlockKind::Quote,
            14 | 15 => BlockKind::List,
            17 => BlockKind::Rule,
            21 => BlockKind::Table,
            _ => BlockKind::Other,
        }
    }
    /// The type size the block is set at, as a multiple of the body size.
    pub fn size_em(self) -> f64 {
        match self {
            BlockKind::Heading(level) => document::HEADING_SCALE[level as usize - 1],
            BlockKind::Table => document::TABLE_EM,
            _ => 1.0,
        }
    }
    /// Space below the block, in ems of the *body* size.
    ///
    /// The estimate and the finished layout read this same function, so an
    /// estimate can never disagree with the block it is standing in for.
    pub(crate) fn space_after_em(self) -> f64 {
        match self {
            BlockKind::Heading(_) => document::HEADING_SPACE_AFTER_EM,
            BlockKind::Rule => document::RULE_SPACING_EM,
            BlockKind::Code => document::CODE_SPACING_EM,
            BlockKind::Table => document::TABLE_SPACING_EM,
            _ => document::BLOCK_SPACING_EM,
        }
    }
    /// Space above the block. Only headings carry one, and it is the larger of
    /// the two gaps so that a heading belongs to the text beneath it.
    pub(crate) fn space_before_em(self) -> f64 {
        match self {
            BlockKind::Heading(_) => document::HEADING_SPACE_BEFORE_EM,
            _ => 0.0,
        }
    }
}

/// What estimating a height needs to know about the type.
#[derive(Clone, Copy, Debug)]
pub struct Metrics {
    /// Mean advance of the body font, in logical pixels.
    pub char_width: f64,
    /// Body size in logical pixels, after zoom.
    pub body_px: f64,
}

impl Metrics {
    pub fn line_height(&self) -> f64 {
        self.body_px * document::LINE_HEIGHT
    }
}

/// One entry of the plan. The 10 MiB fixture has 144 651 of them, so every
/// field is sized for what it can actually hold: a height is a pixel count and
/// does not need double precision, and the source limit of 20 MiB divided by
/// the smallest part bounds the part count well below `u16::MAX`.
#[derive(Clone, Copy, Debug)]
pub struct Block {
    pub kind: BlockKind,
    pub op_start: u32,
    pub op_count: u32,
    pub text_start: u32,
    pub text_len: u32,
    /// Which part of its source block this is, how many parts that block was
    /// cut into, and whether the height beside them has been measured — packed
    /// into one word because three separate fields cost eight bytes of a
    /// 24-byte entry and the plan has 144 651 of them.
    ///
    /// `part` in the low 15 bits, `parts` in the next 15, `measured` on top.
    /// The source limit of 20 MiB over the smallest part keeps both counts far
    /// below their 32 767.
    cut: u32,
    /// Height including the space above and below the block.
    pub height: f32,
}

/// Lines of a code block's text, counted when the plan is built. Only code
/// uses it — code does not wrap, so its height follows its lines — so it is
/// held for those blocks alone instead of costing four bytes in every entry.
const PART_BITS: u32 = 15;
const PART_MASK: u32 = (1 << PART_BITS) - 1;

impl Block {
    fn pack(part: usize, parts: usize) -> u32 {
        debug_assert!(parts <= PART_MASK as usize, "part count must fit 15 bits");
        (part as u32 & PART_MASK) | ((parts as u32 & PART_MASK) << PART_BITS)
    }
    pub fn part(&self) -> u32 {
        self.cut & PART_MASK
    }
    pub fn parts(&self) -> u32 {
        (self.cut >> PART_BITS) & PART_MASK
    }
    /// False while `height` is still an estimate.
    pub fn measured(&self) -> bool {
        self.cut >> (2 * PART_BITS) != 0
    }
    fn set_measured(&mut self, measured: bool) {
        let flag = (measured as u32) << (2 * PART_BITS);
        self.cut = (self.cut & !(1 << (2 * PART_BITS))) | flag;
    }
}

impl Block {
    /// The height, widened for arithmetic against document coordinates.
    pub fn height(&self) -> f64 {
        self.height as f64
    }
    /// True for the first part of a cut-up block, and for every block that was
    /// not cut up. The space above a block, the top of a code panel and its
    /// copy control all belong to this part alone.
    pub fn is_first(&self) -> bool {
        self.part() == 0
    }
    /// True for the last part, and for every block that was not cut up.
    pub fn is_last(&self) -> bool {
        self.part() + 1 >= self.parts()
    }
}

/// A block is set in one piece, so a block far taller than a screen breaks the
/// plan's load-bearing assumption: virtualization cannot reach inside it, and
/// drawing any of it costs setting all of it. Two fixtures show how far that
/// goes — `large-code.md` is one fenced block of 70 004 lines and froze the
/// window for 67 seconds, and `long-line.md` is one paragraph of a million
/// characters that took 172 ms and held a Pango layout over the whole of it.
///
/// Blocks above these limits are therefore cut into parts, and a part is an
/// ordinary block of the plan: estimated, measured, cached and evicted on its
/// own. The limits come from measuring what setting costs, and each keeps one
/// part well inside the 16 ms budget of SPEC.md, section 9:
///
/// * Text wraps, so its limit is a byte count, and 2 KiB is about one screen
///   of the reading column. Setting that much costs well under a millisecond
///   of ordinary prose and about 2 ms of text without a single space — the
///   pathological case, where Pango's search for a break makes the cost grow
///   with the square of the length. It was 4 KiB until two such parts on one
///   screen came to 16.3 ms against a 16 ms budget; halving the part quarters
///   that, because the cost is quadratic.
/// * Code does not wrap, so its cost is driven by its line count, which grows
///   quadratically as well; 256 lines cost about 1 ms. The byte limit beside it
///   catches the block whose few lines are each very long.
///
/// Lists and tables are not cut: their parts are items and rows rather than
/// stretches of text, which the plan cannot address (docs/limitations.md).
const TEXT_PART_BYTES: usize = 2048;
const CODE_PART_LINES: u32 = 256;
const CODE_PART_BYTES: usize = 32 * 1024;

/// The cuts through one block's text, as `(start, end, lines)` relative to the
/// block. Always at least one, and always on character boundaries.
type Cut = (u32, u32, u32);

/// Cuts a code block after every `CODE_PART_LINES` lines or `CODE_PART_BYTES`
/// bytes, whichever comes first, and never inside a line: a part therefore
/// holds whole lines and sets exactly as it would inside the whole block.
fn code_cuts(text: &str, cuts: &mut Vec<Cut>) {
    let bytes = text.as_bytes();
    let (mut start, mut cursor, mut lines) = (0usize, 0usize, 0u32);
    while cursor < bytes.len() {
        let end = match bytes[cursor..].iter().position(|&byte| byte == b'\n') {
            Some(offset) => cursor + offset + 1,
            None => bytes.len(),
        };
        cursor = end;
        lines += 1;
        if lines >= CODE_PART_LINES || end - start >= CODE_PART_BYTES {
            cuts.push((start as u32, end as u32, lines));
            start = end;
            lines = 0;
        }
    }
    if start < bytes.len() || cuts.is_empty() {
        cuts.push((start as u32, bytes.len() as u32, lines.max(1)));
    }
}

/// Cuts running text every `TEXT_PART_BYTES`, preferring the last word
/// boundary in the quarter before the limit. A cut therefore falls between
/// words wherever the text has any, and only text without a single space —
/// which is exactly the case this exists for — is cut mid-word.
///
/// Wrapping still differs from the uncut block at one place per cut: the last
/// line of a part ends where the part does instead of where the column does.
/// That is the price of reaching inside a block the plan otherwise cannot.
fn text_cuts(text: &str, cuts: &mut Vec<Cut>) {
    let mut start = 0usize;
    while text.len() - start > TEXT_PART_BYTES {
        // Both ends of the window are floored to a character boundary before
        // anything is sliced: a limit landing inside a multi-byte character is
        // the normal case, not the exception.
        let limit = floor_boundary(text, start + TEXT_PART_BYTES);
        let window = floor_boundary(text, start + TEXT_PART_BYTES * 3 / 4);
        let cut = text[window..limit]
            .char_indices()
            .rev()
            .find(|(_, character)| character.is_whitespace())
            .map(|(offset, character)| window + offset + character.len_utf8())
            .unwrap_or(limit);
        cuts.push((start as u32, cut as u32, 0));
        start = cut;
    }
    cuts.push((start as u32, text.len() as u32, 0));
}

/// The character boundary at or below `at`; slicing anywhere else would panic.
fn floor_boundary(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

pub struct BlockPlan {
    blocks: Vec<Block>,
    /// `(plan index, lines)` for code blocks only, in index order.
    code_lines: Vec<(u32, u32)>,
    offsets: Offsets,
    metrics: Metrics,
    width: f64,
}

impl BlockPlan {
    /// Builds the plan for the whole document in one pass. Nothing is laid out
    /// here: this must stay cheap enough for the 10 MiB fixture.
    pub fn new(document: &OpDocument, metrics: Metrics, width: f64) -> Self {
        let count = document.blocks.len() / BLOCK_WORDS;
        let mut blocks = Vec::with_capacity(count);
        let mut code_lines: Vec<(u32, u32)> = Vec::new();
        let mut cuts: Vec<Cut> = Vec::new();
        for index in 0..count {
            let words = &document.blocks[index * BLOCK_WORDS..(index + 1) * BLOCK_WORDS];
            let kind = BlockKind::from_tag(words[0]);
            let (start, len) = (words[3], words[4]);
            let text = &document.text[start as usize..(start + len) as usize];
            cuts.clear();
            match kind {
                BlockKind::Code => code_cuts(text, &mut cuts),
                // A rule has no text, and a list or a table is structure the
                // plan cannot cut through.
                BlockKind::Rule | BlockKind::List | BlockKind::Table => cuts.push((0, len, 0)),
                _ => text_cuts(text, &mut cuts),
            }
            let parts = cuts.len();
            for (part, &(from, to, lines)) in cuts.iter().enumerate() {
                let mut block = Block {
                    kind,
                    op_start: words[1],
                    op_count: words[2],
                    text_start: start + from,
                    text_len: to - from,
                    cut: Block::pack(part, parts),
                    height: 0.0,
                };
                if kind == BlockKind::Code {
                    code_lines.push((blocks.len() as u32, lines));
                }
                block.height = estimate(&block, metrics, width, lines) as f32;
                blocks.push(block);
            }
        }
        let offsets = Offsets::new(blocks.iter().map(|block| block.height as f64));
        code_lines.shrink_to_fit();
        BlockPlan {
            blocks,
            code_lines,
            offsets,
            metrics,
            width,
        }
    }

    pub fn len(&self) -> usize {
        self.blocks.len()
    }
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }
    pub fn block(&self, index: usize) -> &Block {
        &self.blocks[index]
    }
    /// How many lines of code this block holds; zero for anything else.
    pub fn lines(&self, index: usize) -> u32 {
        self.code_lines
            .binary_search_by_key(&(index as u32), |&(at, _)| at)
            .map(|found| self.code_lines[found].1)
            .unwrap_or(0)
    }
    pub fn metrics(&self) -> Metrics {
        self.metrics
    }
    pub fn width(&self) -> f64 {
        self.width
    }

    /// The document's height, estimates included.
    pub fn total_height(&self) -> f64 {
        self.offsets.total() + document::PAD_TOP + document::PAD_BOTTOM
    }

    /// Top edge of a block, in document coordinates.
    pub fn y_of(&self, index: usize) -> f64 {
        document::PAD_TOP + self.offsets.prefix(index)
    }

    /// The block containing `y`, clamped to the document.
    pub fn block_at(&self, y: f64) -> usize {
        if self.blocks.is_empty() {
            return 0;
        }
        self.offsets.search(y - document::PAD_TOP)
    }

    /// The blocks to lay out for a viewport, plus one screen of buffer in each
    /// direction (SPEC.md, section 5).
    pub fn visible_range(&self, top: f64, height: f64) -> std::ops::Range<usize> {
        if self.blocks.is_empty() {
            return 0..0;
        }
        let first = self.block_at(top - height);
        let last = self.block_at(top + height * 2.0);
        first..(last + 1).min(self.blocks.len())
    }

    /// The blocks actually on screen, without the buffer around them. These are
    /// the ones a frame cannot do without; the buffer can wait for an idle
    /// moment, which is what keeps a jump inside the 16 ms budget
    /// (docs/decisions/014-competitive-targets.md, section 3.3).
    pub fn onscreen_range(&self, top: f64, height: f64) -> std::ops::Range<usize> {
        if self.blocks.is_empty() {
            return 0..0;
        }
        let first = self.block_at(top);
        let last = self.block_at(top + height);
        first..(last + 1).min(self.blocks.len())
    }

    /// Replaces a block's estimate with its measured height.
    ///
    /// Returns how much everything below the block moved. The view adds this to
    /// its scroll offset when the block sits above the reading position, which
    /// is what keeps the visible text still — the single most common defect of
    /// virtualized lists, and an acceptance failure here (SPEC.md, section 5).
    pub fn set_measured(&mut self, index: usize, height: f64) -> f64 {
        let previous = self.blocks[index].height as f64;
        let delta = height - previous;
        if delta != 0.0 {
            self.blocks[index].height = height as f32;
            self.offsets.add(index, height as f32 as f64 - previous);
        }
        self.blocks[index].set_measured(true);
        delta
    }

    /// Drops every measurement and estimates again, for a new width, zoom or
    /// font. The reading anchor is what restores the position afterwards
    /// (SPEC.md, section 7).
    pub fn reflow(&mut self, metrics: Metrics, width: f64) {
        self.metrics = metrics;
        self.width = width;
        for index in 0..self.blocks.len() {
            let lines = self.lines(index);
            let block = &mut self.blocks[index];
            block.set_measured(false);
            block.height = estimate(block, metrics, width, lines) as f32;
        }
        self.offsets = Offsets::new(self.blocks.iter().map(|block| block.height as f64));
    }

    /// The plan indices that make up one block of the document. A block that
    /// was small enough to set in one piece is alone in its range.
    pub fn source_blocks(&self, index: usize) -> std::ops::Range<usize> {
        let block = &self.blocks[index];
        let first = index - block.part() as usize;
        first..first + block.parts() as usize
    }

    /// The source block at `index` as it would be without any cut: one block
    /// covering the whole of the author's text. Nothing in the reader sets
    /// this — the point of cutting is that it never has to — but comparing a
    /// part against the whole is how the cuts are checked.
    pub fn whole(&self, index: usize) -> Block {
        let range = self.source_blocks(index);
        let (start, end) = self.source_text_range(index);
        let mut block = self.blocks[range.start];
        block.text_start = start;
        block.text_len = end - start;
        block.cut = Block::pack(0, 1);
        block
    }

    /// The document text of the *whole* block at `index`, not just of the part
    /// that index names — what copying a code block has to produce even when
    /// the control was clicked on its first part.
    pub fn source_text_range(&self, index: usize) -> (u32, u32) {
        let range = self.source_blocks(index);
        let first = &self.blocks[range.start];
        let last = &self.blocks[range.end - 1];
        (first.text_start, last.text_start + last.text_len)
    }

    /// The block whose text range contains `offset`, for a search hit or a
    /// selection position. Ranges are ordered and non-overlapping, so this is a
    /// binary search; separators between blocks belong to no block and resolve
    /// to the block that follows.
    pub fn block_for_text(&self, offset: u32) -> Option<usize> {
        if self.blocks.is_empty() {
            return None;
        }
        let found = self
            .blocks
            .binary_search_by(|block| {
                if offset < block.text_start {
                    std::cmp::Ordering::Greater
                } else if offset >= block.text_start + block.text_len {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .ok();
        found.or_else(|| {
            self.blocks
                .iter()
                .position(|block| block.text_start >= offset)
        })
    }
}

/// The estimate: characters, the width they have, and the type they are set in.
///
/// It is deliberately crude. Its job is to make the scrollbar plausible and the
/// jump to an unmeasured block land close, not to predict the layout — that is
/// what measuring is for. The one thing it may not do is be wrong by an order
/// of magnitude, because then the scrollbar describes a document that does not
/// exist and every jump lands somewhere else.
fn estimate(block: &Block, metrics: Metrics, width: f64, lines: u32) -> f64 {
    let em = metrics.body_px;
    // The space around a block belongs to its outer edges: a block cut into
    // parts is still one block of the document.
    let before = if block.is_first() {
        block.kind.space_before_em()
    } else {
        0.0
    };
    let after = if block.is_last() {
        block.kind.space_after_em()
    } else {
        0.0
    };
    let space = (before + after) * em;
    if block.kind == BlockKind::Rule {
        return space + 1.0;
    }
    // A code block does not wrap: it scrolls inside its own block, so its
    // height follows the lines the plan counted, not the width. Its padding is
    // part of the panel and, like the spacing, belongs to the outer parts.
    if block.kind == BlockKind::Code {
        let line_height = em * document::INLINE_CODE_EM * document::CODE_LINE_HEIGHT;
        let pad_top = if block.is_first() {
            document::CODE_PAD_TOP
        } else {
            0.0
        };
        let pad_bottom = if block.is_last() {
            document::CODE_PAD_BOTTOM
        } else {
            0.0
        };
        return lines.max(1) as f64 * line_height + pad_top + pad_bottom + space;
    }
    let size = metrics.body_px * block.kind.size_em();
    let line_height = match block.kind {
        BlockKind::Heading(_) => size * document::HEADING_LINE_HEIGHT,
        _ => size * document::LINE_HEIGHT,
    };
    let char_width = metrics.char_width * block.kind.size_em();
    let per_line = (width / char_width).max(1.0);
    let lines = (block.text_len as f64 / per_line).ceil().max(1.0);
    lines * line_height + space
}
