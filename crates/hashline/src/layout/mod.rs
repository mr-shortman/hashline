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
    code_language, set_block, BlockLayout, Decoration, ImageSource, Link, NoImages, Piece, Style,
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
    fn space_after_em(self) -> f64 {
        match self {
            BlockKind::Heading(_) => document::HEADING_SPACE_AFTER_EM,
            BlockKind::Rule => document::RULE_SPACING_EM,
            _ => document::BLOCK_SPACING_EM,
        }
    }
    /// Space above the block. Only headings carry one, and it is the larger of
    /// the two gaps so that a heading belongs to the text beneath it.
    fn space_before_em(self) -> f64 {
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

#[derive(Clone, Copy, Debug)]
pub struct Block {
    pub kind: BlockKind,
    pub op_start: u32,
    pub op_count: u32,
    pub text_start: u32,
    pub text_len: u32,
    /// Height including the space above and below the block.
    pub height: f64,
    /// False while `height` is still an estimate.
    pub measured: bool,
}

pub struct BlockPlan {
    blocks: Vec<Block>,
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
        for index in 0..count {
            let words = &document.blocks[index * BLOCK_WORDS..(index + 1) * BLOCK_WORDS];
            let kind = BlockKind::from_tag(words[0]);
            let mut block = Block {
                kind,
                op_start: words[1],
                op_count: words[2],
                text_start: words[3],
                text_len: words[4],
                height: 0.0,
                measured: false,
            };
            block.height = estimate(&block, metrics, width);
            blocks.push(block);
        }
        let offsets = Offsets::new(blocks.iter().map(|block| block.height));
        BlockPlan {
            blocks,
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

    /// Replaces a block's estimate with its measured height.
    ///
    /// Returns how much everything below the block moved. The view adds this to
    /// its scroll offset when the block sits above the reading position, which
    /// is what keeps the visible text still — the single most common defect of
    /// virtualized lists, and an acceptance failure here (SPEC.md, section 5).
    pub fn set_measured(&mut self, index: usize, height: f64) -> f64 {
        let previous = self.blocks[index].height;
        let delta = height - previous;
        if delta != 0.0 {
            self.blocks[index].height = height;
            self.offsets.add(index, delta);
        }
        self.blocks[index].measured = true;
        delta
    }

    /// Drops every measurement and estimates again, for a new width, zoom or
    /// font. The reading anchor is what restores the position afterwards
    /// (SPEC.md, section 7).
    pub fn reflow(&mut self, metrics: Metrics, width: f64) {
        self.metrics = metrics;
        self.width = width;
        for block in self.blocks.iter_mut() {
            block.measured = false;
            block.height = estimate(block, metrics, width);
        }
        self.offsets = Offsets::new(self.blocks.iter().map(|block| block.height));
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
/// what measuring is for.
fn estimate(block: &Block, metrics: Metrics, width: f64) -> f64 {
    let em = metrics.body_px;
    let space = (block.kind.space_before_em() + block.kind.space_after_em()) * em;
    if block.kind == BlockKind::Rule {
        return space + 1.0;
    }
    let size = metrics.body_px * block.kind.size_em();
    let line_height = match block.kind {
        BlockKind::Heading(_) => size * document::HEADING_LINE_HEIGHT,
        _ => size * document::LINE_HEIGHT,
    };
    let char_width = metrics.char_width * block.kind.size_em();
    let per_line = (width / char_width).max(1.0);
    // A code block does not wrap: it scrolls inside its own block, so its
    // height follows the number of newlines rather than the width.
    let lines = match block.kind {
        BlockKind::Code => 1.0,
        _ => (block.text_len as f64 / per_line).ceil().max(1.0),
    };
    lines * line_height + space
}
