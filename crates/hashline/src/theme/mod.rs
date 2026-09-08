//! Design tokens (SPEC.md, section 3).
//!
//! The values are transcribed from `docs/design/styles.css`, which is the
//! binding design reference. Nothing here is invented and nothing is invented
//! at the point of use either: a colour or a spacing the view needs is added
//! here first.

/// A colour as GTK wants it: straight-alpha RGBA in the 0..=1 range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
    pub alpha: f32,
}

impl Color {
    /// `0xRRGGBB`, opaque. Written as hex so the table below can be read
    /// against the stylesheet without arithmetic.
    const fn hex(value: u32) -> Self {
        Color {
            red: ((value >> 16) & 0xff) as f32 / 255.0,
            green: ((value >> 8) & 0xff) as f32 / 255.0,
            blue: (value & 0xff) as f32 / 255.0,
            alpha: 1.0,
        }
    }
    pub fn with_alpha(self, alpha: f32) -> Self {
        Color { alpha, ..self }
    }
    pub fn to_gdk(self) -> gtk::gdk::RGBA {
        gtk::gdk::RGBA::new(self.red, self.green, self.blue, self.alpha)
    }
}

/// Which of the two variants the palette describes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    Light,
    Dark,
}

/// The token table from SPEC.md, section 3. Both variants come from this one
/// definition; neither is derived from the other by lightening or darkening.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub variant: Variant,
    pub bg: Color,
    pub surface: Color,
    pub subtle: Color,
    pub text: Color,
    pub muted: Color,
    pub border: Color,
    pub accent: Color,
    pub accent_soft: Color,
    pub code: Color,
    pub error: Color,
    pub syntax_keyword: Color,
    pub syntax_string: Color,
    pub syntax_number: Color,
}

pub const LIGHT: Palette = Palette {
    variant: Variant::Light,
    bg: Color::hex(0xfbfaf8),
    surface: Color::hex(0xffffff),
    subtle: Color::hex(0xf1f0ed),
    text: Color::hex(0x242826),
    muted: Color::hex(0x636b65),
    border: Color::hex(0xdedfd9),
    accent: Color::hex(0x37684d),
    accent_soft: Color::hex(0xe6eee7),
    code: Color::hex(0xf0f1ee),
    error: Color::hex(0x963b30),
    syntax_keyword: Color::hex(0x88458f),
    syntax_string: Color::hex(0x326943),
    syntax_number: Color::hex(0x895519),
};

pub const DARK: Palette = Palette {
    variant: Variant::Dark,
    bg: Color::hex(0x1c201e),
    surface: Color::hex(0x222724),
    subtle: Color::hex(0x2a302c),
    text: Color::hex(0xe4e7e2),
    muted: Color::hex(0xa5aea7),
    border: Color::hex(0x3b433d),
    accent: Color::hex(0x9ac6a6),
    accent_soft: Color::hex(0x2d4133),
    code: Color::hex(0x242b26),
    error: Color::hex(0xf0a89b),
    syntax_keyword: Color::hex(0xd8a2de),
    syntax_string: Color::hex(0xabd2a3),
    syntax_number: Color::hex(0xe7bf85),
};

impl Palette {
    pub fn of(variant: Variant) -> Self {
        match variant {
            Variant::Light => LIGHT,
            Variant::Dark => DARK,
        }
    }
    /// The selection wash. The stylesheet tints the accent rather than using
    /// the system highlight, so that selection reads as part of the document.
    pub fn selection(&self) -> Color {
        match self.variant {
            Variant::Light => self.accent.with_alpha(0.20),
            Variant::Dark => self.accent.with_alpha(0.28),
        }
    }
}

/// Geometry that does not depend on the type size.
pub const RADIUS: f32 = 7.0;
/// The base spacing step; the document's own spacings are em-based.
pub const SPACING: f32 = 8.0;
/// Transition duration, honoured only when `gtk-enable-animations` is on.
pub const MOTION_MS: u32 = 120;

/// Type sizes and spacings for the document, in ems of the body size unless
/// stated otherwise (SPEC.md, section 3, "Dokumentsatz").
pub mod document {
    /// Body size in logical pixels at 100 % zoom.
    pub const BODY_PX: f64 = 17.0;
    pub const LINE_HEIGHT: f64 = 1.65;
    /// The reading column, in characters — not pixels. It is measured against
    /// the real character width of the body font, so it reflows on zoom.
    pub const COLUMN_CHARS: f64 = 76.0;
    pub const PAD_TOP: f64 = 42.0;
    pub const PAD_SIDE: f64 = 32.0;
    pub const PAD_BOTTOM: f64 = 100.0;

    /// H1..H6, in ems.
    pub const HEADING_SCALE: [f64; 6] = [2.1, 1.5, 1.2, 1.05, 1.0, 1.0];
    pub const HEADING_WEIGHT: i32 = 650;
    pub const HEADING_TRACKING_EM: f64 = -0.025;
    pub const HEADING_LINE_HEIGHT: f64 = 1.3;
    /// The larger gap sits *before* a heading, so it belongs to the text that
    /// follows it rather than to the text above.
    pub const HEADING_SPACE_BEFORE_EM: f64 = 1.8;
    pub const HEADING_SPACE_AFTER_EM: f64 = 0.7;
    /// Gap between an H2 and its underline.
    pub const H2_RULE_GAP_EM: f64 = 0.35;

    pub const BLOCK_SPACING_EM: f64 = 1.15;
    pub const LIST_ITEM_SPACING_EM: f64 = 0.25;
    pub const LIST_PARAGRAPH_SPACING_EM: f64 = 0.4;
    pub const RULE_SPACING_EM: f64 = 2.0;

    pub const INLINE_CODE_EM: f64 = 0.85;
    pub const TABLE_EM: f64 = 0.92;

    // The block decoration, transcribed from `docs/design/styles.css`. Values
    // in em are relative to the size the block itself is set at, matching how
    // CSS resolves them.
    /// `ul, ol { padding-left: 1.6em }`
    pub const LIST_INDENT_EM: f64 = 1.6;
    /// `input[type=checkbox] { margin-right: 0.45em }`
    pub const MARKER_GAP_EM: f64 = 0.45;
    /// `blockquote { border-left: 3px; padding: 0.15em 1.2em }`
    pub const QUOTE_BAR_PX: f64 = 3.0;
    pub const QUOTE_PAD_X_EM: f64 = 1.2;
    pub const QUOTE_PAD_Y_EM: f64 = 0.15;
    /// `hr { border-top: 1px }`
    pub const RULE_PX: f64 = 1.0;
    /// `pre { padding: 42px 18px 18px; line-height: 1.55; margin: 1.3em 0 }`
    /// The generous top padding is where the copy-code control sits.
    pub const CODE_PAD_TOP: f64 = 42.0;
    pub const CODE_PAD_X: f64 = 18.0;
    pub const CODE_PAD_BOTTOM: f64 = 18.0;
    pub const CODE_LINE_HEIGHT: f64 = 1.55;
    pub const CODE_SPACING_EM: f64 = 1.3;
    /// `:not(pre) > code { border-radius: 4px; padding: 0.15em 0.35em }`
    pub const INLINE_CODE_RADIUS: f32 = 4.0;
    /// `th, td { padding: 0.6em 0.9em; border: 1px }`
    pub const CELL_PAD_X_EM: f64 = 0.9;
    pub const CELL_PAD_Y_EM: f64 = 0.6;
    pub const CELL_BORDER_PX: f64 = 1.0;
    /// `.table-scroll { margin: 1.2em 0 }`
    pub const TABLE_SPACING_EM: f64 = 1.2;
    /// `a { text-underline-offset: 3px }`
    pub const LINK_UNDERLINE_OFFSET_PX: f64 = 3.0;

    /// Zoom is 80..=200 % in steps of ten (SPEC.md, section 3, "Bedienung").
    pub const ZOOM_MIN: i32 = 80;
    pub const ZOOM_MAX: i32 = 200;
    pub const ZOOM_STEP: i32 = 10;

    pub fn clamp_zoom(percent: i32) -> i32 {
        percent.clamp(ZOOM_MIN, ZOOM_MAX)
    }
}
