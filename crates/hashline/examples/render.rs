//! Renders a document to a PNG with no window and no compositor.
//!
//! It drives the same `layout` code the widget does, so what comes out is the
//! real typography rather than a second implementation of it. That makes it
//! usable for the visual acceptance in SPEC.md, section 12 — comparing the
//! native setting against the reference captures — and for looking at the
//! output at all on a machine where screenshots are not permitted.
//!
//! Usage: `cargo run -p hashline --example render -- <file.md> [out.png] [width] [dark]`

use hashline::layout::{set_block, BlockPlan, Decoration, ImageSource, Metrics, Style};
use hashline::theme::{document as tokens, DARK, LIGHT};
use pango::prelude::*;
use pangocairo::cairo;

/// Pictures resolved against the document's directory, the same rule the view
/// applies — enough for looking at the result offscreen.
struct Pictures {
    base: std::path::PathBuf,
}

impl ImageSource for Pictures {
    fn intrinsic(&self, source: &str) -> Option<(f64, f64)> {
        if source.contains("://") {
            return None;
        }
        let path = self.base.join(source).canonicalize().ok()?;
        if !path.starts_with(self.base.canonicalize().ok()?) {
            return None;
        }
        let (_, width, height) = gtk::gdk_pixbuf::Pixbuf::file_info(&path)?;
        (width > 0 && height > 0).then_some((width as f64, height as f64))
    }
}

fn main() {
    let mut arguments = std::env::args().skip(1);
    let source_path = arguments.next().expect("a Markdown file");
    let out_path = arguments.next().unwrap_or_else(|| "render.png".to_string());
    let width: f64 = arguments
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(900.0);
    let dark = arguments.next().is_some_and(|value| value == "dark");
    let palette = if dark { DARK } else { LIGHT };

    let source = std::fs::read_to_string(&source_path).expect("readable file");
    let document = hashline_markdown::parse(&source);

    let context = pangocairo::FontMap::default().create_context();
    let style = Style::new("sans", "monospace", tokens::BODY_PX, palette);

    let metrics = context.metrics(Some(&style.body), None);
    let char_width = metrics.approximate_char_width() as f64 / pango::SCALE as f64;
    let column = (char_width * tokens::COLUMN_CHARS).min(width - 2.0 * tokens::PAD_SIDE);
    let plan = BlockPlan::new(
        &document,
        Metrics {
            char_width,
            body_px: tokens::BODY_PX,
        },
        column,
    );

    // Set blocks until the page is full, exactly as the view would for its
    // first screen.
    let page_height = 1400.0;
    let left = ((width - column) / 2.0).max(tokens::PAD_SIDE);
    let surface =
        cairo::ImageSurface::create(cairo::Format::ARgb32, width as i32, page_height as i32)
            .expect("surface");
    let cairo = cairo::Context::new(&surface).expect("context");
    cairo.set_source_rgb(
        palette.bg.red as f64,
        palette.bg.green as f64,
        palette.bg.blue as f64,
    );
    cairo.paint().unwrap();

    let pictures = Pictures {
        base: std::path::Path::new(&source_path)
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| ".".into()),
    };
    let mut y = tokens::PAD_TOP;
    let mut drawn = 0;
    for index in 0..plan.len() {
        if y > page_height {
            break;
        }
        let block = *plan.block(index);
        let set = set_block(&context, &document, &block, &style, column, &pictures);
        let block_top = y + set.baseline_offset();

        for decoration in &set.decorations {
            if let Decoration::Image {
                x,
                y: dy,
                width: w,
                height: h,
                source,
            } = decoration
            {
                let path = pictures.base.join(source);
                if let Ok(pixbuf) = gtk::gdk_pixbuf::Pixbuf::from_file(&path) {
                    cairo.save().unwrap();
                    let scale_x = w / pixbuf.width() as f64;
                    let scale_y = h / pixbuf.height() as f64;
                    cairo.translate(left + x, block_top + dy);
                    cairo.scale(scale_x, scale_y);
                    gtk::gdk::prelude::GdkCairoContextExt::set_source_pixbuf(
                        &cairo, &pixbuf, 0.0, 0.0,
                    );
                    cairo.paint().unwrap();
                    cairo.restore().unwrap();
                }
                continue;
            }
            let (x, dy, w, h, colour) = match decoration {
                Decoration::Fill {
                    x,
                    y,
                    width,
                    height,
                    color,
                    ..
                } => (*x, *y, *width, *height, *color),
                Decoration::Line {
                    x,
                    y,
                    width,
                    height,
                    color,
                } => (*x, *y, *width, *height, *color),
                Decoration::Image { .. } => unreachable!("handled above"),
            };
            cairo.set_source_rgb(colour.red as f64, colour.green as f64, colour.blue as f64);
            cairo.rectangle(left + x, block_top + dy, w, h);
            cairo.fill().unwrap();
        }
        for piece in &set.pieces {
            cairo.set_source_rgb(
                piece.color.red as f64,
                piece.color.green as f64,
                piece.color.blue as f64,
            );
            cairo.move_to(left + piece.x, block_top + piece.y);
            pangocairo::functions::show_layout(&cairo, &piece.layout);
        }
        y += set.height();
        drawn += 1;
    }

    drop(cairo);
    let mut file = std::fs::File::create(&out_path).expect("writable output");
    surface.write_to_png(&mut file).expect("png");
    println!(
        "{out_path}: {drawn} of {} blocks, column {column:.0}px of {width:.0}px",
        plan.len()
    );
}
