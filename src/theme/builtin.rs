//! The built-in themes.
//!
//! Both themes are derived from a [`Palette`] by the same function, so the two are
//! guaranteed to define exactly the same slots and cannot drift apart.

use super::{
    BlockStyles, CodeStyles, Color, DiagramStyles, Palette, TableStyles, TextStyles, Theme,
    UiStyles,
};
use crate::theme::style::Style;

/// The signature dark theme.
pub(super) fn dark() -> Theme {
    from_palette(
        "dark",
        true,
        Palette {
            bg: Color::hex(0x11141b),
            surface: Color::hex(0x181c25),
            overlay: Color::hex(0x222836),
            fg: Color::hex(0xd6dbe5),
            muted: Color::hex(0x7c869b),
            border: Color::hex(0x39414f),
            accent: Color::hex(0x64b5ff),
            red: Color::hex(0xff6b7f),
            orange: Color::hex(0xffa657),
            yellow: Color::hex(0xf2d06b),
            green: Color::hex(0x76d7a0),
            cyan: Color::hex(0x5fd7d7),
            blue: Color::hex(0x7aa2f7),
            purple: Color::hex(0xb99bf8),
            magenta: Color::hex(0xf07fd0),
        },
    )
}

/// The built-in light theme.
pub(super) fn light() -> Theme {
    from_palette(
        "light",
        false,
        Palette {
            bg: Color::hex(0xfdfcf9),
            surface: Color::hex(0xf1efe9),
            overlay: Color::hex(0xe3e0d7),
            fg: Color::hex(0x2b2f38),
            muted: Color::hex(0x6b7280),
            border: Color::hex(0xc3c0b6),
            accent: Color::hex(0x1a6fd4),
            red: Color::hex(0xc0392b),
            orange: Color::hex(0xb35c00),
            yellow: Color::hex(0x8a6d00),
            green: Color::hex(0x1f7a4d),
            cyan: Color::hex(0x0f7b7b),
            blue: Color::hex(0x2f5fbf),
            purple: Color::hex(0x6b3fc0),
            magenta: Color::hex(0xa8298f),
        },
    )
}

/// How far each heading level is blended from [`Palette::accent`] towards
/// [`Palette::muted`].
///
/// Depth must read as *recession*, so the ramp stays inside one hue family and loses
/// saturation and contrast at every step instead of introducing a new hue. The step of
/// `0.16` is chosen so adjacent levels stay perceptibly apart (≥ 24 in RGB Manhattan
/// distance) in both built-in palettes; `theme_headings` asserts it.
const HEADING_RAMP: [f32; 6] = [0.0, 0.16, 0.32, 0.48, 0.64, 0.80];

/// The heading level at and above which the text is drawn bold.
///
/// The top three levels carry weight as well as colour; below that only the colour
/// steps down, so a deep heading settles into the page rather than shouting from it.
const HEADING_BOLD_THROUGH: usize = 3;

/// How far a heading's prefix glyph is blended towards the page background.
///
/// The glyph takes the heading's own colour — that is the whole point of it — but one
/// shade quieter, so the marker introduces the heading rather than competing with it.
const HEADING_PREFIX_FADE: f32 = 0.15;

/// How far a heading's rule is blended towards [`Palette::border`].
///
/// Kept small on purpose: the rule under the signature heading must not be fainter
/// than the text it underlines, which is exactly what a plain border-coloured rule was.
const HEADING_RULE_FADE: f32 = 0.12;

/// The heading foreground for a zero-based level index.
pub(super) fn heading_color(p: &Palette, index: usize) -> Color {
    p.accent
        .blend(p.muted, HEADING_RAMP[index.min(HEADING_RAMP.len() - 1)])
}

/// Builds a complete theme from a palette.
pub(super) fn from_palette(name: &str, is_dark: bool, p: Palette) -> Theme {
    // A style carrying the page background, used as the base for everything that is
    // not explicitly raised onto a surface.
    let base = Style::new().fg(p.fg).bg(p.bg);
    let on_surface = Style::new().fg(p.fg).bg(p.surface);
    let muted = Style::new().fg(p.muted).bg(p.bg);
    // Slightly stronger than `muted` so highlighted code still reads on the surface.
    let code_muted = Style::new().fg(p.muted).bg(p.surface);
    // A quiet neutral for chrome that must be seen but must not read as an accent:
    // scrollbar thumb, list bullets, overflow markers.
    let chrome = p.muted.blend(p.fg, 0.5);

    // One hue family, dimming with depth. `std::array::from_fn` keeps the ramp, the
    // prefix tint and the rule derived from a single rule rather than six literals.
    let headings: [Style; 6] = std::array::from_fn(|i| {
        let style = base.fg(heading_color(&p, i));
        if i < HEADING_BOLD_THROUGH {
            style.bold()
        } else {
            style
        }
    });
    let heading_prefixes: [Style; 6] =
        std::array::from_fn(|i| base.fg(heading_color(&p, i).blend(p.bg, HEADING_PREFIX_FADE)));
    let heading_rules: [Style; 6] =
        std::array::from_fn(|i| base.fg(heading_color(&p, i).blend(p.border, HEADING_RULE_FADE)));

    Theme {
        name: name.to_string(),
        is_dark,
        headings,
        heading_prefixes,
        heading_rules,
        text: TextStyles {
            body: base,
            emphasis: base.italic(),
            strong: base.bold(),
            strikethrough: muted.strikethrough(),
            link: base.fg(p.blue).underline(),
            link_url: muted,
            code: on_surface.fg(p.magenta),
            // A footnote reference is a link to elsewhere in the document, so it wears
            // the link hue rather than the heading accent.
            footnote_ref: base.fg(p.blue),
            image_alt: base.fg(p.cyan).italic(),
            dim: muted,
        },
        block: BlockStyles {
            quote_bar: base.fg(p.accent),
            quote_text: base.fg(p.muted).italic(),
            // Bullets are punctuation, not accents: they mark where an item starts and
            // then get out of the way.
            list_marker: base.fg(chrome),
            task_checked: base.fg(p.green),
            task_unchecked: base.fg(p.muted),
            rule: base.fg(p.border),
            // Level-aware variants live in `heading_rules` / `heading_prefixes`; these
            // two keep the level-1 value so existing call sites stay correct.
            heading_rule: heading_rules[0],
            heading_prefix: heading_prefixes[0],
            footnote_label: base.fg(p.blue).bold(),
            caption: muted.italic(),
            image_border: base.fg(p.border),
        },
        code: CodeStyles {
            text: on_surface,
            background: Style::new().bg(p.surface),
            frame: Style::new().fg(p.border).bg(p.bg),
            language: Style::new().fg(p.accent).bg(p.bg),
            line_number: code_muted,
            // Neutral, not orange: orange is the current search match, and a truncation
            // mark is chrome reporting on the layout, not content worth an accent.
            overflow_marker: on_surface.fg(chrome),
            keyword: on_surface.fg(p.purple),
            string: on_surface.fg(p.green),
            number: on_surface.fg(p.orange),
            comment: code_muted.italic(),
            function: on_surface.fg(p.blue),
            type_name: on_surface.fg(p.cyan),
            // Identifiers carry a faint cool tint so they separate from prose-like
            // plain text without becoming a colour of their own.
            variable: on_surface.fg(p.fg.blend(p.cyan, 0.22)),
            // Named constants sit beside numbers in the warm family but lean towards
            // the keyword hue, because `true` and `None` belong to the language rather
            // than to arithmetic.
            constant: on_surface.fg(p.orange.blend(p.purple, 0.35)),
            operator: on_surface.fg(p.muted),
            attribute: on_surface.fg(p.yellow),
            invalid: on_surface.fg(p.red).underline(),
            macro_name: on_surface.fg(p.magenta.blend(p.blue, 0.3)),
            // Quieter than `operator`: blended towards the border colour, which is the
            // palette's "structure, not content" hue.
            punctuation: on_surface.fg(p.muted.blend(p.border, 0.45)),
            namespace: on_surface.fg(p.cyan.blend(p.fg, 0.4)),
            // An escape must break the string run without shouting; amber sits between
            // the number and attribute hues and appears only inside green strings.
            escape: on_surface.fg(p.yellow.blend(p.orange, 0.5)),
        },
        table: TableStyles {
            border: base.fg(p.border),
            // Weight, not hue: the accent belongs to the heading hierarchy, and a table
            // header is already set apart by its rule and its position.
            header: base.fg(p.fg).bold(),
            cell: base,
            row_alt: Style::new().bg(p.surface),
            overflow_marker: base.fg(chrome),
        },
        diagram: DiagramStyles {
            // Lines are structure and must stay quieter than the labels riding on them,
            // but not so quiet that the diagram falls apart — halfway to the text.
            line: base.fg(p.border.blend(p.fg, 0.6)),
            // One ink for the whole edge: the arrowhead is the end of the line it is
            // attached to, and it shares the node hue so a diagram reads as one object.
            arrow: base.fg(p.blue),
            node_border: base.fg(p.blue),
            node_text: base,
            group_border: base.fg(p.purple),
            group_title: base.fg(p.purple).bold(),
            // In a sequence diagram the labels *are* the content, so they read at body
            // weight while the lines they sit on stay dim.
            edge_label: base,
            note: base.fg(p.yellow),
            // A lifeline is a line: it stays quieter than the messages riding on it,
            // but it must not be the faintest ink on the page, which a bare border
            // colour was. Kept one step under `line`, which carries arrowheads.
            lifeline: base.fg(p.border.blend(p.fg, 0.55)),
            activation: base.fg(p.cyan),
            compartment: base.fg(p.border),
            stereotype: base.fg(p.magenta).italic(),
            // A diagram title is a heading of its own; giving it the heading hue stops
            // it reading as a bold sentence of body text.
            title: base.fg(heading_color(&p, 1)).bold(),
            axis: base.fg(p.muted),
            legend: base.fg(p.fg),
            task_done: base.fg(p.green),
            task_active: base.fg(p.blue),
            task_crit: base.fg(p.red),
            milestone: base.fg(p.yellow).bold(),
        },
        ui: UiStyles {
            status_bar: Style::new().fg(p.fg).bg(p.overlay),
            // The file name is identity, not hierarchy: bold body text on the bar.
            status_accent: Style::new().fg(p.fg).bg(p.overlay).bold(),
            status_key: Style::new().fg(p.yellow).bg(p.overlay),
            scrollbar_track: Style::new().fg(p.border).bg(p.bg),
            // Chrome neutral. The scrollbar reports position; it is not a place the eye
            // should be pulled to, and the accent is spoken for by the headings.
            scrollbar_thumb: Style::new().fg(chrome).bg(p.bg),
            toc_border: base.fg(p.border),
            toc_item: base,
            toc_active: Style::new().fg(p.bg).bg(p.accent).bold(),
            toc_match: base.fg(p.yellow).bold(),
            help_border: base.fg(p.accent),
            help_title: base.fg(p.accent).bold(),
            search_match: Style::new().fg(p.bg).bg(p.yellow),
            search_current: Style::new().fg(p.bg).bg(p.orange).bold(),
            prompt: Style::new().fg(p.fg).bg(p.overlay),
            error: base.fg(p.red).bold(),
            warning: base.fg(p.yellow),
        },
        palette: p,
    }
}
