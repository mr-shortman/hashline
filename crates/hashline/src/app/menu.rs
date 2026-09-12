//! The header bar's menu.
//!
//! It was a `GMenu`, which can only be a column of rows, so the two settings
//! that are not commands — the theme and the type size — were spelled out as
//! six separate rows, and the two overlays that already have their own buttons
//! in the header bar had rows as well. The design reference has neither: its
//! settings panel is two commands, a segmented theme switch and one zoom row
//! (`.menu-row`, `.theme-options`, `.zoom-options` in `docs/design.md`).
//!
//! That is what this builds. Every control is still an activation of a window
//! action, so the menu, the keyboard and assistive technology continue to share
//! one source (docs/design.md).

use gtk::prelude::*;

/// The three theme modes, in the order the switch shows them.
const MODES: [(&str, &str); 3] = [("System", "system"), ("Hell", "light"), ("Dunkel", "dark")];

/// The menu, and the two things in it the window has to keep up to date.
pub struct Menu {
    pub popover: gtk::Popover,
    /// The segmented switch, in `MODES` order.
    themes: Vec<gtk::ToggleButton>,
    /// The type size, shown on the button that resets it.
    zoom: gtk::Label,
}

impl Menu {
    /// Marks the mode that is in force. Called for the stored preference and
    /// for every later change, so the switch never disagrees with the action.
    pub fn set_theme(&self, mode: &str) {
        for (button, (_, value)) in self.themes.iter().zip(MODES) {
            if (value == mode) != button.is_active() {
                button.set_active(value == mode);
            }
        }
    }

    /// Shows the type size the document is set at.
    pub fn set_zoom(&self, percent: i32) {
        self.zoom.set_text(&format!("{percent} %"));
    }

    /// The mode the switch is showing, and what it says about the size.
    #[cfg(test)]
    pub fn showing(&self) -> (Option<&'static str>, String) {
        let mode = self
            .themes
            .iter()
            .zip(MODES)
            .find(|(button, _)| button.is_active())
            .map(|(_, (_, mode))| mode);
        (mode, self.zoom.text().to_string())
    }
}

pub fn build() -> Menu {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 2);
    content.set_width_request(260);

    // The two commands. They close the menu: what they do happens elsewhere,
    // and a panel left standing over a file dialog is in the way.
    for (label, shortcut, action) in [
        ("Datei öffnen …", "Ctrl+O", "win.open"),
        ("Neu laden", "Ctrl+R", "win.reload"),
    ] {
        content.append(&command_row(label, shortcut, action));
    }

    content.append(&separator());

    let heading = gtk::Label::new(Some("Darstellung"));
    heading.set_xalign(0.0);
    heading.add_css_class("settings-label");
    heading.set_margin_start(10);
    heading.set_margin_top(4);
    heading.set_margin_bottom(6);
    content.append(&heading);
    let (switch, themes) = theme_switch();
    content.append(&switch);

    content.append(&separator());

    let (row, zoom) = zoom_row();
    content.append(&row);

    let popover = gtk::Popover::builder()
        .child(&content)
        .has_arrow(false)
        .build();
    // Without an arrow the panel hangs straight off the button, so its top edge
    // lands inside the header bar, on the header's own bottom rule. It should
    // begin just below the title bar instead, and how far below the button that
    // is depends on the padding the system theme gives the header — so it is
    // measured when the panel is shown rather than guessed here.
    popover.connect_show(drop_below_titlebar);
    Menu {
        popover,
        themes,
        zoom,
    }
}

/// Air between the title bar and the panel under it.
const PANEL_GAP: i32 = 6;

/// Moves the panel down to clear the header bar.
///
/// The offset is whatever is left of the header below the button the panel
/// hangs from, plus the gap. Measured at every showing, so it survives a theme
/// with different header padding and a header that changes height.
fn drop_below_titlebar(popover: &gtk::Popover) {
    let Some(button) = popover.parent() else {
        return;
    };
    let Some(header) = button.ancestor(gtk::HeaderBar::static_type()) else {
        return;
    };
    let Some(bounds) = button.compute_bounds(&header) else {
        return;
    };
    let below = header.height() as f32 - (bounds.y() + bounds.height());
    popover.set_offset(0, below.max(0.0).round() as i32 + PANEL_GAP);
}

/// A row that reads as one line: what it does on the left, its key on the
/// right.
fn command_row(label: &str, shortcut: &str, action: &str) -> gtk::Button {
    let name = gtk::Label::new(Some(label));
    name.set_xalign(0.0);
    name.set_hexpand(true);
    let key = gtk::Label::new(Some(shortcut));
    key.add_css_class("menu-shortcut");
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.append(&name);
    row.append(&key);
    let button = gtk::Button::builder().child(&row).build();
    button.add_css_class("flat");
    button.add_css_class("menu-row");
    button.set_action_name(Some(action));
    button.connect_clicked(|button| {
        if let Some(popover) = button.ancestor(gtk::Popover::static_type()) {
            popover.downcast_ref::<gtk::Popover>().unwrap().popdown();
        }
    });
    button
}

fn separator() -> gtk::Separator {
    let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
    separator.set_margin_top(6);
    separator.set_margin_bottom(6);
    separator
}

/// System, Hell and Dunkel side by side, one of them held down.
///
/// The buttons share a group, so exactly one is ever down, and each carries the
/// window's theme action with its own mode as the target. Pressing one is
/// therefore the same event as choosing that mode from the keyboard.
fn theme_switch() -> (gtk::Box, Vec<gtk::ToggleButton>) {
    // Two pixels between the segments, not four: the track around them is what
    // groups the three, so wide gaps only cut it into three separate buttons.
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    row.add_css_class("theme-options");
    row.set_homogeneous(true);
    row.update_property(&[gtk::accessible::Property::Label("Darstellung")]);
    let mut buttons: Vec<gtk::ToggleButton> = Vec::new();
    for (label, mode) in MODES {
        let button = gtk::ToggleButton::with_label(label);
        button.set_hexpand(true);
        if let Some(first) = buttons.first() {
            button.set_group(Some(first));
        }
        button.set_detailed_action_name(&format!("win.theme::{mode}"));
        row.append(&button);
        buttons.push(button);
    }
    (row, buttons)
}

/// One line: what it sets on the left, minus, the size, plus on the right.
/// The size in the middle is itself the button that puts it back to 100 %.
fn zoom_row() -> (gtk::Box, gtk::Label) {
    let label = gtk::Label::new(Some("Textgröße"));
    label.set_xalign(0.0);
    label.set_hexpand(true);
    label.add_css_class("settings-label");
    label.set_margin_start(10);

    let zoom = gtk::Label::new(Some("100 %"));
    zoom.set_width_chars(5);

    // Typographic minus and plus rather than icons: the reference sets them in
    // type, and an icon theme is free to draw `zoom-out-symbolic` as anything.
    let smaller = gtk::Button::with_label("\u{2212}");
    smaller.set_tooltip_text(Some("Text verkleinern (Ctrl+-)"));
    smaller.update_property(&[gtk::accessible::Property::Label("Text verkleinern")]);
    smaller.set_action_name(Some("win.zoom-out"));
    let reset = gtk::Button::builder().child(&zoom).build();
    reset.set_tooltip_text(Some("Originalgröße (Ctrl+0)"));
    reset.update_property(&[gtk::accessible::Property::Label("Originalgröße")]);
    reset.set_action_name(Some("win.zoom-reset"));
    let larger = gtk::Button::with_label("+");
    larger.set_tooltip_text(Some("Text vergrößern (Ctrl++)"));
    larger.update_property(&[gtk::accessible::Property::Label("Text vergrößern")]);
    larger.set_action_name(Some("win.zoom-in"));

    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    controls.add_css_class("zoom-options");
    for button in [&smaller, &reset, &larger] {
        button.add_css_class("flat");
        controls.append(button);
    }

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.append(&label);
    row.append(&controls);
    (row, zoom)
}
