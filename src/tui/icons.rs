//! The glyph set used by the chrome.
//!
//! Two sets exist and they are structurally identical, so nothing in the drawing code
//! needs to know which one is in force: `--no-icons` (or `icons = false`) simply swaps
//! Nerd Font glyphs for plain Unicode of the same display width.

/// The glyphs the status bar, table of contents and help overlay draw with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Icons {
    /// Marks the file name in the status bar.
    pub file: &'static str,
    /// Marks the table-of-contents pane title.
    pub toc: &'static str,
    /// Marks the search state in the status bar.
    pub search: &'static str,
    /// Marks the current heading in the status bar.
    pub heading: &'static str,
    /// Marks the help overlay title.
    pub help: &'static str,
    /// Drawn in front of the selected table-of-contents entry.
    pub selected: &'static str,
    /// Drawn in front of an unselected table-of-contents entry.
    pub unselected: &'static str,
    /// Separates status-bar segments.
    pub separator: &'static str,
    /// Marks a warning or error notice.
    pub warning: &'static str,
    /// Marks the horizontal offset when content is scrolled sideways.
    pub horizontal: &'static str,
}

impl Icons {
    /// The Nerd Font glyph set.
    pub const NERD: Icons = Icons {
        file: "\u{f0219}",
        toc: "\u{f02d}",
        search: "\u{f002}",
        heading: "\u{f0f6}",
        help: "\u{f059}",
        // Deliberately not a Nerd Font glyph: the marker sits in a column the rest of
        // the pane aligns against, and private-use glyphs are ambiguous-width.
        selected: "\u{25b8}",
        unselected: " ",
        separator: "\u{e0b1}",
        warning: "\u{f071}",
        // Plain in both sets: an arrow that renders double-width in some terminals
        // would shift the whole status bar.
        horizontal: "\u{2194}",
    };

    /// The plain-Unicode fallback, for terminals without a Nerd Font.
    pub const PLAIN: Icons = Icons {
        file: "\u{25a4}",
        toc: "\u{2261}",
        search: "\u{2315}",
        heading: "\u{00a7}",
        help: "?",
        selected: "\u{25b8}",
        unselected: " ",
        separator: "\u{2502}",
        warning: "!",
        horizontal: "\u{2194}",
    };

    /// Picks a glyph set.
    pub fn new(nerd_font: bool) -> Self {
        if nerd_font { Self::NERD } else { Self::PLAIN }
    }
}

/// The eighth-block glyphs, from empty to full.
///
/// Used to draw the status-bar progress meter with sub-cell precision, so a long
/// document's position reads smoothly instead of jumping a whole column at a time.
pub const EIGHTHS: [&str; 9] = [
    " ", "\u{258f}", "\u{258e}", "\u{258d}", "\u{258c}", "\u{258b}", "\u{258a}", "\u{2589}",
    "\u{2588}",
];

/// The glyph the unfilled part of a meter is drawn with.
///
/// A gauge needs a track. Left blank, an empty meter is eight cells of bar colour and
/// reads as a hole rather than as "nothing yet" (visual review P9).
pub const TROUGH: &str = "\u{2591}";

/// Renders a fractional bar `width` cells wide as its filled and unfilled halves.
///
/// The two are returned separately because they are drawn in different colours; their
/// display widths always add up to exactly `width`.
pub fn meter(fraction: f32, width: usize) -> (String, String) {
    let fraction = fraction.clamp(0.0, 1.0);
    let eighths = (fraction * (width * 8) as f32).round() as usize;
    let full = (eighths / 8).min(width);
    let remainder = eighths % 8;
    let mut filled = String::with_capacity(width * 3);
    for _ in 0..full {
        filled.push_str(EIGHTHS[8]);
    }
    if full < width && remainder > 0 {
        filled.push_str(EIGHTHS[remainder]);
    }
    let drawn = full + usize::from(full < width && remainder > 0);
    (filled, TROUGH.repeat(width - drawn))
}
